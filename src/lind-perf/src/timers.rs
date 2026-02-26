use std::time::Duration;

/// Formats nanosecond totals for reports. Converts nanosecond input to larger units where
/// appropriate and truncates to 3 decimal points.
pub struct PrettyDuration(pub Duration);

impl std::fmt::Display for PrettyDuration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ns_f = self.0.as_nanos() as f64;

        let format = if ns_f < 1_000.0 {
            format!("{:.3}ns", ns_f)
        } else if ns_f < 1_000_000.0 {
            format!("{:.3}µs", ns_f / 1_000.0)
        } else if ns_f < 1_000_000_000.0 {
            format!("{:.3}ms", ns_f / 1_000_000.0)
        } else {
            format!("{:.3}s", ns_f / 1_000_000_000.0)
        };

        write!(f, "{}", format)
    }
}

/// TimerKind defines the timer-backend to be used for benchmarks. We support two kinds of timers
/// currently,
///
/// RDTSC: Time Stamp Counter that counts the number of CPU cycles that have elapsed.
/// Clock: Uses CLOCK_MONOTONIC_RAW to get the current time in nanoseconds.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimerKind {
    Rdtsc = 0,
    Clock = 1,
}

/// Get the default timer.
pub const fn default_timer_kind() -> TimerKind {
    TimerKind::Clock
}

/// Public functions to record start and end times depending on the TimerKind being used.
#[inline(always)]
pub fn read_start(kind: TimerKind) -> u64 {
    match kind {
        TimerKind::Rdtsc => rdtsc_start(),
        TimerKind::Clock => clock_now(),
    }
}

#[inline(always)]
pub fn read_end(kind: TimerKind) -> u64 {
    match kind {
        TimerKind::Rdtsc => rdtsc_end(),
        TimerKind::Clock => clock_now(),
    }
}

#[inline(always)]
fn rdtsc_start() -> u64 {
    // RDTSC is only available of x864 machines.
    // In case this API is not exposed, default back to Clock.
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // From Intel's documentation <https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html#text=_mm_lfence&ig_expand=3977>:
        //
        // Perform a serializing operation on all load-from-memory instructions that were
        // issued prior to this instruction. Guarantees that every load instruction that precedes,
        // in program order, is globally visible before any load instruction which follows
        // the fence in program order.
        core::arch::x86_64::_mm_lfence();
        return core::arch::x86_64::_rdtsc();
    }
    return clock_now();
}

#[inline(always)]
fn rdtsc_end() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // End the TSC timer.
        let mut aux = 0u32;
        let tsc = core::arch::x86_64::__rdtscp(&mut aux);
        // End the load fence.
        core::arch::x86_64::_mm_lfence();
        return tsc;
    }
    return clock_now();
}

#[inline(always)]
fn clock_now() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC_RAW, &mut ts) };
    if rc != 0 {
        panic!("Unable to get a CLOCK_MONOTONIC_RAW time. Aborting benchmarks.");
    }
    return (ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64);
}
