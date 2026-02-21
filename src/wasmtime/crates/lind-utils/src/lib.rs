#![allow(dead_code)]

use std::sync::{Condvar, Mutex, atomic::{AtomicU32, Ordering}};

use dashmap::DashMap;

pub mod lind_syscall_numbers;

// used to manage global active cage count. Used to determine when wasmtime can exit
// (i.e. only after all the cages exited, we can exit the process)
// this class may be used by many crates (e.g. lind-commmon, lind-multi-process)
// therefore put into a seperate module to prevent cyclic dependency
#[allow(missing_docs)]
#[derive(Default)]
pub struct LindCageManager {
    cage_count: Mutex<i32>,
    condvar: Condvar,
}

impl LindCageManager {
    pub fn new(value: i32) -> Self {
        LindCageManager {
            cage_count: Mutex::new(value),
            condvar: Condvar::new(),
        }
    }

    pub fn increment(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        *cage_count += 1;
    }

    pub fn decrement(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        *cage_count -= 1;
        if *cage_count == 0 {
            self.condvar.notify_all();
        }
    }

    pub fn wait(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        while *cage_count != 0 {
            cage_count = self.condvar.wait(cage_count).unwrap();
        }
    }
}

// parse an environment variable, return its name and value
pub fn parse_env_var(env_var: &str) -> (String, Option<String>) {
    // Find the position of the first '=' character
    if let Some(pos) = env_var.find('=') {
        // If '=' is found, return the key and value as String and Some(String)
        let key = env_var[..pos].to_string();
        let value = env_var[pos + 1..].to_string();
        (key, Some(value))
    } else {
        // If '=' is not found, return the whole string as the key and None for the value
        (env_var.to_string(), None)
    }
}

/// Round `base` up to the nearest multiple of `align`.
///
/// - If `align > 0`, returns the smallest multiple of `align` that is >= `base`.
/// - If `base == 0`, returns 0.
/// - If `align == 0`, returns `base` unchanged (no alignment applied).
///
/// Commonly used for page-size or allocation alignment when managing
/// linear memory or mmap-related regions.
pub fn round_size(base: u32, align: u32) -> u32 {
    if align == 0 {
        return base;
    }

    if base == 0 {
        return 0;
    }

    (base.saturating_add(align - 1) / align) * align
}

/// A per-process (wasm instance) Global Offset Table (GOT)-like structure used by Lind/Wasmtime
/// dynamic linking support.
///
/// The map stores: symbol name -> address of a `u32` slot ("GOT cell").
///
/// That `u32` slot is the indirection point that generated code / trampolines can read from.
/// When a symbol is resolved, we write the resolved address/value into the slot.
/// Unresolved entries keep value 0 and can be warned about in `warning_undefined()`.
#[derive(Default)]
pub struct LindGOT {
    /// Concurrent hashmap because GOT resolution/patching may happen from multiple threads.
    ///
    /// We store the pointer as `u64` instead of `*mut u32` so this struct can be shared across
    /// threads without Rust's raw-pointer `Send/Sync` friction.
    global_offset_table: DashMap<String, u64>
}

impl LindGOT {
    /// Create an empty GOT table.
    pub fn new() -> Self {
        Self {
            global_offset_table: DashMap::new()
        }
    }

    /// Helper: interpret the stored address as an AtomicU32 pointer.
    #[inline]
    fn get_atomic_cell(&self, name: &str) -> Option<*const AtomicU32> {
        self.global_offset_table
            .get(name)
            .map(|h| (*h) as *const AtomicU32)
    }

    /// Register a new GOT cell for a symbol.
    ///
    /// We intentionally do not overwrite an existing entry. This mirrors ELF-like dynamic
    /// symbol resolution/interposition semantics: if multiple loaded modules define the same
    /// symbol name, the definition that appears first in the runtime's search order (often the
    /// earliest-loaded module / first in the link-map) takes priority.
    ///
    /// In our implementation, "first" means: the first time `new_entry()` is called for `name`.
    /// Later duplicates are ignored to keep the GOT mapping stable and consistent with
    /// load-order precedence.
    pub fn new_entry(&mut self, name: String, handler: *mut u32) {
        if !self.global_offset_table.contains_key(&name) {
            self.global_offset_table.insert(name, handler as u64);
        } else {
            #[cfg(feature = "debug-dylink")]
            println!("[debug] Warning: ignore duplicated GOT entry {}", name);
        }
    }

    /// Update the GOT cell if the symbol exists.
    ///
    /// Returns `true` if the entry was found and updated, `false` otherwise.
    pub fn update_entry_if_exist(&self, name: &str, val: u32) -> bool {
        let Some(cell_ptr) = self.get_atomic_cell(name) else {
            return false;
        };

        // SAFETY: cell_ptr must be valid, properly aligned for AtomicU32, and writable.
        let cell = unsafe { &*(cell_ptr as *const AtomicU32) };
        cell.store(val, Ordering::Release);
        true
    }

    /// Update the GOT cell *only if* it is still unresolved (i.e., currently `0`).
    ///
    /// This is useful when multiple modules may race/compete to resolve the same symbol:
    /// the first successful resolution "claims" the slot, and later resolutions do not
    /// overwrite it. This matches ELF-like load-order/first-definition-wins behavior
    ///
    /// Returns `true` if the symbol exists in the GOT (regardless of whether we updated
    /// the slot), and `false` if the symbol was not registered.
    pub fn update_entry_if_unresolved(&self, name: &str, val: u32) -> bool {
        let Some(cell_ptr) = self.get_atomic_cell(name) else {
            return false;
        };

        let cell = unsafe { &*(cell_ptr as *const AtomicU32) };

        // Atomically: if current == 0, set to val; otherwise keep existing.
        match cell.compare_exchange(
            0,
            val,
            Ordering::AcqRel,  // success ordering
            Ordering::Acquire, // failure ordering
        ) {
            Ok(_) => true, // we updated from 0 -> val
            Err(_) => false, // either already resolved or raced and lost
        }
    }

    /// Read the GOT cell if the symbol exists.
    ///
    /// Returns `Some(value)` if found, otherwise `None`.
    pub fn get_entry_if_exist(&self, name: &str) -> Option<u32> {
        let cell_ptr = self.get_atomic_cell(name)?;
        let cell = unsafe { &*(cell_ptr as *const AtomicU32) };
        Some(cell.load(Ordering::Acquire))
    }

    /// print unresolved symbols.
    ///
    /// Convention: a GOT cell value of 0 means "unresolved".
    /// 
    /// This is safe in Lind because:
    /// - A memory address of `0` is never a valid resolved target.
    /// - A function index of `0` is also not used as a valid callable entry.
    ///
    /// Therefore, `0` can reliably serve as a sentinel value indicating
    /// that symbol resolution has not yet populated the GOT slot.
    pub fn warning_undefined(&self) {
        // Clone the map so we can iterate without holding locks/guards for the whole traversal.
        // (DashMap's iterators keep shard locks; cloning trades memory for simpler iteration.)
        for (name, handler) in self.global_offset_table.clone() {
            let cell = unsafe { &*(handler as *const AtomicU32) };
            let val = cell.load(Ordering::Acquire);

            if val == 0 {
                #[cfg(feature = "debug-dylink")]
                println!("[debug] Warning: GOT entry \"{}\" unresolved", name);
            }
        }
    }
}
