use crate::{TimerKind, default_timer_kind, read_end, read_start};
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};

/// Counter stores information pertaining to a specific benchmarking site.
pub struct Counter {
    /// Counts the total number of CPU cycles or Nanoseconds spent.
    pub cycles: AtomicU64,
    /// Counts the total number of invocations.
    pub calls: AtomicU64,
    pub name: &'static str,
    /// Only one Counter is globally enabled during a given run.
    pub enabled: AtomicBool,
    /// Stores TimerKind.
    timer: AtomicU8,
}

impl Counter {
    /// Create a counter with the default timer.
    pub const fn new(name: &'static str) -> Self {
        Self {
            cycles: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            name,
            enabled: AtomicBool::new(false),
            timer: AtomicU8::new(default_timer_kind() as u8),
        }
    }

    pub fn get_name(&self) -> Option<&'static str> {
        Some(self.name)
    }

    #[inline(always)]
    /// Start a measurement for this counter.
    ///
    /// Returns `0` if the counter is disabled.
    pub fn start(&self) -> u64 {
        if self.enabled.load(Ordering::Relaxed) {
            read_start(self.timer_kind())
        } else {
            0
        }
    }

    #[inline(always)]
    /// Record a measurement using the start timestamp.
    ///
    /// This is a no-op when the counter is disabled.
    pub fn record(&self, start: u64) {
        if self.enabled.load(Ordering::Relaxed) {
            let elapsed = read_end(self.timer_kind()).saturating_sub(start);
            self.cycles.fetch_add(elapsed, Ordering::Relaxed);
            self.calls.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[inline(always)]
    /// Create an RAII scope guard that records on drop.
    pub fn get_timer(&self) -> Scope<'_> {
        Scope {
            counter: self,
            start: self.start(),
        }
    }

    /// Enable this counter.
    pub fn enable(&self) {
        self.enabled.store(true, Ordering::Relaxed);
    }

    /// Disable this counter.
    pub fn disable(&self) {
        self.enabled.store(false, Ordering::Relaxed);
    }

    /// Reset totals for this counter.
    pub fn reset(&self) {
        self.cycles.store(0, Ordering::Relaxed);
        self.calls.store(0, Ordering::Relaxed);
    }

    /// Set the timer backend for this counter.
    pub fn set_timer_kind(&self, kind: TimerKind) {
        self.timer.store(kind as u8, Ordering::Relaxed);
    }

    /// Read the current timer backend.
    pub fn timer_kind(&self) -> TimerKind {
        match self.timer.load(Ordering::Relaxed) {
            0 => TimerKind::Rdtsc,
            _ => TimerKind::Clock,
        }
    }
}

/// Scope is the RAII guard that records elapsed time on drop.
pub struct Scope<'a> {
    counter: &'a Counter,
    start: u64,
}

impl Drop for Scope<'_> {
    fn drop(&mut self) {
        self.counter.record(self.start);
    }
}

/// Reset all counters in a group.
pub fn reset_all(counters: &[&Counter]) {
    for c in counters {
        c.reset();
    }
}

/// Set a timer for a counter group.
pub fn set_timer(counters: &[&Counter], kind: TimerKind) {
    for c in counters {
        c.set_timer_kind(kind);
    }
}

/// Enable only the named counter in a group.
pub fn enable_name(counters: &[&Counter], name: &str) {
    for c in counters {
        if c.name == name {
            c.enable();
        } else {
            c.disable();
        }
    }
}
