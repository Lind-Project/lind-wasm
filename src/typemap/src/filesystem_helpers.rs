use libc;
use sysdefs::data::fs_struct::{FSData, StatData};

// These conversion functions are necessary because:
// 1. Host kernel's libc structures vary across platforms, while our StatData/FSData provide a stable ABI
// 2. WASM linear memory has different alignment requirements than native host memory
// 3. Explicit type casts ensure consistent field sizes across different host platforms

/// Copies fields from a `libc::stat` structure into a `StatData` located inside wasm linear memory.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `StatData` (user-space buffer).
/// - `libc_statbuf`: Source `libc::stat` obtained from the host kernel.
pub fn convert_statdata_to_user(stat_ptr: &mut StatData, libc_statbuf: libc::stat) {
    stat_ptr.st_blksize = libc_statbuf.st_blksize as i32;
    stat_ptr.st_blocks = libc_statbuf.st_blocks as u32;
    stat_ptr.st_dev = libc_statbuf.st_dev as u64;
    stat_ptr.st_gid = libc_statbuf.st_gid;
    stat_ptr.st_ino = libc_statbuf.st_ino as usize;
    stat_ptr.st_mode = libc_statbuf.st_mode as u32;
    stat_ptr.st_nlink = libc_statbuf.st_nlink as u32;
    stat_ptr.st_rdev = libc_statbuf.st_rdev as u64;
    stat_ptr.st_size = libc_statbuf.st_size as usize;
    stat_ptr.st_uid = libc_statbuf.st_uid;
}

/// Copies fields from a `libc::statfs` structure into a `FSData` located inside wasm linear memory.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `FSData` (user-space buffer).
/// - `libc_statbuf`: Source `libc::statfs` obtained from the host kernel.
pub fn convert_fstatdata_to_user(stat_ptr: &mut FSData, libc_databuf: libc::statfs) {
    stat_ptr.f_bavail = libc_databuf.f_bavail;
    stat_ptr.f_bfree = libc_databuf.f_bfree;
    stat_ptr.f_blocks = libc_databuf.f_blocks;
    stat_ptr.f_bsize = libc_databuf.f_bsize as u64;
    stat_ptr.f_files = libc_databuf.f_files;
    /* TODO: different from libc struct */
    stat_ptr.f_fsid = 0;
    stat_ptr.f_type = libc_databuf.f_type as u64;
    stat_ptr.f_ffiles = 1024 * 1024 * 515;
    stat_ptr.f_namelen = 254;
    stat_ptr.f_frsize = 4096;
    stat_ptr.f_spare = [0; 32];
}
