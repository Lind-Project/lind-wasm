//! Cross-cage authority derived from currently registered handlers.
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, THREEI_CAGEID, WASMTIME_CAGEID};

use crate::threei_const::THREEI_DEREGISTER;

#[derive(Default)]
pub struct AccessTable {
    // Reverse edges: target cage -> syscall -> handler cage.
    registrations: HashMap<u64, HashMap<u64, u64>>,
}

lazy_static::lazy_static! {
    /// Mutations take this write lock before touching the handler table.
    pub static ref ACCESS_TABLE: RwLock<AccessTable> = RwLock::new(AccessTable::default());
}

impl AccessTable {
    pub fn clear(&mut self) {
        self.registrations.clear();
    }

    pub(crate) fn register(&mut self, target: u64, syscall: u64, handler: u64) {
        if matches!(
            handler,
            THREEI_DEREGISTER | RAWPOSIX_CAGEID | WASMTIME_CAGEID | THREEI_CAGEID
        ) {
            if let Some(calls) = self.registrations.get_mut(&target) {
                calls.remove(&syscall);
                if calls.is_empty() {
                    self.registrations.remove(&target);
                }
            }
        } else {
            self.registrations
                .entry(target)
                .or_default()
                .insert(syscall, handler);
        }
    }

    pub(crate) fn copy(&mut self, source: u64, target: u64) {
        if let Some(calls) = self.registrations.get(&source).cloned() {
            self.registrations.insert(target, calls);
        } else {
            self.registrations.remove(&target);
        }
    }

    pub(crate) fn remove_target(&mut self, target: u64) {
        self.registrations.remove(&target);
    }

    pub(crate) fn remove_handler(&mut self, handler: u64) {
        self.registrations.retain(|_, calls| {
            calls.retain(|_, registered| *registered != handler);
            !calls.is_empty()
        });
    }

    pub fn allows(&self, caller: u64, target: u64) -> bool {
        // RawPOSIX delegates clone, exec and exit to the Wasmtime endpoint.
        if caller == target || (caller == RAWPOSIX_CAGEID && target == WASMTIME_CAGEID) {
            return true;
        }
        let mut pending = vec![target];
        let mut visited = HashSet::new();
        while let Some(cage) = pending.pop() {
            if cage == caller {
                return true;
            }
            if visited.insert(cage) {
                if let Some(calls) = self.registrations.get(&cage) {
                    pending.extend(calls.values().copied());
                }
            }
        }
        false
    }
}
