pub use std::collections::HashMap;
pub use std::sync::Arc;
pub use parking_lot::RwLock;
pub use once_cell::sync::Lazy;
pub use std::sync::atomic::{AtomicI32, AtomicU64};
pub use std::path::{Path, PathBuf};
use crate::rawposix::vmmap::*;

#[derive(Debug, Clone, Copy)]
pub struct Zombie {
    pub cageid: u64,
    pub exit_code: i32
}

/// I only kept required fields for cage struct
#[derive(Debug)]
pub struct Cage {
    // Identifying ID number for this cage
    pub cageid: u64,
    pub parent: u64,
    // Current working directory of cage, must be able to be unique from other cages
    pub cwd: RwLock<Arc<PathBuf>>, 
    // Identifiers for gid/uid/egid/euid 
    pub gid: AtomicI32,
    pub uid: AtomicI32,
    pub egid: AtomicI32,
    pub euid: AtomicI32,
    // The kernel thread id of the main thread of current cage, used because when we want to send signals, 
    // we want to send to the main thread 
    pub main_threadid: AtomicU64,
    // The zombies field in the Cage struct is used to manage information about child cages that have 
    // exited, but whose exit status has not yet been retrieved by their parent using wait() / waitpid().
    // When a cage exits, shared memory segments are detached, file descriptors are removed from fdtable, 
    // and cage struct is cleaned up, but its exit status are inserted along with its cage id into the end of 
    // its parent cage's zombies list
    pub zombies: RwLock<Vec<Zombie>>,
    pub child_num: AtomicU64,
    pub vmmap: RwLock<Vmmap>
}

/// I borrowed the Linux process management method (bidirectional linked list and tree-like hierarchical management) 
/// to modify our previous cage_table to use hash map. Because it takes into account search efficiency and 
/// higher memory utilization (the purpose of our design is high-performance/high demand(?) computing) and 
/// is suitable for dynamic and discontinuously distributed process management scenarios, and supports additional 
/// logical extensions (eg: process groups or parent-child relationships).
pub static CAGE_MAP: Lazy<RwLock<HashMap<u64, Arc<Cage>>>> = Lazy::new(|| {
    RwLock::new(HashMap::new())
});

pub fn add_cage(cage: Cage) {
    let mut map = CAGE_MAP.write();
    map.insert(cage.cageid, Arc::new(cage));
}

pub fn remove_cage(cageid: u64) {
    let mut map = CAGE_MAP.write();
    map.remove(&cageid);
}

/// Get a copy for current cage struct
pub fn get_cage(cageid: u64) -> Option<Arc<Cage>> {
    let map = CAGE_MAP.read();
    map.get(&cageid).cloned()
}

