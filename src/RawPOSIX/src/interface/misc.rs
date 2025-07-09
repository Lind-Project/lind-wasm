// Misc functions for interface
// Random, locks, etc.
#![allow(dead_code)]

pub use dashmap::{
    mapref::entry::Entry as RustHashEntry, DashMap as RustHashMap, DashSet as RustHashSet,
};
pub use parking_lot::{
    Condvar, Mutex, RwLock as RustLock, RwLockReadGuard as RustLockReadGuard,
    RwLockWriteGuard as RustLockWriteGuard,
};
use std::cell::RefCell;
pub use std::cmp::{max as rust_max, min as rust_min};
pub use std::collections::VecDeque as RustDeque;
use std::fs::File;
use std::io::{self, Read, Write};
use std::str::{from_utf8, Utf8Error};
pub use std::sync::atomic::{
    AtomicBool as RustAtomicBool, AtomicI32 as RustAtomicI32, AtomicU16 as RustAtomicU16,
    AtomicU32 as RustAtomicU32, AtomicU64 as RustAtomicU64, AtomicUsize as RustAtomicUsize,
    Ordering as RustAtomicOrdering,
};
pub use std::sync::Arc as RustRfc;
pub use std::thread::spawn as helper_thread;

use libc::{mmap, pthread_exit, pthread_kill, pthread_self, sched_yield};
use std::ffi::c_void;

pub use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
pub use serde_cbor::{
    from_slice as serde_deserialize_from_bytes, ser::to_vec_packed as serde_serialize_to_bytes,
};

use crate::interface;
use std::sync::LazyLock;
use std::time::Duration;

// Import constants
use sysdefs::constants::err_const::VERBOSE;
use sysdefs::constants::fs_const::SEM_VALUE_MAX;
// Import data struct
use sysdefs::data::fs_struct::SigsetType;

pub const MAXCAGEID: i32 = 1024;
const EXIT_SUCCESS: i32 = 0;

pub static RUSTPOSIX_TESTSUITE: LazyLock<RustAtomicBool> =
    LazyLock::new(|| RustAtomicBool::new(false));

thread_local! {
    static TRUSTED_SIGNAL_FLAG: RefCell<u64> = RefCell::new(0);
}

use crate::safeposix::cage::Cage;

pub static mut CAGE_TABLE: Vec<Option<RustRfc<Cage>>> = Vec::new();

pub fn check_cageid(cageid: u64) {
    if cageid >= MAXCAGEID as u64 {
        panic!("Cage ID is outside of valid range");
    }
}

pub fn cagetable_init() {
    unsafe {
        for _cage in 0..MAXCAGEID {
            CAGE_TABLE.push(None);
        }
    }
}

pub fn cagetable_insert(cageid: u64, cageobj: Cage) {
    check_cageid(cageid);
    let _insertret = unsafe { CAGE_TABLE[cageid as usize].insert(RustRfc::new(cageobj)) };
}

pub fn cagetable_remove(cageid: u64) {
    check_cageid(cageid);
    unsafe { CAGE_TABLE[cageid as usize].take() };
}

pub fn cagetable_getref(cageid: u64) -> RustRfc<Cage> {
    check_cageid(cageid);
    unsafe { CAGE_TABLE[cageid as usize].as_ref().unwrap().clone() }
}

pub fn cagetable_getref_opt(cageid: u64) -> Option<RustRfc<Cage>> {
    check_cageid(cageid);
    unsafe {
        match CAGE_TABLE[cageid as usize].as_ref() {
            Some(cage) => Some(cage.clone()),
            None => None,
        }
    }
}

pub fn cagetable_clear() {
    let mut exitvec = Vec::new();
    unsafe {
        for cage in CAGE_TABLE.iter_mut() {
            let cageopt = cage.take();
            if cageopt.is_some() {
                exitvec.push(cageopt.unwrap());
            }
        }
    }

    for cage in exitvec {
        cage.exit_syscall(EXIT_SUCCESS);
    }
}

pub fn log_from_ptr(buf: *const u8, length: usize) {
    if let Ok(s) = from_utf8(unsafe { std::slice::from_raw_parts(buf, length) }) {
        log_to_stdout(s);
    }
}

// Print text to stdout
pub fn log_to_stdout(s: &str) {
    print!("{}", s);
}

pub fn log_verbose(s: &str) {
    if *VERBOSE.get().unwrap() > 0 {
        log_to_stdout(s);
    }
}

// Print text to stderr
pub fn log_to_stderr(s: &str) {
    eprintln!("{}", s);
}

// Flush contents of stdout
pub fn flush_stdout() {
    io::stdout().flush().unwrap();
}

pub fn get_errno() -> i32 {
    (unsafe { *libc::__errno_location() }) as i32
}

// Cancellation functions

pub fn lind_threadexit() {
    unsafe {
        pthread_exit(0 as *mut c_void);
    }
}

pub fn lind_threadkill(thread_id: u64, sig: i32) -> i32 {
    unsafe { pthread_kill(thread_id as libc::pthread_t, sig) as i32 }
}

pub fn get_pthreadid() -> u64 {
    unsafe { pthread_self() as u64 }
}

pub fn lind_yield() {
    unsafe {
        sched_yield();
    }
}

// this function checks if a thread is killable and returns that state
pub fn check_thread(cageid: u64, tid: u64) -> bool {
    let cage = cagetable_getref(cageid);
    let killable = *cage.thread_table.get(&tid).unwrap();
    killable
}

// in-rustposix cancelpoints checks if the thread is killable,
// and if sets killable back to false and kills the thread
pub fn cancelpoint(cageid: u64) {
    if RUSTPOSIX_TESTSUITE.load(RustAtomicOrdering::Relaxed) {
        return;
    } // we don't use this when testing rustposix standalone

    let pthread_id = get_pthreadid();
    if check_thread(cageid, pthread_id) {
        let cage = cagetable_getref(cageid);
        cage.thread_table.insert(pthread_id, false);
        lind_threadexit();
    }
}

pub fn signalflag_set(value: u64) {
    TRUSTED_SIGNAL_FLAG.with(|v| *v.borrow_mut() = value);
}

pub fn signalflag_get() -> u64 {
    TRUSTED_SIGNAL_FLAG.with(|v| *v.borrow())
}

pub fn sigcheck() -> bool {
    if RUSTPOSIX_TESTSUITE.load(RustAtomicOrdering::Relaxed) {
        return false;
    }

    let boolptr = signalflag_get() as *const bool;
    let sigbool = unsafe { *boolptr };

    sigbool
}

pub fn fillrandom(bufptr: *mut u8, count: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts_mut(bufptr, count) };
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(false)
        .open("/dev/urandom")
        .unwrap();
    f.read(slice).unwrap() as i32
}
pub fn fillzero(bufptr: *mut u8, count: usize) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts_mut(bufptr, count) };
    for i in 0..count {
        slice[i] = 0u8;
    }
    count as i32
}

pub fn fill(bufptr: *mut u8, count: usize, values: &Vec<u8>) -> i32 {
    let slice = unsafe { std::slice::from_raw_parts_mut(bufptr, count) };
    slice.copy_from_slice(&values[..count]);
    count as i32
}

pub fn copy_fromrustdeque_sized(bufptr: *mut u8, count: usize, vecdeq: &RustDeque<u8>) {
    let (slice1, slice2) = vecdeq.as_slices();
    if slice1.len() >= count {
        unsafe {
            std::ptr::copy(slice1.as_ptr(), bufptr, count);
        }
    } else {
        unsafe {
            std::ptr::copy(slice1.as_ptr(), bufptr, slice1.len());
        }
        unsafe {
            std::ptr::copy(
                slice2.as_ptr(),
                bufptr.wrapping_offset(slice1.len() as isize),
                count - slice1.len(),
            );
        }
    }
}

pub fn extend_fromptr_sized(bufptr: *const u8, count: usize, vecdeq: &mut RustDeque<u8>) {
    let byteslice = unsafe { std::slice::from_raw_parts(bufptr, count) };
    vecdeq.extend(byteslice.iter());
}

// Wrapper to return a dictionary (hashmap)
pub fn new_hashmap<K: std::cmp::Eq + std::hash::Hash, V>() -> RustHashMap<K, V> {
    RustHashMap::new()
}

#[cfg(target_os = "macos")]
type CharPtr = *const u8;

#[cfg(not(target_os = "macos"))]
type CharPtr = *const i8;

pub unsafe fn charstar_to_ruststr<'a>(cstr: CharPtr) -> Result<&'a str, Utf8Error> {
    std::ffi::CStr::from_ptr(cstr as *const _).to_str() //returns a result to be unwrapped later
}

pub fn libc_mmap(addr: *mut u8, len: usize, prot: i32, flags: i32, fildes: i32, off: i64) -> i32 {
    return ((unsafe { mmap(addr as *mut c_void, len, prot, flags, fildes, off) } as i64)
        & 0xffffffff) as i32;
}

// Sigset Operations
//
// sigsetops defined here are different from the ones in glibc. Since the sigset is just a u64
// bitmask, we can just return the modified version of the sigset instead of changing it in-place.
// It would also avoid any ownership issue and make the code cleaner.

pub fn lind_sigemptyset() -> SigsetType {
    0
}

pub fn lind_sigfillset() -> SigsetType {
    u64::MAX
}

pub fn lind_sigaddset(set: SigsetType, signum: i32) -> SigsetType {
    set | (1 << (signum - 1))
}

pub fn lind_sigdelset(set: SigsetType, signum: i32) -> SigsetType {
    set & !(1 << (signum - 1))
}

pub fn lind_sigismember(set: SigsetType, signum: i32) -> bool {
    set & (1 << (signum - 1)) != 0
}

#[derive(Debug)]
pub struct AdvisoryLock {
    //0 signifies unlocked, -1 signifies locked exclusively, positive number signifies that many shared lock holders
    advisory_lock: RustRfc<Mutex<i32>>,
    advisory_condvar: Condvar,
}
