use crate::bench::{SLOTS, current_slot, timing_active};
use crate::timers::{read_end, read_start};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

thread_local! {
    /// Open span on this thread: (start time, slot, open?). One slot is enough because only one
    /// counter is enabled per process.
    static SPAN: Cell<(u64, u16, bool)> = const { Cell::new((0, 0, false)) };
}

/// What a counter's totals are measured in.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Unit {
    /// Measured by host timers: nanoseconds or cycles, depending on the selected `TimerKind`.
    Timer,
    /// Always nanoseconds, reported from outside (e.g. the guest's own loop timing).
    Nanos,
}

/// One measured location. Declared as a `static` in the crate that owns the location.
///
/// Besides overall totals it keeps totals per *slot* (syscall number x origin, see
/// `bench.rs`), so a report can break every location down by syscall.
pub struct Counter {
    /// Globally unique name, `<layer>::<what>`. The layer prefix groups the report.
    pub name: &'static str,
    pub unit: Unit,
    total: AtomicU64,
    calls: AtomicU64,
    /// Timer overhead per measurement in the run that produced the totals (0 for `Nanos`).
    overhead: AtomicU64,
    /// Only one counter is enabled per process, so timers don't measure each other.
    enabled: AtomicBool,
    slot_total: [AtomicU64; SLOTS],
    slot_calls: [AtomicU64; SLOTS],
}

impl Counter {
    pub const fn new(name: &'static str) -> Self {
        Self::with_unit(name, Unit::Timer)
    }

    pub const fn with_unit(name: &'static str, unit: Unit) -> Self {
        Self {
            name,
            unit,
            total: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            overhead: AtomicU64::new(0),
            enabled: AtomicBool::new(false),
            slot_total: [const { AtomicU64::new(0) }; SLOTS],
            slot_calls: [const { AtomicU64::new(0) }; SLOTS],
        }
    }

    /// Start timing. Returns `None` (and reads no clock) unless this counter is the enabled
    /// one *and* the thread is servicing a syscall that is being timed. Timing stops when the
    /// returned guard is dropped; the time is credited to the slot current at the start.
    #[inline(always)]
    pub fn timer(&'static self) -> Option<Scope> {
        if self.enabled.load(Ordering::Relaxed) && timing_active() {
            Some(Scope {
                counter: self,
                slot: current_slot(),
                start: read_start(),
            })
        } else {
            None
        }
    }

    /// Start a span: like `timer()`, but the matching `span_end()` may be in another function
    /// or crate, as long as it runs on the same thread (grate calls do: 3i → lind-boot → lind-3i
    /// all run on the calling thread). No-op unless this counter is enabled and timing is on.
    #[inline(always)]
    pub fn span_start(&'static self) {
        if self.enabled.load(Ordering::Relaxed) && timing_active() {
            let slot = current_slot();
            let start = read_start();
            SPAN.with(|s| s.set((start, slot, true)));
        }
    }

    /// Like `timer()`, but records whenever this counter is the enabled one, whether or not the
    /// thread is servicing a timed syscall. For paths that run outside a benchmark syscall or on
    /// threads the syscall entry never saw: fork (whose child is built on a new thread) and exit.
    #[inline(always)]
    pub fn timer_any(&'static self) -> Option<Scope> {
        if self.enabled.load(Ordering::Relaxed) {
            Some(Scope {
                counter: self,
                slot: current_slot(),
                start: read_start(),
            })
        } else {
            None
        }
    }

    /// Like `span_start()`, but opens the span whenever this counter is enabled (see
    /// `timer_any`). Close it with `span_end()` on the same thread.
    #[inline(always)]
    pub fn span_start_any(&'static self) {
        if self.enabled.load(Ordering::Relaxed) {
            let slot = current_slot();
            let start = read_start();
            SPAN.with(|s| s.set((start, slot, true)));
        }
    }

    /// End the span opened by `span_start()` on this thread and record it. No-op if this
    /// counter is disabled or no span is open.
    #[inline(always)]
    pub fn span_end(&'static self) {
        if self.enabled.load(Ordering::Relaxed) {
            let end = read_end();
            let (start, slot, open) = SPAN.with(|s| s.replace((0, 0, false)));
            if open {
                self.add_sample(end.saturating_sub(start), slot);
            }
        }
    }

    #[inline(always)]
    fn add_sample(&self, elapsed: u64, slot: u16) {
        self.total.fetch_add(elapsed, Ordering::Relaxed);
        self.calls.fetch_add(1, Ordering::Relaxed);
        let s = slot as usize;
        self.slot_total[s].fetch_add(elapsed, Ordering::Relaxed);
        self.slot_calls[s].fetch_add(1, Ordering::Relaxed);
    }

    /// Add an externally measured total (used for the guest's own timing).
    pub fn add(&self, total: u64, calls: u64) {
        if self.enabled.load(Ordering::Relaxed) {
            self.total.fetch_add(total, Ordering::Relaxed);
            self.calls.fetch_add(calls, Ordering::Relaxed);
        }
    }

    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn calls(&self) -> u64 {
        self.calls.load(Ordering::Relaxed)
    }

    pub fn overhead(&self) -> u64 {
        self.overhead.load(Ordering::Relaxed)
    }

    /// Overwrite the totals (used by the parent process when collecting child results).
    pub fn set_result(&self, total: u64, calls: u64, overhead: u64) {
        self.total.store(total, Ordering::Relaxed);
        self.calls.store(calls, Ordering::Relaxed);
        self.overhead.store(overhead, Ordering::Relaxed);
    }

    /// Raw average per call, or `None` if nothing was recorded.
    pub fn avg(&self) -> Option<f64> {
        let calls = self.calls();
        (calls > 0).then(|| self.total() as f64 / calls as f64)
    }

    /// Average per call minus the timer's own overhead: the best estimate of the real cost.
    pub fn net_avg(&self) -> Option<f64> {
        self.avg().map(|a| a - self.overhead() as f64)
    }

    // ---- per-slot totals ----

    pub fn slot_total(&self, slot: usize) -> u64 {
        self.slot_total[slot].load(Ordering::Relaxed)
    }

    pub fn slot_calls(&self, slot: usize) -> u64 {
        self.slot_calls[slot].load(Ordering::Relaxed)
    }

    /// Slots that recorded anything, as `(slot, total, calls)`.
    pub fn nonzero_slots(&self) -> impl Iterator<Item = (usize, u64, u64)> + '_ {
        (0..SLOTS).filter_map(move |s| {
            let calls = self.slot_calls(s);
            (calls > 0).then(|| (s, self.slot_total(s), calls))
        })
    }

    pub fn set_slot(&self, slot: usize, total: u64, calls: u64) {
        if slot < SLOTS {
            self.slot_total[slot].store(total, Ordering::Relaxed);
            self.slot_calls[slot].store(calls, Ordering::Relaxed);
        }
    }

    /// Net average per call for one slot.
    pub fn slot_net_avg(&self, slot: usize) -> Option<f64> {
        let calls = self.slot_calls(slot);
        (calls > 0).then(|| self.slot_total(slot) as f64 / calls as f64 - self.overhead() as f64)
    }
}

/// Timing guard: records the elapsed time into its counter when dropped.
pub struct Scope {
    counter: &'static Counter,
    slot: u16,
    start: u64,
}

impl Drop for Scope {
    #[inline(always)]
    fn drop(&mut self) {
        let elapsed = read_end().saturating_sub(self.start);
        self.counter.add_sample(elapsed, self.slot);
    }
}
