use std::sync::{Condvar, Mutex};

/// Tracks the cages belonging to one Lind execution.
///
/// The runtime reports cage creation and final exit; the launcher waits here
/// rather than treating an idle runtime worker as completion.
#[derive(Default)]
pub struct LindCageManager {
    cage_count: Mutex<i32>,
    condvar: Condvar,
}

impl LindCageManager {
    pub fn new(value: i32) -> Self {
        Self {
            cage_count: Mutex::new(value),
            condvar: Condvar::new(),
        }
    }

    pub fn increment(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        *cage_count += 1;
    }

    pub fn decrement(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        debug_assert!(*cage_count > 0, "Cage count should never be negative");
        *cage_count -= 1;
        if *cage_count <= 0 {
            self.condvar.notify_all();
        }
    }

    pub fn wait(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        while *cage_count > 0 {
            cage_count = self.condvar.wait(cage_count).unwrap();
        }
    }
}
