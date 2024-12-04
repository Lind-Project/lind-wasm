use crate::constants::{
    PROT_NONE, PROT_READ, PROT_WRITE, PROT_EXEC,
    MAP_SHARED, MAP_PRIVATE, MAP_FIXED, MAP_ANONYMOUS,
    MAP_FAILED
};
use std::io;
use nodit::NoditMap;
use nodit::{interval::ie, Interval};

/// Used to identify whether the vmmap entry is backed anonymously,
/// by an fd, or by a shared memory segment

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
    pub page_num: u32, /* base virtual addr >> NACL_PAGESHIFT */
    pub npages: u32,   /* number of pages */
    pub prot: i32,     /* mprotect attribute */
    pub maxprot: i32,
    pub flags: i32,       /* mapping flags */
    pub removed: bool,    /* flag set in fn Update(); */
    pub file_offset: i64, /* offset into desc */
    pub file_size: i64,   /* backing store size */
    pub cage_id: u64,
    pub backing: MemoryBackingType,
}


// Implement methods for VmmapEntry
// Constructor to create a new VmmapEntry
#[allow(dead_code)]
impl VmmapEntry {
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

    // get maximum protection for file based mappings
    // this is effectively whatever mode the file was opened with
    // we need this because we shouldnt be able to change filed backed mappings 
    // to have protections exceeding that of the file
    fn get_max_prot(&self, cage_id: u64, virtual_fd: u64) -> i32 {

        let wrappedvfd = fdtables::translate_virtual_fd(cage_id, virtual_fd as u64);
        if wrappedvfd.is_err() {
            return syscall_error(Errno::EBADF, "fstat", "Bad File Descriptor");
        }
        let vfd = wrappedvfd.unwrap();

        // Declare statbuf by ourselves 
        let mut libc_statbuf: libc::stat = unsafe { std::mem::zeroed() };
        let libcret = unsafe {
            libc::fstat(vfd.underfd as i32, &mut libc_statbuf)
        };

        libc_statbuf.st_mode as i32
    }
}

// VmmapOps trait provides an interface that can be shared by different virtual memory management implementations, 
// allowing different Vmmap versions to share the same interface.
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

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Vmmap {
    pub entries: NoditMap<u32, Interval<u32>, VmmapEntry>, // Keyed by `page_num`
    pub cached_entry: Option<VmmapEntry>,                  // TODO: is this still needed?
                                                           // Use Option for safety
    pub base_address: Option<i64>,                         // wasm base address. None means uninitialized yet

}

#[allow(dead_code)]
impl Vmmap {
    pub fn new() -> Self {
        Vmmap {
            entries: NoditMap::new(),
            cached_entry: None,
            base_address: None
        }
    }

    // Method to round page number up to the nearest multiple of pages_per_map
    fn round_page_num_up_to_map_multiple(&self, npages: u32, pages_per_map: u32) -> u32 {
        (npages + pages_per_map - 1) & !(pages_per_map - 1)
    }

    // Method to truncate page number down to the nearest multiple of pages_per_map
    fn trunc_page_num_down_to_map_multiple(&self, npages: u32, pages_per_map: u32) -> u32 {
        npages & !(pages_per_map - 1)
    }

    pub fn set_base_address(&mut self, base_address: i64) {
        self.base_address = Some(base_address);
    }

    pub fn user_to_sys(&self, address: i32) -> i64 {
        address as i64 + self.base_address.unwrap()
    }

    pub fn sys_to_user(&self, address: i64) -> i32 {
        (address as i64 - self.base_address.unwrap()) as i32
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
    fn add_entry(&mut self, vmmap_entry_ref: VmmapEntry) {
        let _ = self.entries.insert_strict(
            // pages x to y, y included
            ie(
                vmmap_entry_ref.page_num,
                vmmap_entry_ref.page_num + vmmap_entry_ref.npages,
            ),
            vmmap_entry_ref,
        );
    }

    // Method to add an entry with override, using update method
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
        self.update(
            page_num,
            npages,
            prot,
            maxprot,
            flags,
            backing,
            false,
            file_offset,
            file_size,
            cage_id,
        )
    }

    /// This function will not return any errors pertaining to the page number not mapping
    /// to any existing pages, as the remove operation is done on a best efforts basis:
    /// 1. First an insert overwrite operation with the below page range is performed, causing
    /// a new interval to be created over the provided page range, appropriately partitioning
    /// boundary pages.
    /// 2. This new interval is then deleted, leaving the underlying range unmapped
    fn remove_entry(&mut self, page_num: u32, npages: u32) -> Result<(), io::Error> {
        self.update(
            page_num,
            npages,
            0,
            0,
            0,
            MemoryBackingType::None,
            true,
            0,
            0,
            0,
        )
    }

    // Method to update a memory map entry, handling insertion and removal
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
        if npages == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Number of pages cannot be zero",
            ));
        }

        let new_region_end_page = page_num + npages;
        let new_region_start_page = page_num; // just for ease of understanding

        // Insert the new entry if not marked for removal
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
        let _ = self
            .entries
            .insert_overwrite(ie(new_region_start_page, new_region_end_page), new_entry);

        if remove {
            // strange way to do this, but this is the best using the library we have at hand
            // while also maintaining the shrunk down entries
            // using remove first, then insert will cause us to lose existing entries
            let _ = self
                .entries
                .remove_overlapping(ie(new_region_start_page, new_region_end_page));
        }

        Ok(())
    }

    // Method to change protection of a memory region
    // Modifies protection for existing pages in the region
    // Should be able to handle splitting of existing pages when necessary
    // Should maintain mapping consistency while changing protections
    fn change_prot(&mut self, page_num: u32, npages: u32, new_prot: i32) {
        let new_region_end_page = page_num + npages;
        let new_region_start_page = page_num;

        let mut to_insert = Vec::new();

        for (overlap_interval, entry) in self
            .entries
            .overlapping_mut(ie(new_region_start_page, new_region_end_page))
        {
            let mut ent_start = overlap_interval.start();
            let ent_end = overlap_interval.end();

            if ent_start < new_region_start_page && ent_end > new_region_start_page {
                to_insert.push(ie(new_region_start_page, ent_end));
                ent_start = new_region_start_page; // need to update incase next condition is true
            }
            if ent_start < new_region_end_page && ent_end > new_region_end_page {
                to_insert.push(ie(ent_start, new_region_end_page));
            } else {
                entry.prot = new_prot;
            }
        }

        for interval in to_insert {
            let mut interval_val = self.entries.get_at_point(interval.start()).unwrap().clone();
            interval_val.prot = new_prot;
            let _ = self.entries.insert_overwrite(interval, interval_val);
        }
    }

    // Method to check if a mapping exists
    fn check_existing_mapping(&self, page_num: u32, npages: u32, prot: i32) -> bool {
        let region_end_page = page_num + npages;
        let region_interval = ie(page_num, region_end_page);

        // If no overlap, return false
        if !self.entries.overlaps(region_interval) {
            return false;
        }

        let mut current_page = page_num;

        // Iterate over overlapping intervals
        for (_interval, entry) in self.entries.overlapping(region_interval) {
            let ent_end_page = entry.page_num + entry.npages;
            let flags = entry.maxprot;

            // Case 1: Fully inside the existing entry
            if entry.page_num <= current_page && region_end_page <= ent_end_page {
                return (prot & !flags) == 0;
            }

            // Case 2: Overlaps with the current entry
            if entry.page_num <= current_page && current_page < ent_end_page {
                if (prot & !flags) != 0 {
                    return false;
                }
                current_page = ent_end_page; // Move to the next region
            }

            // Case 3: If there's a gap (no backing store), return false
            if current_page < entry.page_num {
                return false;
            }
        }

        false
    }

    // Method to check address mapping, using cached entry if possible
    fn check_addr_mapping(&mut self, page_num: u32, npages: u32, prot: i32) -> Option<u32> {
        let region_end_page = page_num + npages;

        // First, check if the cached entry can be used
        if let Some(ref cached_entry) = self.cached_entry {
            let ent_end_page = cached_entry.page_num + cached_entry.npages;
            let mut flags = cached_entry.prot;

            // If the protection is not PROT_NONE, enforce PROT_READ
            if flags & (PROT_EXEC | PROT_READ | PROT_WRITE) != PROT_NONE {
                flags |= PROT_READ;
            }

            if cached_entry.page_num <= page_num && region_end_page <= ent_end_page {
                if prot & !flags == 0 {
                    return Some(ent_end_page); // Mapping found inside the cached entry
                }
            }
        }

        // If no cached entry, check the overlapping regions in memory map
        let mut current_page = page_num;
        for (_, entry) in self.entries.overlapping(ie(page_num, region_end_page)) {
            let ent_end_page = entry.page_num + entry.npages;
            let mut flags = entry.prot;

            // If the protection is not PROT_NONE, enforce PROT_READ
            if flags & (PROT_EXEC | PROT_READ | PROT_WRITE) != PROT_NONE {
                flags |= PROT_READ;
            }

            if entry.page_num <= current_page && region_end_page <= ent_end_page {
                // Mapping is fully inside the current entry
                self.cached_entry = Some(entry.clone()); // Cache the entry
                if prot & !flags == 0 {
                    return Some(ent_end_page);
                }
            } else if entry.page_num <= current_page && current_page < ent_end_page {
                // Mapping overlaps with this entry
                if prot & !flags != 0 {
                    return None;
                }
                current_page = ent_end_page; // Move to next region
            } else if current_page < entry.page_num {
                // There's a gap between entries, return failure
                return None;
            }
        }

        // If no valid mapping is found, return None
        None
    }

    //Method to find a page in the memory map
    fn find_page(&self, page_num: u32) -> Option<&VmmapEntry> {
        self.entries.get_at_point(page_num)
    }
    // Method to find a mutable page in the memory map
    fn find_page_mut(&mut self, page_num: u32) -> Option<&mut VmmapEntry> {
        self.entries.get_at_point_mut(page_num)
    }

    // Method to get the last entry in the memory map
    fn last_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)> {
        self.entries.last_key_value()
    }

    // Method to get the first entry in the memory map
    fn first_entry(&self) -> Option<(&Interval<u32>, &VmmapEntry)> {
        self.entries.first_key_value()
    }

    // Method to iterate over entries in both directions
    fn double_ended_iter(&self) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &VmmapEntry)> {
        self.entries.iter()
    }

    // Method to iterate over mutable entries in both directions
    fn double_ended_iter_mut(
        &mut self,
    ) -> impl DoubleEndedIterator<Item = (&Interval<u32>, &mut VmmapEntry)> {
        self.entries.iter_mut()
    }

    // Method to iterate over pages, starting from a given page number
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

    // Method to iterate over mutable pages, starting from a given page number
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

    // Method to find space in the memory map
    fn find_space(&self, npages: u32) -> Option<Interval<u32>> {
        let start = self.first_entry();
        let end = self.last_entry();

        if start == None || end == None {
            return None;
        } else {
            let start_unwrapped = start.unwrap().0.start();
            let end_unwrapped = end.unwrap().0.end();

            let desired_space = npages + 1; // TODO: check if this is correct

            for gap in self
                .entries
                .gaps_trimmed(ie(start_unwrapped, end_unwrapped))
            {
                if gap.end() - gap.start() >= desired_space {
                    return Some(gap);
                }
            }
        }

        None
    }

    // Method to find space above a hint
    fn find_space_above_hint(&self, npages: u32, hint: u32) -> Option<Interval<u32>> {
        let start = hint;
        let end = self.last_entry();

        if end == None {
            return None;
        } else {
            let end_unwrapped = end.unwrap().0.end();

            let desired_space = npages + 1; // TODO: check if this is correct

            for gap in self.entries.gaps_trimmed(ie(start, end_unwrapped)) {
                if gap.end() - gap.start() >= desired_space {
                    return Some(gap);
                }
            }
        }

        None
    }

    // Method to find map space, rounding page numbers up to the nearest multiple of pages_per_map
    fn find_map_space(&self, num_pages: u32, pages_per_map: u32) -> Option<Interval<u32>> {
        let start = self.first_entry();
        let end = self.last_entry();

        if start == None || end == None {
            return None;
        } else {
            let start_unwrapped = start.unwrap().0.start();
            let end_unwrapped = end.unwrap().0.end();

            let rounded_num_pages =
                self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

            for gap in self
                .entries
                .gaps_trimmed(ie(start_unwrapped, end_unwrapped))
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
        }

        None
    }

    // Method to find map space with a hint, rounding page numbers up to the nearest multiple of pages_per_map
    fn find_map_space_with_hint(
        &self,
        num_pages: u32,
        pages_per_map: u32,
        hint: u32,
    ) -> Option<Interval<u32>> {
        let start = hint;
        let end = self.last_entry();

        if end == None {
            return None;
        } else {
            let end_unwrapped = end.unwrap().0.end();

            let rounded_num_pages =
                self.round_page_num_up_to_map_multiple(num_pages, pages_per_map);

            for gap in self.entries.gaps_trimmed(ie(start, end_unwrapped)) {
                let aligned_start_page =
                    self.trunc_page_num_down_to_map_multiple(gap.start(), pages_per_map);
                let aligned_end_page =
                    self.round_page_num_up_to_map_multiple(gap.end(), pages_per_map);

                let gap_size = aligned_end_page - aligned_start_page;
                if gap_size >= rounded_num_pages {
                    return Some(ie(aligned_end_page - rounded_num_pages, aligned_end_page));
                }
            }
        }

        None
    }
}

