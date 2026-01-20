//! VMMAP data structure and structure-related operation
//!
//! This file provides data structures and helper functions of `vmmap` for handling virtual memory regions,
//! including memory allocation, permission management, and address translation. The file introduces
//! `VmmapEntry` to represent memory mappings and `Vmmap` to store and manage these mappings. It also
//! implements the `VmmapOps` trait, which provides core operations such as adding, updating, removing,
//! and searching for memory regions, ensuring proper alignment, protection, and handling of shared
//! and file-backed memory.
//! This file defines `vmmap` data structures.
use fdtables;
use nodit::NoditMap;
use nodit::{interval::ie, Interval};
use std::io;
use sysdefs::constants::err_const::{syscall_error, Errno};
use sysdefs::constants::fs_const::{PAGESHIFT, PROT_EXEC, PROT_NONE, PROT_READ, PROT_WRITE};

/// Default number of virtual memory pages in a `vmmap`.
/// Calculated as 2^(32 - PAGESHIFT), which represents the total pages
/// addressable in a 32-bit virtual address space given the page size.
/// For example, with a 4 KB page size (PAGESHIFT = 12), this equals 1 MB of pages,
/// covering the full 4 GB virtual address space.
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
    pub page_num: u32,    // Base virtual address shifted right by NACL_PAGESHIFT
    pub npages: u32,      // Number of pages in this mapping
    pub prot: i32,        // Current memory protection flags (read/write/execute)
    pub maxprot: i32,     // Maximum allowed protection flags
    pub flags: i32,       // Memory mapping flags (shared/private/fixed/anonymous)
    pub removed: bool,    // Flag indicating if entry has been marked for removal
    pub file_offset: i64, // Offset into the backing file/device
    pub file_size: i64,   // Size of the backing store
    pub cage_id: u64,     // Identifier for the security cage
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
        let _libcret = unsafe { libc::fstat(vfd.underfd as i32, &mut libc_statbuf) };

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
    pub base_address: Option<usize>, // wasm base address. None means uninitialized yet

    pub start_address: u32, // start address of valid vmmap address range
    pub end_address: u32,   // end address of valid vmmap address range
    pub program_break: u32, // program break (i.e. heap bottom) of the memory
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

    // Clear the vmmap struct, used for exec syscall
    // The purpose of `clear()` is to reset the address space of exec. It
    // ensures that all old mappings and states are discarded, allowing the new cage to
    // run in a clean virtual address space, while reusing the existing `Vmmap` instance
    // to avoid extra allocations.
    pub fn clear(&mut self) {
        self.entries = NoditMap::new();
        self.cached_entry = None;
        self.base_address = None;
        self.start_address = 0;
        self.end_address = DEFAULT_VMMAP_SIZE;
        self.program_break = 0;
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

        // Collect information about overlapping entries that need to be modified
        let mut entries_to_modify = Vec::new();

        for (overlap_interval, entry) in self
            .entries
            .overlapping(ie(new_region_start_page, new_region_end_page))
        {
            let ent_start = overlap_interval.start();
            let ent_end = overlap_interval.end();

            // Clone the entry to work with
            let original_entry = entry.clone();

            // Calculate the three potential parts:
            // 1. Before the target region (keep old protection)
            // 2. Inside the target region (apply new protection)
            // 3. After the target region (keep old protection)

            let overlap_start = ent_start.max(new_region_start_page);
            let overlap_end = ent_end.min(new_region_end_page);

            // Store the parts we need to create
            entries_to_modify.push((
                ent_start,
                ent_end,
                overlap_start,
                overlap_end,
                original_entry,
            ));
        }

        // Now modify the entries
        for (ent_start, ent_end, overlap_start, overlap_end, original_entry) in entries_to_modify {
            // Remove the original entry
            let _ = self.entries.remove_overlapping(ie(ent_start, ent_end));

            // Check if protection is actually changing
            let prot_unchanged = original_entry.prot == new_prot;

            if prot_unchanged {
                // Protection isn't changing, keep the entry as-is (no fragmentation)
                let _ = self
                    .entries
                    .insert_overwrite(ie(ent_start, ent_end), original_entry);
            } else {
                // Protection is changing, need to split

                // Part 1: Before the target region (if exists)
                if ent_start < overlap_start {
                    let mut before_entry = original_entry.clone();
                    before_entry.page_num = ent_start;
                    before_entry.npages = overlap_start - ent_start;
                    // Keep original protection
                    let _ = self
                        .entries
                        .insert_overwrite(ie(ent_start, overlap_start), before_entry);
                }

                // Part 2: Inside the target region (apply new protection)
                if overlap_start < overlap_end {
                    let mut inside_entry = original_entry.clone();
                    inside_entry.page_num = overlap_start;
                    inside_entry.npages = overlap_end - overlap_start;
                    inside_entry.prot = new_prot;
                    let _ = self
                        .entries
                        .insert_overwrite(ie(overlap_start, overlap_end), inside_entry);
                }

                // Part 3: After the target region (if exists)
                if overlap_end < ent_end {
                    let mut after_entry = original_entry.clone();
                    after_entry.page_num = overlap_end;
                    after_entry.npages = ent_end - overlap_end;
                    // Keep original protection
                    let _ = self
                        .entries
                        .insert_overwrite(ie(overlap_end, ent_end), after_entry);
                }
            }
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

        for gap in self.entries.gaps_trimmed(ie(start, end)) {
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
    /// - hint: Starting address (in pages) to search from
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

        let rounded_num_pages = self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

        for gap in self.entries.gaps_trimmed(ie(start, end)) {
            let aligned_start_page =
                self.trunc_page_num_down_to_map_multiple(gap.start(), pages_per_map);
            let aligned_end_page = self.round_page_num_up_to_map_multiple(gap.end(), pages_per_map);

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
    /// - hint: Starting address (in pages) to search from
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

        let rounded_num_pages = self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

        for gap in self.entries.gaps_trimmed(ie(start, end)) {
            let aligned_start_page =
                self.trunc_page_num_down_to_map_multiple(gap.start(), pages_per_map);
            let aligned_end_page = self.round_page_num_up_to_map_multiple(gap.end(), pages_per_map);

            let gap_size = aligned_end_page - aligned_start_page;
            if gap_size >= rounded_num_pages {
                // Calculate the aligned end position
                let result_end = aligned_end_page;
                // Calculate aligned start by ensuring it's a multiple of pages_per_map
                let result_start = result_end - rounded_num_pages;
                // Verify both boundaries are properly aligned
                debug_assert!(result_start % pages_per_map == 0);
                debug_assert!(result_end % pages_per_map == 0);
                return Some(ie(result_start, result_end));
            }
        }

        None
    }
}

// Testing
#[cfg(test)]
mod tests {
    use super::*;

    /// Test: Change protection on entire continuous region
    /// Expected: Region should remain continuous with updated protection
    /// Verifies that changing protection on an entire region doesn't fragment it
    #[test]
    fn test_change_prot_entire_region() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region of 10 pages with READ|WRITE protection
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection of the entire region to READ|EXEC
        vmmap.change_prot(100, 10, PROT_READ | PROT_EXEC);

        // Verify the region is still continuous
        let entry = vmmap.find_page(100).expect("Entry should exist");
        assert_eq!(entry.page_num, 100);
        assert_eq!(entry.npages, 10);
        assert_eq!(entry.prot, PROT_READ | PROT_EXEC);

        // Verify no fragmentation - should still be one entry
        let count = vmmap.entries.overlapping(ie(100, 110)).count();
        assert_eq!(count, 1, "Region should remain as a single entry");
    }

    /// Test: Change protection on middle portion of continuous region
    /// Expected: Region should be split into 3 parts
    /// Confirms proper splitting into 3 parts when modifying middle pages
    #[test]
    fn test_change_prot_middle_of_region() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region of 10 pages
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection of middle 4 pages (102-105)
        vmmap.change_prot(102, 4, PROT_READ);

        // Verify the region is split into 3 parts
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 110)).collect();
        assert_eq!(entries.len(), 3, "Region should be split into 3 parts");

        // First part: pages 100-101 with original protection
        let first = vmmap.find_page(100).expect("First part should exist");
        assert_eq!(first.prot, PROT_READ | PROT_WRITE);

        // Middle part: pages 102-105 with new protection
        let middle = vmmap.find_page(102).expect("Middle part should exist");
        assert_eq!(middle.prot, PROT_READ);

        // Last part: pages 106-109 with original protection
        let last = vmmap.find_page(106).expect("Last part should exist");
        assert_eq!(last.prot, PROT_READ | PROT_WRITE);
    }

    /// Test: Change protection on beginning of continuous region
    /// Expected: Region should be split into 2 parts
    /// Tests splitting into 2 parts when modifying start pages
    #[test]
    fn test_change_prot_beginning_of_region() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region of 10 pages
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection of first 3 pages
        vmmap.change_prot(100, 3, PROT_READ);

        // Verify the region is split into 2 parts
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 110)).collect();
        assert_eq!(entries.len(), 2, "Region should be split into 2 parts");

        // First part: pages 100-102 with new protection
        let first = vmmap.find_page(100).expect("First part should exist");
        assert_eq!(first.prot, PROT_READ);

        // Second part: pages 103-109 with original protection
        let second = vmmap.find_page(103).expect("Second part should exist");
        assert_eq!(second.prot, PROT_READ | PROT_WRITE);
    }

    /// Test: Change protection on end of continuous region
    /// Expected: Region should be split into 2 parts
    /// Tests splitting into 2 parts when modifying end pages
    #[test]
    fn test_change_prot_end_of_region() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region of 10 pages
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection of last 3 pages
        vmmap.change_prot(107, 3, PROT_READ);

        // Verify the region is split into 2 parts
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 110)).collect();
        assert_eq!(entries.len(), 2, "Region should be split into 2 parts");

        // First part: pages 100-106 with original protection
        let first = vmmap.find_page(100).expect("First part should exist");
        assert_eq!(first.prot, PROT_READ | PROT_WRITE);

        // Second part: pages 107-109 with new protection
        let second = vmmap.find_page(107).expect("Second part should exist");
        assert_eq!(second.prot, PROT_READ);
    }

    /// Test: Change protection spanning multiple continuous regions
    /// Expected: All affected regions should have updated protection
    /// Verifies correct handling across multiple non-contiguous regions
    #[test]
    fn test_change_prot_spanning_multiple_regions() {
        let mut vmmap = Vmmap::new();

        // Allocate three separate continuous regions
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        vmmap
            .add_entry_with_overwrite(
                120,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        vmmap
            .add_entry_with_overwrite(
                140,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection spanning parts of all three regions (105-145)
        vmmap.change_prot(105, 40, PROT_READ);

        // Verify first region is split (100-104 original, 105-109 changed)
        let first_orig = vmmap
            .find_page(100)
            .expect("First original part should exist");
        assert_eq!(first_orig.prot, PROT_READ | PROT_WRITE);

        let first_changed = vmmap
            .find_page(105)
            .expect("First changed part should exist");
        assert_eq!(first_changed.prot, PROT_READ);

        // Verify second region is fully changed (120-129)
        let second = vmmap.find_page(120).expect("Second region should exist");
        assert_eq!(second.prot, PROT_READ);

        // Verify third region is split (140-144 changed, 145-149 original)
        let third_changed = vmmap
            .find_page(140)
            .expect("Third changed part should exist");
        assert_eq!(third_changed.prot, PROT_READ);

        let third_orig = vmmap
            .find_page(145)
            .expect("Third original part should exist");
        assert_eq!(third_orig.prot, PROT_READ | PROT_WRITE);
    }

    /// Test: Change protection to same value on a smaller subrange
    /// Expected: Region should remain unchanged and not fragment
    /// Ensures no fragmentation when protection doesn't actually change on a subrange
    #[test]
    fn test_change_prot_to_same_value() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection to the same value over a smaller subrange
        vmmap.change_prot(103, 4, PROT_READ | PROT_WRITE);

        // Verify the region remains continuous and unchanged
        let entry = vmmap.find_page(100).expect("Entry should exist");
        assert_eq!(entry.page_num, 100);
        assert_eq!(entry.npages, 10);
        assert_eq!(entry.prot, PROT_READ | PROT_WRITE);

        let count = vmmap.entries.overlapping(ie(100, 110)).count();
        assert_eq!(
            count, 1,
            "Region should remain as a single entry (no fragmentation)"
        );
    }

    /// Test: Change protection on overlapping boundary
    /// Expected: Proper handling of exact boundary cases
    /// Tests single-page modifications and precise boundary handling
    #[test]
    fn test_change_prot_exact_boundaries() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection on a single page in the middle
        vmmap.change_prot(105, 1, PROT_READ);

        // Should be split into 3 parts: [100-104], [105], [106-109]
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 110)).collect();
        assert_eq!(entries.len(), 3, "Should be split into 3 parts");

        // Verify boundaries are correct
        let before = vmmap.find_page(104).expect("Before should exist");
        assert_eq!(before.prot, PROT_READ | PROT_WRITE);

        let changed = vmmap.find_page(105).expect("Changed page should exist");
        assert_eq!(changed.prot, PROT_READ);

        let after = vmmap.find_page(106).expect("After should exist");
        assert_eq!(after.prot, PROT_READ | PROT_WRITE);
    }

    /// Test: Change protection multiple times on same region
    /// Expected: Protection should be updated correctly each time
    /// Verifies correct state after successive protection changes
    #[test]
    fn test_change_prot_multiple_times() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // First change: middle portion to READ
        vmmap.change_prot(103, 4, PROT_READ);

        // Second change: overlapping portion to EXEC
        vmmap.change_prot(105, 3, PROT_EXEC);

        // Verify the final state
        // Should have: [100-102: R|W], [103-104: R], [105-107: X], [108-109: R|W]
        let page_102 = vmmap.find_page(102).expect("Page 102 should exist");
        assert_eq!(page_102.prot, PROT_READ | PROT_WRITE);

        let page_103 = vmmap.find_page(103).expect("Page 103 should exist");
        assert_eq!(page_103.prot, PROT_READ);

        let page_105 = vmmap.find_page(105).expect("Page 105 should exist");
        assert_eq!(page_105.prot, PROT_EXEC);

        let page_108 = vmmap.find_page(108).expect("Page 108 should exist");
        assert_eq!(page_108.prot, PROT_READ | PROT_WRITE);
    }

    /// Test: Change protection with PROT_NONE
    /// Expected: Region should accept PROT_NONE protection
    /// Verifies proper handling of PROT_NONE protection
    #[test]
    fn test_change_prot_to_none() {
        let mut vmmap = Vmmap::new();

        // Allocate a continuous region
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection to PROT_NONE
        vmmap.change_prot(103, 4, PROT_NONE);

        // Verify the protection is set to PROT_NONE
        let entry = vmmap.find_page(103).expect("Entry should exist");
        assert_eq!(entry.prot, PROT_NONE);

        // Verify the region is properly split
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 110)).collect();
        assert_eq!(entries.len(), 3, "Should be split into 3 parts");
    }

    /// Test: Change protection on adjacent regions with different backing
    /// Expected: Each region should maintain its backing type
    /// Confirms backing type (Anonymous, SharedMemory, etc.) is preserved
    #[test]
    fn test_change_prot_preserves_backing_type() {
        let mut vmmap = Vmmap::new();

        // Allocate anonymous region
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Allocate shared memory region
        vmmap
            .add_entry_with_overwrite(
                120,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::SharedMemory(12345),
                0,
                0,
                0,
            )
            .unwrap();

        // Change protection on both regions
        vmmap.change_prot(105, 20, PROT_READ);

        // Verify backing types are preserved
        let anon_entry = vmmap.find_page(105).expect("Anonymous entry should exist");
        assert_eq!(anon_entry.backing, MemoryBackingType::Anonymous);
        assert_eq!(anon_entry.prot, PROT_READ);

        let shm_entry = vmmap
            .find_page(120)
            .expect("Shared memory entry should exist");
        assert_eq!(shm_entry.backing, MemoryBackingType::SharedMemory(12345));
        assert_eq!(shm_entry.prot, PROT_READ);
    }

    /// Test: Verify maxprot is preserved during change_prot
    /// Expected: maxprot should remain unchanged
    /// Verifies that maxprot is preserved during protection changes
    #[test]
    fn test_change_prot_preserves_maxprot() {
        let mut vmmap = Vmmap::new();

        // Allocate with specific maxprot
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ,
                PROT_READ | PROT_WRITE, // maxprot allows write but current doesn't
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Change current protection
        vmmap.change_prot(100, 10, PROT_READ | PROT_WRITE);

        // Verify maxprot is unchanged
        let entry = vmmap.find_page(100).expect("Entry should exist");
        assert_eq!(entry.prot, PROT_READ | PROT_WRITE);
        assert_eq!(entry.maxprot, PROT_READ | PROT_WRITE);
    }

    // ============================================================================
    // Tests for add_entry_with_overwrite behavior
    // These tests clarify what "overwrite" actually means
    // ============================================================================

    /// Test: add_entry_with_overwrite DOES overwrite/replace overlapping entries
    /// Clarifies: "overwrite" means existing overlapping entries are replaced, not merged
    #[test]
    fn test_add_entry_with_overwrite_replaces_existing_full_overlap() {
        let mut vmmap = Vmmap::new();

        // Add initial entry with READ|WRITE protection
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Verify initial state
        let initial = vmmap.find_page(100).expect("Initial entry should exist");
        assert_eq!(initial.prot, PROT_READ | PROT_WRITE);
        assert_eq!(initial.npages, 10);

        // Overwrite with new entry (exact same range, different protection)
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ, // Different protection
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Verify the old entry was replaced, not merged
        let replaced = vmmap.find_page(100).expect("Entry should exist");
        assert_eq!(replaced.prot, PROT_READ, "Protection should be replaced");
        assert_eq!(replaced.npages, 10);

        // Verify there's still only one entry
        let count = vmmap.entries.overlapping(ie(100, 110)).count();
        assert_eq!(count, 1, "Should be exactly one entry (old one replaced)");
    }

    /// Test: add_entry_with_overwrite with partial overlap splits existing entry
    /// Clarifies: Partial overlaps cause the old entry to be split/modified
    #[test]
    fn test_add_entry_with_overwrite_partial_overlap() {
        let mut vmmap = Vmmap::new();

        // Add initial large entry: pages 100-119
        vmmap
            .add_entry_with_overwrite(
                100,
                20,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Overwrite middle portion: pages 105-114
        vmmap
            .add_entry_with_overwrite(
                105,
                10,
                PROT_READ, // Different protection
                PROT_READ | PROT_EXEC,
                0,
                MemoryBackingType::SharedMemory(999), // Different backing
                0,
                0,
                0,
            )
            .unwrap();

        // Verify the entry was split into 3 parts
        let entries: Vec<_> = vmmap.entries.overlapping(ie(100, 120)).collect();
        assert_eq!(entries.len(), 3, "Should be split into 3 parts");

        // First part: pages 100-104 (original)
        let first = vmmap.find_page(100).expect("First part should exist");
        assert_eq!(first.prot, PROT_READ | PROT_WRITE);
        assert_eq!(first.backing, MemoryBackingType::Anonymous);

        // Middle part: pages 105-114 (new/overwritten)
        let middle = vmmap.find_page(105).expect("Middle part should exist");
        assert_eq!(middle.prot, PROT_READ);
        assert_eq!(middle.backing, MemoryBackingType::SharedMemory(999));

        // Last part: pages 115-119 (original)
        let last = vmmap.find_page(115).expect("Last part should exist");
        assert_eq!(last.prot, PROT_READ | PROT_WRITE);
        assert_eq!(last.backing, MemoryBackingType::Anonymous);
    }

    /// Test: add_entry_with_overwrite completely removes overlapped smaller entries
    /// Clarifies: If new entry completely covers old entries, they are removed
    #[test]
    fn test_add_entry_with_overwrite_removes_completely_covered_entries() {
        let mut vmmap = Vmmap::new();

        // Add three separate small entries
        vmmap
            .add_entry_with_overwrite(
                100,
                5,
                PROT_READ,
                PROT_READ | PROT_WRITE,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        vmmap
            .add_entry_with_overwrite(
                110,
                5,
                PROT_WRITE,
                PROT_READ | PROT_WRITE,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        vmmap
            .add_entry_with_overwrite(
                120,
                5,
                PROT_EXEC,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Verify 3 separate entries exist
        assert_eq!(vmmap.entries.iter().count(), 3);

        // Overwrite with one large entry covering all three
        vmmap
            .add_entry_with_overwrite(
                95,
                35, // Covers pages 95-129, including all three previous entries
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                0,
                MemoryBackingType::SharedMemory(12345),
                0,
                0,
                0,
            )
            .unwrap();

        // Verify old entries are gone, replaced by single new entry
        assert_eq!(vmmap.entries.iter().count(), 1, "Should be only one entry");

        let new_entry = vmmap.find_page(100).expect("New entry should exist");
        assert_eq!(new_entry.prot, PROT_READ | PROT_WRITE);
        assert_eq!(new_entry.backing, MemoryBackingType::SharedMemory(12345));
    }

    /// Test: add_entry_with_overwrite at boundaries of existing entries
    /// Clarifies: Behavior when new entry exactly borders existing entries
    #[test]
    fn test_add_entry_with_overwrite_exact_boundaries() {
        let mut vmmap = Vmmap::new();

        // Add entry at pages 100-109
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Add adjacent entry at pages 110-119 (no overlap)
        vmmap
            .add_entry_with_overwrite(
                110,
                10,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Verify two separate entries
        assert_eq!(vmmap.entries.iter().count(), 2);

        // Add entry that ends exactly where first starts (pages 90-99)
        vmmap
            .add_entry_with_overwrite(
                90,
                10,
                PROT_EXEC,
                PROT_EXEC,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Should now have 3 separate entries with no overlap
        assert_eq!(vmmap.entries.iter().count(), 3);

        let entry1 = vmmap.find_page(90).expect("First entry should exist");
        let entry2 = vmmap.find_page(100).expect("Second entry should exist");
        let entry3 = vmmap.find_page(110).expect("Third entry should exist");

        assert_eq!(entry1.prot, PROT_EXEC);
        assert_eq!(entry2.prot, PROT_READ | PROT_WRITE);
        assert_eq!(entry3.prot, PROT_READ);
    }

    // ============================================================================
    // Tests for find_map_space_with_hint parameter expectations
    // These tests clarify that hint is a PAGE NUMBER, not an ADDRESS
    // ============================================================================

    /// Test: find_map_space_with_hint expects PAGE NUMBER, not address
    /// Clarifies: The hint parameter is in pages, not bytes
    /// Confirms hint parameter is a PAGE NUMBER, not a byte address
    #[test]
    fn test_find_map_space_with_hint_uses_page_number() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 1000;

        // Add entry at pages 100-109
        vmmap
            .add_entry_with_overwrite(
                100,
                10,
                PROT_READ | PROT_WRITE,
                PROT_READ | PROT_WRITE,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Find space with hint at page 50 (NOT address 50*PAGESIZE)
        let space = vmmap.find_map_space_with_hint(5, 1, 50);

        assert!(space.is_some(), "Should find space");
        let interval = space.unwrap();

        // Space should be found after hint page 50 but before occupied page 100
        assert!(interval.start() >= 50, "Should start at or after hint page");
        assert!(
            interval.end() <= 100 || interval.start() >= 110,
            "Should not overlap with occupied pages 100-109"
        );
    }

    /// Test: find_map_space_with_hint searches from hint page onwards
    /// Clarifies: Hint is the starting page for search, not an address
    #[test]
    fn test_find_map_space_with_hint_searches_from_hint_page() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 1000;

        // Add entries leaving gap at pages 40-99
        // pages 10-39 (ends at 40)
        // pages 100-149 (starts at 100)
        vmmap
            .add_entry_with_overwrite(
                10,
                30,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        vmmap
            .add_entry_with_overwrite(
                100,
                50,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Search with hint=60 (page number, not address)
        // Should find space in gap 40-99, specifically from page 60 onwards
        let space = vmmap.find_map_space_with_hint(10, 1, 60);

        assert!(space.is_some(), "Should find space");
        let interval = space.unwrap();

        // The found space should be at or after hint page 60
        assert!(
            interval.start() >= 60,
            "Space should start at or after hint page 60, got page {}",
            interval.start()
        );
    }

    /// Test: find_map_space_with_hint with hint=0 behaves like find_map_space
    /// Clarifies: Hint is a page number, 0 means start from beginning
    // Shows that hint=0 searches from the beginning
    #[test]
    fn test_find_map_space_with_hint_zero_hint() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 1000;

        // Add entry leaving space at beginning
        vmmap
            .add_entry_with_overwrite(
                100,
                50,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Find space with hint=0 (start from page 0)
        let space_with_hint = vmmap.find_map_space_with_hint(10, 1, 0);

        assert!(space_with_hint.is_some(), "Should find space");
        let interval = space_with_hint.unwrap();

        // Should find space starting from page 0
        assert!(
            interval.end() <= 100,
            "Should find space before occupied region starting at page 100"
        );
    }

    /// Test: find_map_space_with_hint with large hint page number
    /// Clarifies: Confirms hint is page-based by using large page numbers
    #[test]
    fn test_find_map_space_with_hint_large_page_number() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 100000; // Large address space

        // Add entry at low pages
        vmmap
            .add_entry_with_overwrite(
                100,
                50,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Search with hint at high page number (e.g., page 50000)
        let space = vmmap.find_map_space_with_hint(100, 1, 50000);

        assert!(space.is_some(), "Should find space in large address space");
        let interval = space.unwrap();

        // Space should be found at or after the high hint page
        assert!(
            interval.start() >= 50000,
            "Should find space at or after hint page 50000, got page {}",
            interval.start()
        );
    }

    /// Test: find_map_space_with_hint respects alignment (pages_per_map)
    /// Clarifies: Both hint and result are in pages, alignment is also in pages
    #[test]
    fn test_find_map_space_with_hint_alignment_in_pages() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 10000;

        // Add entry creating a gap
        vmmap
            .add_entry_with_overwrite(
                50,
                20,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Find aligned space: 10 pages, aligned to 8-page boundaries, hint at page 100
        let space = vmmap.find_map_space_with_hint(10, 8, 100);

        assert!(space.is_some(), "Should find aligned space");
        let interval = space.unwrap();

        // Result should be aligned to pages_per_map (8 pages)
        assert_eq!(
            interval.start() % 8,
            0,
            "Start should be aligned to 8-page boundary"
        );
        assert_eq!(
            interval.end() % 8,
            0,
            "End should be aligned to 8-page boundary"
        );
    }

    // ============================================================================
    // Additional tests for unclear function behaviors
    // ============================================================================

    /// Test: add_entry (without overwrite) fails on overlap
    /// Clarifies: add_entry is strict and won't overlap, unlike add_entry_with_overwrite
    #[test]
    fn test_add_entry_strict_no_overlap() {
        let mut vmmap = Vmmap::new();

        // Add entry at pages 100-109
        let entry1 = VmmapEntry::new(
            100,
            10,
            PROT_READ,
            PROT_READ,
            0,
            false,
            0,
            0,
            0,
            MemoryBackingType::Anonymous,
        );
        vmmap.add_entry(entry1);

        // Try to add overlapping entry at pages 105-114
        // Note: add_entry uses insert_strict, which should fail on overlap
        // However, the current implementation doesn't return Result, so we
        // just verify the behavior
        let entry2 = VmmapEntry::new(
            105,
            10,
            PROT_WRITE,
            PROT_WRITE,
            0,
            false,
            0,
            0,
            0,
            MemoryBackingType::Anonymous,
        );
        vmmap.add_entry(entry2);

        // Check if the second entry was actually added or rejected
        // With insert_strict, it should be rejected
        let count = vmmap.entries.overlapping(ie(100, 115)).count();
        // insert_strict should prevent the overlap, so we should still have 1 entry
        assert_eq!(
            count, 1,
            "add_entry with insert_strict should not allow overlapping entries"
        );

        // Verify original entry is unchanged
        let original = vmmap.find_page(100).expect("Original entry should exist");
        assert_eq!(original.prot, PROT_READ);
    }

    /// Test: Verify find_space returns None when no space available
    /// Clarifies: Return value behavior when search fails
    #[test]
    fn test_find_space_returns_none_when_full() {
        let mut vmmap = Vmmap::new();
        vmmap.start_address = 0;
        vmmap.end_address = 100;

        // Fill entire space
        vmmap
            .add_entry_with_overwrite(
                0,
                100,
                PROT_READ,
                PROT_READ,
                0,
                MemoryBackingType::Anonymous,
                0,
                0,
                0,
            )
            .unwrap();

        // Try to find space - should return None
        let space = vmmap.find_space(10);
        assert!(
            space.is_none(),
            "Should return None when no space available"
        );
    }
}
