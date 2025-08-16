// Filesystem metadata struct
#![allow(dead_code)]

use crate::cage::get_cage;

pub use parking_lot::Mutex;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::os::unix::io::AsRawFd;
pub use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
pub use std::sync::{Arc, LazyLock};
use std::time::SystemTime;
use sysdefs::constants::fs_const::{MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_NONE};
use sysdefs::data::fs_struct::{IpcPermStruct, ShmidsStruct};

pub use dashmap::{mapref::entry::Entry, DashMap, DashSet};

#[derive(Debug)]
pub struct ShmFile {
    fobj: Arc<Mutex<File>>,
    key: i32,
    size: usize,
}

pub static SHM_METADATA: LazyLock<Arc<ShmMetadata>> =
    LazyLock::new(|| Arc::new(ShmMetadata::init_shm_metadata()));

pub struct ShmSegment {
    pub shminfo: ShmidsStruct,
    pub key: i32,
    pub size: usize,
    pub filebacking: ShmFile,
    pub rmid: bool,
    pub attached_cages: DashMap<u64, i32>, // attached cages, number of references in cage
}

pub fn new_shm_backing(key: i32, size: usize) -> std::io::Result<ShmFile> {
    ShmFile::new(key, size)
}

// timestamp function to fill shm data structures
pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// Calls the system `mmap` via `libc` and returns the mapped address as a 32-bit integer.
///
/// In Lind-WASM, addresses are represented as 32-bit values within the linear memory of a
/// Wasm module. The return value is truncated to 32 bits to match this representation.
/// On error, returns `-1` (corresponding to `MAP_FAILED`).
///
/// # Safety
/// This function uses raw pointers and directly invokes `libc::mmap`. The caller must ensure
/// that all arguments are valid and that using the returned address as a 32-bit pointer is safe
/// in the Lind-WASM context.
pub fn libc_mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fildes: i32, off: i64) -> i32 {
    return ((unsafe { libc::mmap(addr as *mut c_void, len, prot, flags, fildes, off) } as i64)
        & 0xffffffff) as i32;
}

// Mimic shared memory in Linux by creating a file backing and truncating it to the segment size
// We can then safely unlink the file while still holding a descriptor to that segment,
// which we can use to map shared across cages.
impl ShmFile {
    fn new(key: i32, size: usize) -> std::io::Result<ShmFile> {
        // open file "shm-#id"
        let filename = format!("{}{}", "shm-", key);
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(filename.clone())
            .unwrap();
        // truncate file to size
        f.set_len(size as u64)?;
        // unlink file
        fs::remove_file(filename)?;
        let shmfile = ShmFile {
            fobj: Arc::new(Mutex::new(f)),
            key,
            size,
        };

        Ok(shmfile)
    }

    //gets the raw fd handle (integer) from a rust fileobject
    pub fn as_fd_handle_raw_int(&self) -> i32 {
        self.fobj.lock().as_raw_fd() as i32
    }
}

pub fn new_shm_segment(
    key: i32,
    size: usize,
    cageid: u32,
    uid: u32,
    gid: u32,
    mode: u16,
) -> ShmSegment {
    ShmSegment::new(key, size, cageid, uid, gid, mode)
}

impl ShmSegment {
    pub fn new(key: i32, size: usize, cageid: u32, uid: u32, gid: u32, mode: u16) -> ShmSegment {
        let filebacking = new_shm_backing(key, size).unwrap();

        let time = timestamp() as isize; //We do a real timestamp now
        let permstruct = IpcPermStruct {
            __key: key,
            uid: uid,
            gid: gid,
            cuid: uid,
            cgid: gid,
            mode: mode,
            __pad1: 0,
            __seq: 0,
            __pad2: 0,
            __unused1: 0,
            __unused2: 0,
        };
        let shminfo = ShmidsStruct {
            shm_perm: permstruct,
            shm_segsz: size as u32,
            shm_atime: 0,
            shm_dtime: 0,
            shm_ctime: time,
            shm_cpid: cageid,
            shm_lpid: 0,
            shm_nattch: 0,
        };

        ShmSegment {
            shminfo: shminfo,
            key: key,
            size: size,
            filebacking: filebacking,
            rmid: false,
            attached_cages: DashMap::new(),
        }
    }
    // mmap shared segment into cage, and increase attachments
    // increase in cage references within attached_cages map
    pub fn map_shm(&mut self, shmaddr: *mut u8, prot: i32, cageid: u64) -> i32 {
        let fobjfdno = self.filebacking.as_fd_handle_raw_int();
        self.shminfo.shm_nattch += 1;
        self.shminfo.shm_atime = timestamp() as isize;

        match self.attached_cages.entry(cageid) {
            Entry::Occupied(mut occupied) => {
                *occupied.get_mut() += 1;
            }
            Entry::Vacant(vacant) => {
                vacant.insert(1);
            }
        };
        libc_mmap(
            shmaddr,
            self.size as usize,
            prot,
            (MAP_SHARED as i32) | (MAP_FIXED as i32),
            fobjfdno,
            0,
        )
    }

    // unmap shared segment, decrease attachments
    // decrease references within attached cages map
    pub fn unmap_shm(&mut self, shmaddr: *mut u8, cageid: u64) {
        libc_mmap(
            shmaddr,
            self.size as usize,
            PROT_NONE,
            (MAP_PRIVATE as i32) | (MAP_ANONYMOUS as i32) | (MAP_FIXED as i32),
            -1,
            0,
        );
        self.shminfo.shm_nattch -= 1;
        self.shminfo.shm_dtime = timestamp() as isize;
        match self.attached_cages.entry(cageid) {
            Entry::Occupied(mut occupied) => {
                *occupied.get_mut() -= 1;
                if *occupied.get() == 0 {
                    occupied.remove_entry();
                }
            }
            Entry::Vacant(_) => {
                panic!("Cage not available in segment attached cages");
            }
        };
    }

    pub fn get_shm_length(&self) -> usize {
        self.size
    }
}

pub struct ShmMetadata {
    pub nextid: AtomicI32,
    pub shmkeyidtable: DashMap<i32, i32>,
    pub shmtable: DashMap<i32, ShmSegment>,
}

impl ShmMetadata {
    pub fn init_shm_metadata() -> ShmMetadata {
        ShmMetadata {
            nextid: AtomicI32::new(1),
            shmkeyidtable: DashMap::new(),
            shmtable: DashMap::new(),
        }
    }
    pub fn get_shm_length(&self, shmid: i32) -> Option<usize> {
        self.shmtable
            .get(&shmid)
            .map(|segment| segment.get_shm_length())
    }

    pub fn new_keyid(&self) -> i32 {
        self.nextid.fetch_add(1, Ordering::Relaxed)
    }
}

pub fn get_shm_length(shmid: i32) -> Option<usize> {
    let metadata: &ShmMetadata = &**SHM_METADATA;
    metadata.get_shm_length(shmid)
}

pub fn unmap_shm_mappings(cageid: u64) {
    let cage = get_cage(cageid).unwrap();
    //unmap shm mappings on exit or exec
    for rev_mapping in cage.rev_shm.lock().iter() {
        let shmid = rev_mapping.1;
        let metadata = &SHM_METADATA;
        match metadata.shmtable.entry(shmid) {
            Entry::Occupied(mut occupied) => {
                let segment = occupied.get_mut();
                segment.shminfo.shm_nattch -= 1;
                segment.shminfo.shm_dtime = timestamp() as isize;
                segment.attached_cages.remove(&cageid);

                if segment.rmid && segment.shminfo.shm_nattch == 0 {
                    let key = segment.key;
                    occupied.remove_entry();
                    metadata.shmkeyidtable.remove(&key);
                }
            }
            Entry::Vacant(_) => {
                panic!("Shm entry not created for some reason");
            }
        };
    }
}
