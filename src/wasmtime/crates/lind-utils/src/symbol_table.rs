use std::{cmp::Ordering, sync::atomic::AtomicI32};

use anyhow::Result;
use dashmap::DashMap;
use skiplist::OrderedSkipList;
use sysdefs::constants::{RTLD_GLOBAL, RTLD_LOCAL, RTLD_NODELETE};

/// SymbolMap represents the symbol namespace of a single dynamically loaded library.
/// It stores exported symbols and associated metadata such as dlopen mode,
/// file identity (inode), and reference count.
#[derive(Default)]
pub struct SymbolMap {
    /// Mapping from symbol name to resolved symbol value (e.g., function index or address).
    symbol_map: DashMap<String, u32>,

    /// dlopen mode flags (e.g., RTLD_GLOBAL, RTLD_LOCAL, RTLD_NODELETE).
    mode: i32,

    /// Unique handler assigned to this library instance.
    handler: i32,

    /// Inode number of the library file. Used to prevent duplicate loads
    /// of the same underlying file.
    inode: u64,

    /// Reference count of this library (for dlopen/dlclose semantics).
    ref_count: AtomicI32, // how many references do this library have
}

impl Clone for SymbolMap {
    fn clone(&self) -> Self {
        Self {
            symbol_map: self.symbol_map.clone(),
            mode: self.mode,
            handler: self.handler,
            inode: self.inode,
            ref_count: AtomicI32::new(self.ref_count.load(std::sync::atomic::Ordering::Relaxed)),
        }
    }
}

// SymbolMap instances are ordered by handler so they can be stored
// in OrderedSkipList and looked up by handler efficiently.
impl PartialEq for SymbolMap {
    fn eq(&self, other: &Self) -> bool {
        self.handler == other.handler
    }
}
impl Eq for SymbolMap {}

impl Ord for SymbolMap {
    fn cmp(&self, other: &Self) -> Ordering {
        self.handler.cmp(&other.handler)
    }
}
impl PartialOrd for SymbolMap {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl SymbolMap {
    /// Create a new SymbolMap for a freshly loaded library.
    /// The handler is assigned later by SymbolTable.
    pub fn new(mode: i32, inode: u64) -> Self {
        Self {
            symbol_map: DashMap::new(),
            mode,
            handler: -1, // placeholder until assigned
            inode,
            ref_count: AtomicI32::new(1), // first reference from initial dlopen
        }
    }

    /// Construct a temporary SymbolMap used only for skiplist lookup
    /// (e.g., deletion or handler search).
    pub fn new_rm_target(handler: i32) -> Self {
        Self {
            symbol_map: DashMap::new(),
            mode: 0,
            handler,
            inode: 0,
            ref_count: AtomicI32::new(1),
        }
    }

    /// Add a resolved symbol into this library's namespace.
    pub fn add(&mut self, name: String, val: u32) {
        self.symbol_map.insert(name, val);
    }

    /// Assign the runtime handler to this library.
    pub fn set_handler(&mut self, handler: i32) {
        self.handler = handler;
    }

    /// Check whether this library was opened with local visibility.
    /// Local libraries do not participate in global symbol resolution.
    pub fn is_local(&self) -> bool {
        (self.mode & RTLD_GLOBAL) == 0
    }

    /// Check whether this library can be unloaded.
    /// RTLD_NODELETE libraries remain resident even if ref_count reaches zero.
    pub fn deletable(&self) -> bool {
        (self.mode & RTLD_NODELETE) == 0
    }

    /// Increment reference count (dlopen on already-loaded library).
    /// Relaxed ordering is sufficient since ref_count is only used
    /// for lifetime management.
    pub fn increment_ref(&self) {
        self.ref_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Decrement reference count.
    /// Returns true if this was the last reference.
    pub fn decrement_ref(&self) -> bool {
        let curr = self
            .ref_count
            .fetch_sub(1, std::sync::atomic::Ordering::Relaxed);

        // Ensure ref_count never goes negative.
        debug_assert!(curr > 0);

        return curr == 1;
    }

    /// Look up a symbol inside this library.
    pub fn find(&self, name: &str) -> Option<u32> {
        if let Some(val) = self.symbol_map.get(name) {
            Some(*val)
        } else {
            None
        }
    }
}

/// SymbolTable manages all loaded libraries and their symbol namespaces.
/// It assigns unique handlers, prevents duplicate loads of the same file,
/// and performs symbol resolution across libraries.
#[derive(Default)]
pub struct SymbolTable {
    /// Ordered collection of loaded libraries, sorted by handler.
    /// This preserves deterministic lookup and enables handler-based search.
    symbol_table: OrderedSkipList<SymbolMap>,
    /// Monotonically increasing handler allocator.
    next_handler: AtomicI32,
    /// Map from inode to handler, used to avoid loading the same file twice.
    inode_map: DashMap<u64, i32>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self {
            symbol_table: OrderedSkipList::new(),
            next_handler: AtomicI32::new(1),
            inode_map: DashMap::new(),
        }
    }

    /// Add a new library's SymbolMap into the table.
    /// If the same inode already exists, return the existing handler.
    pub fn add(&mut self, mut symbol_map: SymbolMap) -> i32 {
        debug_assert!(self.check_library_loaded(symbol_map.inode).is_none());

        let handler = self.get_next_handler();
        symbol_map.set_handler(handler);

        self.inode_map.insert(symbol_map.inode, handler);
        self.symbol_table.insert(symbol_map);

        handler
    }

    /// Decrement reference count for the library identified by handler.
    /// If the reference count reaches zero and the library is deletable,
    /// remove it from the symbol table.
    pub fn delete_by_handler(&mut self, handler: i32) -> Result<()> {
        // Create lookup key based on handler.
        let target = SymbolMap::new_rm_target(handler);

        // Locate library in ordered skiplist.
        let index = self
            .symbol_table
            .index_of(&target)
            .ok_or(anyhow::anyhow!("handler does not exist"))?;
        let mut symbol_map = self.symbol_table.get(index as usize).unwrap();

        // Only remove if:
        // 1) reference count drops to zero
        // 2) library is not marked RTLD_NODELETE
        if symbol_map.decrement_ref() && symbol_map.deletable() {
            self.inode_map.remove(&symbol_map.inode);
            self.symbol_table.remove(&target);
        }

        Ok(())
    }

    /// Perform global-scope symbol resolution.
    /// Iterate through loaded libraries in order and return the first
    /// matching symbol from a globally visible library.
    pub fn find_symbol_from_global_scope(&self, name: &str) -> Option<u32> {
        for symbol_map in &self.symbol_table {
            // Skip libraries that are not globally visible.
            if symbol_map.is_local() {
                continue;
            }

            if let Some(val) = symbol_map.find(name) {
                return Some(val);
            }
        }

        None
    }

    /// Look up a symbol from a specific library identified by handler.
    pub fn find_symbol_from_handler(&self, handler: i32, name: &str) -> Option<u32> {
        let target = SymbolMap::new_rm_target(handler);
        let index = self.symbol_table.index_of(&target);
        if index.is_none() {
            return None;
        }

        let symbol_map = self.symbol_table.get(index.unwrap() as usize).unwrap();

        symbol_map.find(name)
    }

    /// Check whether a library identified by `inode` has already been loaded.
    ///
    /// If the library is present, increment its reference count (dlopen semantics)
    /// and return its existing handler. Otherwise, return `None`.
    ///
    /// This prevents loading the same physical library file multiple times and
    /// ensures reference counting is properly maintained for repeated dlopen calls.
    pub fn check_library_loaded(&self, inode: u64) -> Option<i32> {
        // Look up existing handler by inode (file identity).
        if let Some(handler) = self.inode_map.get(&inode) {
            let target = SymbolMap::new_rm_target(*handler);
            let index = self.symbol_table.index_of(&target).unwrap();
            let mut symbol_map = self.symbol_table.get(index as usize).unwrap();
            // Increment reference count to reflect another dlopen reference.
            symbol_map.increment_ref();

            return Some(*handler);
        } else {
            None
        }
    }

    /// Return the number of loaded libraries.
    pub fn count(&self) -> usize {
        self.symbol_table.len()
    }

    /// Allocate the next unique library handler.
    /// Relaxed ordering is sufficient because handlers only need uniqueness.
    fn get_next_handler(&self) -> i32 {
        self.next_handler
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

impl Clone for SymbolTable {
    fn clone(&self) -> Self {
        let mut symbol_table = OrderedSkipList::new();

        for item in self.symbol_table.iter() {
            symbol_table.insert(item.clone());
        }

        Self {
            symbol_table: symbol_table,
            next_handler: AtomicI32::new(
                self.next_handler.load(std::sync::atomic::Ordering::Relaxed),
            ),
            inode_map: self.inode_map.clone(),
        }
    }
}
