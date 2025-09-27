use sysdefs::data::fs_struct::{StatData, FSData};
use libc;

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
