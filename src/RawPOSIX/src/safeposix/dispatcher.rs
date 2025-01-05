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

const CLONE_SYSCALL: i32 = 171;
const WAIT_SYSCALL: i32 = 172;
const WAITPID_SYSCALL: i32 = 173;
const BRK_SYSCALL: i32 = 175;
const SBRK_SYSCALL: i32 = 176;

const NANOSLEEP_TIME64_SYSCALL : i32 = 181;

use std::ffi::CString;
use std::ffi::CStr;
use super::cage::*;
use super::syscalls::kernel_close;

const FDKIND_KERNEL: u32 = 0;
const FDKIND_IMPIPE: u32 = 1;
const FDKIND_IMSOCK: u32 = 2;

use std::io::{Read, Write};
use std::io;

use crate::interface::types;
use crate::interface::{SigactionStruct, StatData};
use crate::{fdtables, interface};
use crate::interface::errnos::*;
use crate::constants::*;
use crate::interface::check_and_convert_addr_ext;

macro_rules! get_onearg {
    ($arg: expr) => {
        match (move || Ok($arg?))() {
            Ok(okval) => okval,
            Err(e) => return e,
        }
    };
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

    interface::signalflag_set(signalflag);
}

/// The `lind_syscall_api` function acts as the main dispatcher for handling system calls 
/// within the Lind virtualized environment. It identifies the syscall to execute based on 
/// `call_number`, and then invokes the appropriate syscall with the given arguments within 
/// the specified cage (`cageid`).
/// 
/// ### Arguments:
/// `lind_syscall_api()` accepts 10 arguments:
/// * `cageid` - Identifier for the cage in which the syscall will be executed.
/// * `call_number` - Unique number for each system call, used to identify which syscall to invoke.
/// * `call_name` - A legacy argument from the initial 3i proposal, currently unused and subject to 
///                 change with future 3i integration.
/// * `start_address` - Base address of WebAssembly linear memory, used for address translation
///                     between virtual and system memory address.
/// * `arg1 - arg6` - Syscall-specific arguments. Any unused argument is set to `0xdeadbeefdeadbeef`.
/// 
/// ### Returns:
/// On success, returns the syscall's return value. On failure, returns the negative errno code.
/// 
/// ### Panics:
/// * If the specified `cageid` does not exist, the function will panic.
#[no_mangle]
pub fn lind_syscall_api(
    cageid: u64,
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
    let call_number = call_number as i32;

    let ret = match call_number {
        WRITE_SYSCALL => {
            // Handles writing data from user buffer to file descriptor
            
            // Get file descriptor
            let fd = arg1 as i32;
        
            let count = arg3 as usize;
            if count == 0 {
                return 0; // Early return for zero-length writes
            }
        
            // Get cage reference for memory operations
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert user buffer address to system address
            // PROT_READ is correct because write() reads FROM the buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_READ) {
                Ok(addr) => addr as *const u8,
                Err(errno) => {
                    return syscall_error(
                        errno,
                        "write",
                        "buffer access violation or invalid address"
                    );
                }
            };
        
            // Perform write operation through cage abstraction
            cage.write_syscall(fd, buf, count)
        }

        WRITEV_SYSCALL => {
            let fd = arg1 as i32;
            let iovcnt = arg3 as i32;
        
            // Validate count first
            if iovcnt <= 0 {
                return syscall_error(
                    Errno::EINVAL,
                    "writev",
                    "invalid iovec count"
                );
            }
        
            let cage = interface::cagetable_getref(cageid);
        
            // Validate the iovec array address first
            let iov_base = match check_and_convert_addr_ext(
                &cage,
                arg2,
                (iovcnt as usize) * std::mem::size_of::<interface::IovecStruct>(),
                PROT_READ
            ) {
                Ok(addr) => addr as *const interface::IovecStruct,
                Err(errno) => {
                    return syscall_error(
                        errno,
                        "writev",
                        "invalid iovec array address"
                    );
                }
            };
        
            // The actual write operation is delegated to the cage implementation
            cage.writev_syscall(fd, iov_base, iovcnt)
        }

        MUNMAP_SYSCALL => {
            let addr = arg1 as *mut u8;
            let length = arg2 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            if length == 0 {
                return syscall_error(
                    Errno::EINVAL,
                    "munmap",
                    "length cannot be zero"
                );
            }
        
            // Perform the unmapping operation
            interface::munmap_handler(cageid, addr, length)
        }

        MMAP_SYSCALL => {
            let addr = arg1 as *mut u8;
            let len = arg2 as usize;
            let prot = arg3 as i32;
            let flags = arg4 as i32;
            let fd = arg5 as i32;
            let off = arg6 as i64;
        
            // Basic length validation
            if len == 0 {
                return syscall_error(
                    Errno::EINVAL,
                    "mmap",
                    "length cannot be zero"
                );
            }
        
            // Force MAP_FIXED
            let flags = flags | MAP_FIXED as i32;
        
            // Turn off PROT_EXEC for non-code pages
            let prot = prot & !PROT_EXEC;
        
            interface::mmap_handler(cageid, addr, len, prot, flags, fd, off)
        }

        PREAD_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let offset = arg4 as i64;
            let cage = interface::cagetable_getref(cageid);
            
            // Validate and convert user buffer address
            // Using PROT_WRITE since pread writes TO the buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "pread", "invalid buffer address"),
            };
        
            cage.pread_syscall(fd, buf, count, offset)
        }

        READ_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
            
            // Validate and convert user buffer address
            // Using PROT_WRITE since read() writes TO the buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "read", "invalid buffer address"),
            };

            // File descriptor validation and actual read operation
            // handled by cage implementation
            cage.read_syscall(fd, buf, count)
        }

        CLOSE_SYSCALL => {
            let fd = arg1 as i32;

            // File descriptor validation and close operation handled by cage
            interface::cagetable_getref(cageid)
                .close_syscall(fd)
        }

        ACCESS_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
            
            // Validate and convert path string from user space
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr as u64) {
                    Ok(path_str) => path_str,
                    Err(_) => return -1,
                },
                Err(errno) => return syscall_error(errno, "access", "invalid path address"),
            };
            let amode = arg2 as i32;

            // Perform access check through cage implementation
            cage.access_syscall(path, amode)
        }

        OPEN_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
            
            // Validate and convert path string from user space
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr as u64) {
                    Ok(path_str) => path_str,
                    Err(_) => return -1,
                },
                Err(errno) => return syscall_error(errno, "open", "invalid path address"),
            };
            let flags = arg2 as i32;
            let mode = arg3 as u32;
        
            // Perform open operation through cage implementation
            cage.open_syscall(path, flags, mode)
        }

        SOCKET_SYSCALL => {
            let domain = arg1 as i32;
            let socktype = arg2 as i32;
            let protocol = arg3 as i32;

            // Perform socket operation through cage implementation
            // Domain, type, and protocol validation handled by cage layer
            interface::cagetable_getref(cageid)
                .socket_syscall(domain, socktype, protocol)
        }

        CONNECT_SYSCALL => {
            let fd = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);

            // Validate and convert sockaddr from user space
            let addr = match check_and_convert_addr_ext(&cage, arg2, arg3 as usize, PROT_READ) {
                Ok(addr) => match interface::get_sockaddr(addr as u64, arg3 as u32) {
                    Ok(sockaddr) => sockaddr,
                    Err(_) => return syscall_error(Errno::EINVAL, "connect", "invalid sockaddr format"),
                },
                Err(errno) => return syscall_error(errno, "connect", "invalid address"),
            };
            
            // Convert to reference for connect operation
            let remoteaddr = match Ok::<&interface::GenSockaddr, i32>(&addr) {
                Ok(addr) => addr,
                Err(_) => panic!("Failed to get sockaddr"), // Handle error appropriately
            };

            // Perform connect operation through cage implementation
            // File descriptor validation handled by cage layer
            cage.connect_syscall(fd, remoteaddr)
        }

        BIND_SYSCALL => {
            let fd = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert sockaddr from user space
            let addr = match check_and_convert_addr_ext(&cage, arg2, arg3 as usize, PROT_READ) {
                Ok(addr) => match interface::get_sockaddr(addr as u64, arg3 as u32) {
                    Ok(sockaddr) => sockaddr,
                    Err(_) => return syscall_error(Errno::EINVAL, "bind", "invalid sockaddr format"),
                },
                Err(errno) => return syscall_error(errno, "bind", "invalid address"),
            };
        
            // Convert to reference for bind operation
            let localaddr = match Ok::<&interface::GenSockaddr, i32>(&addr) {
                Ok(addr) => addr,
                Err(_) => panic!("Failed to get sockaddr"), // Handle error appropriately
            };
        
            // Perform bind operation through cage implementation
            // File descriptor validation handled by cage layer
            cage.bind_syscall(fd, localaddr)
        }

        ACCEPT_SYSCALL => {
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default());
            let nullity1 = interface::arg_nullity(arg2);
            let nullity2 = interface::arg_nullity(arg3);
            let cage = interface::cagetable_getref(cageid);
        
            // Handle NULL address case (both NULL)
            if nullity1 && nullity2 {
                cage.accept_syscall(arg1 as i32, &mut Some(&mut addr))
            } 
            // Handle non-NULL case (both non-NULL)
            else if !(nullity1 || nullity2) {
                // Perform accept operation first
                let rv = cage.accept_syscall(arg1 as i32, &mut Some(&mut addr));
                if rv >= 0 {
                    let addr2_addr = match check_and_convert_addr_ext(&cage, arg2, arg3 as usize, PROT_WRITE) {
                        Ok(addr) => addr,
                        Err(errno) => return syscall_error(errno, "accept", "invalid address buffer"),
                    };
                    let len_addr = match check_and_convert_addr_ext(&cage, arg3, std::mem::size_of::<u32>(), PROT_WRITE) {
                        Ok(addr) => addr,
                        Err(errno) => return syscall_error(errno, "accept", "invalid length buffer"),
                    };
                    interface::copy_out_sockaddr(addr2_addr as u64, len_addr as u64, addr);
                }
                rv
            } 
            // Handle invalid case (one NULL, one non-NULL)
            else {
                syscall_error(
                    Errno::EINVAL,
                    "accept",
                    "exactly one of the last two arguments was zero",
                )
            }
        }

        EXEC_SYSCALL => {
            let child_cageid = arg1 as u64;
        
            // Perform exec operation through cage implementation
            // Child cage validation handled by cage layer
            interface::cagetable_getref(cageid)
                .exec_syscall(child_cageid)
        }

        EXIT_SYSCALL => {
            let status = arg1 as i32;
        
            // Perform exit operation through cage implementation
            // Cleanup handled by cage layer
            interface::cagetable_getref(cageid)
                .exit_syscall(status)
        }

        SELECT_SYSCALL => {
            // Get the number of file descriptors to check (highest fd + 1)
            let nfds = arg1 as i32;
            // Get reference to the cage for memory operations
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert readfds buffer
            // 1. check_and_convert_addr_ext validates memory access and converts user address to system address
            // 2. PROT_READ | PROT_WRITE because select() both reads from and modifies the fd_sets
            // 3. get_fdset converts the raw memory into an fd_set structure
            let readfds = match check_and_convert_addr_ext(&cage, arg2, std::mem::size_of::<interface::FdSet>(), PROT_READ | PROT_WRITE) {
                Ok(addr) => match interface::get_fdset(addr) {
                    Ok(fds) => fds,
                    Err(_) => return syscall_error(Errno::EFAULT, "select", "failed to get readfds"),
                },
                Err(errno) => return syscall_error(errno, "select", "invalid readfds address"),
            };
        
            // Validate and convert writefds buffer
            // Similar to readfds, this fd_set indicates which descriptors to check for write-ready status
            // The fd_set will be modified to show which descriptors are actually write-ready
            let writefds = match check_and_convert_addr_ext(&cage, arg3, std::mem::size_of::<interface::FdSet>(), PROT_READ | PROT_WRITE) {
                Ok(addr) => match interface::get_fdset(addr) {
                    Ok(fds) => fds,
                    Err(_) => return syscall_error(Errno::EFAULT, "select", "failed to get writefds"),
                },
                Err(errno) => return syscall_error(errno, "select", "invalid writefds address"),
            };
        
            // Validate and convert errorfds buffer (also called exceptfds in some implementations)
            // This fd_set is used to monitor for exceptional conditions (like out-of-band data)
            // Also requires both read and write access as select() modifies it
            let errorfds = match check_and_convert_addr_ext(&cage, arg4, std::mem::size_of::<interface::FdSet>(), PROT_READ | PROT_WRITE) {
                Ok(addr) => match interface::get_fdset(addr) {
                    Ok(fds) => fds,
                    Err(_) => return syscall_error(Errno::EFAULT, "select", "failed to get errorfds"),
                },
                Err(errno) => return syscall_error(errno, "select", "invalid errorfds address"),
            };
        
            // Validate and convert timeout buffer
            // 1. Only needs PROT_READ as the timeout value is not modified by select()
            // 2. duration_fromtimeval converts the timeval structure to a Duration
            // 3. This specifies how long select() should block waiting for activity
            let rposix_timeout = match check_and_convert_addr_ext(&cage, arg5, std::mem::size_of::<interface::TimeVal>(), PROT_READ) {
                Ok(addr) => match interface::duration_fromtimeval(addr) {
                    Ok(timeout) => timeout,
                    Err(_) => return syscall_error(Errno::EFAULT, "select", "failed to get timeout"),
                },
                Err(errno) => return syscall_error(errno, "select", "invalid timeout address"),
            };
        
            // Delegate to the cage's select implementation
            // This will:
            // 1. Monitor the specified file descriptors for activity
            // 2. Modify the fd_sets to indicate which descriptors are ready
            // 3. Return the number of ready descriptors or an error code
            cage.select_syscall(nfds, readfds, writefds, errorfds, rposix_timeout)
        }

        RENAME_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert old path from user space
            let old_path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "rename", "invalid old path string"),
                },
                Err(errno) => return syscall_error(errno, "rename", "invalid old path address"),
            };
        
            // Validate and convert new path from user space
            let new_path = match check_and_convert_addr_ext(&cage, arg2, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "rename", "invalid new path string"),
                },
                Err(errno) => return syscall_error(errno, "rename", "invalid new path address"),
            };
            
            // Perform rename operation through cage implementation
            cage.rename_syscall(old_path, new_path)
        }

        XSTAT_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (stat takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "xstat", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "xstat", "invalid path address"),
            };
        
            // Validate stat buffer and prepare for writing
            // Using PROT_WRITE because stat() writes the results TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, std::mem::size_of::<StatData>(), PROT_WRITE) {
                Ok(addr) => match interface::get_statdatastruct(addr) {
                    Ok(val) => val,
                    Err(errno) => return errno,
                },
                Err(errno) => return syscall_error(errno, "xstat", "invalid stat buffer address"),
            };
            
            // Perform stat operation through cage implementation
            // Results written directly to user buffer by cage layer
            cage.stat_syscall(path, buf)
        }

        MKDIR_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (mkdir takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "mkdir", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "mkdir", "invalid path address"),
            };
            let mode = arg2 as u32;
        
            // Perform mkdir operation through cage implementation
            cage.mkdir_syscall(path, mode)
        }

        RMDIR_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);

            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (rmdir takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "rmdir", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "rmdir", "invalid path address"),
            };
            
            // Perform rmdir operation through cage implementation
            cage.rmdir_syscall(path)
        }

        FCHDIR_SYSCALL => {
            let fd = arg1 as i32;
            
            // Perform fchdir operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .fchdir_syscall(fd)
        }

        CHDIR_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (chdir takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "chdir", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "chdir", "invalid path address"),
            };
            
            // Perform chdir operation through cage implementation
            cage.chdir_syscall(path)
        }

        GETCWD_SYSCALL => {
            let bufsize = arg2 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing
            // Using PROT_WRITE because getcwd() writes the current working directory path 
            // TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg1, bufsize, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "getcwd", "invalid buffer address"),
            };
        
            // Perform getcwd operation through cage implementation
            // On success (ret == 0), return the buffer address
            let ret = cage.getcwd_syscall(buf, bufsize as u32);
            if ret == 0 { return arg1 as i32; }
            ret
        }
        FSTATFS_SYSCALL => {
            let fd = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing filesystem information
            // Using PROT_WRITE because fstatfs() writes filesystem information 
            // TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, 1, PROT_WRITE) {
                Ok(addr) => match interface::get_fsdatastruct(addr) {
                    Ok(val) => val,
                    Err(errno) => return errno,
                },
                Err(errno) => return syscall_error(errno, "fstatfs", "invalid buffer address"),
            };
            
            // Perform fstatfs operation through cage implementation
            // File descriptor validation and actual operation handled by cage layer
            cage.fstatfs_syscall(fd, buf)
        }

        CHMOD_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (chmod takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "chmod", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "chmod", "invalid path address"),
            };
            let mode = arg2 as u32;
        
            // Perform chmod operation through cage implementation
            cage.chmod_syscall(path, mode)
        }
        
        DUP_SYSCALL => {
            let fd = arg1 as i32;
            
            // Convert second argument to Option<i32> if it's within valid range
            let fd2: Option<i32> = if arg1 <= i32::MAX as u64 {
                Some(arg1 as i32)
            } else {
                None
            };
        
            // Perform dup operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .dup_syscall(fd, fd2)
        }

        DUP2_SYSCALL => {
            let fd = arg1 as i32;
            let fd2 = arg2 as i32;
            
            // Perform dup2 operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .dup2_syscall(fd, fd2)
        }

        FCHMOD_SYSCALL => {
            let fd = arg1 as i32;
            let mode = arg2 as u32;
        
            // Perform fchmod operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .fchmod_syscall(fd, mode)
        }

        FXSTAT_SYSCALL => {
            let fd = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate stat buffer and prepare for writing
            // Using PROT_WRITE because fstat() writes the results TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, std::mem::size_of::<interface::StatData>(), PROT_WRITE) {
                Ok(addr) => interface::get_statdatastruct(addr).unwrap(),
                Err(errno) => return syscall_error(errno, "fxstat", "invalid buffer address"),
            };
            
            // Perform fstat operation through cage implementation
            // File descriptor validation and actual operation handled by cage layer
            cage.fstat_syscall(fd, buf)
        }
        
        UNLINK_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            // (unlink takes the path as input, we don't write to it)
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "unlink", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "unlink", "invalid path address"),
            };
            
            // Perform unlink operation through cage implementation
            cage.unlink_syscall(path)
        }

        LINK_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert old path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            let old_path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "link", "invalid old path string"),
                },
                Err(errno) => return syscall_error(errno, "link", "invalid old path address"),
            };
            
            // Validate and convert new path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            let new_path = match check_and_convert_addr_ext(&cage, arg2, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "link", "invalid new path string"),
                },
                Err(errno) => return syscall_error(errno, "link", "invalid new path address"),
            };
        
            // Perform link operation through cage implementation
            cage.link_syscall(old_path, new_path)
        }

        LSEEK_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let offset = arg2 as isize;
            let whence = arg3 as i32;
        
            // Perform lseek operation through cage implementation
            // File descriptor validation and bounds checking handled by cage layer
            interface::cagetable_getref(cageid)
                .lseek_syscall(virtual_fd, offset, whence)
        }

        IOCTL_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let request = arg2 as u64;
            let ptrunion = (start_address + arg3) as *mut u8;
            
            // Perform ioctl operation through cage implementation
            // Note: We restrict ioctl operations for security
            interface::cagetable_getref(cageid)
                .ioctl_syscall(virtual_fd, request, ptrunion)
        }

        TRUNCATE_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "truncate", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "truncate", "invalid path address"),
            };
            let length = arg2 as isize;
        
            // Perform truncate operation through cage implementation
            cage.truncate_syscall(path, length)
        }

        FTRUNCATE_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let length = arg2 as isize;
        
            // Perform ftruncate operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .ftruncate_syscall(virtual_fd, length)
        }

        GETDENTS_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let nbytes = arg3 as u32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing directory entries
            // Using PROT_WRITE because getdents() writes directory entries TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, nbytes as usize, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "getdents", "invalid buffer address"),
            };
        
            // Perform getdents operation through cage implementation
            // File descriptor validation handled by cage layer
            cage.getdents_syscall(virtual_fd, buf, nbytes)
        }

        STATFS_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate and convert path string from user space
            // Using PROT_READ because we need to read the path string FROM user space
            let path = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ) {
                Ok(addr) => match interface::types::get_cstr(addr) {
                    Ok(path_str) => path_str,
                    Err(_) => return syscall_error(Errno::EFAULT, "statfs", "invalid path string"),
                },
                Err(errno) => return syscall_error(errno, "statfs", "invalid path address"),
            };
            
            // Validate buffer for writing filesystem information
            // Using PROT_WRITE because statfs() writes filesystem information TO this user space buffer
            let rposix_databuf = match check_and_convert_addr_ext(&cage, arg2, 1, PROT_WRITE) {
                Ok(addr) => match interface::get_fsdatastruct(addr) {
                    Ok(val) => val,
                    Err(errno) => return syscall_error(Errno::EFAULT, "statfs", "invalid stat buffer format"),
                },
                Err(errno) => return syscall_error(errno, "statfs", "invalid stat buffer address"),
            };
            
            // Perform statfs operation through cage implementation
            cage.statfs_syscall(&path, rposix_databuf)
        }

        FCNTL_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let cmd = arg2 as i32;
            let arg = arg3 as i32;
        
            // Perform fcntl operation through cage implementation
            // File descriptor validation and command validation handled by cage layer
            interface::cagetable_getref(cageid)
                .fcntl_syscall(virtual_fd, cmd, arg)
        }

        RECV_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing received data
            // Using PROT_WRITE because recv() writes received data TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "recv", "invalid buffer address"),
            };
            let flag = arg4 as i32;
        
            // Perform recv operation through cage implementation
            // File descriptor validation handled by cage layer
            cage.recv_syscall(fd, buf, count, flag)
        }

        SENDTO_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for reading data to send
            // Using PROT_READ because we need to read the data FROM user space
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_READ) {
                Ok(addr) => addr as *const u8,
                Err(errno) => return syscall_error(errno, "sendto", "invalid buffer address"),
            };
            let flag = arg4 as i32;
        
            // Get and validate socket address
            let addrlen = arg6 as u32;
            let addr = match interface::get_sockaddr(start_address + arg5, addrlen) {
                Ok(addr) => addr,
                Err(_) => return syscall_error(Errno::EFAULT, "sendto", "invalid socket address"),
            };
        
            // Perform sendto operation through cage implementation
            // File descriptor validation handled by cage layer
            cage.sendto_syscall(fd, buf, count, flag, &addr)
        }

        RECVFROM_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing received data
            // Using PROT_WRITE because recvfrom() writes received data TO this user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "recvfrom", "invalid buffer address"),
            };
            let flag = arg4 as i32;
        
            // Check if address and length arguments are provided
            let nullity1 = interface::arg_nullity(arg5);
            let nullity2 = interface::arg_nullity(arg6);
        
            // Handle different cases based on address arguments
            if nullity1 && nullity2 {
                // Both address and length are NULL - simple receive
                cage.recvfrom_syscall(fd, buf, count, flag, &mut None)
            }
            else if !(nullity1 || nullity2) {
                // Both address and length are provided
                // Create a default sockaddr to store the sender's address
                let mut newsockaddr = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        
                // Perform recvfrom operation
                let rv = cage.recvfrom_syscall(fd, buf, count, flag, &mut Some(&mut newsockaddr));
                if rv >= 0 {
                    // Copy address information back to user space on success
                    interface::copy_out_sockaddr(start_address + arg5, start_address + arg6, newsockaddr);
                }
                rv
            } else {
                // Invalid case: one argument is NULL while the other isn't
                syscall_error(
                    Errno::EINVAL,
                    "recvfrom",
                    "exactly one of the last two arguments was zero",
                )
            }
        }

        FLOCK_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let operation = arg2 as i32;
        
            // Perform flock operation through cage implementation
            // File descriptor validation handled by cage layer
            interface::cagetable_getref(cageid)
                .flock_syscall(virtual_fd, operation)
        }
        
        SHMGET_SYSCALL => {
            let key = arg1 as i32;
            let size = arg2 as usize;
            let shmfig = arg3 as i32;
        
            // Perform shmget operation through cage implementation
            interface::cagetable_getref(cageid)
                .shmget_syscall(key, size, shmfig)
        }
        SHMAT_SYSCALL => {
            let shmid = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate shared memory address
            // Using both PROT_READ and PROT_WRITE since shared memory needs both access types
            let shmaddr = match check_and_convert_addr_ext(&cage, arg2, 1, PROT_READ | PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "shmat", "invalid shared memory address"),
            };
            let shmflg = arg3 as i32;
        
            // Perform shmat operation through cage implementation
            cage.shmat_syscall(shmid, shmaddr, shmflg)
        }

        SHMDT_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate shared memory address
            // Using both PROT_READ and PROT_WRITE since shared memory needs both access types
            let shmaddr = match check_and_convert_addr_ext(&cage, arg1, 1, PROT_READ | PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "shmdt", "invalid shared memory address"),
            };
            
            // Perform shmdt operation through cage implementation
            cage.shmdt_syscall(shmaddr)
        }

        MUTEX_DESTROY_SYSCALL => {
            let mutex_handle = arg1 as i32;
        
            // Perform mutex destroy operation through cage implementation
            interface::cagetable_getref(cageid)
                .mutex_destroy_syscall(mutex_handle)
        }

        MUTEX_LOCK_SYSCALL => {
            let mutex_handle = arg1 as i32;
        
            // Perform mutex lock operation through cage implementation
            interface::cagetable_getref(cageid)
                .mutex_lock_syscall(mutex_handle)
        }

        MUTEX_TRYLOCK_SYSCALL => {
            let mutex_handle = arg1 as i32;
        
            // Perform mutex trylock operation through cage implementation
            interface::cagetable_getref(cageid)
                .mutex_trylock_syscall(mutex_handle)
        }

        MUTEX_UNLOCK_SYSCALL => {
            let mutex_handle = arg1 as i32;
        
            // Perform mutex unlock operation through cage implementation
            interface::cagetable_getref(cageid)
                .mutex_unlock_syscall(mutex_handle)
        }

        COND_DESTROY_SYSCALL => {
            let cv_handle = arg1 as i32;
        
            // Perform condition variable destroy operation through cage implementation
            interface::cagetable_getref(cageid)
                .cond_destroy_syscall(cv_handle)
        }

        COND_WAIT_SYSCALL => {
            let cv_handle = arg1 as i32;
            let mutex_handle = arg2 as i32;
            
            // Perform condition variable wait operation through cage implementation
            interface::cagetable_getref(cageid)
                .cond_wait_syscall(cv_handle, mutex_handle)
        }

        COND_BROADCAST_SYSCALL => {
            let cv_handle = arg1 as i32;
        
            // Perform condition variable broadcast operation through cage implementation
            interface::cagetable_getref(cageid)
                .cond_broadcast_syscall(cv_handle)
        }

        COND_SIGNAL_SYSCALL => {
            let cv_handle = arg1 as i32;
        
            // Perform condition variable signal operation through cage implementation
            interface::cagetable_getref(cageid)
                .cond_signal_syscall(cv_handle)
        }

        SEM_INIT_SYSCALL => {
            let sem_handle = arg1 as u32;
            let pshared = arg2 as i32;
            let value = arg3 as u32;
        
            // Perform semaphore initialization operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_init_syscall(sem_handle, pshared, value)
        }

        SEM_WAIT_SYSCALL => {
            let sem_handle = arg1 as u32;
        
            // Perform semaphore wait operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_wait_syscall(sem_handle)
        }

        SEM_TRYWAIT_SYSCALL => {
            let sem_handle = arg1 as u32;
        
            // Perform semaphore try wait operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_trywait_syscall(sem_handle)
        }

        SEM_POST_SYSCALL => {
            let sem_handle = arg1 as u32;
        
            // Perform semaphore post operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_post_syscall(sem_handle)
        }

        SEM_DESTROY_SYSCALL => {
            let sem_handle = arg1 as u32;
        
            // Perform semaphore destroy operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_destroy_syscall(sem_handle)
        }

        SEM_GETVALUE_SYSCALL => {
            let sem_handle = arg1 as u32;
        
            // Perform semaphore get value operation through cage implementation
            interface::cagetable_getref(cageid)
                .sem_getvalue_syscall(sem_handle)
        }

        PWRITE_SYSCALL => {
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for reading data to write
            // Using PROT_READ because we need to read the data FROM user space
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_READ) {
                Ok(addr) => addr as *const u8,
                Err(errno) => return syscall_error(errno, "pwrite", "invalid buffer address"),
            };
            let virtual_fd = arg1 as i32;
            let offset = arg4 as i64;
        
            cage.pwrite_syscall(virtual_fd, buf, count, offset)
        }

        GETUID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .getuid_syscall()
        }

        GETEUID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .geteuid_syscall()
        }

        GETGID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .getgid_syscall()
        }

        GETEGID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .getegid_syscall()
        }

        EPOLL_CREATE_SYSCALL => {
            let size = arg1 as i32;
            
            // Perform epoll create operation through cage implementation
            interface::cagetable_getref(cageid)
                .epoll_create_syscall(size)
        }

        EPOLL_CTL_SYSCALL=> {
            let virtual_epfd = arg1 as i32;
            let op = arg2 as i32;
            let virtual_fd = arg3 as i32;
            
            // Validate and convert epoll_event structure
            let epollevent = interface::get_epollevent(arg4).unwrap();
        
            // Perform epoll_ctl operation through cage implementation
            interface::cagetable_getref(cageid)
                .epoll_ctl_syscall(virtual_epfd, op, virtual_fd, epollevent)
        }

        EPOLL_WAIT_SYSCALL => {
            let virtual_epfd = arg1 as i32;
            let maxevents = arg3 as i32;
            let events = interface::get_epollevent_slice(arg2, maxevents).unwrap();
            let timeout = arg4 as i32;
            interface::cagetable_getref(cageid)
                .epoll_wait_syscall(virtual_epfd, events, maxevents, timeout)
        }


        SETSOCKOPT_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let level = arg2 as i32;
            let optname = arg3 as i32;
            let optlen = arg5 as u32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for reading socket option value
            // Using PROT_READ because we need to read the option value FROM user space
            let optval = match check_and_convert_addr_ext(&cage, arg4, optlen as usize, PROT_READ) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "setsockopt", "invalid optval address"),
            };
            
            // Perform setsockopt operation through cage implementation
            cage.setsockopt_syscall(virtual_fd, level, optname, optval, optlen)
        }

        SHUTDOWN_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let how = arg2 as i32;
            
            // Perform shutdown operation through cage implementation
            interface::cagetable_getref(cageid)
                .shutdown_syscall(virtual_fd, how)
        }

        GETPPID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .getppid_syscall()
        }

        SEND_SYSCALL => {
            let fd = arg1 as i32;
            let count = arg3 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for reading data to send
            // Using PROT_READ because we need to read the data FROM user space
            let buf = match check_and_convert_addr_ext(&cage, arg2, count, PROT_READ) {
                Ok(addr) => addr as *const u8,
                Err(errno) => return syscall_error(errno, "send", "invalid buffer address"),
            };
            let flags = arg4 as i32;
        
            cage.send_syscall(fd, buf, count, flags)
        }

        LISTEN_SYSCALL  => {
            let virtual_fd = arg1 as i32;
            let backlog = arg2 as i32;
            interface::cagetable_getref(cageid)
                .listen_syscall(virtual_fd, backlog)
        }

        MUTEX_CREATE_SYSCALL => {
            interface::cagetable_getref(cageid)
                .mutex_create_syscall()
        }

        COND_CREATE_SYSCALL => {
            interface::cagetable_getref(cageid)
                .cond_create_syscall()
        } 

        GETHOSTNAME_SYSCALL => {
            let len = arg2 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing hostname
            // Using PROT_WRITE because gethostname() writes hostname TO this user space buffer
            let name = match check_and_convert_addr_ext(&cage, arg1, len, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "gethostname", "invalid name address"),
            };
        
            // Perform gethostname operation through cage implementation
            cage.gethostname_syscall(name, len as isize)
        }

        GETIFADDRS_SYSCALL => {
            let count = arg2 as usize;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing interface addresses
            // Using PROT_WRITE because getifaddrs() writes interface data TO user space buffer
            let buf = match check_and_convert_addr_ext(&cage, arg1, count, PROT_WRITE) {
                Ok(addr) => addr as *mut u8,
                Err(errno) => return syscall_error(errno, "getifaddrs", "invalid address"),
            };
        
            // Perform getifaddrs operation through cage implementation
            cage.getifaddrs_syscall(buf, count)
        }

        KILL_SYSCALL => {
            let cage_id = arg1 as i32;
            let sig = arg2 as i32;
            interface::cagetable_getref(cageid)
                .kill_syscall(cage_id, sig)
        } 

        FSYNC_SYSCALL => {
            let virtual_fd = arg1 as i32;

            interface::cagetable_getref(cageid)
                .fsync_syscall(virtual_fd)
        } 

        FDATASYNC_SYSCALL => {
            let virtual_fd = arg1 as i32;

            interface::cagetable_getref(cageid)
                .fdatasync_syscall(virtual_fd)
        } 

        SYNC_FILE_RANGE => {
            let virtual_fd = arg1 as i32;
            let offset = arg2 as isize;
            let nbytes = arg3 as isize;
            let flags = arg4 as u32;

            interface::cagetable_getref(cageid)
                .sync_file_range_syscall(virtual_fd, offset, nbytes, flags)
        } 

        PIPE_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing pipe file descriptors
            // Using PROT_WRITE because pipe() writes two fds TO this user space buffer
            // Size is 8 bytes for two integers (file descriptors)
            let pipe = match check_and_convert_addr_ext(&cage, arg1, 8, PROT_WRITE) {
                Ok(addr) => match interface::get_pipearray(addr) {
                    Ok(pipe_array) => pipe_array,
                    Err(_) => return syscall_error(Errno::EFAULT, "pipe", "failed to get pipe array"),
                },
                Err(errno) => return syscall_error(errno, "pipe", "invalid pipe address"),
            };
        
            cage.pipe_syscall(pipe)
        }

        PIPE2_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing pipe file descriptors
            // Using PROT_WRITE because pipe2() writes two fds TO this user space buffer
            // Size is 8 bytes for two integers (file descriptors)
            let pipe = match check_and_convert_addr_ext(&cage, arg1, 8, PROT_WRITE) {
                Ok(addr) => match interface::get_pipearray(addr) {
                    Ok(pipe_array) => pipe_array,
                    Err(_) => return syscall_error(Errno::EFAULT, "pipe2", "failed to get pipe array"),
                },
                Err(errno) => return syscall_error(errno, "pipe2", "invalid pipe address"),
            };
            let flag = arg2 as i32;
        
            cage.pipe2_syscall(pipe, flag)
        }
        
        GETSOCKNAME_SYSCALL => {
            let fd = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing socket address
            // Using PROT_WRITE because getsockname() writes address TO user space
            let name_addr = match check_and_convert_addr_ext(&cage, arg2, 16, PROT_WRITE) {
                Ok(addr) => addr,
                Err(errno) => return syscall_error(errno, "getsockname", "invalid name address"),
            };
        
            // Validate buffer for writing address length
            // Using PROT_WRITE because getsockname() writes length TO user space
            let namelen_addr = match check_and_convert_addr_ext(&cage, arg3, 4, PROT_WRITE) {
                Ok(addr) => addr,
                Err(errno) => return syscall_error(errno, "getsockname", "invalid length address"),
            };
        
            // Initialize default socket address structure
            let mut addr = interface::GenSockaddr::V4(interface::SockaddrV4::default());
        
            // Check for null pointers
            if interface::arg_nullity(arg2) || interface::arg_nullity(arg3) {
                return syscall_error(
                    Errno::EINVAL,
                    "getsockname",
                    "Either the address or the length were null",
                );
            }
        
            let rv = cage.getsockname_syscall(fd, &mut Some(&mut addr));
        
            // Copy out the address if operation was successful
            if rv >= 0 {
                interface::copy_out_sockaddr(name_addr, namelen_addr, addr);
            }
            rv
        }

        GETSOCKOPT_SYSCALL => {
            let virtual_fd = arg1 as i32;
            let level = arg2 as i32;
            let optname = arg3 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing socket option value
            // Using PROT_WRITE because getsockopt() writes option value TO user space
            // Size is 4 bytes for an integer value
            let optval_ptr = match check_and_convert_addr_ext(&cage, arg4, 4, PROT_WRITE) {
                Ok(addr) => addr as *mut i32,
                Err(errno) => return syscall_error(errno, "getsockopt", "invalid optval address"),
            };
            let optval = unsafe { &mut *optval_ptr };
        
            cage.getsockopt_syscall(virtual_fd, level, optname, optval)
        }

        SOCKETPAIR_SYSCALL => {
            let domain = arg1 as i32;
            let _type = arg2 as i32;
            let protocol = arg3 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing socket pair file descriptors
            // Using PROT_WRITE because socketpair() writes two fds TO this user space buffer
            // Size is 8 bytes for two integers (file descriptors)
            let virtual_socket_vector = match check_and_convert_addr_ext(&cage, arg4, 8, PROT_WRITE) {
                Ok(addr) => match interface::get_sockpair(addr) {
                    Ok(sock_pair) => sock_pair,
                    Err(_) => return syscall_error(Errno::EFAULT, "socketpair", "failed to get socket pair array"),
                },
                Err(errno) => return syscall_error(errno, "socketpair", "invalid socket vector address"),
            };
        
            cage.socketpair_syscall(domain, _type, protocol, virtual_socket_vector)
        }

        POLL_SYSCALL => {
            let nfds = arg2 as u64;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for reading and writing poll file descriptors
            // Using PROT_READ | PROT_WRITE because:
            // - READ: poll needs to read the fds and events to monitor
            // - WRITE: poll needs to write back the returned events
            // Size is nfds * 8 (size of pollfd struct)
            let pollfds = match check_and_convert_addr_ext(&cage, arg1, (nfds * 8) as usize, PROT_READ | PROT_WRITE) {
                Ok(addr) => match interface::get_pollstruct_slice(addr, nfds as usize) {
                    Ok(poll_array) => poll_array,
                    Err(_) => return syscall_error(Errno::EFAULT, "poll", "failed to get poll array"),
                },
                Err(errno) => return syscall_error(errno, "poll", "invalid fds address"),
            };
            let timeout = arg3 as i32;
        
            cage.poll_syscall(pollfds, nfds, timeout)
        }

        GETPID_SYSCALL => {
            interface::cagetable_getref(cageid)
                .getpid_syscall()
        }

        FORK_SYSCALL => {
            let id = arg1 as u64;
            interface::cagetable_getref(cageid)
                .fork_syscall(id)
        }

        FUTEX_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for futex operations
            // Using PROT_READ | PROT_WRITE because:
            // - READ: futex needs to read current value
            // - WRITE: futex may modify the value depending on operation
            let uaddr = match check_and_convert_addr_ext(&cage, arg1, 4, PROT_READ | PROT_WRITE) {
                Ok(addr) => addr,
                Err(errno) => return syscall_error(errno, "futex", "invalid uaddr address"),
            };
            
            // Convert remaining arguments
            let futex_op = arg2 as u32;
            let val = arg3 as u32;
            let timeout = arg4 as u32;
            let uaddr2 = arg5 as u32;
            let val3 = arg6 as u32;
        
            cage.futex_syscall(uaddr, futex_op, val, timeout, uaddr2, val3)
        }

        NANOSLEEP_TIME64_SYSCALL => {
            let clockid = arg1 as u32;
            let flags = arg2 as i32;
            let cage = interface::cagetable_getref(cageid);
            
            // Validate buffer for reading requested sleep time
            // Using PROT_READ because we need to read the requested time FROM user space
            // Size is 16 bytes for timespec64 structure
            let req = match check_and_convert_addr_ext(&cage, arg3, 16, PROT_READ) {
                Ok(addr) => addr as usize,
                Err(errno) => return syscall_error(errno, "nanosleep", "invalid req address"),
            };
        
            // Validate buffer for writing remaining time
            // Using PROT_WRITE because nanosleep writes remaining time TO user space
            // Size is 16 bytes for timespec64 structure
            let rem = match check_and_convert_addr_ext(&cage, arg4, 16, PROT_WRITE) {
                Ok(addr) => addr as usize, 
                Err(errno) => return syscall_error(errno, "nanosleep", "invalid rem address"),
            };
            
            cage.nanosleep_time64_syscall(clockid, flags, req, rem)
        }

        WAIT_SYSCALL => {
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing process status
            // Using PROT_WRITE because wait() writes status information TO user space
            // Size is 4 bytes for status integer
            let status = match check_and_convert_addr_ext(&cage, arg1, 4, PROT_WRITE) {
                Ok(addr) => match interface::get_i32_ref(addr) {
                    Ok(status_ref) => status_ref,
                    Err(_) => return syscall_error(Errno::EFAULT, "wait", "failed to get status reference"),
                },
                Err(errno) => return syscall_error(errno, "wait", "invalid status address"),
            };
        
            cage.wait_syscall(status)
        }

        WAITPID_SYSCALL => {
            let pid = arg1 as i32;
            let cage = interface::cagetable_getref(cageid);
        
            // Validate buffer for writing process status
            // Using PROT_WRITE because waitpid() writes status information TO user space
            // Size is 4 bytes for status integer
            let status = match check_and_convert_addr_ext(&cage, arg2, 4, PROT_WRITE) {
                Ok(addr) => match interface::get_i32_ref(addr) {
                    Ok(status_ref) => status_ref,
                    Err(_) => return syscall_error(Errno::EFAULT, "waitpid", "failed to get status reference"),
                },
                Err(errno) => return syscall_error(errno, "waitpid", "invalid status address"), 
            };
            let options = arg3 as i32;
            
            cage.waitpid_syscall(pid, status, options)
        }


        SBRK_SYSCALL => {
            let brk = arg1 as u32;

            interface::sbrk_handler(cageid, brk)
        }

        _ => -1, // Return -1 for unknown syscalls
    };
    ret
}

// initilize the vmmap, invoked by wasmtime
pub fn lind_cage_vmmap_init(cageid: u64) {
    let cage = interface::cagetable_getref(cageid);
    let mut vmmap = cage.vmmap.write();
    vmmap.add_entry(VmmapEntry::new(0, 0x30, PROT_WRITE | PROT_READ, 0 /* not sure about this field */, (MAP_PRIVATE | MAP_ANONYMOUS) as i32, false, 0, 0, cageid, MemoryBackingType::Anonymous));
    // BUG: currently need to insert an entry at the end to indicate the end of memory space. This should be fixed soon so that
    //      no dummy entries are required to be inserted
    vmmap.add_entry(VmmapEntry::new(1 << 18, 1, PROT_NONE, 0 /* not sure about this field */, (MAP_PRIVATE | MAP_ANONYMOUS) as i32, false, 0, 0, cageid, MemoryBackingType::Anonymous));
}

// set the wasm linear memory base address to vmmap
pub fn set_base_address(cageid: u64, base_address: i64) {
    let cage = interface::cagetable_getref(cageid);
    let mut vmmap = cage.vmmap.write();
    vmmap.set_base_address(base_address);
}

// clone the cage memory. Invoked by wasmtime after cage is forked
pub fn fork_vmmap_helper(parent_cageid: u64, child_cageid: u64) {
    let parent_cage = interface::cagetable_getref(parent_cageid);
    let child_cage = interface::cagetable_getref(child_cageid);
    let parent_vmmap = parent_cage.vmmap.read();
    let child_vmmap = child_cage.vmmap.read();

    interface::fork_vmmap(&parent_vmmap, &child_vmmap);
}

#[no_mangle]
pub fn lindcancelinit(cageid: u64) {
    let cage = interface::cagetable_getref(cageid);
    cage.cancelstatus
        .store(true, interface::RustAtomicOrdering::Relaxed);
    cage.signalcvs();
}

#[no_mangle]
pub fn lindsetthreadkill(cageid: u64, pthreadid: u64, kill: bool) {
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
pub fn lindcheckthread(cageid: u64, pthreadid: u64) -> bool {
    interface::check_thread(cageid, pthreadid)
}

#[no_mangle]
pub fn lindthreadremove(cageid: u64, pthreadid: u64) {
    let cage = interface::cagetable_getref(cageid);
    cage.thread_table.remove(&pthreadid);
}

#[no_mangle]
pub fn lindgetsighandler(cageid: u64, signo: i32) -> u32 {
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
pub fn lindrustinit(verbosity: isize) {
    let _ = interface::VERBOSE.set(verbosity); //assigned to suppress unused result warning
    interface::cagetable_init();

    // TODO: needs to add close() that handling im-pipe
    fdtables::register_close_handlers(FDKIND_KERNEL, fdtables::NULL_FUNC, kernel_close);
    
    let utilcage = Cage {
        cageid: 0,
        cwd: interface::RustLock::new(interface::RustRfc::new(interface::RustPathBuf::from("/"))),
        parent: 0,
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
        main_threadid: interface::RustAtomicU64::new(0),
        interval_timer: interface::IntervalTimer::new(0),
        vmmap: interface::RustLock::new(Vmmap::new()), // Initialize empty virtual memory map for new process
        zombies: interface::RustLock::new(vec![]),
        child_num: interface::RustAtomicU64::new(0),
    };

    interface::cagetable_insert(0, utilcage);
    fdtables::init_empty_cage(0);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // STDIN
    let dev_null = CString::new("/home/lind/lind_project/src/safeposix-rust/tmp/dev/null").unwrap();
    unsafe {
        libc::open(dev_null.as_ptr(), O_RDONLY);
        libc::open(dev_null.as_ptr(), O_WRONLY);
        libc::dup(1);
    }
    
    fdtables::get_specific_virtual_fd(0, 0, FDKIND_KERNEL, 0, false, 0).unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(0, 1, FDKIND_KERNEL, 1, false, 0).unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(0, 2, FDKIND_KERNEL, 2, false, 0).unwrap();

    //init cage is its own parent
    let initcage = Cage {
        cageid: 1,
        cwd: interface::RustLock::new(interface::RustRfc::new(interface::RustPathBuf::from("/"))),
        parent: 1,
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
        main_threadid: interface::RustAtomicU64::new(0),
        interval_timer: interface::IntervalTimer::new(1),
        vmmap: interface::RustLock::new(Vmmap::new()), // Initialize empty virtual memory map for new process
        zombies: interface::RustLock::new(vec![]),
        child_num: interface::RustAtomicU64::new(0),
    };
    interface::cagetable_insert(1, initcage);
    fdtables::init_empty_cage(1);
    // Set the first 3 fd to STDIN / STDOUT / STDERR
    // STDIN
    fdtables::get_specific_virtual_fd(1, 0, FDKIND_KERNEL, 0, false, 0).unwrap();
    // STDOUT
    fdtables::get_specific_virtual_fd(1, 1, FDKIND_KERNEL, 1, false, 0).unwrap();
    // STDERR
    fdtables::get_specific_virtual_fd(1, 2, FDKIND_KERNEL, 2, false, 0).unwrap();

}

#[no_mangle]
pub fn lindrustfinalize() {
    interface::cagetable_clear();
}
