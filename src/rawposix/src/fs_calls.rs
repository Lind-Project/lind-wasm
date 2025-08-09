//! File System Syscall Implementation
//!
//! This file provides all system related syscall implementation in RawPOSIX
use cage::get_cage;
use cage::memory::mem_helper::*;
use cage::memory::vmmap::{VmmapOps, *};
use fdtables;
use libc::*;
use parking_lot::RwLock;
use std::sync::atomic::{AtomicI32, AtomicU64};
use std::sync::Arc;
use std::collections::HashMap;
use once_cell::sync::Lazy;
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::data::fs_struct::ShmidsStruct;
use sysdefs::constants::sys_const::DEFAULT_GID;
use sysdefs::constants::fs_const;
use sysdefs::constants::fs_const::{
    F_GETFL, F_GETOWN, F_SETOWN, MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_PRIVATE, MAP_SHARED,
    PAGESHIFT, PAGESIZE, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE, MAXFD, 
};
use typemap::syscall_type_conversion::*;
use typemap::{get_pipearray, sc_convert_path_to_host, convert_fd_to_host};

// --------------------- Shared Memory (SysV) minimal state for shmget ---------------------
// This mirrors the semantics used in main-reference for shmget only. Other SHM syscalls
// (shmat/shmdt/shmctl) are not implemented here.

#[derive(Clone)]
struct ShmSegmentMeta {
    key: i32,
    size: usize,
    mode: u16,
    creator_cageid: u64,
    rmid: bool,
    shminfo: ShmidsStruct,
}

#[derive(Default)]
struct ShmGlobalState {
    next_id: i32,
    key_to_id: HashMap<i32, i32>,
    id_to_seg: HashMap<i32, ShmSegmentMeta>,
    // reverse mapping per cage: (base_user_addr, shmid)
    cage_rev: HashMap<u64, Vec<(u32, i32)>>,
}

static SHM_STATE: Lazy<RwLock<ShmGlobalState>> = Lazy::new(|| {
    RwLock::new(ShmGlobalState {
        next_id: 1,
        key_to_id: HashMap::new(),
        id_to_seg: HashMap::new(),
        cage_rev: HashMap::new(),
    })
});

/// Lind-WASM is running as same Linux-Process from host kernel perspective, so standard fds shouldn't
/// be closed in Lind-WASM execution, which preventing issues where other threads might reassign these
/// fds, causing unintended behavior or errors.
pub fn kernel_close(fdentry: fdtables::FDTableEntry, _count: u64) {
    let kernel_fd = fdentry.underfd as i32;
    
    // TODO:
    // Need to update once we merge with vmmap-alice
    if kernel_fd == STDIN_FILENO || kernel_fd == STDOUT_FILENO || kernel_fd == STDERR_FILENO {
        return;
    }

    let ret = unsafe { libc::close(fdentry.underfd as i32) };
    if ret < 0 {
        let errno = get_errno();
        panic!("kernel_close failed with errno: {:?}", errno);
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/open.2.html
///
/// Linux `open()` syscall will open a file descriptor and set file status and permissions according to user needs. Since we
/// implement a file descriptor management subsystem (called `fdtables`), so we need to open a new virtual fd
/// after getting the kernel fd. `fdtables` currently only manage when a fd should be closed after open, so
/// then we need to set `O_CLOEXEC` flags according to input.
///
/// Input:
///     This call will only have one cageid indicates current cage, and three regular arguments same with Linux
///     - cageid: current cage
///     - path_arg: This argument points to a pathname naming the file. User's perspective.
///     - oflag_arg: This argument contains the file status flags and file access modes which will be alloted to
///                 the open file description. The flags are combined together using a bitwise-inclusive-OR and the
///                 result is passed as an argument to the function. We need to check if `O_CLOEXEC` has been set.
///     - mode_arg: This represents the permission of the newly created file. Directly passing to kernel.
pub fn open_syscall(
    cageid: u64,
    path_arg: u64,
    path_cageid: u64,
    oflag_arg: u64,
    oflag_cageid: u64,
    mode_arg: u64,
    mode_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_cageid, cageid);
    // Note the cageid here isn't really relevant because the argument is pass-by-value.
    // But it could be checked to ensure it's not set to something unexpected.
    let oflag = sc_convert_sysarg_to_i32(oflag_arg, oflag_cageid, cageid);
    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "open_syscall", "Invalide Cage ID");
    }

    
    // Get the kernel fd first
    let kernel_fd = unsafe { libc::open(path.as_ptr(), oflag, mode) };

    if kernel_fd < 0 {
        return handle_errno(get_errno(), "open_syscall");
    }

    // Check if `O_CLOEXEC` has been est
    let should_cloexec = (oflag & fs_const::O_CLOEXEC) != 0;

    // Mapping a new virtual fd and set `O_CLOEXEC` flag
    match fdtables::get_unused_virtual_fd(
        cageid,
        fs_const::FDKIND_KERNEL,
        kernel_fd as u64,
        should_cloexec,
        0,
    ) {
        Ok(virtual_fd) => virtual_fd as i32,
        Err(_) => syscall_error(Errno::EMFILE, "open_syscall", "Too many files opened"),
    }
}

/// Implements shmget-like behavior: create or look up a SysV shared memory segment ID.
/// Only the allocation of an ID and metadata tracking is handled here; attachment and
/// control operations are implemented elsewhere.
///
/// Semantics follow the main-reference:
/// - If key == IPC_PRIVATE: return ENOENT (not implemented here).
/// - If key exists and IPC_CREAT|IPC_EXCL are both set: EEXIST.
/// - If key missing and IPC_CREAT not set: ENOENT.
/// - Enforce SHMMIN <= size <= SHMMAX on create, else EINVAL.
/// - On create, store mode = low 9 bits of shmflg.
pub fn shmget_syscall(
    cageid: u64,
    key_arg: u64,
    key_cageid: u64,
    size_arg: u64,
    size_cageid: u64,
    shmflg_arg: u64,
    shmflg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let key = sc_convert_sysarg_to_i32(key_arg, key_cageid, cageid);
    let size = sc_convert_sysarg_to_usize(size_arg, size_cageid, cageid);
    let shmflg = sc_convert_sysarg_to_i32(shmflg_arg, shmflg_cageid, cageid);

    // Validate unused args
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "shmget", "Invalid Cage ID");
    }

    // Constants
    use sysdefs::constants::fs_const::{IPC_CREAT, IPC_EXCL, IPC_PRIVATE, SHMMAX, SHMMIN};

    if key == IPC_PRIVATE {
        return syscall_error(Errno::ENOENT, "shmget", "IPC_PRIVATE not implemented");
    }

    let mut state = SHM_STATE.write();

    if let Some(&existing_id) = state.key_to_id.get(&key) {
        // If both IPC_CREAT and IPC_EXCL are present, fail if key exists
        if (IPC_CREAT | IPC_EXCL) == (shmflg & (IPC_CREAT | IPC_EXCL)) {
            return syscall_error(
                Errno::EEXIST,
                "shmget",
                "key already exists and IPC_CREAT and IPC_EXCL were used",
            );
        }
        return existing_id;
    }

    // Creating new segment requires IPC_CREAT
    if (shmflg & IPC_CREAT) == 0 {
        return syscall_error(
            Errno::ENOENT,
            "shmget",
            "tried to use a key that did not exist, and IPC_CREAT was not specified",
        );
    }

    // Validate size within SHMMIN ..= SHMMAX
    if (size as u32) < SHMMIN || (size as u32) > SHMMAX {
        return syscall_error(
            Errno::EINVAL,
            "shmget",
            "Size is less than SHMMIN or more than SHMMAX",
        );
    }

    // Allocate a new id and record metadata
    let shmid = state.next_id;
    state.next_id = state
        .next_id
        .checked_add(1)
        .unwrap_or_else(|| 1); // wrap safely

    let mode = (shmflg & 0x1FF) as u16; // lower 9 bits
    let mut shminfo = ShmidsStruct::default();
    shminfo.shm_perm.__key = key;
    shminfo.shm_perm.uid = 0;
    shminfo.shm_perm.gid = 0;
    shminfo.shm_perm.cuid = 0;
    shminfo.shm_perm.cgid = 0;
    shminfo.shm_perm.mode = mode;
    shminfo.shm_segsz = size as u32;
    shminfo.shm_cpid = 0;
    shminfo.shm_lpid = 0;
    shminfo.shm_nattch = 0;

    let meta = ShmSegmentMeta {
        key,
        size,
        mode,
        creator_cageid: cageid,
        rmid: false,
        shminfo,
    };
    state.key_to_id.insert(key, shmid);
    state.id_to_seg.insert(shmid, meta);

    shmid
}

/// SHMAT: attach a shared memory segment to the caller's address space.
/// - Returns user-space address (positive) on success or negative errno on failure.
/// - If SHM_RDONLY set: map read-only; else read-write.
pub fn shmat_syscall(
    cageid: u64,
    shmid_arg: u64,
    shmid_cageid: u64,
    shmaddr_arg: u64,
    shmaddr_cageid: u64,
    shmflg_arg: u64,
    shmflg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let shmid = sc_convert_sysarg_to_i32(shmid_arg, shmid_cageid, cageid);
    let shmaddr = shmaddr_arg as *mut u8;
    let shmflg = sc_convert_sysarg_to_i32(shmflg_arg, shmflg_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "shmat", "Invalid Cage ID");
    }

    use sysdefs::constants::fs_const::{SHM_RDONLY, MAP_SHARED};

    let prot = if (shmflg & SHM_RDONLY) != 0 { PROT_READ } else { PROT_READ | PROT_WRITE };

    // Lookup segment
    let (size, rmid);
    {
        let state = SHM_STATE.read();
        if let Some(seg) = state.id_to_seg.get(&shmid) {
            size = seg.size;
            rmid = seg.rmid;
        } else {
            return syscall_error(Errno::EINVAL, "shmat", "Invalid shmid value");
        }
    }
    if rmid {
        return syscall_error(Errno::EINVAL, "shmat", "Segment marked for removal");
    }

    // Align and validate addr if provided (we allow hint or fixed behavior via user-specified aligned addr)
    let addr_u32 = shmaddr as u32;
    if addr_u32 != 0 {
        let rounded = round_up_page(addr_u32 as u64);
        if rounded != addr_u32 as u64 {
            return syscall_error(Errno::EINVAL, "shmat", "unaligned address");
        }
    }

    // Perform mapping using existing mmap handler semantics into user address space
    // Choose address: if not provided, pass 0 and let mmap_syscall pick space, else use requested
    let length_rounded = round_up_page(size as u64) as usize;
    let flags = (MAP_SHARED | MAP_FIXED) as i32; // map at exact address once chosen

    // Find or use addr via vmmap
    let cage = get_cage(cageid).unwrap();
    let useraddr = if addr_u32 == 0 {
        // find free space
        let mut vmmap = cage.vmmap.write();
        let npages = (length_rounded as u32) >> PAGESHIFT;
        let found = vmmap.find_map_space(npages, 1);
        if found.is_none() {
            return syscall_error(Errno::ENOMEM, "shmat", "no memory");
        }
        (found.unwrap().start() << PAGESHIFT) as u32
    } else {
        addr_u32
    };

    // Convert to system address and map anonymously (backed logically by shared segment)
    let sysaddr = {
        let vmmap = cage.vmmap.read();
        vmmap.user_to_sys(useraddr)
    };

    let mapret = mmap_inner(
        cageid,
        sysaddr as *mut u8,
        length_rounded,
        prot,
        flags | MAP_ANONYMOUS as i32,
        -1,
        0,
    );
    if mapret as i64 == -1 {
        return syscall_error(Errno::EINVAL, "shmat", "mmap failed");
    }

    // Track in vmmap and reverse map for detach
    {
        let mut vmmap = cage.vmmap.write();
        let backing = MemoryBackingType::SharedMemory(shmid as u64);
        let _ = vmmap.add_entry_with_overwrite(
            useraddr >> PAGESHIFT,
            (length_rounded as u32) >> PAGESHIFT,
            prot,
            prot,
            MAP_SHARED as i32 | MAP_FIXED as i32 | MAP_ANONYMOUS as i32,
            backing,
            0,
            size as i64,
            cageid,
        );
    }

    // Update metadata: nattch++ and reverse map
    {
        let mut state = SHM_STATE.write();
        if let Some(seg) = state.id_to_seg.get_mut(&shmid) {
            seg.shminfo.shm_nattch = seg.shminfo.shm_nattch.saturating_add(1);
        }
        let rev = state.cage_rev.entry(cageid).or_insert_with(Vec::new);
        rev.push((useraddr, shmid));
    }

    useraddr as i32
}

/// SHMDT: detach the shared memory at provided address. Returns shmid on success, negative errno on failure.
pub fn shmdt_syscall(
    cageid: u64,
    shmaddr_arg: u64,
    shmaddr_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let shmaddr = shmaddr_arg as *mut u8;
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "shmdt", "Invalid Cage ID");
    }

    let addr_u32 = shmaddr as u32;
    // Lookup rev map to find shmid and size
    let (shmid, size, rmid_after): (i32, usize, bool);
    {
        let mut state = SHM_STATE.write();
        let entry = state.cage_rev.entry(cageid).or_insert_with(Vec::new);
        if let Some(index) = entry.iter().position(|(a, _)| *a == addr_u32) {
            shmid = entry[index].1;
            entry.swap_remove(index);
        } else {
            return syscall_error(Errno::EINVAL, "shmdt", "No shared memory segment at shmaddr");
        }
        if let Some(seg) = state.id_to_seg.get_mut(&shmid) {
            seg.shminfo.shm_nattch = seg.shminfo.shm_nattch.saturating_sub(1);
            size = seg.size;
            rmid_after = seg.rmid && seg.shminfo.shm_nattch == 0;
        } else {
            return syscall_error(Errno::EINVAL, "shmdt", "Invalid shmid");
        }

        if rmid_after {
            // remove from tables
            let key = state.id_to_seg.get(&shmid).unwrap().key;
            state.id_to_seg.remove(&shmid);
            state.key_to_id.remove(&key);
        }
    }

    // Unmap by setting PROT_NONE, and remove vmmap entry
    let cage = get_cage(cageid).unwrap();
    let sysaddr = {
        let vmmap = cage.vmmap.read();
        vmmap.user_to_sys(addr_u32)
    };
    let length_rounded = round_up_page(size as u64) as usize;
    let result = unsafe {
        libc::mmap(
            sysaddr as *mut libc::c_void,
            length_rounded,
            PROT_NONE,
            (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
            -1,
            0,
        ) as usize
    };
    if result != sysaddr {
        panic!("MAP_FIXED not fixed");
    }
    {
        let mut vmmap = cage.vmmap.write();
        let _ = vmmap.remove_entry(addr_u32 >> PAGESHIFT, (length_rounded as u32) >> PAGESHIFT);
    }

    shmid
}

/// SHMCTL: IPC_STAT and IPC_RMID supported.
pub fn shmctl_syscall(
    cageid: u64,
    shmid_arg: u64,
    shmid_cageid: u64,
    cmd_arg: u64,
    cmd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let shmid = sc_convert_sysarg_to_i32(shmid_arg, shmid_cageid, cageid);
    let cmd = sc_convert_sysarg_to_i32(cmd_arg, cmd_cageid, cageid);
    let buf_ptr = sc_convert_buf(buf_arg, buf_cageid, cageid) as *mut ShmidsStruct;

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "shmctl", "Invalid Cage ID");
    }

    use sysdefs::constants::fs_const::{IPC_STAT, IPC_RMID, SHM_DEST};

    let mut state = SHM_STATE.write();
    let seg = match state.id_to_seg.get_mut(&shmid) {
        Some(s) => s,
        None => return syscall_error(Errno::EINVAL, "shmctl", "Invalid identifier"),
    };

    match cmd {
        IPC_STAT => {
            if buf_ptr.is_null() {
                return syscall_error(Errno::EINVAL, "shmctl", "buf is null");
            }
            unsafe { *buf_ptr = seg.shminfo };
            0
        }
        IPC_RMID => {
            seg.rmid = true;
            seg.shminfo.shm_perm.mode |= SHM_DEST as u16;
            if seg.shminfo.shm_nattch == 0 {
                let key = seg.key;
                drop(seg);
                state.id_to_seg.remove(&shmid);
                state.key_to_id.remove(&key);
            }
            0
        }
        _ => syscall_error(
            Errno::EINVAL,
            "shmctl",
            "Arguments provided do not match implemented parameters",
        ),
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/read.2.html
///
/// Linux `read()` syscall attempts to read up to a specified number of bytes from a file descriptor into a buffer.
/// Since we implement a file descriptor management subsystem (called `fdtables`), we first translate the virtual file
/// descriptor into the corresponding kernel file descriptor before invoking the kernel's `libc::read()` function.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - virtual_fd: the virtual file descriptor from the RawPOSIX environment.
///     - buf_arg: pointer to a buffer where the read data will be stored (user's perspective).
///     - count_arg: the maximum number of bytes to read from the file descriptor.
pub fn read_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Convert the virtual fd to the underlying kernel file descriptor.
    let kernel_fd = convert_fd_to_host(virtual_fd, vfd_cageid, cageid);
    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "read", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "read", "Bad File Descriptor");
    }

    // Convert the user buffer and count.
    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    if buf.is_null() {
        return syscall_error(Errno::EFAULT, "read", "Buffer is null");
    }

    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);

    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "read", "Invalid Cage ID");
    }

    // Early return if count is zero.
    if count == 0 {
        return 0;
    }

    // Call the underlying libc read.
    let ret = unsafe { libc::read(kernel_fd, buf as *mut c_void, count) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "read");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/close.2.html
///
/// Linux `close()` syscall closes a file descriptor. In our implementation, we use a file descriptor management
/// subsystem (called `fdtables`) to handle virtual file descriptors. This syscall removes the virtual file
/// descriptor from the subsystem, and if necessary, closes the underlying kernel file descriptor.
///
/// Input:
///     This call will have one cageid indicating the current cage, and several regular arguments similar to Linux:
///     - cageid: current cage identifier.
///     - virtual_fd: the virtual file descriptor from the RawPOSIX environment to be closed.
///     - arg3, arg4, arg5, arg6: additional arguments which are expected to be unused.
pub fn close_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "close", "Invalid Cage ID");
    }

    // Since `virtual_fd` is unsigned value, so we don't need to compare negative case here
    if virtual_fd > MAXFD as u64 {
        return syscall_error(Errno::EBADF, "close", "Bad File Descriptor");
    }

    match fdtables::close_virtualfd(cageid, virtual_fd) {
        Ok(()) => 0,
        Err(e) => {
            if e == Errno::EBADF as u64 {
                syscall_error(Errno::EBADF, "close", "Bad File Descriptor")
            } else if e == Errno::EINTR as u64 {
                syscall_error(Errno::EINTR, "close", "Interrupted system call")
            } else {
                syscall_error(Errno::EIO, "close", "I/O error")
            }
        }
    }
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pipe.2.html
///
/// Linux `pipe()` syscall is equivalent to calling `pipe2()` with flags set to zero.
/// Therefore, our implementation simply delegates to pipe2_syscall with flags = 0.
///
/// Input:
///     - cageid: current cage identifier.
///     - pipefd_arg: a u64 representing the pointer to the PipeArray (user's perspective).
///     - pipefd_cageid: cage identifier for the pointer argument.
pub fn pipe_syscall(
    cageid: u64,
    pipefd_arg: u64,
    pipefd_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Delegate to pipe2_syscall with flags set to 0.
    pipe2_syscall(
        cageid,
        pipefd_arg,
        pipefd_cageid,
        0,
        0,
        arg3,
        arg3_cageid,
        arg4,
        arg4_cageid,
        arg5,
        arg5_cageid,
        arg6,
        arg6_cageid,
    )
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/pipe2.2.html
///
/// Linux `pipe2()` syscall creates a unidirectional data channel and returns two file descriptors,
/// one for reading and one for writing. In our implementation, we first convert the user-supplied
/// pointer to a mutable reference to a PipeArray. Then, we call libc::pipe2() with the provided flags.
/// Finally, we obtain new virtual file descriptors for both ends of the pipe using our fd management
/// subsystem (`fdtables`).
///
/// Input:
///     - cageid: current cage identifier.
///     - pipefd_arg: a u64 representing the pointer to the PipeArray (user's perspective).
///     - pipefd_cageid: cage identifier for the pointer argument.
///     - flags_arg: this argument contains flags (e.g., O_CLOEXEC) to be passed to pipe2.
///     - flags_cageid: cage identifier for the flags argument.
pub fn pipe2_syscall(
    cageid: u64,
    pipefd_arg: u64,
    pipefd_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Convert the flags argument.
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);

    // Validate flags - only O_NONBLOCK and O_CLOEXEC are allowed
    let allowed_flags = fs_const::O_NONBLOCK | fs_const::O_CLOEXEC;
    if flags & !allowed_flags != 0 {
        return syscall_error(Errno::EINVAL, "pipe2_syscall", "Invalid flags");
    }

    // Ensure unused arguments are truly unused.
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "pipe2_syscall", "Invalid Cage ID");
    }
    // Convert the u64 pointer into a mutable reference to PipeArray.
    let pipefd = match get_pipearray(pipefd_arg) {
        Ok(p) => p,
        Err(e) => return e,
    };
    // Create an array to hold the two kernel file descriptors.
    let mut kernel_fds: [i32; 2] = [0; 2];
    let ret = unsafe { libc::pipe2(kernel_fds.as_mut_ptr(), flags) };
    if ret < 0 {
        return handle_errno(get_errno(), "pipe2_syscall");
    }

    // Check whether O_CLOEXEC is set.
    let should_cloexec = (flags & fs_const::O_CLOEXEC) != 0;

    // Get virtual fd for read end
    let read_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        fs_const::FDKIND_KERNEL,
        kernel_fds[0] as u64,
        should_cloexec,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_) => {
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(Errno::EMFILE, "pipe2_syscall", "Too many files opened");
        }
    };

    // Get virtual fd for write end
    let write_vfd = match fdtables::get_unused_virtual_fd(
        cageid,
        fs_const::FDKIND_KERNEL,
        kernel_fds[1] as u64,
        should_cloexec,
        0,
    ) {
        Ok(fd) => fd as i32,
        Err(_) => {
            unsafe {
                libc::close(kernel_fds[0]);
                libc::close(kernel_fds[1]);
            }
            return syscall_error(Errno::EMFILE, "pipe2_syscall", "Too many files opened");
        }
    };

    pipefd.readfd = read_vfd;
    pipefd.writefd = write_vfd;
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/mkdir.2.html
///
/// Linux `mkdir()` syscall creates a new directory named by the path name pointed to by a path as the input parameter
/// in the function. Since path seen by user is different from actual path on host, we need to convert the path first.
/// RawPOSIX doesn't have any other operations, so all operations will be handled by host. RawPOSIX does error handling
/// for this syscall.
///
/// Input:
///     - cageid: current cageid
///     - path_arg: This argument points to a pathname naming the file. User's perspective.
///     - mode_arg: This represents the permission of the newly created file. Directly passing to kernel.
///
/// Return:
///     - return zero on success.  On error, -1 is returned and errno is set to indicate the error.
pub fn mkdir_syscall(
    cageid: u64,
    path_arg: u64,
    path_arg_cageid: u64,
    mode_arg: u64,
    mode_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let path = sc_convert_path_to_host(path_arg, path_arg_cageid, cageid);
    // Note the cageid here isn't really relevant because the argument is pass-by-value.
    // But it could be checked to ensure it's not set to something unexpected.
    let mode = sc_convert_sysarg_to_u32(mode_arg, mode_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "mkdir_syscall", "Invalide Cage ID");
    }

    let ret = unsafe { libc::mkdir(path.as_ptr(), mode) };
    // Error handling
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "mkdir");
    }
    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/write.2.html
///
/// Linux `write()` syscall attempts to write `count` bytes from the buffer pointed to by `buf` to the file associated
/// with the open file descriptor, `fd`. RawPOSIX first converts virtual fd to kernel fd due to the `fdtable` subsystem, second
/// translates the `buf_arg` pointer to actual system pointer
///
/// Input:
///     - cageid: current cageid
///     - virtual_fd: virtual file descriptor, needs to be translated kernel fd for future kernel operation
///     - buf_arg: pointer points to a buffer that stores the data
///     - count_arg: length of the buffer
///
/// Output:
///     - Upon successful completion of this call, we return the number of bytes written. This number will never be greater
///         than `count`. The value returned may be less than `count` if the write_syscall() was interrupted by a signal, or
///         if the file is a pipe or FIFO or special file and has fewer than `count` bytes immediately available for writing.
pub fn write_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64,
    buf_arg: u64,
    buf_cageid: u64,
    count_arg: u64,
    count_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let kernel_fd = convert_fd_to_host(virtual_fd, vfd_cageid, cageid);

    if kernel_fd == -1 {
        return syscall_error(Errno::EFAULT, "write", "Invalid Cage ID");
    } else if kernel_fd == -9 {
        return syscall_error(Errno::EBADF, "write", "Bad File Descriptor");
    }

    let buf = sc_convert_buf(buf_arg, buf_cageid, cageid);
    let count = sc_convert_sysarg_to_usize(count_arg, count_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "write", "Invalide Cage ID");
    }

    // Early return
    if count == 0 {
        return 0;
    }

    let ret = unsafe { libc::write(kernel_fd, buf as *const c_void, count) as i32 };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "write");
    }
    return ret;
}

pub fn dup_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "dup", "Invalide Cage ID");
    }

    if virtual_fd < 0 {
        return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
    }
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, virtual_fd as u64);
    if wrappedvfd.is_err() {
        return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
    }
    let vfd = wrappedvfd.unwrap();
    let ret_kernelfd = unsafe { libc::dup(vfd.underfd as i32) };
    let ret_virtualfd =
        fdtables::get_unused_virtual_fd(cageid, vfd.fdkind, ret_kernelfd as u64, false, 0).unwrap();
    return ret_virtualfd as i32;
}

pub fn dup2_syscall(
    cageid: u64,
    old_virtualfd: u64,
    old_vfd_cageid: u64,
    new_virtualfd: u64,
    new_vfd_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "dup2", "Invalide Cage ID");
    }

    if old_virtualfd < 0 || new_virtualfd < 0 {
        return syscall_error(Errno::EBADF, "dup2", "Bad File Descriptor");
    }

    match fdtables::translate_virtual_fd(cageid, old_virtualfd) {
        Ok(old_vfd) => {
            let new_kernelfd = unsafe { libc::dup(old_vfd.underfd as i32) };
            // Map new kernel fd with provided kernel fd
            let _ret_kernelfd = unsafe { libc::dup2(old_vfd.underfd as i32, new_kernelfd) };
            let _ = fdtables::get_specific_virtual_fd(
                cageid,
                new_virtualfd,
                old_vfd.fdkind,
                new_kernelfd as u64,
                false,
                old_vfd.perfdinfo,
            )
            .unwrap();
            return new_virtualfd as i32;
        }
        Err(_e) => {
            return syscall_error(Errno::EBADF, "dup2", "Bad File Descriptor");
        }
    }
}

/// Handles the `mmap_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `mmap_syscall` by updating the `vmmap` entries and performing
/// the necessary mmap operations. The handling logic is as follows:
/// 1. Restrict allowed flags to `MAP_FIXED`, `MAP_SHARED`, `MAP_PRIVATE`, and `MAP_ANONYMOUS`.
/// 2. Disallow `PROT_EXEC`; return `EINVAL` if the `prot` argument includes `PROT_EXEC`.
/// 3. If `MAP_FIXED` is not specified, query the `vmmap` structure to locate an available memory region.
///    Otherwise, use the address provided by the user.
/// 4. Invoke the actual `mmap` syscall with the `MAP_FIXED` flag to configure the memory region's protections.
/// 5. Update the corresponding `vmmap` entry.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `mmap` syscall.
/// * `addr` - Starting address of the memory region to mmap.
/// * `len` - Length of the memory region to mmap.
/// * `prot` - Memory protection flags (e.g., `PROT_READ`, `PROT_WRITE`).
/// * `flags` - Mapping flags (e.g., `MAP_SHARED`, `MAP_ANONYMOUS`).
/// * `fildes` - File descriptor associated with the mapping, if applicable.
/// * `off` - Offset within the file, if applicable.
///
/// # Returns
/// * `u32` - Result of the `mmap` operation. See "man mmap" for details
pub fn mmap_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    prot_arg: u64,
    prot_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    virtual_fd_arg: u64,
    vfd_cageid: u64,
    off_arg: u64,
    off_cageid: u64,
) -> i32 {
    let mut addr = addr_arg as *mut u8;
    let mut len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    let mut prot = sc_convert_sysarg_to_i32(prot_arg, prot_cageid, cageid);
    let mut flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let mut fildes = convert_fd_to_host(virtual_fd_arg, vfd_cageid, cageid);
    let mut off = sc_convert_sysarg_to_i64(off_arg, off_cageid, cageid);

    let cage = get_cage(cageid).unwrap();

    let mut maxprot = PROT_READ | PROT_WRITE;

    // only these four flags are allowed
    let allowed_flags =
        MAP_FIXED as i32 | MAP_SHARED as i32 | MAP_PRIVATE as i32 | MAP_ANONYMOUS as i32;
    if flags & !allowed_flags > 0 {
        // truncate flag to remove flags that are not allowed
        flags &= allowed_flags;
    }

    if prot & PROT_EXEC > 0 {
        return syscall_error(Errno::EINVAL, "mmap", "PROT_EXEC is not allowed");
    }

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64);
    if rounded_addr != addr as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "address it not aligned");
    }

    // offset should be non-negative and multiple of pages
    if off < 0 {
        return syscall_error(Errno::EINVAL, "mmap", "offset cannot be negative");
    }
    let rounded_off = round_up_page(off as u64);
    if rounded_off != off as u64 {
        return syscall_error(Errno::EINVAL, "mmap", "offset it not aligned");
    }

    // round up length to be multiple of pages
    let rounded_length = round_up_page(len as u64);

    let mut useraddr = addr as u32;
    // if MAP_FIXED is not set, then we need to find an address for the user
    if flags & MAP_FIXED as i32 == 0 {
        let mut vmmap = cage.vmmap.write();
        let result;

        // pick an address of appropriate size, anywhere
        if useraddr == 0 {
            result = vmmap.find_map_space(rounded_length as u32 >> PAGESHIFT, 1);
        } else {
            // use address user provided as hint to find address
            result =
                vmmap.find_map_space_with_hint(rounded_length as u32 >> PAGESHIFT, 1, addr as u32);
        }

        // did not find desired memory region
        if result.is_none() {
            return syscall_error(Errno::ENOMEM, "mmap", "no memory");
        }

        let space = result.unwrap();
        useraddr = (space.start() << PAGESHIFT) as u32;
    }

    flags |= MAP_FIXED as i32;

    // either MAP_PRIVATE or MAP_SHARED should be set, but not both
    if (flags & MAP_PRIVATE as i32 == 0) == (flags & MAP_SHARED as i32 == 0) {
        return syscall_error(Errno::EINVAL, "mmap", "invalid flags");
    }

    let vmmap = cage.vmmap.read();

    let sysaddr = vmmap.user_to_sys(useraddr);

    drop(vmmap);

    if rounded_length > 0 {
        if flags & MAP_ANONYMOUS as i32 > 0 {
            fildes = -1;
        }

        let result = mmap_inner(
            cageid,
            sysaddr as *mut u8,
            rounded_length as usize,
            prot,
            flags,
            fildes,
            off,
        );

        let vmmap = cage.vmmap.read();
        let result = vmmap.sys_to_user(result);
        drop(vmmap);

        // if mmap addr is positive, that would mean the mapping is successful and we need to update the vmmap entry
        if result >= 0 {
            if result != useraddr {
                panic!("MAP_FIXED not fixed");
            }

            let mut vmmap = cage.vmmap.write();
            let backing = {
                if flags as u32 & MAP_ANONYMOUS > 0 {
                    MemoryBackingType::Anonymous
                } else {
                    // if we are doing file-backed mapping, we need to set maxprot to the file permission
                    let flags = fcntl_syscall(
                        cageid,
                        fildes as u64,
                        vfd_cageid,
                        F_GETFL as u64,
                        flags_cageid,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                        0,
                    );
                    if flags < 0 {
                        return syscall_error(Errno::EINVAL, "mmap", "invalid file descriptor")
                            as i32;
                    }
                    maxprot &= flags;
                    MemoryBackingType::FileDescriptor(fildes as u64)
                }
            };

            // update vmmap entry
            let _ = vmmap.add_entry_with_overwrite(
                useraddr >> PAGESHIFT,
                (rounded_length >> PAGESHIFT) as u32,
                prot,
                maxprot,
                flags,
                backing,
                off,
                len as i64,
                cageid,
            );
        }
    }

    useraddr as i32
}

/// Helper function for `mmap` / `munmap`
///
/// This function calls underlying libc::mmap and serves as helper functions for memory related (vmmap related)
/// syscalls. This function provides fd translation between virtual to kernel and error handling.
pub fn mmap_inner(
    cageid: u64,
    addr: *mut u8,
    len: usize,
    prot: i32,
    flags: i32,
    virtual_fd: i32,
    off: i64,
) -> usize {
    if virtual_fd != -1 {
        match fdtables::translate_virtual_fd(cageid, virtual_fd as u64) {
            Ok(kernel_fd) => {
                let ret = unsafe {
                    libc::mmap(
                        addr as *mut c_void,
                        len,
                        prot,
                        flags,
                        kernel_fd.underfd as i32,
                        off,
                    ) as i64
                };

                // Check if mmap failed and return the appropriate error if so
                if ret == -1 {
                    return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags")
                        as usize;
                }

                ret as usize
            }
            Err(_e) => {
                return syscall_error(Errno::EBADF, "mmap", "Bad File Descriptor") as usize;
            }
        }
    } else {
        // Handle mmap with fd = -1 (anonymous memory mapping or special case)
        let ret = unsafe { libc::mmap(addr as *mut c_void, len, prot, flags, -1, off) as i64 };
        // Check if mmap failed and return the appropriate error if so
        if ret == -1 {
            return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags") as usize;
        }

        ret as usize
    }
}

/// Handler of the `munmap_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `munmap_syscall` by updating the `vmmap` entries and managing
/// the unmap operation. Instead of invoking the actual `munmap` syscall, the unmap operation
/// is simulated by setting the specified region to `PROT_NONE`. The memory remains valid but
/// becomes inaccessible due to the `PROT_NONE` setting.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that calls the `munmap`
/// * `addr` - Starting address of the region to unmap
/// * `length` - Length of the region to unmap
///
/// # Returns
/// * `i32` - 0 for success and -1 for failure
pub fn munmap_syscall(
    cageid: u64,
    addr_arg: u64,
    addr_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let addr = addr_arg as *mut u8;
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "munmap", "Invalide Cage ID");
    }

    if len == 0 {
        return syscall_error(Errno::EINVAL, "munmap", "length cannot be zero");
    }
    let cage = get_cage(addr_cageid).unwrap();

    // check if the provided address is multiple of pages
    let rounded_addr = round_up_page(addr as u64) as usize;
    if rounded_addr != addr as usize {
        return syscall_error(Errno::EINVAL, "munmap", "address it not aligned");
    }

    let vmmap = cage.vmmap.read();
    let sysaddr = vmmap.user_to_sys(rounded_addr as u32);
    drop(vmmap);

    let rounded_length = round_up_page(len as u64) as usize;

    // we are replacing munmap with mmap because we do not want to really deallocate the memory region
    // we just want to set the prot of the memory region back to PROT_NONE
    // Directly call libc::mmap to improve performance
    let result = unsafe {
        libc::mmap(
            sysaddr as *mut c_void,
            rounded_length,
            PROT_NONE,
            (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
            -1,
            0,
        ) as usize
    };
    if result != sysaddr {
        panic!("MAP_FIXED not fixed");
    }

    let mut vmmap = cage.vmmap.write();

    vmmap.remove_entry(rounded_addr as u32 >> PAGESHIFT, len as u32 >> PAGESHIFT);

    0
}

/// Handles the `brk_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `brk_syscall` by updating the `vmmap` entries and performing
/// the necessary operations to adjust the program break. Specifically, it updates the program
/// break by modifying the end of the heap entry (the first entry in `vmmap`) and invokes `mmap`
/// to adjust the memory protection as needed.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `brk` syscall.
/// * `brk` - The new program break address.
///
/// # Returns
/// * `u32` - Returns `0` on success or `-1` on failure.
///
pub fn brk_syscall(
    cageid: u64,
    brk_arg: u64,
    brk_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let brk = sc_convert_sysarg_to_i32(brk_arg, brk_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "brk", "Invalide Cage ID");
    }

    let cage = get_cage(cageid).unwrap();

    let mut vmmap = cage.vmmap.write();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    assert!(heap.npages == vmmap.program_break);

    let old_brk_page = heap.npages;
    // round up the break to multiple of pages
    let brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as u32;

    // if we are incrementing program break, we need to check if we have enough space
    if brk_page > old_brk_page {
        if vmmap.check_existing_mapping(old_brk_page, brk_page - old_brk_page, 0) {
            return syscall_error(Errno::ENOMEM, "brk", "no memory");
        }
    }

    // update vmmap entry
    vmmap.add_entry_with_overwrite(
        0,
        brk_page,
        heap.prot,
        heap.maxprot,
        heap.flags,
        heap.backing,
        heap.file_offset,
        heap.file_size,
        heap.cage_id,
    );

    let old_heap_end_usr = (old_brk_page * PAGESIZE) as u32;
    let old_heap_end_sys = vmmap.user_to_sys(old_heap_end_usr) as *mut u8;

    let new_heap_end_usr = (brk_page * PAGESIZE) as u32;
    let new_heap_end_sys = vmmap.user_to_sys(new_heap_end_usr) as *mut u8;

    vmmap.set_program_break(brk_page);

    drop(vmmap);

    // if new brk is larger than old brk
    // we need to mmap the new region
    if brk_page > old_brk_page {
        let ret = mmap_inner(
            brk_cageid,
            old_heap_end_sys,
            ((brk_page - old_brk_page) * PAGESIZE) as usize,
            heap.prot,
            (heap.flags as u32 | MAP_FIXED) as i32,
            -1,
            0,
        );

        if ret < 0 {
            panic!("brk mmap failed");
        }
    }
    // if we are shrinking the brk
    // we need to do something similar to munmap
    // to unmap the extra memory
    else if brk_page < old_brk_page {
        let ret = mmap_inner(
            brk_cageid,
            new_heap_end_sys,
            ((old_brk_page - brk_page) * PAGESIZE) as usize,
            PROT_NONE,
            (MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED) as i32,
            -1,
            0,
        );

        if ret < 0 {
            panic!("brk mmap failed");
        }
    }

    0
}

/// Handles the `sbrk_syscall`, interacting with the `vmmap` structure.
///
/// This function processes the `sbrk_syscall` by updating the `vmmap` entries and managing
/// the program break. It calculates the target program break after applying the specified
/// increment and delegates further processing to the `brk_handler`.
///
/// # Arguments
/// * `cageid` - Identifier of the cage that initiated the `sbrk` syscall.
/// * `brk` - Increment to adjust the program break, which can be negative.
///
/// # Returns
/// * `u32` - Result of the `sbrk` operation. Refer to `man sbrk` for details.
pub fn sbrk_syscall(
    cageid: u64,
    sbrk_arg: u64,
    sbrk_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let brk = sc_convert_sysarg_to_i32(sbrk_arg, sbrk_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg2, arg2_cageid)
        && sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "sbrk_syscall", "Invalide Cage ID");
    }

    let cage = get_cage(sbrk_cageid).unwrap();

    // get the heap entry
    let mut vmmap = cage.vmmap.read();
    let heap = vmmap.find_page(HEAP_ENTRY_INDEX).unwrap().clone();

    // program break should always be the same as the heap entry end
    assert!(heap.npages == vmmap.program_break);

    // pass 0 to sbrk will just return the current brk
    if brk == 0 {
        return (PAGESIZE * heap.npages) as i32;
    }

    // round up the break to multiple of pages
    // brk increment could possibly be negative
    let brk_page;
    if brk < 0 {
        brk_page = -((round_up_page(-brk as u64) >> PAGESHIFT) as i32);
    } else {
        brk_page = (round_up_page(brk as u64) >> PAGESHIFT) as i32;
    }

    // drop the vmmap so that brk_handler will not deadlock
    drop(vmmap);

    if brk_syscall(
        cageid,
        ((heap.npages as i32 + brk_page) << PAGESHIFT) as u64,
        sbrk_cageid,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    ) < 0
    {
        return syscall_error(Errno::ENOMEM, "sbrk", "no memory") as i32;
    }

    // sbrk syscall should return previous brk address before increment
    (PAGESIZE * heap.npages) as i32
}

//------------------------------------FCNTL SYSCALL------------------------------------
/// This function will be different in new code base (when splitting out type conversion function)
/// since the conversion from u64 -> i32 in negative number will be different. These lines are repeated
/// in 5 out of 6 fcntl_syscall cases, so wrapped these loc into helper functions to make code cleaner.
///
/// ## Arguments
/// cageid: cage ID associate with virtual file descriptor
/// virtual_fd: virtual file descriptor
///
/// ## Return Type
/// On success:
/// Return corresponding FDTableEntry that contains
/// (1) underlying kernel fd.
/// (2) file descriptor kind.
/// (3) O_CLOEXEC flag.
/// (4) file descriptor specific extra information.
///
/// On error:
/// Return error num EBADF(Bad File Descriptor)
pub fn _fcntl_helper(cageid: u64, virtual_fd: u64) -> Result<fdtables::FDTableEntry, Errno> {
    if virtual_fd > MAXFD as u64 {
        return Err(Errno::EBADF);
    }
    // Get underlying kernel fd
    let wrappedvfd = fdtables::translate_virtual_fd(cageid, virtual_fd);
    if wrappedvfd.is_err() {
        return Err(Errno::EBADF);
    }
    Ok(wrappedvfd.unwrap())
}

/// Reference: https://man7.org/linux/man-pages/man2/fcntl.2.html
///
/// Due to the design of `fdtables` library, different virtual fds created by `dup`/`dup2` are
/// actually refer to the same underlying kernel fd. Therefore, in `fcntl_syscall` we need to
/// handle the cases of `F_DUPFD`, `F_DUPFD_CLOEXEC`, `F_GETFD`, and `F_SETFD` separately.
///
/// Among these, `F_DUPFD` and `F_DUPFD_CLOEXEC` cannot directly use the `dup_syscall` because,
/// in `fcntl`, the duplicated fd is assigned to the lowest available number starting from `arg`,
/// whereas the `dup_syscall` does not have this restriction and instead assigns the lowest
/// available fd number globally.
///
/// Additionally, `F_DUPFD_CLOEXEC` and `F_SETFD` require updating the fd flag information
/// (`O_CLOEXEC`) in fdtables after modifying the underlying kernel fd.
///
/// For all other command operations, after translating the virtual fd to the corresponding
/// kernel fd, they are redirected to the kernel `fcntl` syscall.
///
/// ## Arguments
/// virtual_fd: virtual file descriptor
/// cmd: The operation
/// arg: an optional third argument.  Whether or not this argument is required is determined by op.  
///
/// ## Return Type
/// The return value is related to the operation determined by `cmd` argument.
///
/// For a successful call, the return value depends on the operation:
/// `F_DUPFD`: The new file descriptor.
/// `F_GETFD`: Value of file descriptor flags.
/// `F_GETFL`: Value of file status flags.
/// `F_GETLEASE`: Type of lease held on file descriptor.
/// `F_GETOWN`: Value of file descriptor owner.
/// `F_GETSIG`: Value of signal sent when read or write becomes possible, or zero for traditional SIGIO behavior.
/// `F_GETPIPE_SZ`, `F_SETPIPE_SZ`: The pipe capacity.
/// `F_GET_SEALS`: A bit mask identifying the seals that have been set for the inode referred to by fd.
/// All other commands: Zero.
/// On error, -1 is returned
///
/// TODO: `F_GETOWN`, `F_SETOWN`, `F_GETOWN_EX`, `F_SETOWN_EX`, `F_GETSIG`, and `F_SETSIG` are used to manage I/O availability signals.
pub fn fcntl_syscall(
    cageid: u64,
    virtual_fd: u64,
    vfd_cageid: u64,
    cmd_arg: u64,
    cmd_cageid: u64,
    arg_arg: u64,
    arg_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let cmd = sc_convert_sysarg_to_i32(cmd_arg, cmd_cageid, cageid);
    let arg = sc_convert_sysarg_to_i32(arg_arg, arg_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "fcntl_syscall", "Invalide Cage ID");
    }

    match (cmd, arg) {
        // Duplicate the file descriptor `virtual_fd` using the lowest-numbered
        // available file descriptor greater than or equal to `arg`. The operation here
        // is quite similar to `dup_syscall`, for specific operation explanation, see
        // comments on `dup_syscall`.
        (F_DUPFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Get lowest-numbered available file descriptor greater than or equal to `arg`
            match fdtables::get_unused_virtual_fd_from_startfd(
                cageid,
                vfd.fdkind,
                vfd.underfd,
                false,
                0,
                arg as u64,
            ) {
                Ok(new_vfd) => return new_vfd as i32,
                Err(_) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // As for `F_DUPFD`, but additionally set the close-on-exec flag
        // for the duplicate file descriptor.
        (F_DUPFD_CLOEXEC, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Get lowest-numbered available file descriptor greater than or equal to `arg`
            // and set the `O_CLOEXEC` flag
            match fdtables::get_unused_virtual_fd_from_startfd(
                cageid,
                vfd.fdkind,
                vfd.underfd,
                true,
                0,
                arg as u64,
            ) {
                Ok(new_vfd) => return new_vfd as i32,
                Err(_) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // Return (as the function result) the file descriptor flags.
        (F_GETFD, ..) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            return vfd.should_cloexec as i32;
        }
        // Set the file descriptor flags to the value specified by arg.
        (F_SETFD, arg) => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            // Set underlying kernel fd flag
            let ret = unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) };
            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "fcntl");
            }
            // Set virtual fd flag
            let cloexec_flag: bool = arg != 0;
            match fdtables::set_cloexec(cageid, virtual_fd as u64, cloexec_flag) {
                Ok(_) => return 0,
                Err(_e) => return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor"),
            }
        }
        // Return (as the function result) the process ID or process
        // group ID currently receiving SIGIO and SIGURG signals for
        // events on file descriptor fd.
        (F_GETOWN, ..) => DEFAULT_GID as i32,
        // Set the process ID or process group ID that will receive
        // SIGIO and SIGURG signals for events on the file descriptor
        // fd.
        (F_SETOWN, arg) if arg >= 0 => 0,
        _ => {
            // Get fdtable entry
            let vfd = match _fcntl_helper(cageid, virtual_fd) {
                Ok(entry) => entry,
                Err(e) => return syscall_error(e, "fcntl", "Bad File Descriptor"),
            };
            let ret = unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) };
            if ret < 0 {
                let errno = get_errno();
                return handle_errno(errno, "fcntl");
            }
            ret
        }
    }
}

pub fn clock_gettime_syscall(
    cageid: u64,
    clockid_arg: u64,
    clockid_cageid: u64,
    tp_arg: u64,
    tp_cageid: u64,
    arg3: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    let clockid = sc_convert_sysarg_to_u32(clockid_arg, clockid_cageid, cageid);
    // let tp = sc_convert_sysarg_to_usize(tp_arg, tp_cageid, cageid);
    let tp = sc_convert_addr_to_host(tp_arg, tp_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg3, arg3_cageid)
        && sc_unusedarg(arg4, arg4_cageid)
        && sc_unusedarg(arg5, arg5_cageid)
        && sc_unusedarg(arg6, arg6_cageid))
    {
        return syscall_error(Errno::EFAULT, "clock_gettime", "Invalide Cage ID");
    }

    let ret = unsafe { syscall(SYS_clock_gettime, clockid, tp) as i32 };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "clock_gettime");
    }

    ret
}

/// Reference to Linux: https://man7.org/linux/man-pages/man2/futex.2.html
///
/// The Linux `futex()` syscall provides a mechanism for fast user-space locking. It allows a process or thread
/// to wait for or wake another process or thread on a shared memory location without invoking heavy kernel-side
/// synchronization primitives unless contention arises. This implementation wraps the futex syscall, allowing
/// direct invocation with the relevant arguments passed from the current cage context.
///
/// Input:
///     - cageid: current cageid
///     - uaddr_arg: pointer to the futex word in user memory
///     - futex_op_arg: operation code indicating futex command type
///     - val_arg: value expected at uaddr or the number of threads to wake
///     - val2_arg: timeout or other auxiliary parameter depending on operation
///     - uaddr2_arg: second address used for requeueing operations
///     - val3_arg: additional value for some futex operations
///
/// Return:
///     - On success: 0 or number of woken threads depending on futex operation
///     - On failure: a negative errno value indicating the syscall error
pub fn futex_syscall(
    cageid: u64,
    uaddr_arg: u64,
    uaddr_cageid: u64,
    futex_op_arg: u64,
    futex_op_cageid: u64,
    val_arg: u64,
    val_cageid: u64,
    val2_arg: u64,
    val2_cageid: u64,
    uaddr2_arg: u64,
    uaddr2_cageid: u64,
    val3_arg: u64,
    val3_cageid: u64,
) -> i32{
    let uaddr = sc_convert_uaddr_to_host(uaddr_arg, uaddr_cageid, cageid);
    let futex_op = sc_convert_sysarg_to_u32(futex_op_arg, futex_op_cageid, cageid);
    let val = sc_convert_sysarg_to_u32(val_arg, val_cageid, cageid);
    let val2 = sc_convert_sysarg_to_u32(val2_arg, val2_cageid, cageid);
    let uaddr2 = sc_convert_sysarg_to_u32(uaddr2_arg, uaddr2_cageid, cageid);
    let val3 = sc_convert_sysarg_to_u32(val3_arg, val3_cageid, cageid);

    let ret = unsafe { syscall(SYS_futex, uaddr, futex_op, val, val2, uaddr2, val3)  as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "futex");
    }
    ret
}

pub fn nanosleep_time64_syscall(
    cageid: u64,
    clockid_arg: u64,
    clockid_cageid: u64,
    flags_arg: u64,
    flags_cageid: u64,
    req_arg: u64,
    req_cageid: u64,
    rem_arg: u64,
    rem_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i32 {
    // Type conversion
    let clockid = sc_convert_sysarg_to_u32(clockid_arg, clockid_cageid, cageid);
    let flags = sc_convert_sysarg_to_i32(flags_arg, flags_cageid, cageid);
    let req = sc_convert_buf(req_arg, req_cageid, cageid);
    let rem = sc_convert_buf(rem_arg, rem_cageid, cageid);
    // would sometimes check, sometimes be a no-op depending on the compiler settings
    if !(sc_unusedarg(arg5, arg5_cageid) && sc_unusedarg(arg6, arg6_cageid)) {
        return syscall_error(Errno::EFAULT, "nanosleep", "Invalide Cage ID");
    }
    let ret = unsafe { syscall(SYS_clock_nanosleep, clockid, flags, req, rem) as i32 };
    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "nanosleep");
    }
    ret
}
