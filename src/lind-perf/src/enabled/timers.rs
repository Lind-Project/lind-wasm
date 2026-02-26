/// TimerKind defines the timer-backend to be used for benchmarks.
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
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_mm_lfence();
        return core::arch::x86_64::_rdtsc();
    }
    clock_now()
}

#[inline(always)]
fn rdtsc_end() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut aux = 0u32;
        let tsc = core::arch::x86_64::__rdtscp(&mut aux);
        core::arch::x86_64::_mm_lfence();
        return tsc;
    }
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
        panic!("Unable to get a CLOCK_MONOTONIC_RAW time. Aborting benchmarks.");
    }
    (ts.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(ts.tv_nsec as u64)
}
