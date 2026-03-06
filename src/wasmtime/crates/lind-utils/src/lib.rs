#![allow(dead_code)]

use std::sync::{Condvar, Mutex};

// used to manage global active cage count. Used to determine when wasmtime can exit
// (i.e. only after all the cages exited, we can exit the process)
// this class may be used by many crates (e.g. lind-commmon, lind-multi-process)
// therefore put into a seperate module to prevent cyclic dependency
#[allow(missing_docs)]
#[derive(Default)]
pub struct LindCageManager {
    cage_count: Mutex<i32>,
    condvar: Condvar,
}

impl LindCageManager {
    pub fn new(value: i32) -> Self {
        LindCageManager {
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
