#![allow(dead_code)]
#![allow(unused_variables)]

// Root directory for Lind filesystem
pub const LIND_ROOT: &str = "/home/lind-wasm/src/RawPOSIX/tmp";

// ===== Standard File Descriptors =====
pub const STDIN_FILENO: i32 = 0; // File descriptor for standard input
pub const STDOUT_FILENO: i32 = 1; // File descriptor for standard output
pub const STDERR_FILENO: i32 = 2; // File descriptor for standard error

// ===== Directory Entry Constant =====
// Source: include/dirent.h
pub const DT_UNKNOWN: u8 = 0;

// ===== File Access Permission Flags =====
pub const F_OK: u32 = 0; // Test for existence
pub const X_OK: u32 = 1; // Test for execute permission
pub const W_OK: u32 = 2; // Test for write permission
pub const R_OK: u32 = 4; // Test for read permission

// ===== File Access Modes =====
// Source: include/uapi/asm-generic/fcntl.h
pub const O_RDONLY: i32 = 0o0; // Open read-only
pub const O_WRONLY: i32 = 0o1; // Open write-only
pub const O_RDWR: i32 = 0o2; // Open read-write
pub const O_RDWRFLAGS: i32 = 0o3; // Mask for access modes

// ===== File Creation and Status Flags =====
// Source: include/linux/coda.h
pub const O_CREAT: i32 = 0o100; // Create file if it doesn't exist
pub const O_EXCL: i32 = 0o200; // Error if O_CREAT and file exists
pub const O_NOCTTY: i32 = 0o400; // Don't assign controlling terminal
pub const O_TRUNC: i32 = 0o1000; // Truncate file to zero length
pub const O_APPEND: i32 = 0o2000; // Append mode - writes always at end
pub const O_NONBLOCK: i32 = 0o4000; // Non-blocking mode
pub const O_SYNC: i32 = 0o10000; // Synchronous writes
pub const O_ASYNC: i32 = 0o20000; // Signal-driven I/O
pub const O_CLOEXEC: i32 = 0o2000000; // Close on exec

// ===== File Permissions =====
// Source: include/uapi/linux/stat.h
pub const S_IRWXA: u32 = 0o777; // All permissions for all users
pub const S_IRWXU: u32 = 0o700; // User read, write, execute
pub const S_IRUSR: u32 = 0o400; // User read
pub const S_IWUSR: u32 = 0o200; // User write
pub const S_IXUSR: u32 = 0o100; // User execute
pub const S_IRWXG: u32 = 0o070; // Group read, write, execute
pub const S_IRGRP: u32 = 0o040; // Group read
pub const S_IWGRP: u32 = 0o020; // Group write
pub const S_IXGRP: u32 = 0o010; // Group execute
pub const S_IRWXO: u32 = 0o007; // Others read, write, execute
pub const S_IROTH: u32 = 0o004; // Others read
pub const S_IWOTH: u32 = 0o002; // Others write
pub const S_IXOTH: u32 = 0o001; // Others execute

//Commands for FCNTL
// Source: include/linux/fcntl.h
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
// Source: include/linux/stat.h
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
pub const MAP_FIXED: u32 = 16;
pub const MAP_ANONYMOUS: u32 = 32;
pub const MAP_HUGE_SHIFT: i32 = 26;
pub const MAP_HUGETLB: i32 = 262144;

// Source: include/linux/fs.h
pub const SEEK_SET: i32 = 0; // Seek from beginning of file
pub const SEEK_CUR: i32 = 1; // Seek from current position
pub const SEEK_END: i32 = 2; // Seek from end of file

// Source: include/linux/ipc.h
pub const IPC_PRIVATE: i32 = 0o0;
pub const IPC_CREAT: i32 = 0o1000;
pub const IPC_EXCL: i32 = 0o2000;

pub const IPC_RMID: i32 = 0;
pub const IPC_SET: i32 = 1;
pub const IPC_STAT: i32 = 2;

// Source: linux/bits/shm.h
pub const SHM_DEST: i32 = 0o1000; // Destroy segment when last process detaches
pub const SHM_LOCKED: i32 = 0o2000; // Lock segment in memory
pub const SHM_HUGETLB: i32 = 0o4000; // Use huge TLB pages

pub const SHM_R: i32 = 0o400; // Read permission
pub const SHM_W: i32 = 0o200; // Write permission
pub const SHM_RDONLY: i32 = 0o10000; // Read-only access
pub const SHM_RND: i32 = 0o20000; // Round attach address to SHMLBA
pub const SHM_REMAP: i32 = 0o40000; // Take-over region on attach
pub const SHM_EXEC: i32 = 0o100000; // Execute permission

pub const SHMMIN: u32 = 1; // Minimum shared memory segment size
pub const SHMMNI: u32 = 4096; // Maximum number of segments system wide
pub const SHMMAX: u32 = 4278190079; // Maximum shared memory segment size
pub const SHMALL: u32 = 4278190079; // Maximum total shared memory system wide
pub const SHMSEG: u32 = SHMMNI; // Maximum segments per process

pub const SEM_VALUE_MAX: u32 = 2147483647; // Maximum value for a semaphore

// ===== Memory Protection Flags =====
// Source: include/uapi/asm-generic/mman-common.h
pub const PROT_NONE: i32 = 0x0; // Page cannot be accessed
pub const PROT_READ: i32 = 0x1; // Page can be read
pub const PROT_WRITE: i32 = 0x2; // Page can be written
pub const PROT_EXEC: i32 = 0x4; // Page can be executed

// Mask for all protection bits
// Note: Some architectures may support additional bits
pub const PROT_MASK: u32 = 0x7;

// ===== Memory Mapping Flags =====
// Source: include/uapi/asm-generic/mman.h
pub const MAP_SHARED: u32 = 0x01; // Share changes with other processes
pub const MAP_PRIVATE: u32 = 0x02; // Changes are private to this process
pub const MAP_SHARING_MASK: u32 = 0x03; // Mask to isolate sharing bits

pub const MAP_ANON: u32 = 0x20; // Don't use a file descriptor

// ===== Page Size Constants =====
// Note: These values are architecture-dependent
// Current values are for x86_64 Linux
pub const PAGESHIFT: u32 = 12; // 4KB pages (1 << 12 = 4096)
pub const PAGESIZE: u32 = 1 << PAGESHIFT;

// ===== Memory Mapping Error Value =====
// Source: include/uapi/asm-generic/mman-common.h
pub const MAP_FAILED: *mut std::ffi::c_void = (-1isize) as *mut std::ffi::c_void;

// ===== Memory Remapping Flags =====
// Source: include/uapi/asm-generic/mman-common.h
pub const MREMAP_MAYMOVE: u32 = 0x01; // Can relocate mapping
pub const MREMAP_FIXED: u32 = 0x02; // New address is specified exactly

// ===== File Access Modes =====
// Source: include/uapi/asm-generic/fcntl.h
pub const O_ACCMODE: i32 = 0o003; // Mask for file access modes
