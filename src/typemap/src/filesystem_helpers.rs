use libc;
use std::io;
use std::path::Path;
use sysdefs::data::fs_struct::{FSData, StatData};

/// ELF magic: \x7fELF
const ELF_MAGIC: [u8; 4] = [0x7f, 0x45, 0x4c, 0x46];
/// Wasm magic: \0asm
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

/// The type of an executable binary as determined by its file header magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFileType {
    Elf,
    Wasm,
    Unknown,
}

/// Reads the first four bytes of `path` and returns the corresponding
/// [`BinaryFileType`] based on the ELF (`\x7fELF`) or Wasm (`\0asm`) magic.
///
/// Returns `BinaryFileType::Unknown` for any file whose magic does not match
/// either format, and also on any I/O error (e.g. file not found).
pub fn detect_binary_type(path: &Path) -> BinaryFileType {
    let mut magic = [0u8; 4];
    match read_magic(path, &mut magic) {
        Ok(4) if magic == ELF_MAGIC => BinaryFileType::Elf,
        Ok(4) if magic == WASM_MAGIC => BinaryFileType::Wasm,
        _ => BinaryFileType::Unknown,
    }
}

fn read_magic(path: &Path, buf: &mut [u8; 4]) -> io::Result<usize> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let n = f.read(buf)?;
    Ok(n)
}

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
    stat_ptr.st_atim = (
        libc_statbuf.st_atime as u64,
        libc_statbuf.st_atime_nsec as u64,
    );
    stat_ptr.st_mtim = (
        libc_statbuf.st_mtime as u64,
        libc_statbuf.st_mtime_nsec as u64,
    );
    stat_ptr.st_ctim = (
        libc_statbuf.st_ctime as u64,
        libc_statbuf.st_ctime_nsec as u64,
    );
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
