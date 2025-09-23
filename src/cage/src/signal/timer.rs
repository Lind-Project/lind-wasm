//! Interval timer implementation for `itimer` and `SIGALRM`
//!
//! This file emulates a per-Cage `ITIMER_REAL` that counts down wall-clock time and
//! delivers `SIGALRM` when it expires. We chose to implement `itimer` / `SIGALRM` entirely 
//! in user space rather than relying on the host kernel because of how our runtime manages 
//! signals:
//! Host timers (e.g., `setitimer`) deliver signals to host processes or threads. Our runtime, 
//! however, models Cages as logical processes inside a Wasm environment. The kernel cannot 
//! deliver a timer interrupt specifically to a Cage or its designated “main thread.” A 
//! user-space timer gives us precise control over which Cage receives `SIGALRM`.
//! All signal delivery in our system is mediated through the epoch-based mechanism.
//! (See our online design doc for more details.)
#![allow(dead_code)]

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
pub use std::time::Duration;
pub use std::time::Instant;

use super::lind_send_signal;
use sysdefs::constants::SIGALRM;

#[derive(Debug)]
struct _IntervalTimer {
    pub cageid: u64,
    pub init_instant: Instant, // The instant this process is created

    pub start_instant: Instant,
    pub curr_duration: Duration,
    pub next_duration: Duration,

    pub is_ticking: bool,
}

#[derive(Clone, Debug)]
pub struct IntervalTimer {
    _ac: Arc<Mutex<_IntervalTimer>>,
}

impl IntervalTimer {
    pub fn new(cageid: u64) -> Self {
        Self {
            _ac: Arc::new(Mutex::new(_IntervalTimer {
                cageid: cageid,
                init_instant: Instant::now(),
                start_instant: Instant::now(),
                curr_duration: Duration::ZERO,
                next_duration: Duration::ZERO,
                is_ticking: false,
            })),
        }
    }

    // Similar to getitimer. Returns (current value, next value)
    pub fn get_itimer(&self) -> (Duration, Duration) {
        let guard = self._ac.lock().unwrap();

        (guard.curr_duration, guard.next_duration)
    }

    fn _set_itimer(
        &self,
        guard: &mut MutexGuard<_IntervalTimer>,
        curr_duration: Duration,
        next_duration: Duration,
    ) {
        if curr_duration.is_zero() {
            guard.is_ticking = false;
        } else {
            guard.start_instant = Instant::now();
            guard.curr_duration = curr_duration;
            guard.next_duration = next_duration;

            if !guard.is_ticking {
                guard.is_ticking = true;

                let self_dup = self.clone();
                thread::spawn(move || {
                    // There is a chance that there'll be two ticking threads running
                    // at the same time
                    self_dup.tick();
                });
            }
        }
    }

    pub fn set_itimer(&self, curr_duration: Duration, next_duration: Duration) {
        let mut guard = self._ac.lock().unwrap();
        self._set_itimer(&mut guard, curr_duration, next_duration);
    }

    pub fn tick(&self) {
        loop {
            {
                let mut guard = self._ac.lock().unwrap();

                if guard.is_ticking {
                    let remaining_seconds = guard
                        .curr_duration
                        .saturating_sub(guard.start_instant.elapsed());

                    if remaining_seconds == Duration::ZERO {
                        // Sends a SIGALRM signal to the cage when the timer expires.
                        // This struct/method is used exclusively by the setitimer and alarm syscall,
                        // which is expected to send a SIGALRM signal upon expiration.
                        lind_send_signal(guard.cageid, SIGALRM);

                        let new_curr_duration = guard.next_duration;
                        // Repeat the intervals until user cancel it
                        let new_next_duration = guard.next_duration;

                        self._set_itimer(&mut guard, new_curr_duration, new_next_duration);
                        // Calling self.set_itimer will automatically turn of the timer if
                        // next_duration is ZERO
                    }
                } else {
                    break;
                }
            }

            thread::sleep(Duration::from_millis(20)); // One jiffy
        }
    }

    pub fn clone_with_new_cageid(&self, cageid: u64) -> Self {
        let mut guard = self._ac.lock().unwrap();
        guard.cageid = cageid;

        self.clone()
    }
}
