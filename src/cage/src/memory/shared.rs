// Filesystem metadata struct
#![allow(dead_code)]

use crate::cage::get_cage;

use dashmap::mapref::entry::Entry::{Occupied, Vacant};
pub use parking_lot::Mutex;
use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::os::unix::io::AsRawFd;
pub use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
pub use std::sync::{Arc, LazyLock};
use std::time::SystemTime;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_NONE, PROT_READ, PROT_WRITE, SHMMAX,
    SHMMIN, SHM_RDONLY,
};
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
    pub fn map_shm(&mut self, shmaddr: *mut u8, prot: i32, cageid: u64) -> usize {
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

        let result = unsafe {
            libc::mmap(
                shmaddr as *mut c_void,
                self.size as usize,
                prot,
                (MAP_SHARED as i32) | (MAP_FIXED as i32),
                fobjfdno,
                0,
            ) as usize
        };

        // Check for mmap errors using the same logic as fs_calls
        let result_signed = result as isize;
        if result_signed == -1
            || (result_signed < 0 && result_signed > -256)
            || (result % 4096 != 0)
        {
            return syscall_error(Errno::EINVAL, "map_shm", "mmap failed") as usize;
        }

        result
    }

    // unmap shared segment, decrease attachments
    // decrease references within attached cages map
    pub fn unmap_shm(&mut self, shmaddr: *mut u8, cageid: u64) {
        let mmap_ret = unsafe {
            (libc::mmap(
                shmaddr as *mut c_void,
                self.size as usize,
                PROT_NONE,
                (MAP_PRIVATE as i32) | (MAP_ANONYMOUS as i32) | (MAP_FIXED as i32),
                -1,
                0,
            ) as usize)
        };
        assert!(mmap_ret == shmaddr as usize);

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

//------------------SHMHELPERS----------------------
/// Finds the index of a given shared memory address in the reverse mapping table.
///
/// # Arguments
/// * `rev_shm`  – A vector of `(addr, shmid)` pairs mapping addresses to shared memory IDs.
/// * `shmaddr`  – The address to search for.
///
/// # Returns
/// * `Some(index)` if the address is found in the vector.
/// * `None` if the address does not exist in the mapping.
pub fn rev_shm_find_index_by_addr(rev_shm: &Vec<(u64, i32)>, shmaddr: u64) -> Option<usize> {
    for (index, val) in rev_shm.iter().enumerate() {
        if val.0 == shmaddr as u64 {
            return Some(index);
        }
    }
    None
}

/// Collects all addresses that map to a given shared memory ID from the reverse mapping table.
///
/// # Arguments
/// * `rev_shm` – A vector of `(addr, shmid)` pairs mapping addresses to shared memory IDs.
/// * `shmid`   – The shared memory ID to search for.
///
/// # Returns
/// * A vector of all addresses (`u64`) associated with the given `shmid`.
/// * Returns an empty vector if no addresses are found.
pub fn rev_shm_find_addrs_by_shmid(rev_shm: &Vec<(u64, i32)>, shmid: i32) -> Vec<u64> {
    let mut addrvec = Vec::new();
    for val in rev_shm.iter() {
        if val.1 == shmid as i32 {
            addrvec.push(val.0);
        }
    }

    return addrvec;
}

/// Searches for a shared memory region that contains a given address.
///
/// # Arguments
/// * `rev_shm`     – A vector of `(addr, shmid)` pairs mapping base addresses to shared memory IDs.
/// * `search_addr` – The address to look up within existing shared memory regions.
///
/// # Returns
/// * `Some((base_addr, shmid))` if `search_addr` falls within the range of a known segment.
/// * `None` if the address is not within any tracked region.
pub fn search_for_addr_in_region(
    rev_shm: &Vec<(u64, i32)>,
    search_addr: u64,
) -> Option<(u64, i32)> {
    let metadata = &SHM_METADATA;
    for val in rev_shm.iter() {
        let addr = val.0;
        let shmid = val.1;
        if let Some(segment) = metadata.shmtable.get_mut(&shmid) {
            let range = addr..(addr + segment.size as u64);
            if range.contains(&search_addr) {
                return Some((addr, shmid));
            }
        }
    }
    None
}

/// Helper function that attaches a shared memory segment to the calling cage’s address space.
///
/// # Arguments
/// * `cageid` – ID of the calling cage.
/// * `shmaddr` – System Address where the shared memory segment should be mapped.
/// * `shmflg` – Flags controlling access (e.g., `SHM_RDONLY`).
/// * `shmid` – Identifier of the shared memory segment to attach.
///
/// This function looks up the shared memory segment identified by `shmid`, determines the appropriate
/// protection flags from `shmflg`, records the mapping `(shmaddr, shmid)` in the cage’s reverse mapping
/// table, and calls `map_shm` to attach the segment to the cage’s address space. If the shmid is
/// invalid, it returns an error.
///
/// # Returns
/// * On success – the mapped address as a `usize`.
/// * On error – a negative errno value as a `usize`.
pub fn shmat_helper(cageid: u64, shmaddr: *mut u8, shmflg: i32, shmid: i32) -> usize {
    let metadata = &SHM_METADATA;
    let prot: i32;

    let cage = get_cage(cageid).unwrap();

    if let Some(mut segment) = metadata.shmtable.get_mut(&shmid) {
        if 0 != (shmflg & SHM_RDONLY) {
            prot = PROT_READ;
        } else {
            prot = PROT_READ | PROT_WRITE;
        }
        let mut rev_shm = cage.rev_shm.lock();
        rev_shm.push((shmaddr as u64, shmid));
        drop(rev_shm);

        segment.map_shm(shmaddr, prot, cageid) as usize
    } else {
        syscall_error(Errno::EINVAL, "shmat", "Invalid shmid value") as usize
    }
}

/// Helper function that detaches a shared memory segment from the calling cage’s address space.
///
/// # Arguments
/// * `cageid` – ID of the calling cage.
/// * `shmaddr` – System Address of the shared memory segment to detach.
///
/// This function searches the cage’s reverse mapping table for the segment mapped at `shmaddr`,
/// detaches it with `unmap_shm`, and removes the reverse mapping entry. If the segment was marked
/// for removal and has no remaining attachments, it is deleted from both shmtable and `shmkeyidtabl`e.
/// On success, it returns the segment length; if no mapping exists at the given address, it returns an error.
///
/// # Returns
/// * On success – the length (in bytes) of the detached segment.
/// * On error – a negative errno value.
pub fn shmdt_helper(cageid: u64, shmaddr: *mut u8) -> i32 {
    let metadata = &SHM_METADATA;
    let mut rm = false;
    let cage = get_cage(cageid).unwrap();
    let mut rev_shm = cage.rev_shm.lock();
    let rev_shm_index = rev_shm_find_index_by_addr(&rev_shm, shmaddr as u64);

    if let Some(index) = rev_shm_index {
        let shmid = rev_shm[index].1;
        match metadata.shmtable.entry(shmid) {
            Occupied(mut occupied) => {
                let segment = occupied.get_mut();
                // Retrieve the length before shmdt_syscall since the segment will be cleaned up after
                // the syscall completes, making the length field unavailable. We need this length
                // value later to remove the correct number of pages from vmmap.
                let length = segment.size as i32;

                segment.unmap_shm(shmaddr, cageid);

                if segment.rmid && segment.shminfo.shm_nattch == 0 {
                    rm = true;
                }
                rev_shm.swap_remove(index);

                if rm {
                    let key = segment.key;
                    occupied.remove_entry();
                    metadata.shmkeyidtable.remove(&key);
                }
                return length;
            }
            Vacant(_) => {
                panic!("Inode not created for some reason");
            }
        };
    } else {
        return syscall_error(
            Errno::EINVAL,
            "shmdt",
            "No shared memory segment at shmaddr",
        );
    }
}
