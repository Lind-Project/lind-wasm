#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Condvar, Mutex};

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
    signal_handlers: DashMap<i32, DashMap<i32, u64>>,
}

impl LindCageManager {
    pub fn new(value: i32) -> Self {
        LindCageManager {
            cage_count: Mutex::new(value),
            condvar: Condvar::new(),
            signal_handlers: DashMap::new(),
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


    pub fn add_cage_signal_handler(&self, pid: i32, tid: i32, handler: u64) {
        let row_map = self.signal_handlers.entry(pid).or_insert_with(DashMap::new);
        row_map.insert(tid, handler);
    }

    pub fn remove_cage_signal_handler(&self, pid: i32, tid: i32) {
        todo!()
    }

    pub fn get_handler(&self, pid: i32, tid: i32) -> Option<u64> {
        if let Some(row_map) = self.signal_handlers.get(&pid) {
            // Get the value for the given column
            if let Some(value) = row_map.get(&tid) {
                return Some(value.clone()); // Clone because DashMap stores values by reference
            }
        }
        None // Return None if row or column doesn't exist
    }
}

// pub struct LindSignalManager {
//     handlers: DashMap<i32, DashMap<i32, u64>>,
// }

// impl LindSignalManager {
//     pub fn new() -> Self {
//         LindSignalManager {
//             handlers: DashMap::new()
//         }
//     }

//     pub fn add_cage_signal_handler(&mut self, pid: i32, tid: i32, handler: u64) {
//         let row_map = self.handlers.entry(pid).or_insert_with(DashMap::new);
//         row_map.insert(tid, handler);
//     }

//     pub fn remove_cage_signal_handler(&mut self, pid: i32, tid: i32) {
//         todo!()
//     }

//     pub fn get_handler(&self, pid: i32, tid: i32) -> Option<u64> {
//         if let Some(row_map) = self.handlers.get(&pid) {
//             // Get the value for the given column
//             if let Some(value) = row_map.get(&tid) {
//                 return Some(value.clone()); // Clone because DashMap stores values by reference
//             }
//         }
//         None // Return None if row or column doesn't exist
//     }
// }

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
