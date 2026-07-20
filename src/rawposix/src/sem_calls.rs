//! Paravirtualized POSIX semaphores for cross-cage (pshared) use.
//!
//! Cross-cage `sem_t` normally relies on two things lind cannot guarantee on
//! every platform: (1) the semaphore value living in a MAP_SHARED page that is
//! physically shared between cages, and (2) futex wait/wake matching across
//! the different host addresses each cage maps that page at. Inside an SGX
//! enclave neither holds: EPC pages cannot be aliased at two linear addresses
//! (the mremap trick used by `fork_vmmap` fails), and the in-enclave futex is
//! keyed by raw virtual address.
//!
//! This module therefore keeps the authoritative state of every pshared
//! semaphore in rawposix itself, which is a single instance shared by all
//! cages. A semaphore is identified not by anything stored in guest memory
//! (the guest page contents cannot be trusted to be shared) but by
//! `(shared region id, offset within the region)`, both derived from the
//! calling cage's vmmap — parent and child compute identical keys because
//! fork copies vmmap entries verbatim. Blocking uses a futex on the
//! `SemState::value` word, which lives in rawposix memory at one single
//! address for every cage, so both the host kernel futex (native) and the
//! address-keyed in-enclave futex (SGX) match waiters and wakers correctly.
//!
//! glibc routes `sem_*` calls here only when the semaphore was initialized
//! with `pshared != 0` (see nptl/sem_wait.c etc. and lind_sem.h); process-
//! private semaphores keep the userspace fast path.

use cage::{get_base_address, get_cage, MemoryBackingType, VmmapOps};
use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, LazyLock};
use sysdefs::constants::err_const::{get_errno, handle_errno, syscall_error, Errno};
use sysdefs::constants::fs_const::PAGESHIFT;

// Futex constants used for blocking on `SemState::value`. Defined locally to
// keep this module self-contained (values match linux/futex.h).
const FUTEX_WAKE: u32 = 1;
const FUTEX_WAIT_BITSET: u32 = 9;
const FUTEX_PRIVATE_FLAG: u32 = 128;
const FUTEX_CLOCK_REALTIME: u32 = 256;
const FUTEX_BITSET_MATCH_ANY: u32 = 0xffffffff;

/// POSIX SEM_VALUE_MAX.
const SEM_VALUE_MAX: u32 = i32::MAX as u32;

// Wait flavors; must match LIND_SEM_WAIT_* in glibc's lind_sem.h.
const SEM_WAIT_BLOCK: u32 = 0;
const SEM_WAIT_TRY: u32 = 1;
const SEM_WAIT_TIMED: u32 = 2;

/// Tag bit distinguishing SysV shm segment ids from anonymous shared region
/// ids in `SemKey::region_id`, so the two id namespaces cannot collide.
const SHM_REGION_TAG: u64 = 1 << 63;

/// Identity of a pshared semaphore: which shared mapping it lives in and
/// where. Offsets are relative to the region start so cages that attach the
/// same segment at different guest addresses (shmat) still agree on the key.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SemKey {
    region_id: u64,
    offset: u32,
}

/// Authoritative state of one pshared semaphore. `value` doubles as the futex
/// word all cages block on; it lives at a single rawposix address.
pub struct SemState {
    value: AtomicU32,
    destroyed: AtomicBool,
}

static SEM_TABLE: LazyLock<DashMap<SemKey, Arc<SemState>>> = LazyLock::new(DashMap::new);

/// Translates a (cage, host address) pair into a `SemKey`.
///
/// The address arrives already translated to a host address by glibc
/// (TRANSLATE_GUEST_POINTER_TO_HOST), so we first convert it back to the
/// cage's guest address, then locate the vmmap entry containing it. Only
/// addresses inside MAP_SHARED mappings are valid pshared semaphore homes.
fn resolve_sem_key(cageid: u64, host_addr: u64, callname: &str) -> Result<SemKey, i32> {
    if get_cage(cageid).is_none() {
        return Err(syscall_error(Errno::ESRCH, callname, "no such cage"));
    }
    let base = get_base_address(cageid) as u64;
    if host_addr < base || host_addr - base > u32::MAX as u64 {
        return Err(syscall_error(
            Errno::EFAULT,
            callname,
            "address outside cage linear memory",
        ));
    }
    let guest = (host_addr - base) as u32;

    let cage = get_cage(cageid).unwrap();
    let vmmap = cage.vmmap.read();
    let entry = match vmmap.find_page(guest >> PAGESHIFT) {
        Some(e) => e,
        None => {
            return Err(syscall_error(Errno::EFAULT, callname, "address not mapped"));
        }
    };
    let region_id = match entry.backing {
        MemoryBackingType::SharedAnonymous(id) => id,
        MemoryBackingType::SharedMemory(shmid) => shmid | SHM_REGION_TAG,
        _ => {
            return Err(syscall_error(
                Errno::EINVAL,
                callname,
                "pshared semaphore must live in a MAP_SHARED mapping",
            ));
        }
    };
    let region_start = entry.page_num << PAGESHIFT;
    Ok(SemKey {
        region_id,
        offset: guest - region_start,
    })
}

/// Looks up the live state for a key, cloning the Arc so no DashMap guard is
/// held while blocking.
fn lookup_sem(key: &SemKey, callname: &str) -> Result<Arc<SemState>, i32> {
    match SEM_TABLE.get(key) {
        Some(entry) => Ok(entry.value().clone()),
        None => Err(syscall_error(
            Errno::EINVAL,
            callname,
            "semaphore not initialized",
        )),
    }
}

/// Futex-wakes `count` waiters blocked on `state.value`.
fn sem_futex_wake(state: &SemState, count: u32) {
    unsafe {
        libc::syscall(
            libc::SYS_futex,
            &state.value as *const AtomicU32 as u64,
            FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
            count,
            0u64,
            0u64,
            0u32,
        );
    }
}

/// Core wait loop shared by sem_wait / sem_trywait / sem_timedwait.
///
/// `abs_sec`/`abs_nsec` are an absolute timeout on `clockid` (only used for
/// SEM_WAIT_TIMED; CLOCK_REALTIME and CLOCK_MONOTONIC are supported, matching
/// what glibc's sem_timedwait/sem_clockwait can produce).
fn sem_do_wait(state: &SemState, flags: u32, abs_sec: i64, abs_nsec: i64, clockid: i32) -> i32 {
    if flags == SEM_WAIT_TIMED && (abs_nsec < 0 || abs_nsec >= 1_000_000_000) {
        return syscall_error(Errno::EINVAL, "sem_wait", "invalid timeout");
    }
    loop {
        if state.destroyed.load(Ordering::SeqCst) {
            return syscall_error(Errno::EINVAL, "sem_wait", "semaphore destroyed");
        }
        let v = state.value.load(Ordering::SeqCst);
        if v > 0 {
            if state
                .value
                .compare_exchange(v, v - 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                return 0;
            }
            continue;
        }
        if flags == SEM_WAIT_TRY {
            return syscall_error(Errno::EAGAIN, "sem_trywait", "semaphore count is zero");
        }

        // Block until the value leaves zero. FUTEX_WAIT_BITSET takes an
        // absolute timeout; without CLOCK_REALTIME it is interpreted against
        // CLOCK_MONOTONIC, matching sem_clockwait semantics.
        let mut op = FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG;
        let ts;
        let ts_ptr = if flags == SEM_WAIT_TIMED {
            if clockid == libc::CLOCK_REALTIME {
                op |= FUTEX_CLOCK_REALTIME;
            }
            ts = libc::timespec {
                tv_sec: abs_sec,
                tv_nsec: abs_nsec,
            };
            &ts as *const libc::timespec as u64
        } else {
            0u64
        };
        let ret = unsafe {
            libc::syscall(
                libc::SYS_futex,
                &state.value as *const AtomicU32 as u64,
                op,
                0u32, // sleep only while the value is still zero
                ts_ptr,
                0u64,
                FUTEX_BITSET_MATCH_ANY,
            )
        };
        if ret < 0 {
            let errno = get_errno();
            if errno == Errno::EAGAIN as i32 {
                // Value changed between our load and the futex compare; retry.
                continue;
            }
            // EINTR / ETIMEDOUT (and anything unexpected) surface to glibc.
            return handle_errno(errno, "sem_wait");
        }
        // Woken up: loop back and race for a token.
    }
}

/// sem_init(sem, pshared=1, value): registers the semaphore in SEM_TABLE.
///
/// Re-initializing an existing key resets it (POSIX leaves re-init of an
/// in-use semaphore undefined; resetting matches the native glibc behavior of
/// simply overwriting the fields).
pub extern "C" fn sem_init_syscall(
    cageid: u64,
    uaddr_arg: u64,
    _uaddr_cageid: u64,
    value_arg: u64,
    _value_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let value = value_arg as u32;
    if value > SEM_VALUE_MAX {
        return syscall_error(Errno::EINVAL, "sem_init", "value exceeds SEM_VALUE_MAX");
    }
    let key = match resolve_sem_key(cageid, uaddr_arg, "sem_init") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let state = Arc::new(SemState {
        value: AtomicU32::new(value),
        destroyed: AtomicBool::new(false),
    });
    SEM_TABLE.insert(key, state);
    0
}

/// sem_wait / sem_trywait / sem_timedwait / sem_clockwait entry point.
/// arg2 = wait flavor (SEM_WAIT_*), arg3/arg4 = absolute timeout sec/nsec,
/// arg5 = clockid (CLOCK_REALTIME or CLOCK_MONOTONIC).
pub extern "C" fn sem_wait_syscall(
    cageid: u64,
    uaddr_arg: u64,
    _uaddr_cageid: u64,
    flags_arg: u64,
    _flags_cageid: u64,
    abs_sec_arg: u64,
    _abs_sec_cageid: u64,
    abs_nsec_arg: u64,
    _abs_nsec_cageid: u64,
    clockid_arg: u64,
    _clockid_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let flags = flags_arg as u32;
    if flags != SEM_WAIT_BLOCK && flags != SEM_WAIT_TRY && flags != SEM_WAIT_TIMED {
        return syscall_error(Errno::EINVAL, "sem_wait", "invalid wait flavor");
    }
    let key = match resolve_sem_key(cageid, uaddr_arg, "sem_wait") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let state = match lookup_sem(&key, "sem_wait") {
        Ok(s) => s,
        Err(e) => return e,
    };
    sem_do_wait(
        &state,
        flags,
        abs_sec_arg as i64,
        abs_nsec_arg as i64,
        clockid_arg as i32,
    )
}

/// sem_post: adds a token and wakes one waiter.
pub extern "C" fn sem_post_syscall(
    cageid: u64,
    uaddr_arg: u64,
    _uaddr_cageid: u64,
    _arg2: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let key = match resolve_sem_key(cageid, uaddr_arg, "sem_post") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let state = match lookup_sem(&key, "sem_post") {
        Ok(s) => s,
        Err(e) => return e,
    };
    loop {
        let v = state.value.load(Ordering::SeqCst);
        if v >= SEM_VALUE_MAX {
            return syscall_error(Errno::EOVERFLOW, "sem_post", "value would exceed SEM_VALUE_MAX");
        }
        if state
            .value
            .compare_exchange(v, v + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            // Wake one waiter on every post (not only on 0->1): waiters that
            // went to sleep while the value was zero are only guaranteed a
            // wakeup this way when several posts race with several waiters.
            sem_futex_wake(&state, 1);
            return 0;
        }
    }
}

/// sem_getvalue: returns the current count as the (non-negative) return value.
pub extern "C" fn sem_getvalue_syscall(
    cageid: u64,
    uaddr_arg: u64,
    _uaddr_cageid: u64,
    _arg2: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let key = match resolve_sem_key(cageid, uaddr_arg, "sem_getvalue") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let state = match lookup_sem(&key, "sem_getvalue") {
        Ok(s) => s,
        Err(e) => return e,
    };
    state.value.load(Ordering::SeqCst) as i32
}

/// sem_destroy: marks the semaphore dead, wakes all waiters (they return
/// EINVAL) and drops it from the table. Destroying a semaphore other threads
/// are blocked on is undefined behavior per POSIX; failing their waits loudly
/// beats leaving them asleep forever.
pub extern "C" fn sem_destroy_syscall(
    cageid: u64,
    uaddr_arg: u64,
    _uaddr_cageid: u64,
    _arg2: u64,
    _arg2_cageid: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32 {
    let key = match resolve_sem_key(cageid, uaddr_arg, "sem_destroy") {
        Ok(k) => k,
        Err(e) => return e,
    };
    let state = match lookup_sem(&key, "sem_destroy") {
        Ok(s) => s,
        Err(e) => return e,
    };
    state.destroyed.store(true, Ordering::SeqCst);
    sem_futex_wake(&state, u32::MAX);
    SEM_TABLE.remove(&key);
    0
}
