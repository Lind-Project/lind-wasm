// File system related constants
#![allow(dead_code)]
#![allow(unused_variables)]

use crate::interface;

// Define constants using static or const
// Imported into fs_calls file
pub const DT_UNKNOWN: u8 = 0;

pub const STARTINGFD: i32 = 0;
pub const MAXFD: i32 = 1024;
pub const STARTINGPIPE: i32 = 0;
pub const MAXPIPE: i32 = 1024;

pub const ROOTDIRECTORYINODE: usize = 1;
pub const STREAMINODE: usize = 2;

pub const PIPE_CAPACITY: usize = 65536;

pub const F_OK: u32 = 0;
pub const X_OK: u32 = 1;
pub const W_OK: u32 = 2;
pub const R_OK: u32 = 4;

pub const O_RDONLY: i32 = 0o0;
pub const O_WRONLY: i32 = 0o1;
pub const O_RDWR: i32 = 0o2;
pub const O_RDWRFLAGS: i32 = 0o3;

pub const O_CREAT: i32 = 0o100;
pub const O_EXCL: i32 = 0o200;
pub const O_NOCTTY: i32 = 0o400;
pub const O_TRUNC: i32 = 0o1000;
pub const O_APPEND: i32 = 0o2000;
pub const O_NONBLOCK: i32 = 0o4000;
// O_NDELAY=O_NONBLOCK
pub const O_SYNC: i32 = 0o10000;
// O_FSYNC=O_SYNC
pub const O_ASYNC: i32 = 0o20000;
pub const O_CLOEXEC: i32 = 0o2000000;

pub const DEFAULTTIME: u64 = 1323630836;

//Standard flag combinations
pub const S_IRWXA: u32 = 0o777;
pub const S_IRWXU: u32 = 0o700;
pub const S_IRUSR: u32 = 0o400;
pub const S_IWUSR: u32 = 0o200;
pub const S_IXUSR: u32 = 0o100;
pub const S_IRWXG: u32 = 0o070;
pub const S_IRGRP: u32 = 0o040;
pub const S_IWGRP: u32 = 0o020;
pub const S_IXGRP: u32 = 0o010;
pub const S_IRWXO: u32 = 0o007;
pub const S_IROTH: u32 = 0o004;
pub const S_IWOTH: u32 = 0o002;
pub const S_IXOTH: u32 = 0o001;

//Commands for FCNTL
pub const F_DUPFD: i32 = 0;
pub const F_GETFD: i32 = 1;
pub const F_SETFD: i32 = 2;
pub const F_GETFL: i32 = 3;
pub const F_SETFL: i32 = 4;
pub const F_GETLK: i32 = 5;
pub const F_GETLK64: i32 = 5;
pub const F_SETLK: i32 = 6;
pub const F_SETLK64: i32 = 6;
pub const F_SETLKW: i32 = 7;
pub const F_SETLKW64: i32 = 7;
pub const F_SETOWN: i32 = 8;
pub const F_GETOWN: i32 = 9;
pub const F_SETSIG: i32 = 10;
pub const F_GETSIG: i32 = 11;
pub const F_SETLEASE: i32 = 1024;
pub const F_GETLEASE: i32 = 1025;
pub const F_NOTIFY: i32 = 1026;

//Commands for IOCTL
pub const FIONBIO: u32 = 21537;
pub const FIOASYNC: u32 = 21586;

//File types for open/stat etc.
pub const S_IFBLK: i32 = 0o60000;
pub const S_IFCHR: i32 = 0o20000;
pub const S_IFDIR: i32 = 0o40000;
pub const S_IFIFO: i32 = 0o10000;
pub const S_IFLNK: i32 = 0o120000;
pub const S_IFREG: i32 = 0o100000;
pub const S_IFSOCK: i32 = 0o140000;
pub const S_FILETYPEFLAGS: i32 = 0o170000;

//for flock syscall
pub const LOCK_SH: i32 = 1;
pub const LOCK_EX: i32 = 2;
pub const LOCK_UN: i32 = 8;
pub const LOCK_NB: i32 = 4;
//for mmap/munmap syscall
pub const MAP_SHARED: i32 = 1;
pub const MAP_PRIVATE: i32 = 2;
pub const MAP_FIXED: i32 = 16;
pub const MAP_ANONYMOUS: i32 = 32;
pub const MAP_HUGE_SHIFT: i32 = 26;
pub const MAP_HUGETLB: i32 = 262144;

pub const PROT_NONE: i32 = 0;
pub const PROT_READ: i32 = 1;
pub const PROT_WRITE: i32 = 2;
pub const PROT_EXEC: i32 = 4;

pub const SEEK_SET: i32 = 0;
pub const SEEK_CUR: i32 = 1;
pub const SEEK_END: i32 = 2;

pub const IPC_PRIVATE: i32 = 0o0;
pub const IPC_CREAT: i32 = 0o1000;
pub const IPC_EXCL: i32 = 0o2000;

pub const IPC_RMID: i32 = 0;
pub const IPC_SET: i32 = 1;
pub const IPC_STAT: i32 = 2;

pub const SHM_DEST: i32 = 0o1000;
pub const SHM_LOCKED: i32 = 0o2000;
pub const SHM_HUGETLB: i32 = 0o4000;

pub const SHM_R: i32 = 0o400;
pub const SHM_W: i32 = 0o200;
pub const SHM_RDONLY: i32 = 0o10000;
pub const SHM_RND: i32 = 0o20000;
pub const SHM_REMAP: i32 = 0o40000;
pub const SHM_EXEC: i32 = 0o100000;

pub const SHMMIN: u32 = 1;
pub const SHMMNI: u32 = 4096;
pub const SHMMAX: u32 = 4278190079; // (ULONG_MAX - (1UL << 24))
pub const SHMALL: u32 = 4278190079; // (ULONG_MAX - (1UL << 24));
pub const SHMSEG: u32 = SHMMNI;

pub const SEM_VALUE_MAX: u32 = 2147483647;

//device info for char files
#[derive(interface::SerdeSerialize, interface::SerdeDeserialize, PartialEq, Eq, Debug)]
pub struct DevNo {
    pub major: u32,
    pub minor: u32,
}
pub const NULLDEVNO: DevNo = DevNo { major: 1, minor: 3 };
pub const ZERODEVNO: DevNo = DevNo { major: 1, minor: 5 };
pub const RANDOMDEVNO: DevNo = DevNo { major: 1, minor: 8 };
pub const URANDOMDEVNO: DevNo = DevNo { major: 1, minor: 9 };

pub const FILEDATAPREFIX: &str = "linddata.";

pub fn is_reg(mode: u32) -> bool {
    (mode as i32 & S_FILETYPEFLAGS) == S_IFREG
}

pub fn is_chr(mode: u32) -> bool {
    (mode as i32 & S_FILETYPEFLAGS) == S_IFCHR
}

pub fn is_dir(mode: u32) -> bool {
    (mode as i32 & S_FILETYPEFLAGS) == S_IFDIR
}

pub fn is_wronly(flags: i32) -> bool {
    (flags & O_RDWRFLAGS) == O_WRONLY
}
pub fn is_rdonly(flags: i32) -> bool {
    (flags & O_RDWRFLAGS) == O_RDONLY
}

//the same as the glibc makedev
pub fn makedev(dev: &DevNo) -> u64 {
    ((dev.major as u64 & 0x00000fff) << 8)
        | ((dev.major as u64 & 0xfffff000) << 32)
        | ((dev.minor as u64 & 0x000000ff) << 0)
        | ((dev.minor as u64 & 0xffffff00) << 12)
}

//the same as the glibc major and minor functions
pub fn major(devnum: u64) -> u32 {
    (((devnum & 0x00000000000fff00) >> 8) | ((devnum & 0xfffff00000000000) >> 32)) as u32
}
pub fn minor(devnum: u64) -> u32 {
    (((devnum & 0x00000000000000ff) >> 0) | ((devnum & 0x00000ffffff00000) >> 12)) as u32
}

pub fn devtuple(devnum: u64) -> DevNo {
    DevNo {
        major: major(devnum),
        minor: minor(devnum),
    }
}
