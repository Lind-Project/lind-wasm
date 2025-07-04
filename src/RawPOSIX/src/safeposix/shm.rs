// Filesystem metadata struct
#![allow(dead_code)]

use sysdefs::constants::fs_const::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PROT_NONE, PROT_READ, PROT_WRITE, SHM_DEST,
    SHM_RDONLY,
};
use sysdefs::data::fs_struct::{IpcPermStruct, ShmidsStruct};

use super::cage::Cage;
use crate::interface;

use libc::*;

pub static SHM_METADATA: interface::RustLazyGlobal<interface::RustRfc<ShmMetadata>> =
    interface::RustLazyGlobal::new(|| interface::RustRfc::new(ShmMetadata::init_shm_metadata()));

pub struct ShmSegment {
    pub shminfo: ShmidsStruct,
    pub key: i32,
    pub size: usize,
    pub filebacking: interface::ShmFile,
    pub rmid: bool,
    pub attached_cages: interface::RustHashMap<u64, i32>, // attached cages, number of references in cage
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
        let filebacking = interface::new_shm_backing(key, size).unwrap();

        let time = interface::timestamp() as isize; //We do a real timestamp now
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
            attached_cages: interface::RustHashMap::new(),
        }
    }
    // mmap shared segment into cage, and increase attachments
    // increase in cage references within attached_cages map
    pub fn map_shm(&mut self, shmaddr: *mut u8, prot: i32, cageid: u64) -> i32 {
        let fobjfdno = self.filebacking.as_fd_handle_raw_int();
        self.shminfo.shm_nattch += 1;
        self.shminfo.shm_atime = interface::timestamp() as isize;

        match self.attached_cages.entry(cageid) {
            interface::RustHashEntry::Occupied(mut occupied) => {
                *occupied.get_mut() += 1;
            }
            interface::RustHashEntry::Vacant(vacant) => {
                vacant.insert(1);
            }
        };
        interface::libc_mmap(
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
        interface::libc_mmap(
            shmaddr,
            self.size as usize,
            PROT_NONE,
            (MAP_PRIVATE as i32) | (MAP_ANONYMOUS as i32) | (MAP_FIXED as i32),
            -1,
            0,
        );
        self.shminfo.shm_nattch -= 1;
        self.shminfo.shm_dtime = interface::timestamp() as isize;
        match self.attached_cages.entry(cageid) {
            interface::RustHashEntry::Occupied(mut occupied) => {
                *occupied.get_mut() -= 1;
                if *occupied.get() == 0 {
                    occupied.remove_entry();
                }
            }
            interface::RustHashEntry::Vacant(_) => {
                panic!("Cage not available in segment attached cages");
            }
        };
    }

    pub fn get_shm_length(&self) -> usize {
        self.size
    }
}

pub struct ShmMetadata {
    pub nextid: interface::RustAtomicI32,
    pub shmkeyidtable: interface::RustHashMap<i32, i32>,
    pub shmtable: interface::RustHashMap<i32, ShmSegment>,
}

impl ShmMetadata {
    pub fn init_shm_metadata() -> ShmMetadata {
        ShmMetadata {
            nextid: interface::RustAtomicI32::new(1),
            shmkeyidtable: interface::RustHashMap::new(),
            shmtable: interface::RustHashMap::new(),
        }
    }
    pub fn get_shm_length(&self, shmid: i32) -> Option<usize> {
        self.shmtable
            .get(&shmid)
            .map(|segment| segment.get_shm_length())
    }

    pub fn new_keyid(&self) -> i32 {
        self.nextid
            .fetch_add(1, interface::RustAtomicOrdering::Relaxed)
    }
}

pub fn get_shm_length(shmid: i32) -> Option<usize> {
    let metadata: &ShmMetadata = &**SHM_METADATA;
    metadata.get_shm_length(shmid)
}
