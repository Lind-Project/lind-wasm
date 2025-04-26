// Filesystem metadata struct
#![allow(dead_code)]

use crate::constants::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PAGESHIFT, PROT_NONE, PROT_READ, PROT_WRITE,
    SHM_DEST, SHM_RDONLY,
};

use crate::interface::{self, syscall_error, Errno};
use crate::safeposix::cage::{MemoryBackingType, VmmapOps};

use libc::*;

use super::cage::Cage;

pub static SHM_METADATA: interface::RustLazyGlobal<interface::RustRfc<ShmMetadata>> =
    interface::RustLazyGlobal::new(|| interface::RustRfc::new(ShmMetadata::init_shm_metadata()));

pub struct ShmSegment {
    pub shminfo: interface::ShmidsStruct,
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
        let permstruct = interface::IpcPermStruct {
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
        let shminfo = interface::ShmidsStruct {
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
        // println!("fobjfdno: {}", fobjfdno);
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
        let rounded_length = interface::round_up_page(self.size as u64);

        let mut useraddr = shmaddr as u32;
        let cage = interface::cagetable_getref(cageid);
        let mut vmmap = cage.vmmap.write();
        let result;
        // pick an address of appropriate size, anywhere
        if useraddr == 0 {
            result = vmmap.find_map_space(rounded_length as u32 >> PAGESHIFT, 1);
        } else {
            // use address user provided as hint to find address
            result = vmmap.find_map_space_with_hint(
                rounded_length as u32 >> PAGESHIFT,
                1,
                useraddr as u32,
            );
        }

        // did not find desired memory region
        if result.is_none() {
            return syscall_error(Errno::ENOMEM, "shm", "no memory") as i32;
        }

        let space = result.unwrap();
        useraddr = (space.start() << PAGESHIFT) as u32;

        let sysaddr = vmmap.user_to_sys(useraddr);

        // let result = cage.mmap_syscall(sysaddr as *mut u8, rounded_length as usize, prot, (MAP_SHARED as i32) | (MAP_FIXED as i32) | (MAP_ANONYMOUS as i32), fobjfdno, 0);
        let result = unsafe {
            libc::mmap(
                sysaddr as *mut c_void,
                rounded_length as usize,
                prot,
                (MAP_SHARED as i32) | (MAP_FIXED as i32),
                fobjfdno,
                0,
            ) as usize
        };
        if (result as i64) < 0 {
            panic!("map_shm");
        }
        // println!("result raw: {}({:?})", result as i64, result as *mut u8);
        // unsafe {
        //     *(result as *mut u64) = 10;
        // }

        let result = vmmap.sys_to_user(result);

        let _ = vmmap.add_entry_with_overwrite(
            useraddr >> PAGESHIFT,
            (rounded_length >> PAGESHIFT) as u32,
            prot,
            PROT_READ | PROT_WRITE,
            (MAP_SHARED as i32) | (MAP_FIXED as i32),
            MemoryBackingType::SharedMemory(fobjfdno as u64),
            0,
            0,
            cageid,
        );

        return result as i32;
    }

    // unmap shared segment, decrease attachments
    // decrease references within attached cages map
    pub fn unmap_shm(&mut self, shmaddr: *mut u8, cageid: u64) {
        let cage = interface::cagetable_getref(cageid);
        let vmmap = cage.vmmap.read();
        let sysaddr = vmmap.user_to_sys(shmaddr as u32);
        interface::libc_mmap(
            sysaddr as *mut u8,
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

    pub fn new_keyid(&self) -> i32 {
        self.nextid
            .fetch_add(1, interface::RustAtomicOrdering::Relaxed)
    }
}
