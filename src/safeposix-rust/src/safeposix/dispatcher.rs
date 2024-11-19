#![allow(dead_code)]
#![allow(unused_variables)]
// retreive cage table

const ACCESS_SYSCALL: i32 = 2;
const UNLINK_SYSCALL: i32 = 4;
const LINK_SYSCALL: i32 = 5;
const RENAME_SYSCALL: i32 = 6;

const XSTAT_SYSCALL: i32 = 9;
const OPEN_SYSCALL: i32 = 10;
const CLOSE_SYSCALL: i32 = 11;
const READ_SYSCALL: i32 = 12;
const WRITE_SYSCALL: i32 = 13;
const LSEEK_SYSCALL: i32 = 14;
const IOCTL_SYSCALL: i32 = 15;
const TRUNCATE_SYSCALL: i32 = 16;
const FXSTAT_SYSCALL: i32 = 17;
const FTRUNCATE_SYSCALL: i32 = 18;
const FSTATFS_SYSCALL: i32 = 19;
const MMAP_SYSCALL: i32 = 21;
const MUNMAP_SYSCALL: i32 = 22;
const GETDENTS_SYSCALL: i32 = 23;
const DUP_SYSCALL: i32 = 24;
const DUP2_SYSCALL: i32 = 25;
const STATFS_SYSCALL: i32 = 26;
const FCNTL_SYSCALL: i32 = 28;

const GETPPID_SYSCALL: i32 = 29;
const EXIT_SYSCALL: i32 = 30;
const GETPID_SYSCALL: i32 = 31;

const BIND_SYSCALL: i32 = 33;
const SEND_SYSCALL: i32 = 34;
const SENDTO_SYSCALL: i32 = 35;
const RECV_SYSCALL: i32 = 36;
const RECVFROM_SYSCALL: i32 = 37;
const CONNECT_SYSCALL: i32 = 38;
const LISTEN_SYSCALL: i32 = 39;
const ACCEPT_SYSCALL: i32 = 40;

const GETSOCKOPT_SYSCALL: i32 = 43;
const SETSOCKOPT_SYSCALL: i32 = 44;
const SHUTDOWN_SYSCALL: i32 = 45;
const SELECT_SYSCALL: i32 = 46;
const GETCWD_SYSCALL: i32 = 47;
const POLL_SYSCALL: i32 = 48;
const SOCKETPAIR_SYSCALL: i32 = 49;
const GETUID_SYSCALL: i32 = 50;
const GETEUID_SYSCALL: i32 = 51;
const GETGID_SYSCALL: i32 = 52;
const GETEGID_SYSCALL: i32 = 53;
const FLOCK_SYSCALL: i32 = 54;
const EPOLL_CREATE_SYSCALL: i32 = 56;
const EPOLL_CTL_SYSCALL: i32 = 57;
const EPOLL_WAIT_SYSCALL: i32 = 58;

const SHMGET_SYSCALL: i32 = 62;
const SHMAT_SYSCALL: i32 = 63;
const SHMDT_SYSCALL: i32 = 64;
const SHMCTL_SYSCALL: i32 = 65;

const PIPE_SYSCALL: i32 = 66;
const PIPE2_SYSCALL: i32 = 67;
const FORK_SYSCALL: i32 = 68;
const EXEC_SYSCALL: i32 = 69;

const MUTEX_CREATE_SYSCALL: i32 = 70;
const MUTEX_DESTROY_SYSCALL: i32 = 71;
const MUTEX_LOCK_SYSCALL: i32 = 72;
const MUTEX_TRYLOCK_SYSCALL: i32 = 73;
const MUTEX_UNLOCK_SYSCALL: i32 = 74;
const COND_CREATE_SYSCALL: i32 = 75;
const COND_DESTROY_SYSCALL: i32 = 76;
const COND_WAIT_SYSCALL: i32 = 77;
const COND_BROADCAST_SYSCALL: i32 = 78;
const COND_SIGNAL_SYSCALL: i32 = 79;
const COND_TIMEDWAIT_SYSCALL: i32 = 80;

const SEM_INIT_SYSCALL: i32 = 91;
const SEM_WAIT_SYSCALL: i32 = 92;
const SEM_TRYWAIT_SYSCALL: i32 = 93;
const SEM_TIMEDWAIT_SYSCALL: i32 = 94;
const SEM_POST_SYSCALL: i32 = 95;
const SEM_DESTROY_SYSCALL: i32 = 96;
const SEM_GETVALUE_SYSCALL: i32 = 97;
const FUTEX_SYSCALL: i32 = 98;

const GETHOSTNAME_SYSCALL: i32 = 125;
const PREAD_SYSCALL: i32 = 126;
const PWRITE_SYSCALL: i32 = 127;
const CHDIR_SYSCALL: i32 = 130;
const MKDIR_SYSCALL: i32 = 131;
const RMDIR_SYSCALL: i32 = 132;
const CHMOD_SYSCALL: i32 = 133;
const FCHMOD_SYSCALL: i32 = 134;

const SOCKET_SYSCALL: i32 = 136;

const GETSOCKNAME_SYSCALL: i32 = 144;
const GETPEERNAME_SYSCALL: i32 = 145;
const GETIFADDRS_SYSCALL: i32 = 146;

const SIGACTION_SYSCALL: i32 = 147;
const KILL_SYSCALL: i32 = 148;
const SIGPROCMASK_SYSCALL: i32 = 149;
const SETITIMER_SYSCALL: i32 = 150;

const FCHDIR_SYSCALL: i32 = 161;
const FSYNC_SYSCALL: i32 = 162;
const FDATASYNC_SYSCALL: i32 = 163;
const SYNC_FILE_RANGE: i32 = 164;

const WRITEV_SYSCALL: i32 = 170;


use super::cage::*;
use super::filesystem::{
    incref_root, load_fs, persist_metadata, remove_domain_sock, FilesystemMetadata, FS_METADATA,
    LOGFILENAME, LOGMAP,
};
use super::net::NET_METADATA;
use super::shm::SHM_METADATA;
use super::syscalls::{fs_constants::IPC_STAT, sys_constants::*};
use crate::interface::types::SockaddrDummy;
use crate::interface::{self, SigactionStruct};
use crate::interface::errnos::*;
use crate::lib_fs_utils::{lind_deltree, visit_children};

macro_rules! get_onearg {
    ($arg: expr) => {
        match (move || Ok($arg?))() {
            Ok(okval) => okval,
            Err(e) => return e,
        }
    };
}

//this macro takes in a syscall invocation name (i.e. cage.fork_syscall), and all of the arguments
//to the syscall. Then it unwraps the arguments, returning the error if any one of them is an error
//value, and returning the value of the function if not. It does this by using the ? operator in
//the body of a closure within the variadic macro
macro_rules! check_and_dispatch {
    ( $cage:ident . $func:ident, $($arg:expr),* ) => {
        match (|| Ok($cage.$func( $($arg?),* )))() {
            Ok(i) => i, Err(i) => i
        }
    };
}

macro_rules! check_and_dispatch_socketpair {
    ( $func:expr, $cage:ident, $($arg:expr),* ) => {
        match (|| Ok($func( $cage, $($arg?),* )))() {
            Ok(i) => i, Err(i) => i
        }
    };
}

use std::ffi::CStr;
use std::str::Utf8Error;

fn u64_to_str(ptr: u64) -> Result<&'static str, Utf8Error> {
    // Convert the u64 to a pointer to a C string (null-terminated)
    let c_str = ptr as *const i8;

    // Unsafe block to handle raw pointer and C string conversion
    unsafe {
        // Create a CStr from the raw pointer
        let c_str = CStr::from_ptr(c_str);

        // Convert the CStr to a Rust &str
        c_str.to_str()
    }
}

impl Arg {
    pub fn from_u64_as_cbuf(value: u64) -> Self {
        Arg {
            dispatch_cbuf: value as *const u8,
        }
    }

    pub fn from_u64_as_socklen_ptr(value: u64) -> Self {
        Arg {
            dispatch_socklen_t_ptr: value as *mut u32,
        }
    }

    pub fn from_u64_as_sockaddrstruct(value: u64) -> Self {
        Arg {
            dispatch_constsockaddrstruct: value as *const SockaddrDummy,
        }
    }
    pub fn from_u64_as_constsigactionstruct(value: u64) -> Self {
        Arg {
            dispatch_constsigactionstruct: value as *const SigactionStruct,
        }
    }
    pub fn from_u64_as_sigactionstruct(value: u64) -> Self {
        Arg {
            dispatch_sigactionstruct: value as *mut SigactionStruct,
        }
    }
}

#[no_mangle]
pub extern "C" fn lind_syscall_api(
    call_number: u32,
    call_name: u64,
    start_address: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i32 {
    let cageid = 1;
    let call_number = call_number as i32;

    // Print all the arguments
    // println!("call_number: {}", call_number);
    // println!("call_name: {}", call_name);
    // println!("start_address: {}", start_address);
    // println!("arg1: {}", arg1);
    // println!("arg2: {}", arg2);
    // println!("arg3: {}", arg3);
    // println!("arg4: {}", arg4);
    // println!("arg5: {}", arg5);
    // println!("arg6: {}", arg6);

    let ret = match call_number {
        WRITE_SYSCALL => {
            let fd = arg1 as i32;
            let buf = (start_address + arg2) as *const u8;
            let count = arg3 as usize;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .write_syscall(fd, buf, count)
            }
        }

        WRITEV_SYSCALL => {
            let fd = arg1 as i32;
            let iovec = (start_address + arg2) as *const interface::IovecStruct;
            let iovcnt = arg3 as i32;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .writev_syscall(fd, iovec, iovcnt)
            }
        }

        MUNMAP_SYSCALL => {
            let addr = (start_address + arg1) as *mut u8;
            let len = arg2 as usize;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .munmap_syscall(addr, len)
            }
        }

        MMAP_SYSCALL => {
            let addr = (start_address + arg1) as *mut u8;
            let len = arg2 as usize;
            let prot = arg3 as i32;
            let flags = arg4 as i32;
            let fildes = arg5 as i32;
            let off = arg6 as i64;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .mmap_syscall(addr, len, prot, flags, fildes, off)
            }
        }

        PREAD_SYSCALL => {
            let fd = arg1 as i32;
            let buf = (start_address + arg2) as *mut u8;
            let count = arg3 as usize;
            let offset = arg4 as isize;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .pread_syscall(fd, buf, count, offset)
            }
        }

        READ_SYSCALL => {
            let fd = arg1 as i32;
            let buf = (start_address + arg2) as *mut u8;
            let count = arg3 as usize;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .read_syscall(fd, buf, count)
            }
        }

        SIGACTION_SYSCALL => {
            let sig = arg1 as i32;
            let act = match interface::get_constsigactionstruct(Arg::from_u64_as_constsigactionstruct(arg2)) {
                Ok(res_act) => res_act,
                Err(_) => return -1, // Handle error appropriately, return an error code
            };

            let oact = match interface::get_sigactionstruct(Arg::from_u64_as_sigactionstruct(arg3)) {
                Ok(res_oact) => res_oact,
                Err(_) => return -1, // Handle error appropriately, return an error code
            };
            
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .sigaction_syscall(sig, act, oact)
            }
        }

        CLOSE_SYSCALL => {
            let fd = arg1 as i32;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .close_syscall(fd)
            }
        }

        ACCESS_SYSCALL => {
            let path = match u64_to_str(arg1) {
                Ok(path_str) => path_str,
                Err(_) => return -1, // Handle error appropriately, return an error code
            };
            let amode = arg2 as u32;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .access_syscall(path, amode)
            }
        }

        OPEN_SYSCALL => {
            let path = match u64_to_str(arg1) {
                Ok(path_str) => path_str,
                Err(_) => return -1, // Handle error appropriately, return an error code
            };
            let flags = arg2 as i32;
            let mode = arg3 as u32;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .open_syscall(path, flags, mode)
            }
        }

        SOCKET_SYSCALL => {
            let domain = arg1 as i32;
            let socktype = arg2 as i32;
            let protocol = arg3 as i32;
            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .socket_syscall(domain, socktype, protocol)
            }
        }

        CONNECT_SYSCALL => {
            let fd = arg1 as i32;
            let addrlen = arg3 as u32;
            let addr = get_onearg!(interface::get_sockaddr(Arg::from_u64_as_sockaddrstruct(arg2), addrlen));
            interface::check_cageid(cageid);
            unsafe {
                let remoteaddr = match Ok::<&interface::GenSockaddr, i32>(&addr) {
                    Ok(addr) => addr,
                    Err(_) => panic!("Failed to get sockaddr"), // Handle error appropriately
                };
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .connect_syscall(fd, remoteaddr)
            }
        }

        BIND_SYSCALL => {
            let fd = arg1 as i32;
            let addrlen = arg3 as u32;
            let addr = get_onearg!(interface::get_sockaddr(Arg::from_u64_as_sockaddrstruct(arg2), addrlen));
            interface::check_cageid(cageid);
            unsafe {
                let localaddr = match Ok::<&interface::GenSockaddr, i32>(&addr) {
                    Ok(addr) => addr,
                    Err(_) => panic!("Failed to get sockaddr"), // Handle error appropriately
                };
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .bind_syscall(fd, localaddr)
            }
        }

        ACCEPT_SYSCALL => {
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //value doesn't matter
            let nullity1 = interface::arg_nullity(&Arg::from_u64_as_cbuf(arg2));
            let nullity2 = interface::arg_nullity(&Arg::from_u64_as_cbuf(arg3));

            if nullity1 && nullity2 {
                interface::check_cageid(cageid);
                unsafe {
                    CAGE_TABLE[cageid as usize]
                        .as_ref()
                        .unwrap()
                        .accept_syscall(arg1 as i32, &mut addr)
                }
            } else if !(nullity1 || nullity2) {
                interface::check_cageid(cageid);
                let rv = unsafe {
                    CAGE_TABLE[cageid as usize]
                        .as_ref()
                        .unwrap()
                        .accept_syscall(arg1 as i32, &mut addr)
                };
                if rv >= 0 {
                    interface::copy_out_sockaddr(Arg::from_u64_as_sockaddrstruct(arg2), Arg::from_u64_as_socklen_ptr(arg3), addr);
                }
                rv
            } else {
                syscall_error(
                    Errno::EINVAL,
                    "accept",
                    "exactly one of the last two arguments was zero",
                )
            }
        }

        FUTEX_SYSCALL => {
            let uaddr = (start_address + arg1) as u64;
            let futex_op = arg2 as u32;
            let val = arg3 as u32;
            let timeout = arg4 as u32;
            let uaddr2 = arg5 as u32;
            let val3 = arg6 as u32;

            interface::check_cageid(cageid);
            unsafe {
                CAGE_TABLE[cageid as usize]
                    .as_ref()
                    .unwrap()
                    .futex_syscall(uaddr, futex_op, val, timeout, uaddr2, val3)
            }
        }

        _ => -1, // Return -1 for unknown syscalls
    };
    // println!("Lind returns: {}", ret);
    ret
}

// the following "quick" functions are implemented for research purposes
// to increase I/O performance by bypassing the dispatcher and type checker
#[no_mangle]
pub extern "C" fn quick_write(fd: i32, buf: *const u8, count: usize, cageid: u64) -> i32 {
    interface::check_cageid(cageid);
    unsafe {
        CAGE_TABLE[cageid as usize]
            .as_ref()
            .unwrap()
            .write_syscall(fd, buf, count)
    }
}

#[no_mangle]
pub extern "C" fn quick_read(fd: i32, buf: *mut u8, size: usize, cageid: u64) -> i32 {
    interface::check_cageid(cageid);
    unsafe {
        CAGE_TABLE[cageid as usize]
            .as_ref()
            .unwrap()
            .read_syscall(fd, buf, size)
    }
}

#[no_mangle]
pub extern "C" fn rustposix_thread_init(cageid: u64, signalflag: u64) {
    let cage = interface::cagetable_getref(cageid);
    let pthreadid = interface::get_pthreadid();
    cage.main_threadid
        .store(pthreadid, interface::RustAtomicOrdering::Relaxed);
    let inheritedsigset = cage.sigset.remove(&0); // in cases of a forked cage, we've stored the inherited sigset at entry 0
    if inheritedsigset.is_some() {
        cage.sigset.insert(pthreadid, inheritedsigset.unwrap().1);
    } else {
        cage.sigset
            .insert(pthreadid, interface::RustAtomicU64::new(0));
    }

    cage.pendingsigset
        .insert(pthreadid, interface::RustAtomicU64::new(0));
    interface::signalflag_set(signalflag);
}

#[no_mangle]
pub extern "C" fn dispatcher(
    cageid: u64,
    callnum: i32,
    arg1: Arg,
    arg2: Arg,
    arg3: Arg,
    arg4: Arg,
    arg5: Arg,
    arg6: Arg,
) -> i32 {
    // need to match based on if cage exists
    let cage = interface::cagetable_getref(cageid);

    match callnum {
        ACCESS_SYSCALL => {
            check_and_dispatch!(
                cage.access_syscall,
                interface::get_cstr(arg1),
                interface::get_uint(arg2)
            )
        }
        UNLINK_SYSCALL => {
            check_and_dispatch!(cage.unlink_syscall, interface::get_cstr(arg1))
        }
        LINK_SYSCALL => {
            check_and_dispatch!(
                cage.link_syscall,
                interface::get_cstr(arg1),
                interface::get_cstr(arg2)
            )
        }
        CHDIR_SYSCALL => {
            check_and_dispatch!(cage.chdir_syscall, interface::get_cstr(arg1))
        }
        FSYNC_SYSCALL => {
            check_and_dispatch!(cage.fsync_syscall, interface::get_int(arg1))
        }
        FDATASYNC_SYSCALL => {
            check_and_dispatch!(cage.fdatasync_syscall, interface::get_int(arg1))
        }
        SYNC_FILE_RANGE => {
            check_and_dispatch!(
                cage.sync_file_range_syscall,
                interface::get_int(arg1),
                interface::get_isize(arg2),
                interface::get_isize(arg3),
                interface::get_uint(arg4)
            )
        }
        FCHDIR_SYSCALL => {
            check_and_dispatch!(cage.fchdir_syscall, interface::get_int(arg1))
        }
        XSTAT_SYSCALL => {
            check_and_dispatch!(
                cage.stat_syscall,
                interface::get_cstr(arg1),
                interface::get_statdatastruct(arg2)
            )
        }
        OPEN_SYSCALL => {
            check_and_dispatch!(
                cage.open_syscall,
                interface::get_cstr(arg1),
                interface::get_int(arg2),
                interface::get_uint(arg3)
            )
        }
        READ_SYSCALL => {
            check_and_dispatch!(
                cage.read_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_usize(arg3)
            )
        }
        WRITE_SYSCALL => {
            check_and_dispatch!(
                cage.write_syscall,
                interface::get_int(arg1),
                interface::get_cbuf(arg2),
                interface::get_usize(arg3)
            )
        }
        CLOSE_SYSCALL => {
            check_and_dispatch!(cage.close_syscall, interface::get_int(arg1))
        }
        LSEEK_SYSCALL => {
            check_and_dispatch!(
                cage.lseek_syscall,
                interface::get_int(arg1),
                interface::get_isize(arg2),
                interface::get_int(arg3)
            )
        }
        FXSTAT_SYSCALL => {
            check_and_dispatch!(
                cage.fstat_syscall,
                interface::get_int(arg1),
                interface::get_statdatastruct(arg2)
            )
        }
        FSTATFS_SYSCALL => {
            check_and_dispatch!(
                cage.fstatfs_syscall,
                interface::get_int(arg1),
                interface::get_fsdatastruct(arg2)
            )
        }
        MMAP_SYSCALL => {
            check_and_dispatch!(
                cage.mmap_syscall,
                interface::get_mutcbuf(arg1),
                interface::get_usize(arg2),
                interface::get_int(arg3),
                interface::get_int(arg4),
                interface::get_int(arg5),
                interface::get_long(arg6)
            )
        }
        MUNMAP_SYSCALL => {
            check_and_dispatch!(
                cage.munmap_syscall,
                interface::get_mutcbuf(arg1),
                interface::get_usize(arg2)
            )
        }
        DUP_SYSCALL => {
            check_and_dispatch!(
                cage.dup_syscall,
                interface::get_int(arg1),
                Ok::<Option<i32>, i32>(None)
            )
        }
        DUP2_SYSCALL => {
            check_and_dispatch!(
                cage.dup2_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        STATFS_SYSCALL => {
            check_and_dispatch!(
                cage.statfs_syscall,
                interface::get_cstr(arg1),
                interface::get_fsdatastruct(arg2)
            )
        }
        FCNTL_SYSCALL => {
            check_and_dispatch!(
                cage.fcntl_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3)
            )
        }
        IOCTL_SYSCALL => {
            check_and_dispatch!(
                cage.ioctl_syscall,
                interface::get_int(arg1),
                interface::get_uint(arg2),
                interface::get_ioctlptrunion(arg3)
            )
        }
        GETPPID_SYSCALL => {
            check_and_dispatch!(cage.getppid_syscall,)
        }
        GETPID_SYSCALL => {
            check_and_dispatch!(cage.getpid_syscall,)
        }
        SOCKET_SYSCALL => {
            check_and_dispatch!(
                cage.socket_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3)
            )
        }
        BIND_SYSCALL => {
            let addrlen = get_onearg!(interface::get_uint(arg3));
            let addr = get_onearg!(interface::get_sockaddr(arg2, addrlen));
            check_and_dispatch!(
                cage.bind_syscall,
                interface::get_int(arg1),
                Ok::<&interface::GenSockaddr, i32>(&addr)
            )
        }
        SEND_SYSCALL => {
            check_and_dispatch!(
                cage.send_syscall,
                interface::get_int(arg1),
                interface::get_cbuf(arg2),
                interface::get_usize(arg3),
                interface::get_int(arg4)
            )
        }
        SENDTO_SYSCALL => {
            let addrlen = get_onearg!(interface::get_uint(arg6));
            let addr = get_onearg!(interface::get_sockaddr(arg5, addrlen));
            check_and_dispatch!(
                cage.sendto_syscall,
                interface::get_int(arg1),
                interface::get_cbuf(arg2),
                interface::get_usize(arg3),
                interface::get_int(arg4),
                Ok::<&interface::GenSockaddr, i32>(&addr)
            )
        }
        RECV_SYSCALL => {
            check_and_dispatch!(
                cage.recv_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_usize(arg3),
                interface::get_int(arg4)
            )
        }
        RECVFROM_SYSCALL => {
            let nullity1 = interface::arg_nullity(&arg5);
            let nullity2 = interface::arg_nullity(&arg6);

            if nullity1 && nullity2 {
                check_and_dispatch!(
                    cage.recvfrom_syscall,
                    interface::get_int(arg1),
                    interface::get_mutcbuf(arg2),
                    interface::get_usize(arg3),
                    interface::get_int(arg4),
                    Ok::<&mut Option<&mut interface::GenSockaddr>, i32>(&mut None)
                )
            } else if !(nullity1 || nullity2) {
                let addrlen = get_onearg!(interface::get_socklen_t_ptr(arg6));
                let mut newsockaddr = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //dummy value, rust would complain if we used an uninitialized value here
                let rv = check_and_dispatch!(
                    cage.recvfrom_syscall,
                    interface::get_int(arg1),
                    interface::get_mutcbuf(arg2),
                    interface::get_usize(arg3),
                    interface::get_int(arg4),
                    Ok::<&mut Option<&mut interface::GenSockaddr>, i32>(&mut Some(
                        &mut newsockaddr
                    ))
                );

                if rv >= 0 {
                    interface::copy_out_sockaddr(arg5, arg6, newsockaddr);
                }
                rv
            } else {
                syscall_error(
                    Errno::EINVAL,
                    "recvfrom",
                    "exactly one of the last two arguments was zero",
                )
            }
        }
        CONNECT_SYSCALL => {
            let addrlen = get_onearg!(interface::get_uint(arg3));
            let addr = get_onearg!(interface::get_sockaddr(arg2, addrlen));
            check_and_dispatch!(
                cage.connect_syscall,
                interface::get_int(arg1),
                Ok::<&interface::GenSockaddr, i32>(&addr)
            )
        }
        LISTEN_SYSCALL => {
            check_and_dispatch!(
                cage.listen_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        ACCEPT_SYSCALL => {
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //value doesn't matter
            let nullity1 = interface::arg_nullity(&arg2);
            let nullity2 = interface::arg_nullity(&arg3);

            if nullity1 && nullity2 {
                check_and_dispatch!(
                    cage.accept_syscall,
                    interface::get_int(arg1),
                    Ok::<&mut interface::GenSockaddr, i32>(&mut addr)
                )
            } else if !(nullity1 || nullity2) {
                let rv = check_and_dispatch!(
                    cage.accept_syscall,
                    interface::get_int(arg1),
                    Ok::<&mut interface::GenSockaddr, i32>(&mut addr)
                );
                if rv >= 0 {
                    interface::copy_out_sockaddr(arg2, arg3, addr);
                }
                rv
            } else {
                syscall_error(
                    Errno::EINVAL,
                    "accept",
                    "exactly one of the last two arguments was zero",
                )
            }
        }
        GETPEERNAME_SYSCALL => {
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //value doesn't matter
            if interface::arg_nullity(&arg2) || interface::arg_nullity(&arg3) {
                return syscall_error(
                    Errno::EINVAL,
                    "getpeername",
                    "Either the address or the length were null",
                );
            }
            let rv = check_and_dispatch!(
                cage.getpeername_syscall,
                interface::get_int(arg1),
                Ok::<&mut interface::GenSockaddr, i32>(&mut addr)
            );

            if rv >= 0 {
                interface::copy_out_sockaddr(arg2, arg3, addr);
            }
            rv
        }
        GETSOCKNAME_SYSCALL => {
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default()); //value doesn't matter
            if interface::arg_nullity(&arg2) || interface::arg_nullity(&arg3) {
                return syscall_error(
                    Errno::EINVAL,
                    "getsockname",
                    "Either the address or the length were null",
                );
            }
            let rv = check_and_dispatch!(
                cage.getsockname_syscall,
                interface::get_int(arg1),
                Ok::<&mut interface::GenSockaddr, i32>(&mut addr)
            );

            if rv >= 0 {
                interface::copy_out_sockaddr(arg2, arg3, addr);
            }
            rv
        }
        GETIFADDRS_SYSCALL => {
            check_and_dispatch!(
                cage.getifaddrs_syscall,
                interface::get_mutcbuf(arg1),
                interface::get_usize(arg2)
            )
        }
        GETSOCKOPT_SYSCALL => {
            let mut sockval = 0;
            if interface::arg_nullity(&arg4) || interface::arg_nullity(&arg5) {
                return syscall_error(
                    Errno::EFAULT,
                    "getsockopt",
                    "Optval or optlen passed as null",
                );
            }
            if get_onearg!(interface::get_socklen_t_ptr(arg5)) != 4 {
                return syscall_error(Errno::EINVAL, "setsockopt", "Invalid optlen passed");
            }
            let rv = check_and_dispatch!(
                cage.getsockopt_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3),
                Ok::<&mut i32, i32>(&mut sockval)
            );

            if rv >= 0 {
                interface::copy_out_intptr(arg4, sockval);
            }
            //we take it as a given that the length is 4 both in and out
            rv
        }
        SETSOCKOPT_SYSCALL => {
            let sockval;
            if !interface::arg_nullity(&arg4) {
                if get_onearg!(interface::get_uint(arg5)) != 4 {
                    return syscall_error(Errno::EINVAL, "setsockopt", "Invalid optlen passed");
                }
                sockval = interface::get_int_from_intptr(arg4);
            } else {
                sockval = 0;
            }
            check_and_dispatch!(
                cage.setsockopt_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3),
                Ok::<i32, i32>(sockval)
            )
        }
        SHUTDOWN_SYSCALL => {
            check_and_dispatch!(
                cage.netshutdown_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        SELECT_SYSCALL => {
            let nfds = get_onearg!(interface::get_int(arg1));
            if nfds < 0 {
                //RLIMIT_NOFILE check as well?
                return syscall_error(
                    Errno::EINVAL,
                    "select",
                    "The number of fds passed was invalid",
                );
            }
            check_and_dispatch!(
                cage.select_syscall,
                Ok::<i32, i32>(nfds),
                interface::get_fdset(arg2),
                interface::get_fdset(arg3),
                interface::get_fdset(arg4),
                interface::duration_fromtimeval(arg5)
            )
        }
        POLL_SYSCALL => {
            let nfds = get_onearg!(interface::get_usize(arg2));
            check_and_dispatch!(
                cage.poll_syscall,
                interface::get_pollstruct_slice(arg1, nfds),
                interface::get_duration_from_millis(arg3)
            )
        }
        SOCKETPAIR_SYSCALL => {
            check_and_dispatch_socketpair!(
                Cage::socketpair_syscall,
                cage,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3),
                interface::get_sockpair(arg4)
            )
        }
        EXIT_SYSCALL => {
            check_and_dispatch!(cage.exit_syscall, interface::get_int(arg1))
        }
        FLOCK_SYSCALL => {
            check_and_dispatch!(
                cage.flock_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        FORK_SYSCALL => {
            check_and_dispatch!(cage.fork_syscall, interface::get_ulong(arg1))
        }
        EXEC_SYSCALL => {
            check_and_dispatch!(cage.exec_syscall, interface::get_ulong(arg1))
        }
        GETUID_SYSCALL => {
            check_and_dispatch!(cage.getuid_syscall,)
        }
        GETEUID_SYSCALL => {
            check_and_dispatch!(cage.geteuid_syscall,)
        }
        GETGID_SYSCALL => {
            check_and_dispatch!(cage.getgid_syscall,)
        }
        GETEGID_SYSCALL => {
            check_and_dispatch!(cage.getegid_syscall,)
        }
        PREAD_SYSCALL => {
            check_and_dispatch!(
                cage.pread_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_usize(arg3),
                interface::get_isize(arg4)
            )
        }
        PWRITE_SYSCALL => {
            check_and_dispatch!(
                cage.pwrite_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_usize(arg3),
                interface::get_isize(arg4)
            )
        }
        CHMOD_SYSCALL => {
            check_and_dispatch!(
                cage.chmod_syscall,
                interface::get_cstr(arg1),
                interface::get_uint(arg2)
            )
        }
        FCHMOD_SYSCALL => {
            check_and_dispatch!(
                cage.fchmod_syscall,
                interface::get_int(arg1),
                interface::get_uint(arg2)
            )
        }
        RMDIR_SYSCALL => {
            check_and_dispatch!(cage.rmdir_syscall, interface::get_cstr(arg1))
        }
        RENAME_SYSCALL => {
            check_and_dispatch!(
                cage.rename_syscall,
                interface::get_cstr(arg1),
                interface::get_cstr(arg2)
            )
        }
        EPOLL_CREATE_SYSCALL => {
            check_and_dispatch!(cage.epoll_create_syscall, interface::get_int(arg1))
        }
        EPOLL_CTL_SYSCALL => {
            check_and_dispatch!(
                cage.epoll_ctl_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::get_int(arg3),
                interface::get_epollevent(arg4)
            )
        }
        EPOLL_WAIT_SYSCALL => {
            let nfds = get_onearg!(interface::get_int(arg3));

            if nfds < 0 {
                //RLIMIT_NOFILE check as well?
                return syscall_error(
                    Errno::EINVAL,
                    "select",
                    "The number of fds passed was invalid",
                );
            }

            check_and_dispatch!(
                cage.epoll_wait_syscall,
                interface::get_int(arg1),
                interface::get_epollevent_slice(arg2, nfds),
                Ok::<i32, i32>(nfds),
                interface::get_duration_from_millis(arg4)
            )
        }
        GETDENTS_SYSCALL => {
            check_and_dispatch!(
                cage.getdents_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_uint(arg3)
            )
        }
        PIPE_SYSCALL => {
            check_and_dispatch!(cage.pipe_syscall, interface::get_pipearray(arg1))
        }
        PIPE2_SYSCALL => {
            check_and_dispatch!(
                cage.pipe2_syscall,
                interface::get_pipearray(arg1),
                interface::get_int(arg2)
            )
        }
        GETCWD_SYSCALL => {
            check_and_dispatch!(
                cage.getcwd_syscall,
                interface::get_mutcbuf(arg1),
                interface::get_uint(arg2)
            )
        }
        GETHOSTNAME_SYSCALL => {
            check_and_dispatch!(
                cage.gethostname_syscall,
                interface::get_mutcbuf(arg1),
                interface::get_isize(arg2)
            )
        }
        MKDIR_SYSCALL => {
            check_and_dispatch!(
                cage.mkdir_syscall,
                interface::get_cstr(arg1),
                interface::get_uint(arg2)
            )
        }
        SHMGET_SYSCALL => {
            check_and_dispatch!(
                cage.shmget_syscall,
                interface::get_int(arg1),
                interface::get_usize(arg2),
                interface::get_int(arg3)
            )
        }
        SHMAT_SYSCALL => {
            check_and_dispatch!(
                cage.shmat_syscall,
                interface::get_int(arg1),
                interface::get_mutcbuf(arg2),
                interface::get_int(arg3)
            )
        }
        SHMDT_SYSCALL => {
            check_and_dispatch!(cage.shmdt_syscall, interface::get_mutcbuf(arg1))
        }
        SHMCTL_SYSCALL => {
            let cmd = get_onearg!(interface::get_int(arg2));
            let buf = if cmd == IPC_STAT {
                Some(get_onearg!(interface::get_shmidstruct(arg3)))
            } else {
                None
            };
            check_and_dispatch!(
                cage.shmctl_syscall,
                interface::get_int(arg1),
                Ok::<i32, i32>(cmd),
                Ok::<Option<&mut interface::ShmidsStruct>, i32>(buf)
            )
        }

        MUTEX_CREATE_SYSCALL => {
            check_and_dispatch!(cage.mutex_create_syscall,)
        }
        MUTEX_DESTROY_SYSCALL => {
            check_and_dispatch!(cage.mutex_destroy_syscall, interface::get_int(arg1))
        }
        MUTEX_LOCK_SYSCALL => {
            check_and_dispatch!(cage.mutex_lock_syscall, interface::get_int(arg1))
        }
        MUTEX_TRYLOCK_SYSCALL => {
            check_and_dispatch!(cage.mutex_trylock_syscall, interface::get_int(arg1))
        }
        MUTEX_UNLOCK_SYSCALL => {
            check_and_dispatch!(cage.mutex_unlock_syscall, interface::get_int(arg1))
        }
        COND_CREATE_SYSCALL => {
            check_and_dispatch!(cage.cond_create_syscall,)
        }
        COND_DESTROY_SYSCALL => {
            check_and_dispatch!(cage.cond_destroy_syscall, interface::get_int(arg1))
        }
        COND_WAIT_SYSCALL => {
            check_and_dispatch!(
                cage.cond_wait_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        COND_BROADCAST_SYSCALL => {
            check_and_dispatch!(cage.cond_broadcast_syscall, interface::get_int(arg1))
        }
        COND_SIGNAL_SYSCALL => {
            check_and_dispatch!(cage.cond_signal_syscall, interface::get_int(arg1))
        }
        COND_TIMEDWAIT_SYSCALL => {
            check_and_dispatch!(
                cage.cond_timedwait_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2),
                interface::duration_fromtimespec(arg3)
            )
        }
        TRUNCATE_SYSCALL => {
            check_and_dispatch!(
                cage.truncate_syscall,
                interface::get_cstr(arg1),
                interface::get_isize(arg2)
            )
        }
        FTRUNCATE_SYSCALL => {
            check_and_dispatch!(
                cage.ftruncate_syscall,
                interface::get_int(arg1),
                interface::get_isize(arg2)
            )
        }
        SIGACTION_SYSCALL => {
            check_and_dispatch!(
                cage.sigaction_syscall,
                interface::get_int(arg1),
                interface::get_constsigactionstruct(arg2),
                interface::get_sigactionstruct(arg3)
            )
        }
        KILL_SYSCALL => {
            check_and_dispatch!(
                cage.kill_syscall,
                interface::get_int(arg1),
                interface::get_int(arg2)
            )
        }
        SIGPROCMASK_SYSCALL => {
            check_and_dispatch!(
                cage.sigprocmask_syscall,
                interface::get_int(arg1),
                interface::get_constsigsett(arg2),
                interface::get_sigsett(arg3)
            )
        }
        SETITIMER_SYSCALL => {
            check_and_dispatch!(
                cage.setitimer_syscall,
                interface::get_int(arg1),
                interface::get_constitimerval(arg2),
                interface::get_itimerval(arg3)
            )
        }
        SEM_INIT_SYSCALL => {
            check_and_dispatch!(
                cage.sem_init_syscall,
                interface::get_uint(arg1),
                interface::get_int(arg2),
                interface::get_uint(arg3)
            )
        }
        SEM_WAIT_SYSCALL => {
            check_and_dispatch!(cage.sem_wait_syscall, interface::get_uint(arg1))
        }
        SEM_POST_SYSCALL => {
            check_and_dispatch!(cage.sem_post_syscall, interface::get_uint(arg1))
        }
        SEM_DESTROY_SYSCALL => {
            check_and_dispatch!(cage.sem_destroy_syscall, interface::get_uint(arg1))
        }
        SEM_GETVALUE_SYSCALL => {
            check_and_dispatch!(cage.sem_getvalue_syscall, interface::get_uint(arg1))
        }
        SEM_TRYWAIT_SYSCALL => {
            check_and_dispatch!(cage.sem_trywait_syscall, interface::get_uint(arg1))
        }
        SEM_TIMEDWAIT_SYSCALL => {
            check_and_dispatch!(
                cage.sem_timedwait_syscall,
                interface::get_uint(arg1),
                interface::duration_fromtimespec(arg2)
            )
        }
        WRITEV_SYSCALL => {
            check_and_dispatch!(
                cage.writev_syscall,
                interface::get_int(arg1),
                interface::get_iovecstruct(arg2),
                interface::get_int(arg3)
            )
        }
        // FUTEX_SYSCALL => {
        //     check_and_dispatch!(
        //         cage.futex_syscall,
        //         interface::get_uint(arg1),
        //         interface::get_uint(arg2),
        //         interface::get_uint(arg3),
        //         interface::get_uint(arg4),
        //         interface::get_uint(arg5),
        //         interface::get_uint(arg6)
        //     ) 
        // }
        _ => {
            //unknown syscall
            -1
        }
    }
}

#[no_mangle]
pub extern "C" fn lindcancelinit(cageid: u64) {
    let cage = interface::cagetable_getref(cageid);
    cage.cancelstatus
        .store(true, interface::RustAtomicOrdering::Relaxed);
    cage.signalcvs();
}

#[no_mangle]
pub extern "C" fn lindsetthreadkill(cageid: u64, pthreadid: u64, kill: bool) {
    let cage = interface::cagetable_getref(cageid);
    cage.thread_table.insert(pthreadid, kill);
    if cage
        .main_threadid
        .load(interface::RustAtomicOrdering::Relaxed)
        == 0
    {
        cage.main_threadid.store(
            interface::get_pthreadid(),
            interface::RustAtomicOrdering::Relaxed,
        );
    }
}

#[no_mangle]
pub extern "C" fn lindcheckthread(cageid: u64, pthreadid: u64) -> bool {
    interface::check_thread(cageid, pthreadid)
}

#[no_mangle]
pub extern "C" fn lindthreadremove(cageid: u64, pthreadid: u64) {
    let cage = interface::cagetable_getref(cageid);
    cage.thread_table.remove(&pthreadid);
}

fn cleartmp(init: bool) {
    let path = "/tmp";

    let cage = interface::cagetable_getref(0);
    let mut statdata = StatData::default();

    if cage.stat_syscall(path, &mut statdata) == 0 {
        visit_children(&cage, path, None, |childcage, childpath, isdir, _| {
            if isdir {
                lind_deltree(childcage, childpath);
            } else {
                childcage.unlink_syscall(childpath);
            }
        });
    } else {
        if init == true {
            cage.mkdir_syscall(path, S_IRWXA);
        }
    }
}

#[no_mangle]
pub extern "C" fn lindgetsighandler(cageid: u64, signo: i32) -> u32 {
    let cage = interface::cagetable_getref(cageid);
    let pthreadid = interface::get_pthreadid();
    let sigset = cage.sigset.get(&pthreadid).unwrap(); // these lock sigset dashmaps for concurrency
    let pendingset = cage.sigset.get(&pthreadid).unwrap();

    if !interface::lind_sigismember(sigset.load(interface::RustAtomicOrdering::Relaxed), signo) {
        return match cage.signalhandler.get(&signo) {
            Some(action_struct) => {
                action_struct.sa_handler // if we have a handler and its not blocked return it
            }
            None => 0, // if we dont have a handler return 0
        };
    } else {
        let mutpendingset = sigset.load(interface::RustAtomicOrdering::Relaxed);
        sigset.store(
            interface::lind_sigaddset(mutpendingset, signo),
            interface::RustAtomicOrdering::Relaxed,
        );
        1 // if its blocked add the signal to the pending set and return 1 to indicated it was blocked
          //  a signal handler cant be located at address 0x1 so this value is fine to return and check
    }
}

#[no_mangle]
pub extern "C" fn lindrustinit(verbosity: isize) {
    let _ = interface::VERBOSE.set(verbosity); //assigned to suppress unused result warning
    interface::cagetable_init();
    load_fs();
    incref_root();
    incref_root();

    let utilcage = Cage {
        cageid: 0,
        cwd: interface::RustLock::new(interface::RustRfc::new(interface::RustPathBuf::from("/"))),
        parent: 0,
        filedescriptortable: init_fdtable(),
        cancelstatus: interface::RustAtomicBool::new(false),
        getgid: interface::RustAtomicI32::new(-1),
        getuid: interface::RustAtomicI32::new(-1),
        getegid: interface::RustAtomicI32::new(-1),
        geteuid: interface::RustAtomicI32::new(-1),
        rev_shm: interface::Mutex::new(vec![]),
        mutex_table: interface::RustLock::new(vec![]),
        cv_table: interface::RustLock::new(vec![]),
        sem_table: interface::RustHashMap::new(),
        thread_table: interface::RustHashMap::new(),
        signalhandler: interface::RustHashMap::new(),
        sigset: interface::RustHashMap::new(),
        pendingsigset: interface::RustHashMap::new(),
        main_threadid: interface::RustAtomicU64::new(0),
        interval_timer: interface::IntervalTimer::new(0),
    };

    interface::cagetable_insert(0, utilcage);

    //init cage is its own parent
    let initcage = Cage {
        cageid: 1,
        cwd: interface::RustLock::new(interface::RustRfc::new(interface::RustPathBuf::from("/"))),
        parent: 1,
        filedescriptortable: init_fdtable(),
        cancelstatus: interface::RustAtomicBool::new(false),
        getgid: interface::RustAtomicI32::new(-1),
        getuid: interface::RustAtomicI32::new(-1),
        getegid: interface::RustAtomicI32::new(-1),
        geteuid: interface::RustAtomicI32::new(-1),
        rev_shm: interface::Mutex::new(vec![]),
        mutex_table: interface::RustLock::new(vec![]),
        cv_table: interface::RustLock::new(vec![]),
        sem_table: interface::RustHashMap::new(),
        thread_table: interface::RustHashMap::new(),
        signalhandler: interface::RustHashMap::new(),
        sigset: interface::RustHashMap::new(),
        pendingsigset: interface::RustHashMap::new(),
        main_threadid: interface::RustAtomicU64::new(0),
        interval_timer: interface::IntervalTimer::new(1),
    };
    interface::cagetable_insert(1, initcage);
    // make sure /tmp is clean
    cleartmp(true);
}

#[no_mangle]
pub extern "C" fn lindrustfinalize() {
    // remove any open domain socket inodes
    for truepath in NET_METADATA.get_domainsock_paths() {
        remove_domain_sock(truepath);
    }

    // clear /tmp folder
    cleartmp(false);
    interface::cagetable_clear();
    // if we get here, persist and delete log
    persist_metadata(&FS_METADATA);
    if interface::pathexists(LOGFILENAME.to_string()) {
        // remove file if it exists, assigning it to nothing to avoid the compiler yelling about unused result
        let mut logobj = LOGMAP.write();
        let log = logobj.take().unwrap();
        let _close = log.close().unwrap();
        let _logremove = interface::removefile(LOGFILENAME.to_string());
    }
}
