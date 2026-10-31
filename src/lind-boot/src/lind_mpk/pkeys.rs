//! Intel MPK protection-key management.
//!
//! Every cage memory reservation (`CAGE_RESERVATION_SIZE` bytes: the 4 GiB
//! cage memory plus the GS-fork arena) owns one protection key. Every page of
//! the reservation is tagged with that key, and the cage runs with a PKRU
//! that only allows its own key. Key 0 is the supervisor key: all host memory
//! (supervisor stacks, context stacks, runtime heap) keeps key 0 and is
//! unreachable from guest code.
//!
//! A GS-forked child keeps executing the code of its family root, but PKRU
//! does not apply to instruction fetches and the rewriter turns every data
//! access (including RIP-relative ones) into a GS-relative access of the
//! child's own copy, so the child needs no access to the root's key.
//!
//! lind-mpk requires PKU: `init` fails if the kernel hands out no keys.

use crate::lind_mpk::gs_fork::CAGE_RESERVATION_SIZE;
use crate::lind_mpk::execute::MPK_MEMORY_SIZE;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// Key used for all supervisor memory. Never handed out to a cage.
pub const SUPERVISOR_PKEY: u32 = 0;
/// Number of protection keys provided by x86-64 MPK.
pub const NUM_PKEYS: u32 = 16;
/// PKRU value allowing every key; used while the supervisor runs.
pub const SUPERVISOR_PKRU: u32 = 0;

/// Tracks which protection keys are still available for cages.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PkeyAllocator {
    /// Bit `k` is set if key `k` is owned by lind and not assigned to a cage.
    free: u16,
}

impl PkeyAllocator {
    /// An allocator owning `keys`. The supervisor key is never handed out.
    pub fn new(keys: impl IntoIterator<Item = u32>) -> Self {
        let mut allocator = Self::default();
        for key in keys {
            allocator.free(key);
        }
        allocator
    }

    /// Takes the lowest free key, or `None` if all keys are assigned.
    pub fn alloc(&mut self) -> Option<u32> {
        if self.free == 0 {
            return None;
        }
        let key = self.free.trailing_zeros();
        self.free &= !(1 << key);
        Some(key)
    }

    /// Returns `key` to the pool.
    pub fn free(&mut self, key: u32) {
        if key != SUPERVISOR_PKEY && key < NUM_PKEYS {
            self.free |= 1 << key;
        }
    }

    pub fn available(&self) -> u32 {
        self.free.count_ones()
    }

    pub fn is_free(&self, key: u32) -> bool {
        key < NUM_PKEYS && self.free & (1 << key) != 0
    }
}

struct PkeyState {
    initialized: bool,
    allocator: PkeyAllocator,
    /// Reservation base -> key of every live cage reservation.
    reservations: BTreeMap<usize, u32>,
}

static STATE: Mutex<PkeyState> = Mutex::new(PkeyState {
    initialized: false,
    allocator: PkeyAllocator { free: 0 },
    reservations: BTreeMap::new(),
});

fn mpk_debug(message: impl AsRef<str>) {
    if std::env::var_os("LIND_MPK_DEBUG").is_some() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

/// Claims every protection key the kernel will hand out. Claiming all keys
/// also keeps the kernel from using one as its execute-only key. Must run
/// before any cage memory is mapped and before threads that run guest code
/// are spawned (they inherit PKRU). Fails if the kernel provides no keys.
pub fn init() -> std::io::Result<()> {
    let mut state = STATE.lock().unwrap();
    if state.initialized {
        return Ok(());
    }
    let mut keys = Vec::new();
    loop {
        let key = unsafe { libc::syscall(libc::SYS_pkey_alloc, 0, 0) };
        if key < 0 {
            break;
        }
        keys.push(key as u32);
    }
    if keys.is_empty() {
        let error = std::io::Error::last_os_error();
        return Err(std::io::Error::new(
            error.kind(),
            format!("lind-mpk requires Intel MPK, but pkey_alloc failed: {error}"),
        ));
    }
    state.allocator = PkeyAllocator::new(keys.iter().copied());
    state.initialized = true;
    unsafe { wrpkru(SUPERVISOR_PKRU) };
    mpk_debug(format!("protection keys enabled, cage keys: {keys:?}"));
    Ok(())
}

/// PKRU allowing access to only `keys`.
pub fn pkru_for_keys(keys: &[u32]) -> u32 {
    let mut pkru = 0x5555_5555u32;
    for &key in keys {
        if key < NUM_PKEYS {
            pkru &= !(0b11 << (2 * key));
        }
    }
    pkru
}

/// Writes PKRU. Requires PKU support.
#[inline]
pub unsafe fn wrpkru(pkru: u32) {
    unsafe {
        std::arch::asm!(
            "wrpkru",
            in("eax") pkru,
            in("ecx") 0,
            in("edx") 0,
            options(nostack, preserves_flags),
        );
    }
}

/// Reads PKRU. Requires PKU support.
#[inline]
pub unsafe fn rdpkru() -> u32 {
    let pkru: u32;
    unsafe {
        std::arch::asm!(
            "rdpkru",
            in("ecx") 0,
            out("eax") pkru,
            out("edx") _,
            options(nostack, preserves_flags, nomem),
        );
    }
    pkru
}

/// Tags `[addr, addr + len)` with `key`, setting its protection to `prot`.
pub fn tag(addr: usize, len: usize, prot: i32, key: u32) -> std::io::Result<()> {
    if len == 0 {
        return Ok(());
    }
    let ret = unsafe {
        libc::syscall(
            libc::SYS_pkey_mprotect,
            addr as *mut libc::c_void,
            len,
            prot,
            key as libc::c_int,
        )
    };
    if ret != 0 {
        let error = std::io::Error::last_os_error();
        mpk_debug(format!(
            "pkey_mprotect({addr:#x}, {len:#x}, {prot:#x}, {key}) failed: {error}"
        ));
        return Err(error);
    }
    Ok(())
}

/// Allocates a key for the reservation at `base` and tags it: the cage memory
/// read/write, the GS-fork arena `PROT_NONE`.
pub fn assign_reservation(base: usize) -> std::io::Result<u32> {
    let key = {
        let mut state = STATE.lock().unwrap();
        if !state.initialized {
            return Err(std::io::Error::other(
                "lind-mpk requires Intel MPK, but protection keys are unavailable",
            ));
        }
        let key = state.allocator.alloc().ok_or_else(|| {
            std::io::Error::other("no free protection key for a new cage")
        })?;
        state.reservations.insert(base, key);
        key
    };
    let result = tag(base, MPK_MEMORY_SIZE, libc::PROT_READ | libc::PROT_WRITE, key).and_then(|_| {
        tag(
            base + MPK_MEMORY_SIZE,
            CAGE_RESERVATION_SIZE - MPK_MEMORY_SIZE,
            libc::PROT_NONE,
            key,
        )
    });
    if let Err(error) = result {
        release_reservation(base);
        return Err(error);
    }
    mpk_debug(format!("assigned pkey {key} to reservation {base:#x}"));
    Ok(key)
}

/// Returns the key of the reservation at `base` to the pool. Call when the
/// reservation is unmapped.
pub fn release_reservation(base: usize) {
    let mut state = STATE.lock().unwrap();
    if let Some(key) = state.reservations.remove(&base) {
        state.allocator.free(key);
        mpk_debug(format!("released pkey {key} of reservation {base:#x}"));
    }
}

/// Key of the reservation starting at `base`.
pub fn reservation_key(base: usize) -> Option<u32> {
    STATE.lock().unwrap().reservations.get(&base).copied()
}

/// Key of the reservation containing `addr`.
pub fn key_for_addr(addr: usize) -> Option<u32> {
    let state = STATE.lock().unwrap();
    let (&base, &key) = state.reservations.range(..=addr).next_back()?;
    (addr - base < CAGE_RESERVATION_SIZE).then_some(key)
}

/// Re-applies the key of the containing reservation to `[addr, addr + len)`,
/// e.g. after a fresh `mmap` reset it to key 0.
pub fn retag(addr: usize, len: usize, prot: i32) {
    if let Some(key) = key_for_addr(addr) {
        let _ = tag(addr, len, prot, key);
    }
}

/// Hook called by rawposix after it maps memory for cage `_cageid`.
pub fn cage_map_hook(_cageid: u64, addr: usize, len: usize, prot: i32) {
    retag(addr, len, prot);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_never_hands_out_supervisor_key() {
        let mut allocator = PkeyAllocator::new(0..NUM_PKEYS);
        assert!(!allocator.is_free(SUPERVISOR_PKEY));
        assert_eq!(allocator.available(), 15);
        let keys: Vec<u32> = std::iter::from_fn(|| allocator.alloc()).collect();
        assert_eq!(keys, (1..16).collect::<Vec<_>>());
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn allocator_reuses_freed_keys() {
        let mut allocator = PkeyAllocator::new([3, 5]);
        assert_eq!(allocator.alloc(), Some(3));
        assert_eq!(allocator.alloc(), Some(5));
        assert_eq!(allocator.alloc(), None);
        allocator.free(5);
        assert_eq!(allocator.alloc(), Some(5));
        allocator.free(SUPERVISOR_PKEY);
        allocator.free(NUM_PKEYS);
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn pkru_allows_only_given_keys() {
        assert_eq!(pkru_for_keys(&[]), 0x5555_5555);
        assert_eq!(pkru_for_keys(&[1]), 0x5555_5551);
        assert_eq!(pkru_for_keys(&[2, 1]), 0x5555_5541);
        assert_eq!(pkru_for_keys(&[15]), 0x1555_5555);
    }

    #[test]
    fn unassigned_addresses_have_no_key() {
        assert_eq!(key_for_addr(0x1000_0000), None);
        assert_eq!(reservation_key(0x1000_0000), None);
        retag(0x1000_0000, 0x1000, libc::PROT_READ);
    }
}
