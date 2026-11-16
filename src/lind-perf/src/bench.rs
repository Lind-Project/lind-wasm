//! Per-thread context of the syscall currently being serviced.
//!
//! The `make-syscall` entry in lind-common opens a [`CallGuard`] for every syscall. It records,
//! for the current thread:
//! - whether timers should record (`timing_active`),
//! - which *slot* the measurements belong to: the syscall number and its [`Origin`],
//! - the nesting depth, to tell a cage's own call from one a grate makes while servicing it.
//!
//! Grate calls run synchronously on the calling thread (lind-3i leases a worker and runs it
//! inline), so a syscall made by a grate handler shows up here as a nested entry.

use std::cell::Cell;
use std::sync::atomic::{AtomicU8, Ordering};

/// Which syscalls are timed.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PerfScope {
    /// Only the benchmark syscalls 2001..=2999 (default). Keeps program startup, `printf`,
    /// `exit` etc. out of the numbers.
    Bench = 0,
    /// Every syscall. For profiling real programs and grates.
    All = 1,
}

static PERF_SCOPE: AtomicU8 = AtomicU8::new(PerfScope::Bench as u8);

pub fn set_perf_scope(scope: PerfScope) {
    PERF_SCOPE.store(scope as u8, Ordering::Relaxed);
}

#[inline(always)]
pub fn perf_scope() -> PerfScope {
    match PERF_SCOPE.load(Ordering::Relaxed) {
        1 => PerfScope::All,
        _ => PerfScope::Bench,
    }
}

/// Where a syscall came from.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Origin {
    /// Top-level call made by an ordinary program (cage).
    Cage = 0,
    /// Top-level call made by a grate for itself (startup, fork, wait, ...).
    Grate = 1,
    /// Nested call made while another syscall is being serviced: a grate forwarding a call
    /// down, copying data between cages, etc.
    Fwd = 2,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Cage => "cage",
            Origin::Grate => "grate",
            Origin::Fwd => "fwd",
        }
    }
}

// Slot layout: ORIGINS x SLOTS_PER_ORIGIN. Within an origin, syscall numbers 0..1024 map to
// themselves (Linux numbers and the 3i calls 1001-1003), 2000..2128 to 1024.., everything else
// to the last "other" slot.
const ORIGINS: usize = 3;
const SLOTS_PER_ORIGIN: usize = 1152;
const OTHER: usize = SLOTS_PER_ORIGIN - 1;
pub const SLOTS: usize = ORIGINS * SLOTS_PER_ORIGIN;

#[inline(always)]
fn slot_of(origin: Origin, syscall: u32) -> u16 {
    let n = syscall as usize;
    let idx = if n < 1024 {
        n
    } else if (2000..2000 + (OTHER - 1024)).contains(&n) {
        1024 + (n - 2000)
    } else {
        OTHER
    };
    (origin as usize * SLOTS_PER_ORIGIN + idx) as u16
}

/// Inverse of the slot mapping: `(origin, syscall number)`, or `None` syscall for "other".
pub fn slot_info(slot: usize) -> (Origin, Option<u32>) {
    let origin = match slot / SLOTS_PER_ORIGIN {
        0 => Origin::Cage,
        1 => Origin::Grate,
        _ => Origin::Fwd,
    };
    let idx = slot % SLOTS_PER_ORIGIN;
    let n = if idx < 1024 {
        Some(idx as u32)
    } else if idx < OTHER {
        Some((2000 + idx - 1024) as u32)
    } else {
        None
    };
    (origin, n)
}

#[derive(Copy, Clone)]
struct Ctx {
    timed: bool,
    slot: u16,
    depth: u16,
}

thread_local! {
    static CTX: Cell<Ctx> = const { Cell::new(Ctx { timed: false, slot: 0, depth: 0 }) };
}

/// Are timers on this thread recording right now?
#[inline(always)]
pub fn timing_active() -> bool {
    CTX.with(|c| c.get().timed)
}

/// Slot of the syscall currently being serviced on this thread.
#[inline(always)]
pub(crate) fn current_slot() -> u16 {
    CTX.with(|c| c.get().slot)
}

/// Opened at the syscall entry for every syscall; restores the previous context on drop.
pub struct CallGuard {
    prev: Ctx,
}

impl CallGuard {
    /// `timed`: whether this syscall should be measured. `is_grate` is only called for timed
    /// top-level calls in `PerfScope::All`, to tell a grate's own calls from a cage's.
    #[inline(always)]
    pub fn enter(timed: bool, syscall: u32, is_grate: impl FnOnce() -> bool) -> Self {
        let prev = CTX.with(|c| c.get());
        let origin = if prev.depth > 0 {
            Origin::Fwd
        } else if timed && perf_scope() == PerfScope::All && is_grate() {
            Origin::Grate
        } else {
            Origin::Cage
        };
        CTX.with(|c| {
            c.set(Ctx {
                timed,
                slot: slot_of(origin, syscall),
                depth: prev.depth + 1,
            })
        });
        Self { prev }
    }
}

impl Drop for CallGuard {
    #[inline(always)]
    fn drop(&mut self) {
        let prev = self.prev;
        CTX.with(|c| c.set(prev));
    }
}
