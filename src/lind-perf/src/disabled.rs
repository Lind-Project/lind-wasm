use crate::timers::TimerKind;

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

// No-op implementations for the rest.
pub fn reset_all_counters(_counters: impl IntoIterator<Item = &'static Counter>) {}

pub fn set_timer(_counters: impl IntoIterator<Item = &'static Counter>, _kind: TimerKind) {}

pub fn enable_counter_by_name(_counters: impl IntoIterator<Item = &'static Counter>, _name: &str) {}

pub fn report(_counters: &[&Counter], _header: String) {}
