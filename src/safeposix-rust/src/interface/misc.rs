// Author: Nicholas Renner
//
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

use libc::{mmap, pthread_exit, pthread_kill, pthread_self, sched_yield, syscall, SYS_futex};
use std::ffi::c_void;

pub use serde::{Deserialize as SerdeDeserialize, Serialize as SerdeSerialize};
pub use serde_cbor::{
    from_slice as serde_deserialize_from_bytes, ser::to_vec_packed as serde_serialize_to_bytes,
};

use crate::interface;
use crate::interface::errnos::VERBOSE;
use crate::interface::types::SigsetType;
use crate::safeposix::syscalls::fs_constants::SEM_VALUE_MAX;
use std::sync::LazyLock;
use std::time::Duration;

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
    unsafe { pthread_kill(thread_id, sig) as i32 }
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

// Signals
pub fn lind_kill_from_id(cage_id: u64, sig: i32) {
    if let Some(cage) = cagetable_getref_opt(cage_id as u64) {
        let cage_main_thread_id = cage.main_threadid.load(RustAtomicOrdering::Relaxed);
        assert!(cage_main_thread_id != 0);
        lind_threadkill(cage_main_thread_id, sig);
    }
}

pub fn libc_futex(uaddr: u64, futex_op: u32, val: u32, val2: u32, uaddr2: u32, val3: u32) -> i32 {
    unsafe { syscall(SYS_futex, uaddr, futex_op, val, val2, uaddr2, val3)  as i32 }
}

#[derive(Debug)]
pub struct AdvisoryLock {
    //0 signifies unlocked, -1 signifies locked exclusively, positive number signifies that many shared lock holders
    advisory_lock: RustRfc<Mutex<i32>>,
    advisory_condvar: Condvar,
}

/*
* AdvisoryLock is used to implement advisory locking for files.
* Specifically, it is used by the flock syscall.
* If works as follows: The underying mutex has a guard value associated with it.
* A guard value of zero indicates that it is unlocked.
* In case an exclusive lock is held, the guard value is set to -1.
* In case a shared lock is held, the guard value is incremented by 1.
*/
impl AdvisoryLock {
    pub fn new() -> Self {
        Self {
            advisory_lock: RustRfc::new(Mutex::new(0)),
            advisory_condvar: Condvar::new(),
        }
    }

    // lock_ex is used to acquire an exclusive lock
    // if the lock cannot be obtained, it waits
    pub fn lock_ex(&self) {
        let mut waitedguard = self.advisory_lock.lock();
        while *waitedguard != 0 {
            self.advisory_condvar.wait(&mut waitedguard);
        }
        *waitedguard = -1;
    }

    // lock_sh is used to acquire a shared lock
    // if the lock cannot be obtained, it waits
    pub fn lock_sh(&self) {
        let mut waitedguard = self.advisory_lock.lock();
        while *waitedguard < 0 {
            self.advisory_condvar.wait(&mut waitedguard);
        }
        *waitedguard += 1;
    }
    // try_lock_ex is used to try to acquire an exclusive lock
    // if the lock cannot be obtained, it returns false
    pub fn try_lock_ex(&self) -> bool {
        if let Some(mut guard) = self.advisory_lock.try_lock() {
            if *guard == 0 {
                *guard = -1;
                return true;
            }
        }
        false
    }
    // try_lock_sh is used to try to acquire a shared lock
    // if the lock cannot be obtained, it returns false
    pub fn try_lock_sh(&self) -> bool {
        if let Some(mut guard) = self.advisory_lock.try_lock() {
            if *guard >= 0 {
                *guard += 1;
                return true;
            }
        }
        false
    }

    /*
     * unlock is used to release a lock
     * If a shared lock was held(guard value > 0), it decrements the guard value by one
     * if no more shared locks are held (i.e. the guard value is now zero), then it notifies a waiting writer
     * If an exclusive lock was held, it sets the guard value to zero and notifies all waiting readers and writers
     */
    pub fn unlock(&self) -> bool {
        let mut guard = self.advisory_lock.lock();

        // check if a shared lock is held
        if *guard > 0 {
            // release one shared lock by decrementing the guard value
            *guard -= 1;

            // if no more shared locks are held, notify a waiting writer and return
            // only a writer could be waiting at this point
            if *guard == 0 {
                self.advisory_condvar.notify_one();
            }
            true
        } else if *guard == -1 {
            // check if an exclusive lock is held
            // release the exclusive lock by setting guard to 0
            *guard = 0;

            // notify any waiting reads or writers and return
            self.advisory_condvar.notify_all();
            true
        } else {
            false
        }
    }
}

pub struct RawMutex {
    inner: libc::pthread_mutex_t,
}

impl RawMutex {
    pub fn create() -> Result<Self, i32> {
        let libcret;
        let mut retval = Self {
            inner: unsafe { std::mem::zeroed() },
        };
        unsafe {
            libcret = libc::pthread_mutex_init(
                (&mut retval.inner) as *mut libc::pthread_mutex_t,
                std::ptr::null(),
            );
        }
        if libcret < 0 {
            Err(libcret)
        } else {
            Ok(retval)
        }
    }

    pub fn lock(&self) -> i32 {
        unsafe {
            libc::pthread_mutex_lock(
                (&self.inner) as *const libc::pthread_mutex_t as *mut libc::pthread_mutex_t,
            )
        }
    }

    pub fn trylock(&self) -> i32 {
        unsafe {
            libc::pthread_mutex_trylock(
                (&self.inner) as *const libc::pthread_mutex_t as *mut libc::pthread_mutex_t,
            )
        }
    }

    pub fn unlock(&self) -> i32 {
        unsafe {
            libc::pthread_mutex_unlock(
                (&self.inner) as *const libc::pthread_mutex_t as *mut libc::pthread_mutex_t,
            )
        }
    }
}

impl std::fmt::Debug for RawMutex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<mutex>")
    }
}

impl Drop for RawMutex {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_mutex_destroy((&mut self.inner) as *mut libc::pthread_mutex_t);
        }
    }
}

pub struct RawCondvar {
    inner: libc::pthread_cond_t,
}

impl RawCondvar {
    pub fn create() -> Result<Self, i32> {
        let libcret;
        let mut retval = Self {
            inner: unsafe { std::mem::zeroed() },
        };
        unsafe {
            libcret = libc::pthread_cond_init(
                (&mut retval.inner) as *mut libc::pthread_cond_t,
                std::ptr::null(),
            );
        }
        if libcret < 0 {
            Err(libcret)
        } else {
            Ok(retval)
        }
    }

    pub fn signal(&self) -> i32 {
        unsafe {
            libc::pthread_cond_signal(
                (&self.inner) as *const libc::pthread_cond_t as *mut libc::pthread_cond_t,
            )
        }
    }

    pub fn broadcast(&self) -> i32 {
        unsafe {
            libc::pthread_cond_broadcast(
                (&self.inner) as *const libc::pthread_cond_t as *mut libc::pthread_cond_t,
            )
        }
    }

    pub fn wait(&self, mutex: &RawMutex) -> i32 {
        unsafe {
            libc::pthread_cond_wait(
                (&self.inner) as *const libc::pthread_cond_t as *mut libc::pthread_cond_t,
                (&mutex.inner) as *const libc::pthread_mutex_t as *mut libc::pthread_mutex_t,
            )
        }
    }

    pub fn timedwait(&self, mutex: &RawMutex, abs_duration: Duration) -> i32 {
        let abstime = libc::timespec {
            tv_sec: abs_duration.as_secs() as i64,
            tv_nsec: (abs_duration.as_nanos() % 1000000000) as i64,
        };
        unsafe {
            libc::pthread_cond_timedwait(
                (&self.inner) as *const libc::pthread_cond_t as *mut libc::pthread_cond_t,
                (&mutex.inner) as *const libc::pthread_mutex_t as *mut libc::pthread_mutex_t,
                (&abstime) as *const libc::timespec,
            )
        }
    }
}

impl Drop for RawCondvar {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_cond_destroy((&mut self.inner) as *mut libc::pthread_cond_t);
        }
    }
}

impl std::fmt::Debug for RawCondvar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<condvar>")
    }
}

/*
* RustSemaphore is the rust version of sem_t
*/
#[derive(Debug)]
pub struct RustSemaphore {
    pub value: Mutex<u32>,
    pub is_shared: RustAtomicBool,
}

// Semaphore implementation
// we busy wait on lock if value is 0, otherwise we decrease the value
// unlock will increase value up to SEM_VALUE_MAX
impl RustSemaphore {
    pub fn new(value_handle: u32, is_shared: bool) -> Self {
        Self {
            value: Mutex::new(value_handle),
            is_shared: RustAtomicBool::new(is_shared),
        }
    }

    pub fn lock(&self) {
        loop {
            // acquire the mutex lock
            let mut value = self.value.lock();
            if *value == 0 {
                // wait for semaphore to be unlocked by another process/thread
                interface::lind_yield();
            } else {
                // decrement the value
                *value = if *value > 0 { *value - 1 } else { 0 };
                break;
            }
        }
    }

    pub fn unlock(&self) -> bool {
        // acquire the mutex lock
        let mut value = self.value.lock();
        // check if the maximum allowable value for a semaphore has been reached
        if *value < SEM_VALUE_MAX {
            // increment the value
            *value = *value + 1;
            return true;
        } else {
            return false;
        }
    }

    pub fn get_value(&self) -> i32 {
        // returns the value of the semaphore
        *self.value.lock() as i32
    }

    pub fn trylock(&self) -> bool {
        // acquire the mutex lock
        let mut value = self.value.lock();
        if *value == 0 {
            // semaphore is locked by another process/thread
            return false;
        } else {
            // decrement the value
            *value = if *value > 0 { *value - 1 } else { 0 };
            return true;
        }
    }

    pub fn timedlock(&self, timeout: Duration) -> bool {
        // start the timer to check for timeout
        let start_time = interface::starttimer();
        loop {
            // acquire the mutex lock
            let mut value = self.value.lock();
            if *value == 0 {
                // check if we have timed out
                let elapsed_time = interface::readtimer(start_time);
                if elapsed_time > timeout {
                    return false;
                }
                // if not timed out wait for semaphore to be unlocked by another process/thread
                interface::lind_yield();
            } else {
                *value = if *value > 0 { *value - 1 } else { 0 };
                return true;
            }
        }
    }
}
