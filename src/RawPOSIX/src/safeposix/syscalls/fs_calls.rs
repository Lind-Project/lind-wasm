#![allow(dead_code)]

use std::fs;

// Add constants imports
use crate::constants::{
    S_IRWXU, S_IRWXG, S_IRWXO,
    PROT_READ, PROT_WRITE,
    O_RDONLY, O_WRONLY, O_RDWR, O_CREAT, O_TRUNC, O_CLOEXEC,
    MAP_SHARED, MAP_PRIVATE,
    SEEK_SET, SEEK_CUR, SEEK_END,
    SHMMIN, SHMMAX, SHM_RDONLY, SHM_DEST,
    DEFAULT_UID, DEFAULT_GID,
    SEM_VALUE_MAX,
};

use crate::interface;
use crate::interface::get_errno;
use crate::interface::handle_errno;
use crate::interface::FSData;
use crate::safeposix::cage::Errno::EINVAL;
use crate::safeposix::cage::*;
use crate::safeposix::filesystem::convpath;
use crate::safeposix::filesystem::normpath;
use crate::safeposix::shm::*;
use crate::interface::ShmidsStruct;
use crate::interface::StatData;

use libc::*;
use std::io::stdout;
use std::os::unix::io::RawFd;
use std::io::{self, Write};
use std::ffi::CStr;
use std::ffi::CString;
use std::ptr;
use std::mem;

use crate::fdtables;
use crate::safeposix::cage::Cage;

static LIND_ROOT: &str = "/home/lind-wasm/src/RawPOSIX/tmp";

const FDKIND_KERNEL: u32 = 0;
const FDKIND_IMPIPE: u32 = 1;

impl Cage {
    //------------------------------------OPEN SYSCALL------------------------------------
    /* 
    *   Open will return a file descriptor 
    *   Mapping a new virtual fd and kernel fd that libc::socket returned
    *   Then return virtual fd
    */
    pub fn open_syscall(&self, path: &str, oflag: i32, mode: u32) -> i32 {

        // Convert data type from &str into *const i8
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        let kernel_fd = unsafe { libc::open(c_path.as_ptr(), oflag, mode) };
        
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "open");
        }

        let should_cloexec = (oflag & O_CLOEXEC) != 0;

        match fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, kernel_fd as u64, should_cloexec, 0) {
            Ok(virtual_fd) => return virtual_fd as i32,
            Err(_) => return syscall_error(Errno::EMFILE, "open", "Too many files opened")
        }
    }

    //------------------MKDIR SYSCALL------------------
    /*
    *   mkdir() will return 0 when success and -1 when fail 
    */
    pub fn mkdir_syscall(&self, path: &str, mode: u32) -> i32 {
        // Convert data type from &str into *const i8
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();
        
        let ret = unsafe {
            libc::mkdir(c_path.as_ptr(), mode)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "mkdir");
        }
        ret
    }

    //------------------MKNOD SYSCALL------------------
    /*
    *   mknod() will return 0 when success and -1 when fail 
    */
    pub fn mknod_syscall(&self, path: &str, mode: u32, dev: u64) -> i32 {
        // Convert data type from &str into *const i8
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();
        let ret = unsafe {
            libc::mknod(c_path.as_ptr(), mode, dev)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "mknod");
        }
        ret
    }

    //------------------------------------LINK SYSCALL------------------------------------
    /*
    *   link() will return 0 when success and -1 when fail 
    */
    pub fn link_syscall(&self, oldpath: &str, newpath: &str) -> i32 {
        // Convert data type from &str into *const i8
        let rel_oldpath = normpath(convpath(oldpath), self);
        let relative_oldpath = rel_oldpath.to_str().unwrap();
        let full_oldpath = format!("{}{}", LIND_ROOT, relative_oldpath);
        let old_cpath = CString::new(full_oldpath).unwrap();

        let rel_newpath = normpath(convpath(newpath), self);
        let relative_newpath = rel_newpath.to_str().unwrap();
        let full_newpath = format!("{}{}", LIND_ROOT, relative_newpath);
        let new_cpath = CString::new(full_newpath).unwrap();

        let ret = unsafe {
            libc::link(old_cpath.as_ptr(), new_cpath.as_ptr())
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "link");
        }
        ret
    }

    //------------------------------------UNLINK SYSCALL------------------------------------
    /*
    *   unlink() will return 0 when success and -1 when fail 
    */
    pub fn unlink_syscall(&self, path: &str) -> i32 {
        // let (path_c, _, _) = path.to_string().into_raw_parts();
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        let ret = unsafe {
            libc::unlink(c_path.as_ptr())
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "unlink");
        }
        ret
    }

    //------------------------------------CREAT SYSCALL------------------------------------
    /*
    *   creat() will return fd when success and -1 when fail 
    */
    pub fn creat_syscall(&self, path: &str, mode: u32) -> i32 {
        // let c_path = CString::new(path).expect("CString::new failed");
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        let kernel_fd = unsafe {
            libc::creat(c_path.as_ptr(), mode)
        };
        if kernel_fd < 0 {
            let errno = get_errno();
            return handle_errno(errno, "creat");
        }
        
        let virtual_fd = fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, kernel_fd as u64, false, 0).unwrap();
        virtual_fd as i32
    }

    //------------------------------------STAT SYSCALL------------------------------------
    /*
    *   stat() will return 0 when success and -1 when fail 
    */
    pub fn stat_syscall(&self, path: &str, rposix_statbuf: &mut StatData) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        // Declare statbuf by ourselves 
        let mut libc_statbuf: stat = unsafe { std::mem::zeroed() };
        let libcret = unsafe {
            libc::stat(c_path.as_ptr(), &mut libc_statbuf)
        };
        
        if libcret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "stat");
        }
        
        rposix_statbuf.st_blksize = libc_statbuf.st_blksize as i32;
        rposix_statbuf.st_blocks = libc_statbuf.st_blocks as u32;
        rposix_statbuf.st_dev = libc_statbuf.st_dev as u64;
        rposix_statbuf.st_gid = libc_statbuf.st_gid;
        rposix_statbuf.st_ino = libc_statbuf.st_ino as usize;
        rposix_statbuf.st_mode = libc_statbuf.st_mode as u32;
        rposix_statbuf.st_nlink = libc_statbuf.st_nlink as u32;
        rposix_statbuf.st_rdev = libc_statbuf.st_rdev as u64;
        rposix_statbuf.st_size = libc_statbuf.st_size as usize;
        rposix_statbuf.st_uid = libc_statbuf.st_uid;

        libcret
    }

    //------------------------------------FSTAT SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fstat() will return 0 when success and -1 when fail 
    */
    pub fn fstat_syscall(&self, virtual_fd: i32, rposix_statbuf: &mut StatData) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fstat", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        // Declare statbuf by ourselves 
        let mut libc_statbuf: stat = unsafe { std::mem::zeroed() };
        let libcret = unsafe {
        libc::fstat(vfd.underfd as i32, &mut libc_statbuf)
        };

        if libcret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fstat");
        }
        
        rposix_statbuf.st_blksize = libc_statbuf.st_blksize as i32;
        rposix_statbuf.st_blocks = libc_statbuf.st_blocks as u32;
        rposix_statbuf.st_dev = libc_statbuf.st_dev as u64;
        rposix_statbuf.st_gid = libc_statbuf.st_gid;
        rposix_statbuf.st_ino = libc_statbuf.st_ino as usize;
        rposix_statbuf.st_mode = libc_statbuf.st_mode as u32;
        rposix_statbuf.st_nlink = libc_statbuf.st_nlink as u32;
        rposix_statbuf.st_rdev = libc_statbuf.st_rdev as u64;
        rposix_statbuf.st_size = libc_statbuf.st_size as usize;
        rposix_statbuf.st_uid = libc_statbuf.st_uid;

        libcret
        
    }

    //------------------------------------STATFS SYSCALL------------------------------------
    /*
    *   statfs() will return 0 when success and -1 when fail 
    */
    pub fn statfs_syscall(&self, path: &str, rposix_databuf: &mut FSData) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        let mut libc_databuf: statfs = unsafe { mem::zeroed() };
        let libcret = unsafe {
            libc::statfs(c_path.as_ptr(), &mut libc_databuf)
        };

        if libcret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "statfs");
        }

        rposix_databuf.f_bavail = libc_databuf.f_bavail;
        rposix_databuf.f_bfree = libc_databuf.f_bfree;
        rposix_databuf.f_blocks = libc_databuf.f_blocks;
        rposix_databuf.f_bsize = libc_databuf.f_bsize as u64;
        rposix_databuf.f_files = libc_databuf.f_files;
        /* TODO: different from libc struct */
        rposix_databuf.f_fsid = 0; 
        rposix_databuf.f_type = libc_databuf.f_type as u64;
        rposix_databuf.f_ffiles = 1024 * 1024 * 515;
        rposix_databuf.f_namelen = 254;
        rposix_databuf.f_frsize = 4096;
        rposix_databuf.f_spare = [0; 32];

        libcret
    }

    //------------------------------------FSTATFS SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fstatfs() will return 0 when success and -1 when fail 
    */
    pub fn fstatfs_syscall(&self, virtual_fd: i32, rposix_databuf: &mut FSData) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fstatfs", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let mut libc_databuf: statfs = unsafe { mem::zeroed() };
        let libcret = unsafe {
            libc::fstatfs(vfd.underfd as i32, &mut libc_databuf)
        };

        if libcret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fstatfs");
        }

        rposix_databuf.f_bavail = libc_databuf.f_bavail;
        rposix_databuf.f_bfree = libc_databuf.f_bfree;
        rposix_databuf.f_blocks = libc_databuf.f_blocks;
        rposix_databuf.f_bsize = libc_databuf.f_bsize as u64;
        rposix_databuf.f_files = libc_databuf.f_files;
        /* TODO: different from libc struct */
        rposix_databuf.f_fsid = 0; 
        rposix_databuf.f_type = libc_databuf.f_type as u64;
        rposix_databuf.f_ffiles = 1024 * 1024 * 515;
        rposix_databuf.f_namelen = 254;
        rposix_databuf.f_frsize = 4096;
        rposix_databuf.f_spare = [0; 32];

        return libcret;
    }

    //------------------------------------READ SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   read() will return:
    *   - the number of bytes read is returned, success
    *   - -1, fail 
    */
    pub fn read_syscall(&self, virtual_fd: i32, readbuf: *mut u8, count: usize) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "read", "Bad File Descriptor");
        }

        let vfd = wrappedvfd.unwrap();
        //kernel fd
        let ret = unsafe {
            libc::read(vfd.underfd as i32, readbuf as *mut c_void, count) as i32
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "read");
        }
        return ret;
    }

    //------------------------------------PREAD SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   pread() will return:
    *   - the number of bytes read is returned, success
    *   - -1, fail 
    */
    pub fn pread_syscall(&self, virtual_fd: i32, buf: *mut u8, count: usize, offset: i64) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "pread", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::pread(vfd.underfd as i32, buf as *mut c_void, count, offset) as i32
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "pread");
        }
        return ret;
        
    }

    //------------------------------------WRITE SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   write() will return:
    *   - the number of bytes writen is returned, success
    *   - -1, fail 
    */
    pub fn write_syscall(&self, virtual_fd: i32, buf: *const u8, count: usize) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "write", "Bad File Descriptor");
        }

        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::write(vfd.underfd as i32, buf as *const c_void, count) as i32
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "write");
        }
        return ret;
    }

    //------------------------------------PWRITE SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   pwrite() will return:
    *   - the number of bytes read is returned, success
    *   - -1, fail 
    */
    pub fn pwrite_syscall(&self, virtual_fd: i32, buf: *const u8, count: usize, offset: i64) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "pwrite", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::pwrite(vfd.underfd as i32, buf as *const c_void, count, offset) as i32
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "pwrite");
        }
        return ret;
        
    }

    //------------------------------------WRITEV SYSCALL------------------------------------

    pub fn writev_syscall(
        &self,
        virtual_fd: i32,
        iovec: *const interface::IovecStruct,
        iovcnt: i32,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "write", "Bad File Descriptor");
        }

        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::writev(vfd.underfd as i32, iovec, iovcnt)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "writev");
        }
        return ret as i32;
    }

    //------------------------------------LSEEK SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   lseek() will return:
    *   -  the resulting offset location as measured in bytes from the beginning of the file
    *   - -1, fail 
    */
    pub fn lseek_syscall(&self, virtual_fd: i32, offset: isize, whence: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "lseek", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::lseek(vfd.underfd as i32, offset as i64, whence) as i32
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "lseek");
        }
        return ret;
        
    }

    //------------------------------------ACCESS SYSCALL------------------------------------
    /*
    *   access() will return 0 when sucess, -1 when fail 
    */
    pub fn access_syscall(&self, path: &str, amode: i32) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();
        let ret = unsafe {
            libc::access(c_path.as_ptr(), amode)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "access");
        }
        ret
    }

    //------------------------------------FCHDIR SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fchdir() will return 0 when sucess, -1 when fail 
    */
    pub fn fchdir_syscall(&self, virtual_fd: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fchdir", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::fchdir(vfd.underfd as i32)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fchdir");
        }

        // Get the working directory in native
        let mut buf = [0; PATH_MAX as usize];
        let cwdret = unsafe {
            libc::getcwd(buf.as_mut_ptr(), buf.len())
        };
        if cwdret == ptr::null_mut() {
            let errno = get_errno();
            return handle_errno(errno, "fchdir");
        }
        let cwdcstr = unsafe { CStr::from_ptr(buf.as_ptr() as *const i8) };
        let cwd = cwdcstr.to_str().unwrap();
        // Update RawPOSIX working directory
        let raw_path = &cwd[LIND_ROOT.len()..];
        let true_path = normpath(convpath(raw_path), self);
        let mut cwd_container = self.cwd.write();
        *cwd_container = interface::RustRfc::new(true_path);

        return ret;
        
    }

    //------------------------------------CHDIR SYSCALL------------------------------------
    /*
    *   We first check if desired path exists in native
    *   chdir() will return 0 when sucess, -1 when fail 
    */
    pub fn chdir_syscall(&self, path: &str) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();

        let ret = unsafe {
            libc::chdir(c_path.as_ptr())
        };

        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "chdir");
        }
        
        let truepath = normpath(convpath(path), self);
        
        //at this point, syscall isn't an error
        let mut cwd_container = self.cwd.write();

        *cwd_container = interface::RustRfc::new(truepath);
        0 
    }

    //------------------------------------DUP & DUP2 SYSCALLS------------------------------------
    /* 
    *   dup() / dup2() will return a file descriptor 
    *   Mapping a new virtual fd and kernel fd that libc::dup returned
    *   Then return virtual fd
    */
    pub fn dup_syscall(&self, virtual_fd: i32, _start_desc: Option<i32>) -> i32 {
        if virtual_fd < 0 {
            return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
        }
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret_kernelfd = unsafe{ libc::dup(vfd.underfd as i32) };
        let ret_virtualfd = fdtables::get_unused_virtual_fd(self.cageid, vfd.fdkind, ret_kernelfd as u64, false, 0).unwrap();
        return ret_virtualfd as i32;
        
    }

    /* 
    */
    pub fn dup2_syscall(&self, old_virtualfd: i32, new_virtualfd: i32) -> i32 {
        if old_virtualfd < 0 || new_virtualfd < 0 {
            return syscall_error(Errno::EBADF, "dup", "Bad File Descriptor");
        }

        match fdtables::translate_virtual_fd(self.cageid, old_virtualfd as u64) {
            Ok(old_vfd) => {
                let new_kernelfd = unsafe {
                    libc::dup(old_vfd.underfd as i32)
                };
                // Map new kernel fd with provided kernel fd
                let _ret_kernelfd = unsafe{ libc::dup2(old_vfd.underfd as i32, new_kernelfd) };
                let _ = fdtables::get_specific_virtual_fd(self.cageid, new_virtualfd as u64, old_vfd.fdkind, new_kernelfd as u64, false, old_vfd.perfdinfo).unwrap();
                return new_virtualfd;
            },
            Err(_e) => {
                return syscall_error(Errno::EBADF, "dup2", "Bad File Descriptor");
            }
        }
        
    }

    //------------------------------------CLOSE SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   close() will return 0 when sucess, -1 when fail 
    */
    pub fn close_syscall(&self, virtual_fd: i32) -> i32 {
        match fdtables::close_virtualfd(self.cageid, virtual_fd as u64) {
            Ok(()) => {
                return 0;
            }
            Err(e) => {
                if e == Errno::EBADFD as u64 {
                    return syscall_error(Errno::EBADF, "close", "bad file descriptor");
                } 
                return -1;
            }
        }
        
    }

    

    //------------------------------------FCNTL SYSCALL------------------------------------
    /*
    *   For a successful call, the return value depends on the operation:

       F_DUPFD
              The new file descriptor.

       F_GETFD
              Value of file descriptor flags.

       F_GETFL
              Value of file status flags.

       F_GETLEASE
              Type of lease held on file descriptor.

       F_GETOWN
              Value of file descriptor owner.

       F_GETSIG
              Value of signal sent when read or write becomes possible,
              or zero for traditional SIGIO behavior.

       F_GETPIPE_SZ, F_SETPIPE_SZ
              The pipe capacity.

       F_GET_SEALS
              A bit mask identifying the seals that have been set for
              the inode referred to by fd.

       All other commands
              Zero.

       On error, -1 is returned 
    */
    pub fn fcntl_syscall(&self, virtual_fd: i32, cmd: i32, arg: i32) -> i32 {
        match (cmd, arg) {
            (F_GETOWN, ..) => {
                // 
                1000
            }
            (F_SETOWN, arg) if arg >= 0 => {
                0
            }
            _ => {
                let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
                if wrappedvfd.is_err() {
                    return syscall_error(Errno::EBADF, "fcntl", "Bad File Descriptor");
                }
                let vfd = wrappedvfd.unwrap();
                if cmd == libc::F_DUPFD {
                    match arg {
                        n if n < 0 => return syscall_error(Errno::EINVAL, "fcntl", "op is F_DUPFD and arg is negative or is greater than the maximum allowable value"),
                        0..=1024 => return self.dup2_syscall(virtual_fd, arg),
                        _ => return syscall_error(Errno::EMFILE, "fcntl", "op is F_DUPFD and the per-process limit on the number of open file descriptors has been reached")
                    }
                }
                let ret = unsafe { libc::fcntl(vfd.underfd as i32, cmd, arg) };
                if ret < 0 {
                    let errno = get_errno();
                    return handle_errno(errno, "fcntl");
                }
                ret
            }
        }
        
    }

    //------------------------------------IOCTL SYSCALL------------------------------------
    /*
    *   The third argument is an untyped pointer to memory.  It's traditionally char *argp 
    *   (from the days before void * was valid C), and will be so named for this discussion.
    *   ioctl() will return 0 when success and -1 when fail. 
    *   Note: A few ioctl() requests use the return value as an output parameter and return 
    *   a nonnegative value on success.
    */
    pub fn ioctl_syscall(&self, virtual_fd: i32, request: u64, ptrunion: *mut u8) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "ioctl", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe { libc::ioctl(vfd.underfd as i32, request, ptrunion as *mut c_void) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "ioctl");
        }
        return ret;
        
    }


    //------------------------------------CHMOD SYSCALL------------------------------------
    /*
    *   chmod() will return 0 when success and -1 when fail 
    */
    pub fn chmod_syscall(&self, path: &str, mode: u32) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();
        let ret = unsafe {
            libc::chmod(c_path.as_ptr(), mode)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "chmod");
        }
        ret
    }

    //------------------------------------FCHMOD SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fchmod() will return 0 when sucess, -1 when fail 
    */
    pub fn fchmod_syscall(&self, virtual_fd: i32, mode: u32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fchmod", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::fchmod(vfd.underfd as i32, mode)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fchmod");
        }
        return ret;
        
    }

    //------------------------------------MMAP SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   mmap() will return:
    *   - a pointer to the mapped area, success
    *   - -1, fail
    */
    pub fn mmap_syscall(
        &self,
        addr: *mut u8,
        len: usize,
        prot: i32,
        flags: i32,
        virtual_fd: i32,
        off: i64
    ) -> usize {
        if virtual_fd != -1 {
            match fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64) {
                Ok(kernel_fd) => {
                    let ret = unsafe {
                        (libc::mmap(addr as *mut c_void, len, prot, flags, kernel_fd.underfd as i32, off) as i64)
                    };

                    // Check if mmap failed and return the appropriate error if so
                    if ret == -1 {
                        return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags") as usize;
                    }

                    ret as usize
                },
                Err(_e) => {
                    return syscall_error(Errno::EBADF, "mmap", "Bad File Descriptor") as usize;
                }
            }
        } else {
            // Handle mmap with fd = -1 (anonymous memory mapping or special case)
            let ret = unsafe {
                libc::mmap(addr as *mut c_void, len, prot, flags, -1, off) as i64
            };
            // Check if mmap failed and return the appropriate error if so
            if ret == -1 {
                return syscall_error(Errno::EINVAL, "mmap", "mmap failed with invalid flags") as usize;
            }

            ret as usize
        }
    }

    //------------------------------------MUNMAP SYSCALL------------------------------------
    /*
    *   munmap() will return:
    *   - 0, success
    *   - -1, fail
    */
    pub fn munmap_syscall(&self, addr: *mut u8, len: usize) -> i32 {
        let ret = unsafe {
            libc::munmap(addr as *mut c_void, len)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "munmap");
        }
        ret
    }

    //------------------------------------FLOCK SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   flock() will return 0 when sucess, -1 when fail 
    */
    pub fn flock_syscall(&self, virtual_fd: i32, operation: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "flock", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::flock(vfd.underfd as i32, operation)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "flock");
        }
        return ret;
        
    }

    //------------------RMDIR SYSCALL------------------
    /*
    *   rmdir() will return 0 when sucess, -1 when fail 
    */
    pub fn rmdir_syscall(&self, path: &str) -> i32 {
        match path {
            "" => return syscall_error(Errno::ENOENT, "rmdir", "A directory component in pathname does not exist"),
            "/" => return syscall_error(Errno::EBUSY, "rmdir", "pathname is currently in use by the system or some process that prevents its removal"),
            _ => {
                let relpath = normpath(convpath(path), self);
                let relative_path = relpath.to_str().unwrap();
                let full_path = format!("{}{}", LIND_ROOT, relative_path);
                let c_path = CString::new(full_path).unwrap();
                let ret = unsafe {
                    libc::rmdir(c_path.as_ptr())
                };
                if ret < 0 {
                    let errno = get_errno();
                    return handle_errno(errno, "rmdir");
                }
                return ret;
            }
        }
        
    }

    //------------------RENAME SYSCALL------------------
    /*
    *   rename() will return 0 when sucess, -1 when fail 
    */
    pub fn rename_syscall(&self, oldpath: &str, newpath: &str) -> i32 {
        let rel_oldpath = normpath(convpath(oldpath), self);
        let relative_oldpath = rel_oldpath.to_str().unwrap();
        let full_oldpath = format!("{}{}", LIND_ROOT, relative_oldpath);
        let old_cpath = CString::new(full_oldpath).unwrap();

        let rel_newpath = normpath(convpath(newpath), self);
        let relative_newpath = rel_newpath.to_str().unwrap();
        let full_newpath = format!("{}{}", LIND_ROOT, relative_newpath);
        let new_cpath = CString::new(full_newpath).unwrap();

        let ret = unsafe {
            libc::rename(old_cpath.as_ptr(), new_cpath.as_ptr())
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "rename");
        }
        ret
    }

    //------------------------------------FSYNC SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fsync() will return 0 when sucess, -1 when fail 
    */
    pub fn fsync_syscall(&self, virtual_fd: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fsync", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::fsync(vfd.underfd as i32)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fsync");
        }
        return ret;
        
    }

    //------------------------------------FDATASYNC SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   fdatasync() will return 0 when sucess, -1 when fail 
    */
    pub fn fdatasync_syscall(&self, virtual_fd: i32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fdatasync", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::fdatasync(vfd.underfd as i32)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fdatasync");
        }
        return ret;
        
    }

    //------------------------------------SYNC_FILE_RANGE SYSCALL------------------------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   sync_file_range() will return 0 when sucess, -1 when fail 
    */
    pub fn sync_file_range_syscall(
        &self,
        virtual_fd: i32,
        offset: isize,
        nbytes: isize,
        flags: u32,
    ) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "sync", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::sync_file_range(vfd.underfd as i32, offset as i64, nbytes as i64, flags)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "sync_file_range");
        }
        ret
    }

    //------------------FTRUNCATE SYSCALL------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   ftruncate() will return 0 when sucess, -1 when fail 
    */
    pub fn ftruncate_syscall(&self, virtual_fd: i32, length: isize) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "ftruncate", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe {
            libc::ftruncate(vfd.underfd as i32, length as i64)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "ftruncate");
        }
        ret
    }

    //------------------TRUNCATE SYSCALL------------------
    /*
    *   truncate() will return 0 when sucess, -1 when fail 
    */
    pub fn truncate_syscall(&self, path: &str, length: isize) -> i32 {
        let relpath = normpath(convpath(path), self);
        let relative_path = relpath.to_str().unwrap();
        let full_path = format!("{}{}", LIND_ROOT, relative_path);
        let c_path = CString::new(full_path).unwrap();
        let ret = unsafe {
            libc::truncate(c_path.as_ptr(), length as i64)
        };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "truncate");
        }
        ret
    }

    //------------------PIPE SYSCALL------------------
    /*
    *   When using the rust libc, a pipe file descriptor (pipefd) is typically an array 
    *   containing two integers. This array is populated by the pipe system call, with one 
    *   integer for the read end and the other for the write end.
    *
    *   pipe() will return 0 when sucess, -1 when fail 
    */
    pub fn pipe_syscall(&self, pipefd: &mut PipeArray) -> i32 {
        self.pipe2_syscall(pipefd, 0)
    }

    pub fn pipe2_syscall(&self, pipefd: &mut PipeArray, flags: i32) -> i32 {
        let mut kernel_fds:[i32; 2] = [0; 2];
            
        let ret = unsafe { libc::pipe2(kernel_fds.as_mut_ptr() as *mut i32, flags as i32) };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "pipe2");
        }

        let should_cloexec = if flags & O_CLOEXEC != 0 {
            true
        } else { false };

        pipefd.readfd = fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, kernel_fds[0] as u64, should_cloexec, 0).unwrap() as i32;
        pipefd.writefd = fdtables::get_unused_virtual_fd(self.cageid, FDKIND_KERNEL, kernel_fds[1] as u64, should_cloexec, 0).unwrap() as i32;

        return ret;
    }

    //------------------GETDENTS SYSCALL------------------
    /*
    *   Get the kernel fd with provided virtual fd first
    *   getdents() will return:
    *   - the number of bytes read is returned, success
    *   - 0, EOF
    *   - -1, fail 
    */
    pub fn getdents_syscall(&self, virtual_fd: i32, buf: *mut u8, nbytes: u32) -> i32 {
        let wrappedvfd = fdtables::translate_virtual_fd(self.cageid, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "getdents", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();
        let ret = unsafe { libc::syscall(libc::SYS_getdents as c_long, vfd.underfd as i32, buf as *mut c_void, nbytes) as i32 };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "getdents");
        }
        ret
    }

    //------------------------------------GETCWD SYSCALL------------------------------------
    /*
    *   getcwd() will return:
    *   - a pointer to a string containing the pathname of the current working directory, success
    *   - null, fail 
    */
    pub fn getcwd_syscall(&self, buf: *mut u8, bufsize: u32) -> i32 {
        if (!buf.is_null() && bufsize == 0) || (buf.is_null() && bufsize != 0) {
            return syscall_error(Errno::EINVAL, "getcwd", "Invalid arguments");
        }
        
        let cwd_container = self.cwd.read();
        let path = cwd_container.to_str().unwrap();
        // The required size includes the null terminator
        let required_size = path.len() + 1;
        if required_size > bufsize as usize {
            unsafe { *libc::__errno_location() = libc::ERANGE };
            return -libc::ERANGE;
        }
        unsafe {
            ptr::copy(path.as_ptr(), buf, path.len());
            *buf.add(path.len()) = 0;
        }
        0
    }

    //------------------SHMHELPERS----------------------

    pub fn rev_shm_find_index_by_addr(rev_shm: &Vec<(u32, i32)>, shmaddr: u32) -> Option<usize> {
        for (index, val) in rev_shm.iter().enumerate() {
            if val.0 == shmaddr as u32 {
                return Some(index);
            }
        }
        None
    }

    pub fn rev_shm_find_addrs_by_shmid(rev_shm: &Vec<(u32, i32)>, shmid: i32) -> Vec<u32> {
        let mut addrvec = Vec::new();
        for val in rev_shm.iter() {
            if val.1 == shmid as i32 {
                addrvec.push(val.0);
            }
        }

        return addrvec;
    }

    pub fn search_for_addr_in_region(
        rev_shm: &Vec<(u32, i32)>,
        search_addr: u32,
    ) -> Option<(u32, i32)> {
        let metadata = &SHM_METADATA;
        for val in rev_shm.iter() {
            let addr = val.0;
            let shmid = val.1;
            if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                let range = addr..(addr + segment.size as u32);
                if range.contains(&search_addr) {
                    return Some((addr, shmid));
                }
            }
        }
        None
    }

    //------------------SHMGET SYSCALL------------------

    pub fn shmget_syscall(&self, key: i32, size: usize, shmflg: i32) -> i32 {
        if key == IPC_PRIVATE {
            return syscall_error(Errno::ENOENT, "shmget", "IPC_PRIVATE not implemented");
        }
        let shmid: i32;
        let metadata = &SHM_METADATA;

        match metadata.shmkeyidtable.entry(key) {
            interface::RustHashEntry::Occupied(occupied) => {
                if (IPC_CREAT | IPC_EXCL) == (shmflg & (IPC_CREAT | IPC_EXCL)) {
                    return syscall_error(
                        Errno::EEXIST,
                        "shmget",
                        "key already exists and IPC_CREAT and IPC_EXCL were used",
                    );
                }
                shmid = *occupied.get();
            }
            interface::RustHashEntry::Vacant(vacant) => {
                if 0 == (shmflg & IPC_CREAT) {
                    return syscall_error(
                        Errno::ENOENT,
                        "shmget",
                        "tried to use a key that did not exist, and IPC_CREAT was not specified",
                    );
                }

                if (size as u32) < SHMMIN || (size as u32) > SHMMAX {
                    return syscall_error(
                        Errno::EINVAL,
                        "shmget",
                        "Size is less than SHMMIN or more than SHMMAX",
                    );
                }

                shmid = metadata.new_keyid();
                vacant.insert(shmid);
                let mode = (shmflg & 0x1FF) as u16; // mode is 9 least signficant bits of shmflag, even if we dont really do anything with them

                let segment = new_shm_segment(
                    key,
                    size,
                    self.cageid as u32,
                    DEFAULT_UID,
                    DEFAULT_GID,
                    mode,
                );
                metadata.shmtable.insert(shmid, segment);
            }
        };
        shmid // return the shmid
    }

    //------------------SHMAT SYSCALL------------------

    pub fn shmat_syscall(&self, shmid: i32, shmaddr: *mut u8, shmflg: i32) -> i32 {
        let metadata = &SHM_METADATA;
        let prot: i32;
        if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
            if 0 != (shmflg & SHM_RDONLY) {
                prot = PROT_READ;
            } else {
                prot = libc::PROT_READ | libc::PROT_WRITE;
            }
            let mut rev_shm = self.rev_shm.lock();
            rev_shm.push((shmaddr as u32, shmid));
            drop(rev_shm);

            // update semaphores
            if !segment.semaphor_offsets.is_empty() {
                // lets just look at the first cage in the set, since we only need to grab the ref from one
                if let Some(cageid) = segment
                    .attached_cages
                    .clone()
                    .into_read_only()
                    .keys()
                    .next()
                {
                    let cage2 = interface::cagetable_getref(*cageid);
                    let cage2_rev_shm = cage2.rev_shm.lock();
                    let addrs = Self::rev_shm_find_addrs_by_shmid(&cage2_rev_shm, shmid); // find all the addresses assoc. with shmid
                    for offset in segment.semaphor_offsets.iter() {
                        let sementry = cage2.sem_table.get(&(addrs[0] + *offset)).unwrap().clone(); //add  semaphors into semtable at addr + offsets
                        self.sem_table.insert(shmaddr as u32 + *offset, sementry);
                    }
                }
            }

            segment.map_shm(shmaddr, prot, self.cageid)
        } else {
            syscall_error(Errno::EINVAL, "shmat", "Invalid shmid value")
        }
    }

    //------------------SHMDT SYSCALL------------------

    pub fn shmdt_syscall(&self, shmaddr: *mut u8) -> i32 {
        let metadata = &SHM_METADATA;
        let mut rm = false;
        let mut rev_shm = self.rev_shm.lock();
        let rev_shm_index = Self::rev_shm_find_index_by_addr(&rev_shm, shmaddr as u32);

        if let Some(index) = rev_shm_index {
            let shmid = rev_shm[index].1;
            match metadata.shmtable.entry(shmid) {
                interface::RustHashEntry::Occupied(mut occupied) => {
                    let segment = occupied.get_mut();

                    // update semaphores
                    for offset in segment.semaphor_offsets.iter() {
                        self.sem_table.remove(&(shmaddr as u32 + *offset));
                    }

                    segment.unmap_shm(shmaddr, self.cageid);

                    if segment.rmid && segment.shminfo.shm_nattch == 0 {
                        rm = true;
                    }
                    rev_shm.swap_remove(index);

                    if rm {
                        let key = segment.key;
                        occupied.remove_entry();
                        metadata.shmkeyidtable.remove(&key);
                    }

                    return shmid; //NaCl relies on this non-posix behavior of returning the shmid on success
                }
                interface::RustHashEntry::Vacant(_) => {
                    panic!("Inode not created for some reason");
                }
            };
        } else {
            return syscall_error(
                Errno::EINVAL,
                "shmdt",
                "No shared memory segment at shmaddr",
            );
        }
    }

    //------------------SHMCTL SYSCALL------------------

    pub fn shmctl_syscall(&self, shmid: i32, cmd: i32, buf: Option<&mut ShmidsStruct>) -> i32 {
        let metadata = &SHM_METADATA;

        if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
            match cmd {
                IPC_STAT => {
                    *buf.unwrap() = segment.shminfo;
                }
                IPC_RMID => {
                    segment.rmid = true;
                    segment.shminfo.shm_perm.mode |= SHM_DEST as u16;
                    if segment.shminfo.shm_nattch == 0 {
                        let key = segment.key;
                        drop(segment);
                        metadata.shmtable.remove(&shmid);
                        metadata.shmkeyidtable.remove(&key);
                    }
                }
                _ => {
                    return syscall_error(
                        Errno::EINVAL,
                        "shmctl",
                        "Arguments provided do not match implemented parameters",
                    );
                }
            }
        } else {
            return syscall_error(Errno::EINVAL, "shmctl", "Invalid identifier");
        }

        0 //shmctl has succeeded!
    }

    //------------------MUTEX SYSCALLS------------------
    pub fn mutex_create_syscall(&self) -> i32 {
        let mut mutextable = self.mutex_table.write();
        let mut index_option = None;
        for i in 0..mutextable.len() {
            if mutextable[i].is_none() {
                index_option = Some(i);
                break;
            }
        }

        let index = if let Some(ind) = index_option {
            ind
        } else {
            mutextable.push(None);
            mutextable.len() - 1
        };

        let mutex_result = interface::RawMutex::create();
        match mutex_result {
            Ok(mutex) => {
                mutextable[index] = Some(interface::RustRfc::new(mutex));
                index as i32
            }
            Err(_) => match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => syscall_error(
                    i,
                    "mutex_create",
                    "The libc call to pthread_mutex_init failed!",
                ),
                Err(()) => panic!("Unknown errno value from pthread_mutex_init returned!"),
            },
        }
    }

    pub fn mutex_destroy_syscall(&self, mutex_handle: i32) -> i32 {
        let mut mutextable = self.mutex_table.write();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            mutextable[mutex_handle as usize] = None;
            0
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_destroy",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
        //the RawMutex is destroyed on Drop

        //this is currently assumed to always succeed, as the man page does not list possible
        //errors for pthread_mutex_destroy
    }

    pub fn mutex_lock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.lock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_lock",
                            "The libc call to pthread_mutex_lock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_lock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_lock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    pub fn mutex_trylock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.trylock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_trylock",
                            "The libc call to pthread_mutex_trylock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_trylock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_trylock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    pub fn mutex_unlock_syscall(&self, mutex_handle: i32) -> i32 {
        let mutextable = self.mutex_table.read();
        if mutex_handle < mutextable.len() as i32
            && mutex_handle >= 0
            && mutextable[mutex_handle as usize].is_some()
        {
            let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
            drop(mutextable);
            let retval = clonedmutex.unlock();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "mutex_unlock",
                            "The libc call to pthread_mutex_unlock failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_mutex_unlock returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "mutex_unlock",
                "Mutex handle does not refer to a valid mutex!",
            )
        }
    }

    //------------------CONDVAR SYSCALLS------------------

    pub fn cond_create_syscall(&self) -> i32 {
        let mut cvtable = self.cv_table.write();
        let mut index_option = None;
        for i in 0..cvtable.len() {
            if cvtable[i].is_none() {
                index_option = Some(i);
                break;
            }
        }

        let index = if let Some(ind) = index_option {
            ind
        } else {
            cvtable.push(None);
            cvtable.len() - 1
        };

        let cv_result = interface::RawCondvar::create();
        match cv_result {
            Ok(cv) => {
                cvtable[index] = Some(interface::RustRfc::new(cv));
                index as i32
            }
            Err(_) => match Errno::from_discriminant(interface::get_errno()) {
                Ok(i) => syscall_error(
                    i,
                    "cond_create",
                    "The libc call to pthread_cond_init failed!",
                ),
                Err(()) => panic!("Unknown errno value from pthread_cond_init returned!"),
            },
        }
    }

    pub fn cond_destroy_syscall(&self, cv_handle: i32) -> i32 {
        let mut cvtable = self.cv_table.write();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            cvtable[cv_handle as usize] = None;
            0
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_destroy",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
        //the RawCondvar is destroyed on Drop

        //this is currently assumed to always succeed, as the man page does not list possible
        //errors for pthread_cv_destroy
    }

    pub fn cond_signal_syscall(&self, cv_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);
            let retval = clonedcv.signal();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "cond_signal",
                            "The libc call to pthread_cond_signal failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_cond_signal returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_signal",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_broadcast_syscall(&self, cv_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);
            let retval = clonedcv.broadcast();

            if retval < 0 {
                match Errno::from_discriminant(interface::get_errno()) {
                    Ok(i) => {
                        return syscall_error(
                            i,
                            "cond_broadcast",
                            "The libc call to pthread_cond_broadcast failed!",
                        );
                    }
                    Err(()) => panic!("Unknown errno value from pthread_cond_broadcast returned!"),
                };
            }

            retval
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_broadcast",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_wait_syscall(&self, cv_handle: i32, mutex_handle: i32) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);

            let mutextable = self.mutex_table.read();
            if mutex_handle < mutextable.len() as i32
                && mutex_handle >= 0
                && mutextable[mutex_handle as usize].is_some()
            {
                let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
                drop(mutextable);
                let retval = clonedcv.wait(&*clonedmutex);

                // if the cancel status is set in the cage, we trap around a cancel point
                // until the individual thread is signaled to cancel itself
                if self
                    .cancelstatus
                    .load(interface::RustAtomicOrdering::Relaxed)
                {
                    loop {
                        interface::cancelpoint(self.cageid);
                    } // we check cancellation status here without letting the function return
                }

                if retval < 0 {
                    match Errno::from_discriminant(interface::get_errno()) {
                        Ok(i) => {
                            return syscall_error(
                                i,
                                "cond_wait",
                                "The libc call to pthread_cond_wait failed!",
                            );
                        }
                        Err(()) => panic!("Unknown errno value from pthread_cond_wait returned!"),
                    };
                }

                retval
            } else {
                //undefined behavior
                syscall_error(
                    Errno::EBADF,
                    "cond_wait",
                    "Mutex handle does not refer to a valid mutex!",
                )
            }
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_wait",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    pub fn cond_timedwait_syscall(
        &self,
        cv_handle: i32,
        mutex_handle: i32,
        time: interface::RustDuration,
    ) -> i32 {
        let cvtable = self.cv_table.read();
        if cv_handle < cvtable.len() as i32
            && cv_handle >= 0
            && cvtable[cv_handle as usize].is_some()
        {
            let clonedcv = cvtable[cv_handle as usize].as_ref().unwrap().clone();
            drop(cvtable);

            let mutextable = self.mutex_table.read();
            if mutex_handle < mutextable.len() as i32
                && mutex_handle >= 0
                && mutextable[mutex_handle as usize].is_some()
            {
                let clonedmutex = mutextable[mutex_handle as usize].as_ref().unwrap().clone();
                drop(mutextable);
                let retval = clonedcv.timedwait(&*clonedmutex, time);
                if retval < 0 {
                    match Errno::from_discriminant(interface::get_errno()) {
                        Ok(i) => {
                            return syscall_error(
                                i,
                                "cond_wait",
                                "The libc call to pthread_cond_wait failed!",
                            );
                        }
                        Err(()) => panic!("Unknown errno value from pthread_cond_wait returned!"),
                    };
                }

                retval
            } else {
                //undefined behavior
                syscall_error(
                    Errno::EBADF,
                    "cond_wait",
                    "Mutex handle does not refer to a valid mutex!",
                )
            }
        } else {
            //undefined behavior
            syscall_error(
                Errno::EBADF,
                "cond_wait",
                "Condvar handle does not refer to a valid condvar!",
            )
        }
    }

    //------------------SEMAPHORE SYSCALLS------------------
    /*
     *  Initialize semaphore object SEM to value
     *  pshared used to indicate whether the semaphore is shared in threads (when equals to 0)
     *  or shared between processes (when nonzero)
     */
    pub fn sem_init_syscall(&self, sem_handle: u32, pshared: i32, value: u32) -> i32 {
        // Boundary check
        if value > SEM_VALUE_MAX {
            return syscall_error(Errno::EINVAL, "sem_init", "value exceeds SEM_VALUE_MAX");
        }

        let metadata = &SHM_METADATA;
        let is_shared = pshared != 0;

        // Iterate semaphore table, if semaphore is already initialzed return error
        let semtable = &self.sem_table;

        // Will initialize only it's new
        if !semtable.contains_key(&sem_handle) {
            let new_semaphore =
                interface::RustRfc::new(interface::RustSemaphore::new(value, is_shared));
            semtable.insert(sem_handle, new_semaphore.clone());

            if is_shared {
                let rev_shm = self.rev_shm.lock();
                // if its shared and exists in an existing mapping we need to add it to other cages
                if let Some((mapaddr, shmid)) =
                    Self::search_for_addr_in_region(&rev_shm, sem_handle)
                {
                    let offset = mapaddr - sem_handle;
                    if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                        for cageid in segment.attached_cages.clone().into_read_only().keys() {
                            // iterate through all cages with segment attached and add semaphor in segments at attached addr + offset
                            let cage = interface::cagetable_getref(*cageid);
                            let addrs = Self::rev_shm_find_addrs_by_shmid(&rev_shm, shmid);
                            for addr in addrs.iter() {
                                cage.sem_table.insert(addr + offset, new_semaphore.clone());
                            }
                        }
                        segment.semaphor_offsets.insert(offset);
                    }
                }
            }
            return 0;
        }

        return syscall_error(Errno::EBADF, "sem_init", "semaphore already initialized");
    }

    pub fn sem_wait_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            semaphore.lock();
        } else {
            return syscall_error(Errno::EINVAL, "sem_wait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_post_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.unlock() {
                return syscall_error(
                    Errno::EOVERFLOW,
                    "sem_post",
                    "The maximum allowable value for a semaphore would be exceeded",
                );
            }
        } else {
            return syscall_error(Errno::EINVAL, "sem_wait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_destroy_syscall(&self, sem_handle: u32) -> i32 {
        let metadata = &SHM_METADATA;

        let semtable = &self.sem_table;
        // remove entry from semaphore table
        if let Some(sementry) = semtable.remove(&sem_handle) {
            if sementry
                .1
                .is_shared
                .load(interface::RustAtomicOrdering::Relaxed)
            {
                // if its shared we'll need to remove it from other attachments
                let rev_shm = self.rev_shm.lock();
                if let Some((mapaddr, shmid)) =
                    Self::search_for_addr_in_region(&rev_shm, sem_handle)
                {
                    // find all segments that contain semaphore
                    let offset = mapaddr - sem_handle;
                    if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
                        for cageid in segment.attached_cages.clone().into_read_only().keys() {
                            // iterate through all cages containing segment
                            let cage = interface::cagetable_getref(*cageid);
                            let addrs = Self::rev_shm_find_addrs_by_shmid(&rev_shm, shmid);
                            for addr in addrs.iter() {
                                cage.sem_table.remove(&(addr + offset)); //remove semapoores at attached addresses + the offset
                            }
                        }
                    }
                }
            }
            return 0;
        } else {
            return syscall_error(Errno::EINVAL, "sem_destroy", "sem is not a valid semaphore");
        }
    }

    /*
     * Take only sem_t *sem as argument, and return int *sval
     */
    pub fn sem_getvalue_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            return semaphore.get_value();
        }
        return syscall_error(
            Errno::EINVAL,
            "sem_getvalue",
            "sem is not a valid semaphore",
        );
    }

    pub fn sem_trywait_syscall(&self, sem_handle: u32) -> i32 {
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.trylock() {
                return syscall_error(
                    Errno::EAGAIN,
                    "sem_trywait",
                    "The operation could not be performed without blocking",
                );
            }
        } else {
            return syscall_error(Errno::EINVAL, "sem_trywait", "sem is not a valid semaphore");
        }
        return 0;
    }

    pub fn sem_timedwait_syscall(&self, sem_handle: u32, time: interface::RustDuration) -> i32 {
        let abstime = libc::timespec {
            tv_sec: time.as_secs() as i64,
            tv_nsec: (time.as_nanos() % 1000000000) as i64,
        };
        if abstime.tv_nsec < 0 {
            return syscall_error(Errno::EINVAL, "sem_timedwait", "Invalid timedout");
        }
        let semtable = &self.sem_table;
        // Check whether semaphore exists
        if let Some(sementry) = semtable.get_mut(&sem_handle) {
            let semaphore = sementry.clone();
            drop(sementry);
            if !semaphore.timedlock(time) {
                return syscall_error(
                    Errno::ETIMEDOUT,
                    "sem_timedwait",
                    "The call timed out before the semaphore could be locked",
                );
            }
        } else {
            return syscall_error(
                Errno::EINVAL,
                "sem_timedwait",
                "sem is not a valid semaphore",
            );
        }
        return 0;
    }

    // We're directly patching in the libc futex call for experimentation with lind-wasm
    // this should allow us to use the nptl data structures such as mutexes and condvars directly
    // as opposed to lind-nacl's individual implementations
    //
    // to perform this we just directly pass futex's var args as unsigned 32 bit integers to syscall() with SYS_futex
    pub fn futex_syscall(&self, uaddr: u64, futex_op: u32, val: u32, val2: u32, uaddr2: u32, val3: u32) -> i32 {
        let ret = unsafe { syscall(SYS_futex, uaddr, futex_op, val, val2, uaddr2, val3)  as i32 };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fcntl");
        }
        ret
    }
    
    //We directly call nanosleep syscall(SYS_clock_nanosleep) from the libc
    //return an `i32` value representing the result of the system call.
    pub fn nanosleep_time64_syscall(&self, clockid: u32, flags: i32, req: usize, rem: usize) -> i32 {
        let ret = unsafe { syscall(SYS_clock_nanosleep, clockid, flags, req, rem)  as i32 };
        if ret < 0 {
            let errno = get_errno();
            return handle_errno(errno, "fcntl");
        }
        ret
    }
}

pub fn kernel_close(fdentry: fdtables::FDTableEntry, _count: u64) {
    let _ret = unsafe {
        libc::close(fdentry.underfd as i32)
    };
}
