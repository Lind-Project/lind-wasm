//! Shared-memory preload and dump regions.
//!
//! An embedder can hand lind two regions of memory before the first cage
//! starts:
//!
//! - a *preload* region: a bundle of IMFS preload archives that was packed on
//!   the host ahead of time (`mkpreload` in lind-wasm-example-grates);
//! - a *dump* region: where the archive of a run's output files is written,
//!   followed by one call that commits it.
//!
//! A grate reaches them with three lind syscalls: `LIND_SHM_INFO` (the sizes
//! of both regions), `LIND_SHM_READ` (copy part of the preload region into
//! the grate's memory) and `LIND_SHM_DUMP` (copy a buffer to the dump region
//! and commit it). The copies are plain memory copies made here, so when lind
//! runs in an SGX enclave and the regions are untrusted memory outside it,
//! staging a preload does not leave the enclave and a dump leaves it once, in
//! the commit.
//!
//! lind-boot maps files given with `--preload-shm` / `--dump-shm` (see
//! [`map_preload_file`] and [`map_dump_file`]). An embedder that maps memory
//! some other way, such as TriSeal asking the untrusted host, registers it
//! with [`register_preload`] and [`register_dump`].

use cage::memory::{check_addr_read, check_addr_rw};
use parking_lot::Mutex;
use std::ffi::CString;
use std::io;
use sysdefs::constants::err_const::{syscall_error, Errno};
use typemap::datatype_conversion::{sc_convert_buf, sc_convert_sysarg_to_usize};

/// Commits a dump of the given length, returning a negative errno on failure.
pub type DumpCommit = Box<dyn Fn(usize) -> Result<(), i32> + Send + Sync>;

struct Preload {
    addr: usize,
    len: usize,
}

struct Dump {
    addr: usize,
    capacity: usize,
    commit: DumpCommit,
}

static PRELOAD: Mutex<Option<Preload>> = Mutex::new(None);
static DUMP: Mutex<Option<Dump>> = Mutex::new(None);

/// Make `len` bytes at `addr` the preload region.
///
/// # Safety
///
/// The memory must stay mapped and readable for the rest of the process. Its
/// contents are not trusted: grates copy it before checking it.
pub unsafe fn register_preload(addr: *const u8, len: usize) {
    *PRELOAD.lock() = Some(Preload {
        addr: addr as usize,
        len,
    });
}

/// Make `capacity` bytes at `addr` the dump region; `commit(len)` is called
/// after a dump of `len` bytes has been written to it.
///
/// # Safety
///
/// The memory must stay mapped and writable for the rest of the process.
pub unsafe fn register_dump(addr: *mut u8, capacity: usize, commit: DumpCommit) {
    *DUMP.lock() = Some(Dump {
        addr: addr as usize,
        capacity,
        commit,
    });
}

fn open_file(path: &str, flags: i32) -> io::Result<i32> {
    let c_path = CString::new(path).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let fd = unsafe { libc::open(c_path.as_ptr(), flags | libc::O_CLOEXEC, 0o644) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(fd)
}

/// Map the file at `path` read-only and make it the preload region.
pub fn map_preload_file(path: &str) -> io::Result<()> {
    let fd = open_file(path, libc::O_RDONLY)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstat(fd, &mut st) } < 0 {
        let e = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(e);
    }
    let len = st.st_size as usize;
    if len == 0 {
        unsafe { libc::close(fd) };
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "empty preload file",
        ));
    }
    let addr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            fd,
            0,
        )
    };
    unsafe { libc::close(fd) };
    if addr == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    unsafe { register_preload(addr as *const u8, len) };
    Ok(())
}

/// Create (or truncate) the file at `path`, map `capacity` bytes of it as
/// the dump region, and commit a dump by truncating the file to its length.
pub fn map_dump_file(path: &str, capacity: usize) -> io::Result<()> {
    let fd = open_file(path, libc::O_RDWR | libc::O_CREAT | libc::O_TRUNC)?;
    if unsafe { libc::ftruncate(fd, capacity as libc::off_t) } < 0 {
        let e = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(e);
    }
    let addr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            capacity,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED,
            fd,
            0,
        )
    };
    if addr == libc::MAP_FAILED {
        let e = io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(e);
    }
    let commit: DumpCommit = Box::new(move |len| {
        if unsafe { libc::ftruncate(fd, len as libc::off_t) } < 0 || unsafe { libc::fsync(fd) } < 0
        {
            return Err(-(Errno::EIO as i32));
        }
        Ok(())
    });
    unsafe { register_dump(addr as *mut u8, capacity, commit) };
    Ok(())
}

/// `LIND_SHM_INFO(out)`: write the preload region's length and the dump
/// region's capacity (0 for a region that is not registered) to `out`, two
/// u64s in the calling cage.
pub extern "C" fn shm_info_syscall(
    cageid: u64,
    out_arg: u64,
    out_cageid: u64,
    _arg2: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let out = sc_convert_buf(out_arg, out_cageid, cageid) as *mut u64;
    if out.is_null() || check_addr_rw(cageid, out as u64, 16).is_err() {
        return syscall_error(Errno::EFAULT, "shm_info", "bad output buffer");
    }
    let preload = PRELOAD.lock().as_ref().map_or(0, |r| r.len) as u64;
    let dump = DUMP.lock().as_ref().map_or(0, |r| r.capacity) as u64;
    unsafe {
        out.write_unaligned(preload);
        out.add(1).write_unaligned(dump);
    }
    0
}

/// `LIND_SHM_READ(offset, dst, len)`: copy `len` bytes of the preload region
/// starting at `offset` into the calling cage's buffer `dst`.
pub extern "C" fn shm_read_syscall(
    cageid: u64,
    offset_arg: u64,
    offset_cageid: u64,
    dst_arg: u64,
    dst_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let offset = sc_convert_sysarg_to_usize(offset_arg, offset_cageid, cageid);
    let dst = sc_convert_buf(dst_arg, dst_cageid, cageid) as *mut u8;
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);

    let preload = PRELOAD.lock();
    let region = match preload.as_ref() {
        Some(region) => region,
        None => return syscall_error(Errno::ENODEV, "shm_read", "no preload region"),
    };
    match offset.checked_add(len) {
        Some(end) if end <= region.len => {}
        _ => return syscall_error(Errno::EINVAL, "shm_read", "read past the preload region"),
    }
    if dst.is_null() || check_addr_rw(cageid, dst as u64, len).is_err() {
        return syscall_error(Errno::EFAULT, "shm_read", "bad destination buffer");
    }
    unsafe { std::ptr::copy_nonoverlapping((region.addr + offset) as *const u8, dst, len) };
    0
}

/// `LIND_SHM_DUMP(src, len)`: copy `len` bytes of the calling cage's buffer
/// `src` to the start of the dump region and commit them.
pub extern "C" fn shm_dump_syscall(
    cageid: u64,
    src_arg: u64,
    src_cageid: u64,
    len_arg: u64,
    len_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let src = sc_convert_buf(src_arg, src_cageid, cageid);
    let len = sc_convert_sysarg_to_usize(len_arg, len_cageid, cageid);

    let dump = DUMP.lock();
    let region = match dump.as_ref() {
        Some(region) => region,
        None => return syscall_error(Errno::ENODEV, "shm_dump", "no dump region"),
    };
    if len > region.capacity {
        return syscall_error(
            Errno::ENOSPC,
            "shm_dump",
            "dump larger than the dump region",
        );
    }
    if src.is_null() || check_addr_read(cageid, src as u64, len).is_err() {
        return syscall_error(Errno::EFAULT, "shm_dump", "bad source buffer");
    }
    unsafe { std::ptr::copy_nonoverlapping(src, region.addr as *mut u8, len) };
    match (region.commit)(len) {
        Ok(()) => 0,
        Err(e) => e,
    }
}
