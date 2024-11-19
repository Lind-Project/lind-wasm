// Author: Nicholas Renner
//
// Timer functions for Rust interface.
#![allow(dead_code)]

use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
pub use std::time::Duration as RustDuration;
pub use std::time::Instant as RustInstant;
use std::time::SystemTime;

use crate::interface::lind_kill_from_id;

pub fn timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// Create a new timer
pub fn starttimer() -> RustInstant {
    RustInstant::now()
}

// Return time since timer was started
pub fn readtimer(now: RustInstant) -> RustDuration {
    now.elapsed()
}

// Sleep function to sleep for specified duration
pub fn sleep(dur: RustDuration) {
    thread::sleep(dur);
}

#[derive(Debug)]
struct _IntervalTimer {
    pub cageid: u64,
    pub init_instant: RustInstant, // The instant this process is created

    pub start_instant: RustInstant,
    pub curr_duration: RustDuration,
    pub next_duration: RustDuration,

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
                init_instant: RustInstant::now(),
                start_instant: RustInstant::now(),
                curr_duration: RustDuration::ZERO,
                next_duration: RustDuration::ZERO,
                is_ticking: false,
            })),
        }
    }

    // Similar to getitimer. Returns (current value, next value)
    pub fn get_itimer(&self) -> (RustDuration, RustDuration) {
        let guard = self._ac.lock().unwrap();

        (guard.curr_duration, guard.next_duration)
    }

    fn _set_itimer(
        &self,
        guard: &mut MutexGuard<_IntervalTimer>,
        curr_duration: RustDuration,
        next_duration: RustDuration,
    ) {
        if curr_duration.is_zero() {
            guard.is_ticking = false;
        } else {
            guard.start_instant = RustInstant::now();
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

    pub fn set_itimer(&self, curr_duration: RustDuration, next_duration: RustDuration) {
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

                    if remaining_seconds == RustDuration::ZERO {
                        lind_kill_from_id(guard.cageid, 14);

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

            thread::sleep(RustDuration::from_millis(20)); // One jiffy
        }
    }

    pub fn clone_with_new_cageid(&self, cageid: u64) -> Self {
        let mut guard = self._ac.lock().unwrap();
        guard.cageid = cageid;

        self.clone()
    }
}
