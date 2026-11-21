//! Runtime-private per-cage state, indexed by cage ID.
//!
//! Core's cage table does not hold runtime state; each runtime keeps what it
//! needs about a cage in its own `CageTable`, installed when the runtime
//! creates or replaces the cage's image and removed by `terminate_cage`.

use std::sync::{Arc, PoisonError, RwLock};
use sysdefs::constants::lind_platform_const::MAX_CAGEID;

const SLOTS: usize = MAX_CAGEID as usize;

pub(crate) struct CageTable<T>([RwLock<Option<Arc<T>>>; SLOTS]);

impl<T> CageTable<T> {
    pub(crate) const fn new() -> Self {
        Self([const { RwLock::new(None) }; SLOTS])
    }

    fn slot(&self, cage_id: u64) -> Option<&RwLock<Option<Arc<T>>>> {
        self.0.get(usize::try_from(cage_id).ok()?)
    }

    /// Returns the state of `cage_id`. The returned `Arc` keeps the state alive
    /// even if it is replaced or removed concurrently.
    pub(crate) fn get(&self, cage_id: u64) -> Option<Arc<T>> {
        self.slot(cage_id)?
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Installs `value` for `cage_id` and returns the state it replaces.
    ///
    /// Panics if `cage_id` is not a valid cage ID.
    pub(crate) fn insert(&self, cage_id: u64, value: T) -> Option<Arc<T>> {
        let slot = self
            .slot(cage_id)
            .unwrap_or_else(|| panic!("cage table: invalid cage id {cage_id}"));
        slot.write()
            .unwrap_or_else(PoisonError::into_inner)
            .replace(Arc::new(value))
    }

    pub(crate) fn remove(&self, cage_id: u64) -> Option<Arc<T>> {
        self.slot(cage_id)?
            .write()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
    }
}
