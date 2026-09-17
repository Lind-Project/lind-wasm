use crate::datatype_conversion::{
    sc_convert_addr_to_fstatdata, sc_convert_addr_to_stat64data, sc_convert_addr_to_statdata,
    sc_convert_addr_to_statfs64data,
};
use cage::cage_is_mpk;
use goblin::elf::header::{ET_DYN, ET_EXEC, ET_REL};
use goblin::Object;
use libc;
use std::io;
use std::path::Path;
use sysdefs::constants::Errno;
use sysdefs::data::fs_struct::{FSData, Stat64Data, Statfs64Data, StatData};

/// Wasm magic: \0asm
const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6d];

/// The type of an executable binary as determined by its file header magic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryFileType {
    ElfExe,
    ElfSo,
    Wasm,
    CWasm,
    Unknown,
}

/// Reads the file at `path` and returns the corresponding [`BinaryFileType`].
/// For ELF files, uses the e_type field to distinguish between:
/// - ET_REL (relocatable) -> CWasm
/// - ET_DYN (shared object) -> ElfSo
/// - ET_EXEC (executable) -> ElfExe
/// For Wasm files, checks the magic bytes (\0asm).
///
/// Returns `BinaryFileType::Unknown` for any file whose format is not recognized,
/// and also on any I/O error (e.g. file not found).
pub fn detect_binary_type(path: &Path) -> BinaryFileType {
    // Read the file
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return BinaryFileType::Unknown,
    };

    // Try to parse with goblin
    match Object::parse(&bytes) {
        Ok(Object::Elf(elf)) => {
            // Check the ELF type
            match elf.header.e_type {
                ET_REL => BinaryFileType::CWasm,
                ET_EXEC => BinaryFileType::ElfExe,
                ET_DYN => BinaryFileType::ElfSo,
                _ => BinaryFileType::Unknown,
            }
        }
        Ok(Object::Unknown(magic)) if magic == u64::from_le_bytes([
            WASM_MAGIC[0], WASM_MAGIC[1], WASM_MAGIC[2], WASM_MAGIC[3], 0, 0, 0, 0
        ]) => BinaryFileType::Wasm,
        _ => {
            // Fallback: check for wasm magic manually
            if bytes.len() >= 4 && &bytes[0..4] == WASM_MAGIC {
                BinaryFileType::Wasm
            } else {
                BinaryFileType::Unknown
            }
        }
    }
}

// These conversion functions are necessary because:
// 1. Host kernel's libc structures vary across platforms, while our StatData/FSData provide a stable ABI
// 2. WASM linear memory has different alignment requirements than native host memory
// 3. Explicit type casts ensure consistent field sizes across different host platforms

/// Converts a `libc::stat` result into the user's stat buffer, choosing the buffer layout
/// based on the calling cage's runtime:
/// - Wasm cages get the repacked `StatData` layout (stable ABI for wasm linear memory).
/// - MPK cages run native code and expect glibc's real `stat64` layout, so they get
///   `Stat64Data` instead.
///
/// ## Arguments:
/// - `cageid`: The Cage ID of the current caller, used to determine the runtime type.
/// - `arg`/`arg_cageid`: The user-provided buffer address and its owning Cage ID.
/// - `libc_statbuf`: Source `libc::stat` obtained from the host kernel.
pub fn convert_statdata_to_user(
    cageid: u64,
    arg: u64,
    arg_cageid: u64,
    libc_statbuf: libc::stat,
) -> Result<(), Errno> {
    if cage_is_mpk(cageid) {
        let stat_ptr = sc_convert_addr_to_stat64data(arg, arg_cageid, cageid)?;
        convert_stat64data_to_user(stat_ptr, libc_statbuf);
    } else {
        let stat_ptr = sc_convert_addr_to_statdata(arg, arg_cageid, cageid)?;
        convert_wasm_statdata_to_user(stat_ptr, libc_statbuf);
    }
    Ok(())
}

/// Copies fields from a `libc::stat` structure into a `StatData` located inside wasm linear memory.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `StatData` (user-space buffer).
/// - `libc_statbuf`: Source `libc::stat` obtained from the host kernel.
fn convert_wasm_statdata_to_user(stat_ptr: &mut StatData, libc_statbuf: libc::stat) {
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

/// Copies fields from a `libc::stat` structure into a `Stat64Data` matching glibc's native
/// `stat64` ABI, for cages running under the MPK runtime.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `Stat64Data` (user-space buffer).
/// - `libc_statbuf`: Source `libc::stat` obtained from the host kernel.
fn convert_stat64data_to_user(stat_ptr: &mut Stat64Data, libc_statbuf: libc::stat) {
    stat_ptr.st_dev = libc_statbuf.st_dev as u64;
    stat_ptr.st_ino = libc_statbuf.st_ino as u64;
    stat_ptr.st_nlink = libc_statbuf.st_nlink as u64;
    stat_ptr.st_mode = libc_statbuf.st_mode as u32;
    stat_ptr.st_uid = libc_statbuf.st_uid;
    stat_ptr.st_gid = libc_statbuf.st_gid;
    stat_ptr.__pad0 = 0;
    stat_ptr.st_rdev = libc_statbuf.st_rdev as u64;
    stat_ptr.st_size = libc_statbuf.st_size as i64;
    stat_ptr.st_blksize = libc_statbuf.st_blksize as i64;
    stat_ptr.st_blocks = libc_statbuf.st_blocks as i64;
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
    stat_ptr.__glibc_reserved = [0; 3];
}

/// Converts a `libc::statfs` result into the user's statfs buffer, choosing the buffer layout
/// based on the calling cage's runtime:
/// - Wasm cages get the repacked `FSData` layout (stable ABI for wasm linear memory).
/// - MPK cages run native code and expect glibc's real `statfs64` layout, so they get
///   `Statfs64Data` instead.
///
/// ## Arguments:
/// - `cageid`: The Cage ID of the current caller, used to determine the runtime type.
/// - `arg`/`arg_cageid`: The user-provided buffer address and its owning Cage ID.
/// - `libc_databuf`: Source `libc::statfs` obtained from the host kernel.
pub fn convert_fstatdata_to_user(
    cageid: u64,
    arg: u64,
    arg_cageid: u64,
    libc_databuf: libc::statfs,
) -> Result<(), Errno> {
    if cage_is_mpk(cageid) {
        let stat_ptr = sc_convert_addr_to_statfs64data(arg, arg_cageid, cageid)?;
        convert_statfs64data_to_user(stat_ptr, libc_databuf);
    } else {
        let stat_ptr = sc_convert_addr_to_fstatdata(arg, arg_cageid, cageid)?;
        convert_wasm_fstatdata_to_user(stat_ptr, libc_databuf);
    }
    Ok(())
}

/// Copies fields from a `libc::statfs` structure into a `FSData` located inside wasm linear memory.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `FSData` (user-space buffer).
/// - `libc_statbuf`: Source `libc::statfs` obtained from the host kernel.
fn convert_wasm_fstatdata_to_user(stat_ptr: &mut FSData, libc_databuf: libc::statfs) {
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

/// Copies fields from a `libc::statfs` structure into a `Statfs64Data` matching glibc's native
/// `statfs64` ABI, for cages running under the MPK runtime.
///
/// ## Arguments:
/// - `stat_ptr`: Destination `Statfs64Data` (user-space buffer).
/// - `libc_databuf`: Source `libc::statfs` obtained from the host kernel.
fn convert_statfs64data_to_user(stat_ptr: &mut Statfs64Data, libc_databuf: libc::statfs) {
    stat_ptr.f_type = libc_databuf.f_type as i64;
    stat_ptr.f_bsize = libc_databuf.f_bsize as i64;
    stat_ptr.f_blocks = libc_databuf.f_blocks as u64;
    stat_ptr.f_bfree = libc_databuf.f_bfree as u64;
    stat_ptr.f_bavail = libc_databuf.f_bavail as u64;
    stat_ptr.f_files = libc_databuf.f_files as u64;
    stat_ptr.f_ffree = libc_databuf.f_ffree as u64;
    // `fsid_t`'s inner field is private, but it's always a fixed 8-byte (two i32)
    // value, so a same-size transmute of just this field (not the whole struct)
    // is safe.
    stat_ptr.f_fsid = (0, 0);
    stat_ptr.f_namelen = libc_databuf.f_namelen as i64;
    stat_ptr.f_frsize = libc_databuf.f_frsize as i64;
    // f_flags/f_spare aren't exposed as named fields by this version of the libc
    // crate's `statfs`, and its true total size isn't guaranteed to match the
    // 120-byte kernel ABI, so we can't safely read them. Zero them instead of
    // guessing at their memory location.
    stat_ptr.f_flags = 0;
    stat_ptr.f_spare = [0; 4];
}
