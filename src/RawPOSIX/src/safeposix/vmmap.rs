use crate::constants::{
    MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED, MAP_PRIVATE, MAP_SHARED, PAGESHIFT, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE
};
use std::io;
use nodit::NoditMap;
use nodit::{interval::ie, Interval};
use crate::fdtables;
use crate::safeposix::cage::syscall_error;
use crate::safeposix::cage::Errno;

const DEFAULT_VMMAP_SIZE: u32 = 1 << (32 - PAGESHIFT);

/// Used to identify whether the vmmap entry is backed anonymously,
/// by an fd, or by a shared memory segment
/// 
/// This enum represents different types of memory backing:
/// - None: Used as a placeholder when no backing type is available
/// - Anonymous: Memory not backed by any file (e.g. heap allocations)
/// - SharedMemory: Memory backed by a shared memory segment, identified by shmid
/// - FileDescriptor: Memory backed by a file, identified by file descriptor
#[allow(dead_code)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryBackingType {
    None, // just a dummy value for places where it needs to be passed, but you dont have the value
    Anonymous,
    SharedMemory(u64),   // stores shmid
    FileDescriptor(u64), // stores file descriptor addr
}


/// An entry in the virtual memory map that contains fields such as page number, number of pages, 
/// permissions, file offset, file size, shared memory ID, and backing fields to distinguish memory types.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct VmmapEntry {
    pub page_num: u32,     // Base virtual address shifted right by NACL_PAGESHIFT
    pub npages: u32,       // Number of pages in this mapping
    pub prot: i32,         // Current memory protection flags (read/write/execute)
    pub maxprot: i32,      // Maximum allowed protection flags
    pub flags: i32,        // Memory mapping flags (shared/private/fixed/anonymous)
    pub removed: bool,     // Flag indicating if entry has been marked for removal
    pub file_offset: i64,  // Offset into the backing file/device
    pub file_size: i64,    // Size of the backing store
    pub cage_id: u64,      // Identifier for the security cage
    pub backing: MemoryBackingType, // Type of memory backing for this region
}


// Implement methods for VmmapEntry
// Constructor to create a new VmmapEntry
#[allow(dead_code)]
impl VmmapEntry {
    /// Creates a new VmmapEntry with the specified parameters
    ///
    /// Arguments:
    /// - page_num: Starting page number of the memory region
    /// - npages: Number of pages in the memory region
    /// - prot: Initial protection flags for the pages
    /// - maxprot: Maximum allowed protection flags
    /// - flags: Memory mapping flags
    /// - removed: Whether this entry is marked for removal
    /// - file_offset: Offset into the backing file/device
    /// - file_size: Size of the backing store
    /// - cage_id: Security cage identifier
    /// - backing: Type of memory backing
    ///
    /// Returns a new VmmapEntry instance initialized with the provided values
    pub fn new(
        page_num: u32,
        npages: u32,
        prot: i32,
        maxprot: i32,
        flags: i32,
        removed: bool,
        file_offset: i64,
        file_size: i64,
        cage_id: u64, //This is the cage id to refer to for file backings
        backing: MemoryBackingType,
    ) -> Self {
        // Create and return a new VmmapEntry with the provided values
        return VmmapEntry {
            page_num,
            npages,
            prot,
            maxprot,
            flags,
            removed,
            file_offset,
            file_size,
            cage_id,
            backing,
        };
    }

    // Gets the maximum protection flags allowed for file-backed memory mappings
    //
    // Arguments:
    // - cage_id: Security cage identifier
    // - virtual_fd: Virtual file descriptor
    //
    // Returns the maximum protection flags as an i32, based on the file's mode
    fn get_max_prot(&self, cage_id: u64, virtual_fd: u64) -> i32 {
        // Translate the virtual file descriptor to a real one
        let wrappedvfd = fdtables::translate_virtual_fd(cage_id, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fstat", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        // Get file stats using fstat
        let mut libc_statbuf: libc::stat = unsafe { std::mem::zeroed() };
        let libcret = unsafe {
            libc::fstat(vfd.underfd as i32, &mut libc_statbuf)
        };

        // Return the file mode which contains protection flags
        libc_statbuf.st_mode as i32
    }
}

/// VmmapOps trait provides an interface that can be shared by different virtual memory management implementations, 
/// allowing different Vmmap versions to share the same interface.
///
/// This trait defines the core operations that any virtual memory map implementation must support:
/// - Adding/removing memory mappings
/// - Updating protection flags
/// - Searching for free space
/// - Querying existing mappings
/// - Iterating over memory regions
#[allow(dead_code)]
pub trait VmmapOps {

    // Method to update a memory map entry
    fn update(
        &mut self,
        page_num: u32,
        npages: u32,
        prot: i32,
        maxprot: i32,
        flags: i32,
        backing: MemoryBackingType,
        remove: bool,
        file_offset: i64,
        file_size: i64,
        cage_id: u64,
    ) -> Result<(), io::Error>;

    // Method to add a new entry to the memory map
    fn add_entry(&mut self, vmmap_entry_ref: VmmapEntry);

    // Method to add an entry with override
    fn add_entry_with_overwrite(
        &mut self,
        page_num: u32,
        npages: u32,
        prot: i32,
        maxprot: i32,
        flags: i32,
        backing: MemoryBackingType,
        file_offset: i64,
        file_size: i64,
        cage_id: u64,
    ) -> Result<(), io::Error>;

    // Method to change protection of a memory region
    // Modifies protection for existing pages in the region
    // Should be able to handle splitting of existing pages when necessary
    // Should maintain mapping consistency while changing protections
    fn change_prot(&mut self, page_num: u32, npages: u32, new_prot: i32);

    // Method to remove an entry from the memory map
    fn remove_entry(&mut self, page_num: u32, npages: u32) -> Result<(), io::Error>;

    // Method to check if requested pages exist with proper permissions
    // NaCl code enforces PROT_READ when any protection exists
    // Returns end page number if mapping is found and has proper permissions
    fn check_existing_mapping(&self, page_num: u32, npages: u32, prot: i32) -> bool;

    // Method to check address mapping
    fn check_addr_mapping(&mut self, page_num: u32, npages: u32, prot: i32) -> Option<u32>;

    // Method to find a page in the memory map
    fn find_page(&self, page_num: u32) -> Option<&VmmapEntry>;

    // Method to find a mutable page in the memory map
    fn find_page_mut(&mut self, page_num: u32) -> Option<&mut VmmapEntry>;

    // Method to iterate over pages
    fn find_page_iter(
        &self,
        page_num: u32,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &VmmapEntry)>;

    // Method to iterate over mutable pages
    fn find_page_iter_mut(
        &mut self,
        page_num: u32,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &mut VmmapEntry)>;


    // Method to get the first entry in the memory map
    fn first_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)>;

    // Method to get the last entry in the memory map
    fn last_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)>;

    // Method to iterate over entries in both directions
    fn double_ended_iter(&self) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &VmmapEntry)>;

    // Method to iterate over mutable entries in both directions
    fn double_ended_iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &mut VmmapEntry)>;

    // Method to find space in the memory map
    // Searches for a contiguous region of at least 'n' free pages
    // Returns the interval of the free space
    fn find_space(&self, npages: u32) -> Option<Interval<u32>>;

    // Method to find space above a hint
    fn find_space_above_hint(&self, npages: u32, hint: u32) -> Option<Interval<u32>>;

    // Method to find space for memory mappings with alignment requirements
    // Rounds page numbers up to the nearest multiple of pages_per_map
    // Returns the interval of the free space
    fn find_map_space(&self, num_pages: u32, pages_per_map: u32) -> Option<Interval<u32>>;

    // Method to find map space with a hint
    fn find_map_space_with_hint(
        &self,
        num_pages: u32,
        pages_per_map: u32,
        hint: u32,
    ) -> Option<Interval<u32>>;
}

/// Represents a virtual memory map that manages memory regions and their attributes
///
/// Fields:
/// - entries: NoditMap storing the memory regions indexed by page number
/// - cached_entry: Optional cached entry for performance optimization
/// - base_address: Optional base address for WASM memory
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Vmmap {
    pub entries: NoditMap<u32, Interval<u32>, VmmapEntry>, // Keyed by `page_num`
    pub cached_entry: Option<VmmapEntry>,                  // TODO: is this still needed?
                                                           // Use Option for safety
    pub base_address: Option<usize>,                       // wasm base address. None means uninitialized yet

    pub start_address: u32,                                // start address of valid vmmap address range
    pub end_address: u32,                                  // end address of valid vmmap address range
    pub program_break: u32,                                // program break (i.e. heap bottom) of the memory
}

#[allow(dead_code)]
impl Vmmap {
    /// Creates a new empty virtual memory map
    pub fn new() -> Self {
        // Initialize a new Vmmap with empty entries and no cached entry or base address
        Vmmap {
            entries: NoditMap::new(),
            cached_entry: None,
            base_address: None,
            start_address: 0,
            end_address: DEFAULT_VMMAP_SIZE,
            program_break: 0,
        }
    }

    /// Rounds up a page number to the nearest multiple of pages_per_map
    ///
    /// Arguments:
    /// - npages: Number of pages to round up
    /// - pages_per_map: Alignment requirement in pages
    ///
    /// Returns the rounded up page number
    fn round_page_num_up_to_map_multiple(&self, npages: u32, pages_per_map: u32) -> u32 {
        // Add (pages_per_map - 1) to npages and mask off lower bits to round up
        (npages + pages_per_map - 1) & !(pages_per_map - 1)
    }

    /// Truncates a page number down to the nearest multiple of pages_per_map
    ///
    /// Arguments:
    /// - npages: Number of pages to truncate
    /// - pages_per_map: Alignment requirement in pages
    ///
    /// Returns the truncated page number
    fn trunc_page_num_down_to_map_multiple(&self, npages: u32, pages_per_map: u32) -> u32 {
        // Mask off lower bits to truncate down to multiple
        npages & !(pages_per_map - 1)
    }

    /// Sets the base address for WASM memory
    ///
    /// Arguments:
    /// - base_address: The base address to set
    pub fn set_base_address(&mut self, base_address: usize) {
        println!("set base address to {}", base_address);
        // Store the provided base address
        self.base_address = Some(base_address);
    }

    /// Sets the program break for the memory
    ///
    /// Arguments:
    /// - program_break: The program break to set
    pub fn set_program_break(&mut self, program_break: u32) {
        self.program_break = program_break;
    }

    /// Converts a user address to a system address
    ///
    /// Arguments:
    /// - address: User space address to convert
    ///
    /// Returns the corresponding system address
    pub fn user_to_sys(&self, address: u32) -> usize {
        // Add base address to user address to get system address
        address as usize + self.base_address.unwrap()
    }

    /// Converts a system address to a user address
    ///
    /// Arguments:
    /// - address: System address to convert
    ///
    /// Returns the corresponding user space address
    pub fn sys_to_user(&self, address: usize) -> u32 {
        // Subtract base address from system address to get user address
        (address as usize - self.base_address.unwrap()) as u32
    }

    // Visits each entry in the vmmap, applying a visitor function to each entry
    // 
    // The visitor function should be used to:
    // - Validate memory map consistency
    // - Gather statistics about memory usage
    // - Perform operations across all entries
    // - Support debugging and auditing features
    fn visit() {}


    // Prints detailed debug information about the vmmap's current state
    // 
    // Should output information including:
    // - Page ranges and sizes for each mapping
    // - Protection flags (current and maximum)
    // - Mapping flags
    // - Backing store information (Anonymous, File, or Shared Memory)
    // - File information (offset and size) when applicable
    // - Any gaps in the address space
    fn debug() {}
}

impl VmmapOps for Vmmap {
    /// Adds a new entry to the virtual memory map
    /// 
    /// This function inserts a new VmmapEntry into the memory map data structure.
    /// The entry is inserted with strict bounds checking to ensure memory safety.
    ///
    /// Arguments:
    /// - vmmap_entry_ref: The VmmapEntry to add containing page numbers, permissions, etc.
    ///
    /// The interval is created from:
    /// - Start: vmmap_entry_ref.page_num 
    /// - End: vmmap_entry_ref.page_num + vmmap_entry_ref.npages (inclusive)
    fn add_entry(&mut self, vmmap_entry_ref: VmmapEntry) {
        // Create interval from page range and insert entry with strict bounds checking
        let _ = self.entries.insert_strict(
            // pages x to y, y included
            ie(
                vmmap_entry_ref.page_num,
                vmmap_entry_ref.page_num + vmmap_entry_ref.npages,
            ),
            vmmap_entry_ref,
        );
    }

    /// Adds a new entry to the virtual memory map with overwrite capability
    ///
    /// This function creates and inserts a new VmmapEntry, overwriting any existing
    /// entries that overlap with the specified page range.
    ///
    /// Arguments:
    /// - page_num: Starting page number for the mapping
    /// - npages: Number of pages to map
    /// - prot: Current protection flags (read/write/execute)
    /// - maxprot: Maximum allowed protection flags
    /// - flags: Mapping flags (shared/private/fixed/anonymous)
    /// - backing: Type of memory backing (Anonymous/SharedMemory/FileDescriptor)
    /// - file_offset: Offset into backing file/device
    /// - file_size: Size of backing store
    /// - cage_id: Security cage identifier
    ///
    /// Returns:
    /// - Ok(()) on success
    /// - Err(io::Error) on failure
    fn add_entry_with_overwrite(
        &mut self,
        page_num: u32,
        npages: u32,
        prot: i32,
        maxprot: i32,
        flags: i32,
        backing: MemoryBackingType,
        file_offset: i64,
        file_size: i64,
        cage_id: u64,
    ) -> Result<(), io::Error> {
        // Call update() to handle the insertion with overwrite capability
        self.update(
            page_num,
            npages,
            prot,
            maxprot,
            flags,
            backing,
            false, // Not removing
            file_offset,
            file_size,
            cage_id,
        )
    }

    /// Removes a memory mapping from the specified page range
    /// 
    /// This function will not return any errors pertaining to the page number not mapping
    /// to any existing pages, as the remove operation is done on a best efforts basis:
    /// 1. First an insert overwrite operation with the below page range is performed, causing
    /// a new interval to be created over the provided page range, appropriately partitioning
    /// boundary pages.
    /// 2. This new interval is then deleted, leaving the underlying range unmapped
    ///
    /// Arguments:
    /// - page_num: Starting page number to remove
    /// - npages: Number of pages to remove
    ///
    /// Returns:
    /// - Ok(()) on success
    /// - Err(io::Error) on failure
    fn remove_entry(&mut self, page_num: u32, npages: u32) -> Result<(), io::Error> {
        // Call update() with remove flag set to true
        self.update(
            page_num,
            npages,
            0,
            0,
            0,
            MemoryBackingType::None,
            true, // Removing
            0,
            0,
            0,
        )
    }

    /// Updates or removes a memory mapping entry
    ///
    /// This is the core function that handles both insertion and removal of memory mappings.
    /// It performs validation and maintains mapping consistency.
    ///
    /// Arguments:
    /// - page_num: Starting page number
    /// - npages: Number of pages
    /// - prot: Current protection flags
    /// - maxprot: Maximum allowed protection
    /// - flags: Mapping flags
    /// - backing: Type of memory backing
    /// - remove: If true, removes the mapping instead of updating
    /// - file_offset: Offset into backing file
    /// - file_size: Size of backing store
    /// - cage_id: Security cage identifier
    ///
    /// Returns:
    /// - Ok(()) on success
    /// - Err(io::Error) if npages is 0 or other error occurs
    fn update(
        &mut self,
        page_num: u32,
        npages: u32,
        prot: i32,
        maxprot: i32,
        flags: i32,
        backing: MemoryBackingType,
        remove: bool,
        file_offset: i64,
        file_size: i64,
        cage_id: u64,
    ) -> Result<(), io::Error> {
        // Validate input - number of pages must be non-zero
        if npages == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Number of pages cannot be zero",
            ));
        }

        // Calculate page range
        let new_region_end_page = page_num + npages;
        let new_region_start_page = page_num;

        // Create new entry if not removing
        let new_entry = VmmapEntry {
            page_num,
            npages,
            prot,
            maxprot,
            flags,
            backing,
            file_offset,
            file_size,
            removed: false,
            cage_id,
        };

        // Insert new entry, overwriting any existing entries in the range
        let _ = self
            .entries
            .insert_overwrite(ie(new_region_start_page, new_region_end_page), new_entry);

        // If removing, delete the entry after insertion
        if remove {
            // Remove all entries that overlap with the specified range
            let _ = self
                .entries
                .remove_overlapping(ie(new_region_start_page, new_region_end_page));
        }

        Ok(())
    }

    /// Changes memory protection flags for a range of pages
    ///
    /// This function modifies the protection flags for existing pages in the specified region.
    /// It handles splitting of existing pages when necessary to maintain proper protection boundaries.
    ///
    /// Arguments:
    /// - page_num: Starting page number
    /// - npages: Number of pages to modify
    /// - new_prot: New protection flags to apply
    ///
    /// Implementation details:
    /// - Handles partial overlaps by splitting entries
    /// - Maintains mapping consistency during protection changes
    /// - Updates protection flags for fully contained pages
    fn change_prot(&mut self, page_num: u32, npages: u32, new_prot: i32) {
        // Calculate page range
        let new_region_end_page = page_num + npages;
        let new_region_start_page = page_num;

        // Store intervals that need to be inserted after iteration
        let mut to_insert = Vec::new();

        // Iterate over overlapping entries
        for (overlap_interval, entry) in self
            .entries
            .overlapping_mut(ie(new_region_start_page, new_region_end_page))
        {
            let mut ent_start = overlap_interval.start();
            let ent_end = overlap_interval.end();

            // Case 1: Entry starts before region but extends into it
            if ent_start < new_region_start_page && ent_end > new_region_start_page {
                to_insert.push(ie(new_region_start_page, ent_end));
                ent_start = new_region_start_page;
            }

            // Case 2: Entry extends beyond region end
            if ent_start < new_region_end_page && ent_end > new_region_end_page {
                to_insert.push(ie(ent_start, new_region_end_page));
            } else {
                // Case 3: Entry is fully contained - update protection
                entry.prot = new_prot;
            }
        }

        // Insert new intervals with updated protection
        for interval in to_insert {
            // Get and clone the entry at the start of the interval
            let mut interval_val = self.entries.get_at_point(interval.start()).unwrap().clone();
            // Update protection
            interval_val.prot = new_prot;
            // Insert the new interval
            let _ = self.entries.insert_overwrite(interval, interval_val);
        }
    }

    /// Checks if a memory mapping exists with specified protection
    ///
    /// Verifies if a continuous mapping exists for the given page range
    /// and checks if the requested protection flags are compatible.
    ///
    /// Arguments:
    /// - page_num: Starting page number to check
    /// - npages: Number of pages to verify
    /// - prot: Required protection flags
    ///
    /// Returns:
    /// - true if mapping exists with compatible protection
    /// - false if mapping doesn't exist or protection is incompatible
    fn check_existing_mapping(&self, page_num: u32, npages: u32, prot: i32) -> bool {
        // Calculate end page and create interval for region
        let region_end_page = page_num + npages;
        let region_interval = ie(page_num, region_end_page);

        // Case 1: No overlapping entries exist
        if !self.entries.overlaps(region_interval) {
            return false;
        }

        let mut current_page = page_num;

        // Iterate over overlapping intervals
        for (_interval, entry) in self.entries.overlapping(region_interval) {
            let ent_end_page = entry.page_num + entry.npages;
            let flags = entry.maxprot;

            // Case 2: Region is fully inside existing entry
            if entry.page_num <= current_page && region_end_page <= ent_end_page {
                return (prot & !flags) == 0;
            }

            // Case 3: Region overlaps with current entry
            if entry.page_num <= current_page && current_page < ent_end_page {
                if (prot & !flags) != 0 {
                    return false;
                }
                current_page = ent_end_page; // Move to next region
            }

            // Case 4: Gap between entries
            if current_page < entry.page_num {
                return false;
            }
        }

        false
    }

    /// Checks address mapping with caching optimization
    ///
    /// Verifies memory mapping and protection flags for a page range,
    /// using a cached entry when possible for better performance.
    ///
    /// Arguments:
    /// - page_num: Starting page number
    /// - npages: Number of pages to check
    /// - prot: Required protection flags
    ///
    /// Returns:
    /// - Some(end_page) if mapping exists and protection is compatible
    /// - None if mapping invalid or protection incompatible
    ///
    /// Implementation details:
    /// - Uses cached entry for quick lookups
    /// - Enforces PROT_READ when other protections are set
    /// - Handles partial overlaps and gaps
    fn check_addr_mapping(&mut self, page_num: u32, npages: u32, prot: i32) -> Option<u32> {
        // Calculate end page of region
        let region_end_page = page_num + npages;

        // Case 1: Check cached entry first for performance
        if let Some(ref cached_entry) = self.cached_entry {
            let ent_end_page = cached_entry.page_num + cached_entry.npages;
            let mut flags = cached_entry.prot;

            // Enforce PROT_READ if any protection exists
            if flags & (PROT_EXEC | PROT_READ | PROT_WRITE) != PROT_NONE {
                flags |= PROT_READ;
            }

            // Check if region is inside cached entry with compatible protection
            if cached_entry.page_num <= page_num && region_end_page <= ent_end_page {
                if prot & !flags == 0 {
                    return Some(ent_end_page);
                }
            }
        }

        // Case 2: Check overlapping entries if cache miss
        let mut current_page = page_num;
        for (_, entry) in self.entries.overlapping(ie(page_num, region_end_page)) {
            let ent_end_page = entry.page_num + entry.npages;
            let mut flags = entry.prot;

            // Enforce PROT_READ if any protection exists
            if flags & (PROT_EXEC | PROT_READ | PROT_WRITE) != PROT_NONE {
                flags |= PROT_READ;
            }

            // Case 2a: Region fully inside entry
            if entry.page_num <= current_page && region_end_page <= ent_end_page {
                self.cached_entry = Some(entry.clone()); // Update cache
                if prot & !flags == 0 {
                    return Some(ent_end_page);
                }
            }
            // Case 2b: Partial overlap
            else if entry.page_num <= current_page && current_page < ent_end_page {
                if prot & !flags != 0 {
                    return None;
                }
                current_page = ent_end_page; // Move to next region
            }
            // Case 2c: Gap between entries
            else if current_page < entry.page_num {
                return None;
            }
        }

        // Case 3: No valid mapping found
        None
    }

    /// Finds a page entry in the memory map
    ///
    /// Arguments:
    /// - page_num: Page number to find
    ///
    /// Returns:
    /// - Some(&VmmapEntry) if page exists
    /// - None if page not found
    fn find_page(&self, page_num: u32) -> Option<&VmmapEntry> {
        // Look up entry containing the specified page number
        self.entries.get_at_point(page_num)
    }

    /// Finds a mutable page entry in the memory map
    ///
    /// Arguments:
    /// - page_num: Page number to find
    ///
    /// Returns:
    /// - Some(&mut VmmapEntry) if page exists
    /// - None if page not found
    fn find_page_mut(&mut self, page_num: u32) -> Option<&mut VmmapEntry> {
        // Look up mutable entry containing the specified page number
        self.entries.get_at_point_mut(page_num)
    }

    /// Gets the last entry in the memory map
    ///
    /// Returns:
    /// - Some((interval, entry)) containing the last mapping
    /// - None if map is empty
    fn last_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)> {
        // Return the last key-value pair in the map
        self.entries.last_key_value()
    }

    /// Gets the first entry in the memory map
    ///
    /// Returns:
    /// - Some((interval, entry)) containing the first mapping
    /// - None if map is empty
    fn first_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)> {
        // Return the first key-value pair in the map
        self.entries.first_key_value()
    }

    /// Creates a double-ended iterator over all entries
    ///
    /// Returns an iterator that can traverse entries in both directions
    fn double_ended_iter(&self) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &VmmapEntry)> {
        // Return iterator over all entries
        self.entries.iter()
    }

    /// Creates a double-ended iterator over all entries with mutable access
    ///
    /// Returns an iterator that can traverse entries in both directions
    /// and modify the entries
    fn double_ended_iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &mut VmmapEntry)> {
        // Return mutable iterator over all entries
        self.entries.iter_mut()
    }

    /// Creates an iterator over pages starting from a given page number
    ///
    /// Arguments:
    /// - page_num: Starting page number for iteration
    ///
    /// Returns:
    /// - Iterator over entries from page_num to end of map
    /// - Empty iterator if map is empty
    fn find_page_iter(
        &self,
        page_num: u32,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &VmmapEntry)> {
        if let Some(last_entry) = self.last_entry() {
            self.entries.overlapping(ie(page_num, last_entry.0.end()))
        } else {
            // Return an empty iterator if no last_entry
            self.entries.overlapping(ie(page_num, page_num))
        }
    }

    /// Creates a mutable iterator over pages starting from a given page number
    ///
    /// Arguments:
    /// - page_num: Starting page number for iteration
    ///
    /// Returns:
    /// - Mutable iterator over entries from page_num to end of map
    /// - Empty iterator if map is empty
    fn find_page_iter_mut(
        &mut self,
        page_num: u32,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &mut VmmapEntry)> {
        if let Some(last_entry) = self.last_entry() {
            self.entries
                .overlapping_mut(ie(page_num, last_entry.0.end()))
        } else {
            // Return an empty iterator if no last_entry
            self.entries.overlapping_mut(ie(page_num, page_num))
        }
    }

    /// Finds available space in the memory map for a new mapping
    ///
    /// Searches for a gap between existing mappings that can accommodate
    /// the requested number of pages.
    ///
    /// Arguments:
    /// - npages: Number of pages needed
    ///
    /// Returns:
    /// - Some(Interval) containing the found space
    /// - None if no suitable space found
    fn find_space(&self, npages: u32) -> Option<Interval<u32>> {
        let start = self.start_address;
        let end = self.end_address;

        let desired_space = npages + 1; // TODO: check if this is correct

        for gap in self
            .entries
            .gaps_trimmed(ie(start, end))
        {
            if gap.end() - gap.start() >= desired_space {
                return Some(gap);
            }
        }

        None
    }

    /// Finds available space above a hint address
    ///
    /// Searches for a gap that can accommodate the requested pages,
    /// starting from the hint address.
    ///
    /// Arguments:
    /// - npages: Number of pages needed
    /// - hint: Starting address to search from
    ///
    /// Returns:
    /// - Some(Interval) containing the found space
    /// - None if no suitable space found
    fn find_space_above_hint(&self, npages: u32, hint: u32) -> Option<Interval<u32>> {
        let start = hint;
        let end = self.end_address;

        let desired_space = npages + 1; // TODO: check if this is correct

        for gap in self.entries.gaps_trimmed(ie(start, end)) {
            if gap.end() - gap.start() >= desired_space {
                return Some(gap);
            }
        }

        None
    }

    /// Finds space for a mapping with alignment constraints
    ///
    /// Searches for available space that satisfies both size and
    /// alignment requirements specified by pages_per_map.
    ///
    /// Arguments:
    /// - num_pages: Number of pages needed
    /// - pages_per_map: Alignment requirement in pages
    ///
    /// Returns:
    /// - Some(Interval) containing aligned space
    /// - None if no suitable space found
    ///
    /// Implementation details:
    /// - Rounds page numbers up to alignment boundaries
    /// - Handles alignment constraints for start and end addresses
    fn find_map_space(&self, num_pages: u32, pages_per_map: u32) -> Option<Interval<u32>> {
        let start = self.start_address;
        let end = self.end_address;

        let rounded_num_pages =
            self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

        for gap in self
            .entries
            .gaps_trimmed(ie(start, end))
        {
            let aligned_start_page =
                self.trunc_page_num_down_to_map_multiple(gap.start(), pages_per_map);
            let aligned_end_page =
                self.round_page_num_up_to_map_multiple(gap.end(), pages_per_map);

            let gap_size = aligned_end_page - aligned_start_page;
            if gap_size >= rounded_num_pages {
                return Some(ie(aligned_end_page - rounded_num_pages, aligned_end_page));
            }
        }

        None
    }

    /// Finds aligned space above a hint address
    ///
    /// Searches for available space that satisfies both size and
    /// alignment requirements, starting from the hint address.
    ///
    /// Arguments:
    /// - num_pages: Number of pages needed
    /// - pages_per_map: Alignment requirement in pages
    /// - hint: Starting address to search from
    ///
    /// Returns:
    /// - Some(Interval) containing aligned space
    /// - None if no suitable space found
    ///
    /// Implementation details:
    /// - Rounds page numbers up to alignment boundaries
    /// - Handles alignment constraints for start and end addresses
    /// - Searches only above the hint address
    fn find_map_space_with_hint(
        &self,
        num_pages: u32,
        pages_per_map: u32,
        hint: u32,
    ) -> Option<Interval<u32>> {
        let start = hint;
        let end = self.end_address;

        let rounded_num_pages =
            self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

        for gap in self.entries.gaps_trimmed(ie(start, end)) {
            let aligned_start_page =
                self.trunc_page_num_down_to_map_multiple(gap.start(), pages_per_map);
            let aligned_end_page =
                self.round_page_num_up_to_map_multiple(gap.end(), pages_per_map);

            let gap_size = aligned_end_page - aligned_start_page;
            if gap_size >= rounded_num_pages {
                return Some(ie(aligned_end_page - rounded_num_pages, aligned_end_page));
            }
        }

        None
    }
}
