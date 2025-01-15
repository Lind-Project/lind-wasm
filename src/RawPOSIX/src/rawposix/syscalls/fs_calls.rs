// use std::sync::Arc;
// use std::collections::HashMap;

use crate::cage::*;
use crate::fdtables;
use crate::rawposix::constants::fs_constants;
use crate::sanitization::errno::*;
use crate::sanitization::misc::*;
use crate::sanitization::*;

const FDKIND_KERNEL: u32 = 0;

pub fn hello_syscall(_cageid: u64, _arg1: u64, _arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> i32 {
    // println!("hello from cageid = {:?}", cageid);
    return 0;
}

/// We will first perform type conversion and then call convert path to adjust input path value.
pub fn open_syscall(cageid: u64, path_arg: u64, oflag_arg: u64, mode_arg: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> i32 {
    // Add sanitization functions for all three args
    let cage = get_cage(cageid).unwrap();

    // Validate and convert path string from user space
    let path = match type_conv::check_and_convert_addr_ext(&cage, path_arg, 1, PROT_READ) {
        Ok(addr) => match type_conv::get_cstr(addr as u64) {
            Ok(path_str) => path_str,
            Err(_) => return -1,
        },
        Err(errno) => return syscall_error(errno, "open", "invalid path address"),
    };
    let oflag = oflag_arg as i32;
    let mode = mode_arg as u32;

    // Convert path
    let c_path = add_lind_root(cageid, path);
    let kernel_fd = unsafe { libc::open(c_path.as_ptr(), oflag, mode) };
    
    if kernel_fd < 0 {
        let errno = get_errno();
        return handle_errno(errno, "open");
    }

    let should_cloexec = (oflag & fs_constants::O_CLOEXEC) != 0;

    match fdtables::get_unused_virtual_fd(cageid, FDKIND_KERNEL, kernel_fd as u64, should_cloexec, 0) {
        Ok(virtual_fd) => return virtual_fd as i32,
        Err(_) => return syscall_error(Errno::EMFILE, "open", "Too many files opened")
    }
}

pub fn write_syscall(cageid: u64, virtual_fd: u64, buf_arg: u64, count_arg: u64, _arg4: u64, _arg5: u64, _arg6: u64) -> i32 {
    // early return
    let count = count_arg as usize;
    if count == 0 {
        return 0;
    }
    // type conversion 
    let cage = get_cage(cageid).unwrap();
    let buf = match type_conv::check_and_convert_addr_ext(&cage, buf_arg, count, PROT_READ) {
        Ok(addr) => addr as *const c_void,
        Err(errno) => {
            return syscall_error(
                errno,
                "write",
                "buffer access violation or invalid address"
            );
        }
    };

    let wrappedvfd = fdtables::translate_virtual_fd(cageid, virtual_fd as u64);
    if wrappedvfd.is_err() {
        return syscall_error(Errno::EBADF, "write", "Bad File Descriptor");
    }

    let vfd = wrappedvfd.unwrap();
    let ret = unsafe {
        libc::write(vfd.underfd as i32, buf, count) as i32
    };

    if ret < 0 {
        let errno = get_errno();
        return handle_errno(errno, "write");
    }
    return ret;
}

pub fn kernel_close(fdentry: fdtables::FDTableEntry, _count: u64) {
    let _ret = unsafe {
        libc::close(fdentry.underfd as i32)
    };
}
