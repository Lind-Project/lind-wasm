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
pub fn read_start(_kind: TimerKind) -> u64 {
    #[cfg(feature = "enabled")]
    match _kind {
        TimerKind::Rdtsc => rdtsc_start(),
        TimerKind::Clock => clock_now(),
    }

    #[cfg(not(feature = "enabled"))]
    0
}

#[inline(always)]
pub fn read_end(_kind: TimerKind) -> u64 {
    #[cfg(feature = "enabled")]
    match _kind {
        TimerKind::Rdtsc => rdtsc_end(),
        TimerKind::Clock => clock_now(),
    }

    #[cfg(not(feature = "enabled"))]
    0
}

// RDTSC timers for measuring CPU Cycles.
// Only available on x86 based processors.
#[inline(always)]
fn rdtsc_start() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        // Serialize execution before reading the TSC so that
        // no prior loads or instructions are speculatively
        // reordered past the timestamp read.
        //
        // See: https://www.intel.com/content/www/us/en/docs/intrinsics-guide/index.html#text=_mm_lfence&ig_expand=3977
        core::arch::x86_64::_mm_lfence();

        // Get the RDTSC counter.
        return core::arch::x86_64::_rdtsc();
    }

    #[cfg(not(target_arch = "x86_64"))]
    clock_now()
}

// Separate start/end functions are required for RDTSC because
// fencing semantics differ before and after the measurement.
#[inline(always)]
fn rdtsc_end() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let mut aux = 0u32;
        // RDTSCP is partially serializing: it waits for prior
        // instructions to complete before reading the TSC.
        let tsc = core::arch::x86_64::__rdtscp(&mut aux);

        // Fence after the read to prevent subsequent instructions
        // from being speculatively executed before the timestamp.
        core::arch::x86_64::_mm_lfence();
        return tsc;
    }

    #[cfg(not(target_arch = "x86_64"))]
    clock_now()
}

// CLOCK_MONOTONIC_RAW based timer used for nanoseconds measurements. Same function can be used for
// start and end.
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
