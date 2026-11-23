use std::sync::atomic::{AtomicU8, Ordering};

/// Clock used by all timers in this process.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimerKind {
    /// `rdtsc`/`rdtscp` with load fences: CPU cycles. Falls back to `Clock` off x86_64.
    Rdtsc = 0,
    /// `clock_gettime(CLOCK_MONOTONIC_RAW)`: nanoseconds.
    Clock = 1,
}

static TIMER_KIND: AtomicU8 = AtomicU8::new(TimerKind::Clock as u8);

/// Select the clock for every timer in this process. Call once, before any timing starts.
pub fn set_timer_kind(kind: TimerKind) {
    TIMER_KIND.store(kind as u8, Ordering::Relaxed);
}

#[inline(always)]
pub fn timer_kind() -> TimerKind {
    match TIMER_KIND.load(Ordering::Relaxed) {
        0 => TimerKind::Rdtsc,
        _ => TimerKind::Clock,
    }
}

#[inline(always)]
pub(crate) fn read_start() -> u64 {
    match timer_kind() {
        TimerKind::Rdtsc => rdtsc_start(),
        TimerKind::Clock => clock_now(),
    }
}

#[inline(always)]
pub(crate) fn read_end() -> u64 {
    match timer_kind() {
        TimerKind::Rdtsc => rdtsc_end(),
        TimerKind::Clock => clock_now(),
    }
}

/// Measure the timer's own cost: the elapsed value an *empty* timed region reports, using the
/// current `TimerKind`. Every real measurement includes this, so it is subtracted from the
/// averages (`net/call`) and from the derived costs.
///
/// Takes the median of each of several batches and returns the lowest batch median. A single
/// median can be inflated by a disturbance during calibration (frequency ramp-up, migration to
/// another core, an interrupt); the lowest of several medians reflects the undisturbed cost.
pub fn calibrate_overhead() -> u64 {
    const WARMUP: usize = 50_000;
    const BATCHES: usize = 20;
    const BATCH: usize = 10_000;
    for _ in 0..WARMUP {
        let s = read_start();
        std::hint::black_box(read_end().saturating_sub(s));
    }
    let mut v: Vec<u64> = Vec::with_capacity(BATCH);
    let mut best = u64::MAX;
    for _ in 0..BATCHES {
        v.clear();
        v.extend((0..BATCH).map(|_| {
            let s = read_start();
            read_end().saturating_sub(s)
        }));
        v.sort_unstable();
        best = best.min(v[BATCH / 2]);
    }
    best
}

#[inline(always)]
fn rdtsc_start() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Keep earlier loads from being reordered past the timestamp read.
        core::arch::x86_64::_mm_lfence();
        return core::arch::x86_64::_rdtsc();
    }
    #[cfg(not(target_arch = "x86_64"))]
    clock_now()
}

#[inline(always)]
fn rdtsc_end() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // RDTSCP waits for earlier instructions to finish; the fence keeps later ones from
        // starting before the read.
        let mut aux = 0u32;
        let tsc = core::arch::x86_64::__rdtscp(&mut aux);
        core::arch::x86_64::_mm_lfence();
        return tsc;
    }
    #[cfg(not(target_arch = "x86_64"))]
    clock_now()
}

#[inline(always)]
fn clock_now() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts) };
    if rc != 0 {
        panic!("lind-perf: clock_gettime(CLOCK_MONOTONIC_RAW) failed");
    }
    (ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64)
}
