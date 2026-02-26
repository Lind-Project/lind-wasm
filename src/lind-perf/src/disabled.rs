use std::time::Duration;

/// Formats nanosecond totals for reports.
pub struct PrettyDuration(pub Duration);

impl std::fmt::Display for PrettyDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ns_f = self.0.as_nanos() as f64;

        let format = if ns_f < 1_000.0 {
            format!("{:.3}ns", ns_f)
        } else if ns_f < 1_000_000.0 {
            format!("{:.3}us", ns_f / 1_000.0)
        } else if ns_f < 1_000_000_000.0 {
            format!("{:.3}ms", ns_f / 1_000_000.0)
        } else {
            format!("{:.3}s", ns_f / 1_000_000_000.0)
        };

        write!(f, "{}", format)
    }
}

/// TimerKind exists in both enabled and disabled builds for API consistency.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimerKind {
    Rdtsc = 0,
    Clock = 1,
}

/// Get the default timer.
pub const fn default_timer_kind() -> TimerKind {
    TimerKind::Clock
}

#[inline(always)]
pub fn read_start(_kind: TimerKind) -> u64 {
    0
}

#[inline(always)]
pub fn read_end(_kind: TimerKind) -> u64 {
    0
}

/// Lightweight no-op counter representation for disabled builds.
pub struct Counter;

impl Counter {
    pub const fn new(_name: &'static str) -> Self {
        Self
    }

    pub fn get_name(&self) -> Option<&'static str> {
        None
    }

    #[inline(always)]
    pub fn start(&self) -> u64 {
        let _ = self;
        0
    }

    #[inline(always)]
    pub fn record(&self, _start: u64) {
        let _ = self;
    }

    #[inline(always)]
    pub fn get_timer(&self) -> Scope {
        let _ = self;
        Scope
    }

    pub fn enable(&self) {
        let _ = self;
    }

    pub fn disable(&self) {
        let _ = self;
    }

    pub fn reset(&self) {
        let _ = self;
    }

    pub fn set_timer_kind(&self, _kind: TimerKind) {
        let _ = self;
    }

    pub fn timer_kind(&self) -> TimerKind {
        TimerKind::Clock
    }
}

/// No-op RAII guard for disabled builds.
pub struct Scope;

impl Drop for Scope {
    fn drop(&mut self) {}
}

pub fn reset_all(_counters: &[&Counter]) {}

pub fn set_timer(_counters: &[&Counter], _kind: TimerKind) {}

pub fn enable_name(_counters: &[&Counter], _name: &str) {}

pub fn report_header(_header: String) {}

pub fn report(_counters: &[&Counter]) {}
