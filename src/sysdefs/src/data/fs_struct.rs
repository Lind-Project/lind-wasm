#![allow(dead_code)]
/// This file is used for type conversion related files
use crate::constants::err_const::{syscall_error, Errno};
pub use libc::*;
pub use std::cmp::{max, min};
pub use std::str::Utf8Error;
pub use std::time::Duration;

//redefining the FSData struct in this file so that we maintain flow of program
//derive eq attributes for testing whether the structs equal other fsdata structs from stat/fstat
#[derive(Eq, PartialEq)]
#[repr(C)]
pub struct FSData {
    pub f_type: u64,
    pub f_bsize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    //total files in the file system -- should be infinite
    pub f_files: u64,
    //free files in the file system -- should be infinite
    pub f_ffiles: u64,
    pub f_fsid: u64,
    //not really a limit for naming, but 254 works
    pub f_namelen: u64,
    //arbitrary val for blocksize as well
    pub f_frsize: u64,
    pub f_spare: [u8; 32],
}

/// Mirrors glibc's native `struct statfs64` layout byte-for-byte (120 bytes total,
/// including `f_flags`), as used by cages running under the MPK runtime -- unlike
/// Wasm cages, MPK cages run native code and expect the host's real statfs64 ABI
/// rather than the repacked `FSData` layout used for Wasm linear memory.
#[derive(Eq, PartialEq, Default, Debug)]
#[repr(C)]
pub struct Statfs64Data {
    pub f_type: i64,
    pub f_bsize: i64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: (i32, i32),
    pub f_namelen: i64,
    pub f_frsize: i64,
    pub f_flags: i64,
    pub f_spare: [i64; 4],
}

//redefining the StatData struct in this file so that we maintain flow of program
//derive eq attributes for testing whether the structs equal other statdata structs from stat/fstat
#[derive(Eq, PartialEq, Default, Debug)]
#[repr(C)]
pub struct StatData {
    pub st_dev: u64,
    pub st_ino: usize,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub st_size: usize,
    pub st_blksize: i32,
    pub st_blocks: u32,
    //currently we don't populate or care about the time bits here
    pub st_atim: (u64, u64),
    pub st_mtim: (u64, u64),
    pub st_ctim: (u64, u64),
}

/// Mirrors glibc's native `struct stat64` layout byte-for-byte (144 bytes total,
/// including padding), as used by cages running under the MPK runtime -- unlike
/// Wasm cages, MPK cages run native code and expect the host's real stat64 ABI
/// rather than the repacked `StatData` layout used for Wasm linear memory.
#[derive(Eq, PartialEq, Default, Debug)]
#[repr(C)]
pub struct Stat64Data {
    pub st_dev: u64,
    pub st_ino: u64,
    pub st_nlink: u64,
    pub st_mode: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub __pad0: i32,
    pub st_rdev: u64,
    pub st_size: i64,
    pub st_blksize: i64,
    pub st_blocks: i64,
    pub st_atim: (u64, u64),
    pub st_mtim: (u64, u64),
    pub st_ctim: (u64, u64),
    pub __glibc_reserved: [i64; 3],
}

/// Resource limit layout used by Wasm cages (32-bit `rlim_t` fields).
#[repr(C)]
pub struct Rlimit {
    pub rlim_cur: u32,
    pub rlim_max: u32,
}

/// Native MPK resource limit layout (64-bit unsigned long `rlim_t` fields).
#[repr(C)]
pub struct Rlimit64 {
    pub rlim_cur: u64,
    pub rlim_max: u64,
}

#[derive(Eq, PartialEq, Default, Copy, Clone, Debug)]
#[repr(C)]
pub struct PipeArray {
    pub readfd: i32,
    pub writefd: i32,
}

#[derive(Eq, PartialEq, Default, Copy, Clone, Debug)]
#[repr(C)]
pub struct SockPair {
    pub sock1: i32,
    pub sock2: i32,
}

//EPOLL
#[derive(Copy, Clone, Debug)]
#[repr(C)]
pub struct EpollEvent {
    pub events: u32,
    pub fd: i32, //in native this is a union which could be one of a number of things
                 //however, we only support EPOLL_CTL subcommands which take the fd
}

#[repr(C)]
pub struct TimeVal {
    pub tv_sec: i64,
    pub tv_usec: i64,
}

#[repr(C)]
pub struct ITimerVal {
    pub it_interval: TimeVal,
    pub it_value: TimeVal,
}

#[repr(C)]
pub struct TimeSpec {
    pub tv_sec: i64,
    pub tv_nsec: i64,
}

#[derive(Copy, Clone)]
#[repr(C)]
pub union IoctlPtrUnion {
    pub int_ptr: *mut i32,
    pub c_char_ptr: *mut u8, //Right now, we do not support passing struct pointers to ioctl as the related call are not implemented
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct IpcPermStruct {
    pub __key: i32,
    pub uid: u32,
    pub gid: u32,
    pub cuid: u32,
    pub cgid: u32,
    pub mode: u16,
    pub __pad1: u16,
    pub __seq: u16,
    pub __pad2: u16,
    pub __unused1: u32,
    pub __unused2: u32,
}

#[derive(Copy, Clone, Default)]
#[repr(C)]
pub struct ShmidsStruct {
    pub shm_perm: IpcPermStruct,
    pub shm_segsz: u32,
    pub shm_atime: isize,
    pub shm_dtime: isize,
    pub shm_ctime: isize,
    pub shm_cpid: u32,
    pub shm_lpid: u32,
    pub shm_nattch: u32,
}

pub type SigsetType = u64;

pub type IovecStruct = libc::iovec;

#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct SigactionStruct {
    pub sa_handler: u64,
    pub sa_mask: SigsetType,
    pub sa_flags: i32,
}

/// Mirrors Linux x86_64 `struct kernel_sigaction` used by native MPK runtime.
#[derive(Copy, Clone, Debug, Default)]
#[repr(C)]
pub struct KernelSigactionStruct {
    pub k_sa_handler: u64,
    pub sa_flags: u64,
    pub sa_restorer: u64,
    pub sa_mask: SigsetType,
}

use std::mem::size_of;

// Represents a Dirent struct without the string, as rust has no flexible array member support
#[repr(C, packed(1))]
pub struct ClippedDirent {
    pub d_ino: u64,
    pub d_off: u64,
    pub d_reclen: u16,
}

pub const CLIPPED_DIRENT_SIZE: u32 = size_of::<ClippedDirent>() as u32;
