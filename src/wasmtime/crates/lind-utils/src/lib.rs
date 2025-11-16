#![allow(dead_code)]

use std::sync::{Condvar, Mutex};

use dashmap::DashMap;

pub mod lind_syscall_numbers;

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
        if *cage_count == 0 {
            self.condvar.notify_all();
        }
    }

    pub fn wait(&self) {
        let mut cage_count = self.cage_count.lock().unwrap();
        while *cage_count != 0 {
            cage_count = self.condvar.wait(cage_count).unwrap();
        }
    }
}

// parse an environment variable, return its name and value
pub fn parse_env_var(env_var: &str) -> (String, Option<String>) {
    // Find the position of the first '=' character
    if let Some(pos) = env_var.find('=') {
        // If '=' is found, return the key and value as String and Some(String)
        let key = env_var[..pos].to_string();
        let value = env_var[pos + 1..].to_string();
        (key, Some(value))
    } else {
        // If '=' is not found, return the whole string as the key and None for the value
        (env_var.to_string(), None)
    }
}


#[allow(missing_docs)]
#[derive(Default)]
pub struct LindGOT {
    global_offset_table: DashMap<String, *mut u32>
}

impl LindGOT {
    pub fn new() -> Self {
        Self {
            global_offset_table: DashMap::new()
        }
    }

    pub fn new_entry(&mut self, name: String, handler: *mut u32) {
        // to-do: handle existing GOT entry
        self.global_offset_table.insert(name, handler);
    }

    pub fn update_entry_if_exist(&self, name: &str, val: u32) -> bool {
        if let Some(handler) = self.global_offset_table.get(name) {
            let handler = *handler;
            unsafe {
                *handler = val;
            }
        } else {
            // do nothing
            return false;
        }

        return true;
    }
}
