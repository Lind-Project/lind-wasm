/// Per-cage MPK (Memory Protection Keys) support.
///
/// Each Wasm cage is assigned a kernel protection key (pkey 1–14). Its linear
/// memory is tagged with that key via `pkey_mprotect`. The PKRU register is
/// tightened when the CPU enters a cage's Wasm code and relaxed when it returns
/// to host code, preventing one cage from reading or writing another cage's
/// linear memory.
///
/// All hardware-specific operations are guarded by
/// `#[cfg(all(target_arch = "x86_64", target_os = "linux"))]`; on other
/// platforms every function is a safe no-op.
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// Process-wide pool of pre-allocated protection keys (pkeys 1–14).
/// Pkey 0 is the kernel default and is never allocated here.
static PKEY_POOL: Mutex<Vec<u32>> = Mutex::new(Vec::new());
static MPK_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// Allocate up to 14 pkeys from the kernel and stash them in the pool.
/// Must be called once, after `cagetable_init()`, before any cage is created.
pub fn init_pkey_pool() {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let mut pool = PKEY_POOL.lock();
        for _ in 0..14 {
            // pkey_alloc(flags=0, access_rights=0)
            let r = unsafe { libc::syscall(libc::SYS_pkey_alloc, 0i64, 0i64) };
            if r < 0 {
                break;
            }
            pool.push(r as u32);
        }
        if !pool.is_empty() {
            MPK_AVAILABLE.store(true, Ordering::Relaxed);
        }
    }
}

/// Returns true if the kernel granted at least one protection key.
pub fn is_mpk_available() -> bool {
    MPK_AVAILABLE.load(Ordering::Relaxed)
}

/// Take a free pkey from the pool.  Returns `None` when the pool is exhausted
/// (more than 14 live cages) — the cage will run without MPK isolation.
pub fn alloc_pkey() -> Option<u32> {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    if is_mpk_available() {
        return PKEY_POOL.lock().pop();
    }
    None
}

/// Return a pkey to the pool after a cage is finalized.
pub fn free_pkey(key: u32) {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    if is_mpk_available() {
        PKEY_POOL.lock().push(key);
    }
}

/// Tag the memory region `[base, base+len)` with `pkey` using
/// `pkey_mprotect(base, len, prot, pkey)`.
/// `prot` should match the region's intended page protection.
pub fn tag_memory(base: *mut u8, len: usize, prot: i32, pkey: u32) {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    if is_mpk_available() {
        unsafe {
            libc::syscall(
                libc::SYS_pkey_mprotect,
                base as usize,
                len,
                prot as usize,
                pkey as usize,
            );
        }
    }
}

/// Compute the PKRU value that allows access to key 0 (host/shared memory)
/// and `pkey_id` (the cage's own memory), disabling all other keys.
///
/// PKRU layout: bits 2n and 2n+1 are the Access-Disable and Write-Disable
/// bits for key n.  0xFFFF_FFFF disables every key; we clear the two bits for
/// key 0 and for `pkey_id`.
pub fn cage_pkru_mask(pkey_id: u32) -> u32 {
    // Clear AD+WD for key 0 and for the cage's key.
    0xFFFF_FFFFu32 & !(0b11u32) & !(0b11u32 << (pkey_id * 2))
}

/// Write the PKRU register to restrict access to only key 0 and `pkey_id`.
/// Called when the CPU is about to enter Wasm code for the cage.
pub fn set_cage_pkru(pkey_id: u32) {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let pkru = cage_pkru_mask(pkey_id);
        let ecx: u32 = 0;
        let edx: u32 = 0;
        unsafe {
            core::arch::asm!(
                "wrpkru",
                in("eax") pkru,
                in("ecx") ecx,
                in("edx") edx,
                options(nomem, nostack, preserves_flags),
            );
        }
    }
}

/// Relax the PKRU register to allow access to all keys (PKRU = 0).
/// Called when the CPU returns from Wasm code to host code.
pub fn relax_pkru() {
    #[cfg(all(target_arch = "x86_64", target_os = "linux"))]
    {
        let ecx: u32 = 0;
        let edx: u32 = 0;
        unsafe {
            core::arch::asm!(
                "wrpkru",
                in("eax") 0u32,
                in("ecx") ecx,
                in("edx") edx,
                options(nomem, nostack, preserves_flags),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cage_pkru_mask_allows_key0_and_cage_key() {
        // Key 0 bits (bits 0-1) must be 0 (allowed).
        let mask = cage_pkru_mask(3);
        assert_eq!(mask & 0b11, 0, "key 0 must be allowed");
        // Key 3 bits (bits 6-7) must be 0.
        assert_eq!(mask & (0b11 << 6), 0, "cage key 3 must be allowed");
        // Other keys (e.g. key 1, bits 2-3) must be disabled.
        assert_ne!(mask & (0b11 << 2), 0, "key 1 must be disabled");
    }

    #[test]
    fn cage_pkru_mask_key1() {
        let mask = cage_pkru_mask(1);
        assert_eq!(mask & 0b11, 0, "key 0 must be allowed");
        assert_eq!(mask & (0b11 << 2), 0, "cage key 1 must be allowed");
        assert_ne!(mask & (0b11 << 4), 0, "key 2 must be disabled");
    }
}
