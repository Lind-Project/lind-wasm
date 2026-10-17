//! Edge cases for `copy_data_between_cages` in Strncpy mode around page
//! boundaries and the `max_len` bound.
use cage::memory::{init_vmmap, MemoryBackingType, VmmapOps};
use cage::timer::IntervalTimer;
use cage::{
    add_cage, cagetable_init, AtomicBool, AtomicU64, Cage, Condvar, Mutex, PathBuf, RwLock,
};
use std::alloc::{alloc_zeroed, Layout};
use std::sync::Arc;
use sysdefs::constants::fs_const::{PAGESIZE, PROT_NONE, PROT_READ, PROT_WRITE};
use threei::{copy_data_between_cages, ELINDAPIABORTED};

const SRC: u64 = 5;
const DST: u64 = 6;
const PG: u64 = PAGESIZE as u64;

fn make_cage(cageid: u64) -> Cage {
    Cage {
        cageid,
        parent: 1,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        signalhandler: Default::default(),
        sigset: AtomicU64::new(0),
        pending_signals: RwLock::new(vec![]),
        epoch_handler: Default::default(),
        os_tid_map: Default::default(),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(cageid),
        zombies: Mutex::new(vec![]),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(cage::Vmmap::new()),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
        execd: AtomicBool::new(false),
    }
}

/// 8 pages of backing memory; `entries` are (first page, npages, prot).
fn setup(cageid: u64, entries: &[(u32, u32, i32)]) -> u64 {
    add_cage(cageid, make_cage(cageid));
    let base = unsafe { alloc_zeroed(Layout::from_size_align(8 * PG as usize, 1 << 16).unwrap()) };
    init_vmmap(cageid, base as usize, None);
    let cage = cage::get_cage(cageid).unwrap();
    let mut vmmap = cage.vmmap.write();
    for &(page, npages, prot) in entries {
        vmmap
            .add_entry_with_overwrite(
                page,
                npages,
                prot,
                PROT_READ | PROT_WRITE,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                cageid,
            )
            .unwrap();
    }
    base as u64
}

fn put(addr: u64, bytes: &[u8]) {
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len()) }
}

fn strncpy(src: u64, dst: u64, max_len: u64) -> u64 {
    copy_data_between_cages(SRC, SRC, src, SRC, dst, DST, max_len, 0, 1, 0, 0, 0, 0, 0)
}

fn read_cstr(addr: u64) -> Vec<u8> {
    let mut v = vec![];
    unsafe {
        let mut p = addr as *const u8;
        while *p != 0 {
            v.push(*p);
            p = p.add(1);
        }
    }
    v
}

#[test]
fn strncpy_page_edges() {
    cagetable_init();
    let rw = PROT_READ | PROT_WRITE;
    // src: pages 0..4 RW, page 4 PROT_NONE, page 5 unmapped, pages 6..8 RO
    let sb = setup(SRC, &[(0, 4, rw), (4, 1, PROT_NONE), (6, 2, PROT_READ)]);
    let db = setup(DST, &[(0, 8, rw)]);
    let dst = db + PG;

    // short string inside one page
    let src = sb + PG + 100;
    put(src, b"hello\0");
    assert_eq!(strncpy(src, dst, 4096), dst);
    assert_eq!(read_cstr(dst), b"hello");

    // string crossing the page 1 -> 2 boundary
    let src = sb + 2 * PG - 6;
    put(src, b"abcdefghijkl\0");
    assert_eq!(strncpy(src, dst, 4096), dst);
    assert_eq!(read_cstr(dst), b"abcdefghijkl");

    // NUL on the very last byte before the PROT_NONE page
    let src = sb + 4 * PG - 96;
    put(src, &[b'x'; 95]);
    put(src + 95, b"\0");
    assert_eq!(strncpy(src, dst, 4096), dst);
    assert_eq!(read_cstr(dst).len(), 95);

    // unterminated string running into the PROT_NONE page
    put(src + 95, b"y");
    assert_eq!(strncpy(src, dst, 4096), ELINDAPIABORTED);
    // ... but max_len stopping before the bad page and before any NUL is also an abort
    assert_eq!(strncpy(src, dst, 50), ELINDAPIABORTED);

    // max_len bound: "hello" needs 6 bytes including NUL
    let src = sb + PG + 100;
    assert_eq!(strncpy(src, dst, 5), ELINDAPIABORTED);
    assert_eq!(strncpy(src, dst, 6), dst);
    assert_eq!(strncpy(src, dst, 0), ELINDAPIABORTED);

    // string in a read-only region is fine as a source (the host buffer
    // itself is writable, only the vmmap marks it read-only)
    let src = sb + 6 * PG + 4000;
    put(src, b"readonly\0");
    assert_eq!(strncpy(src, dst, 4096), dst);
    assert_eq!(read_cstr(dst), b"readonly");

    // string running from the RO region off the end of the vmmap
    let src = sb + 8 * PG - 4;
    put(src, b"tail");
    assert_eq!(strncpy(src, dst, 4096), ELINDAPIABORTED);

    // source in the unmapped page
    assert_eq!(strncpy(sb + 5 * PG, dst, 4096), ELINDAPIABORTED);

    // long string spanning several pages
    let src = sb + 10;
    let long = vec![b'z'; 3 * PG as usize];
    put(src, &long);
    put(src + long.len() as u64, b"\0");
    let dst2 = db + 10;
    assert_eq!(strncpy(src, dst2, 1 << 20), dst2);
    assert_eq!(read_cstr(dst2).len(), long.len());
}
