use libc::{c_void, pid_t};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use cage::{get_cage, MemoryBackingType, RuntimeInfo, RwLock, VmmapOps};
use sysdefs::constants::fs_const::{PAGESHIFT, PROT_NONE, PROT_READ, PROT_WRITE};

const MPK_GRATE_STACK_SIZE: usize = 8 * 1024 * 1024;
const MPK_GRATE_STACK_GUARD: usize = 4096;

/// Type alias for the __enable_syscall_interpose function pointer.
/// This function is provided by the custom glibc loaded in the dlmopen
/// namespace and is used to register syscall interposition handlers.
pub type EnableInterposeF = unsafe extern "C" fn(
    handler: Option<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64, i32, i64, u64) -> i64>,
    make_threei_call: Option<extern "C" fn(
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
        u64, u64, u64, u64, 
    ) -> i64>,
) -> libc::c_int;

/// Maximum number of nested MPK contexts in a thread's GS segment.
pub const LIND_MPK_MAX_CONTEXTS: usize = 16;

/// A saved MPK execution context.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MPKSupervisorContext {
    pub super_rsp: u64,
    pub cage_id: u64,
    pub pkru: u32,
}


#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MPKCageContext {
    pub rsp: u64,
}

/// GS segment data layout for the syscall interposition assembly.
///
/// This matches `lind_mpk_context_stack_t` in `syscall-interpose.h`:
///   - gs:0  — pointer to the active MPK context
///   - gs:8  — supervisor stack top
///   - gs:16 — OS thread ID
///   - gs:24 — fixed-capacity saved-context stack
///
/// The GS base register is pointed at this struct via arch_prctl(ARCH_SET_GS, ...).
/// Each thread must have its own instance because its active context is written on
/// every intercepted syscall.
#[repr(C)]
#[derive(Debug)]
pub struct MPKSupervisorCtxStack {
    pub current_context: usize,
    pub os_tid: u64,
    pub contexts: [MPKSupervisorContext; LIND_MPK_MAX_CONTEXTS],
}

#[repr(C)]
#[derive(Debug)]
pub struct MPKCageCtxStack {
    pub current_context: usize,
    pub contexts: [MPKCageContext; LIND_MPK_MAX_CONTEXTS],
}

/// Per-thread supervisor stack state for MPK syscall interposition.
///
/// Each in-process thread needs its own `GsSegmentData` and supervisor stack.
/// Instances are stored in `MPKRuntimeInfo::threads`, keyed by OS thread ID
/// (`gettid()`), and are freed when removed from that map or when the cage exits.
#[derive(Debug)]
pub struct MpkThreadInfo {
    /// GS segment data for this thread.  The Box gives a stable heap address;
    /// GS is set to point here via arch_prctl(ARCH_SET_GS, ...).
    /// The GS data is stable accross all cages for this thread and only accessible to 
    /// the supervisor.
    pub gs_data: *mut MPKSupervisorCtxStack,
    /// Cage context data stored at the start of the second context page.
    pub cage_data: *mut MPKCageCtxStack,
    /// Mapping containing `gs_data` and `cage_data`, one page each.
    pub context_pages: usize,
    pub context_pages_size: usize,
    /// Base address of the mmap'd supervisor stack region (includes guard page).
    /// Stored as `usize` rather than a raw pointer so that `MpkThreadInfo` is `Send`.
    pub supervisor_stack_base: usize,
    /// Total allocation size of the supervisor stack region in bytes (guard + usable).
    pub supervisor_stack_size: usize,
    /// Cage IDs whose thread maps contain an MpkCageThreadInfo for this OS thread.
    pub cage_ids: Mutex<HashSet<u64>>,
}

impl MpkThreadInfo {
    pub fn register_cage(&self, cage_id: u64) {
        self.cage_ids.lock().unwrap().insert(cage_id);
    }
}


impl Drop for MpkThreadInfo {
    fn drop(&mut self) {
        unsafe {
            if !self.gs_data.is_null() {
                std::ptr::drop_in_place(self.gs_data);
            }
            if !self.cage_data.is_null() {
                std::ptr::drop_in_place(self.cage_data);
            }
        }
        if self.context_pages != 0 && self.context_pages_size != 0 {
            unsafe {
                libc::munmap(self.context_pages as *mut c_void, self.context_pages_size);
            }
        }
        if self.supervisor_stack_base != 0 && self.supervisor_stack_size != 0 {
            unsafe {
                libc::munmap(
                    self.supervisor_stack_base as *mut c_void,
                    self.supervisor_stack_size,
                );
            }
        }
        // gs_data (Box) is freed automatically.
    }
}

// SAFETY: MpkThreadInfo is owned by MPKRuntimeInfo::threads, which is protected
// by a RwLock. supervisor_stack_base is a raw address stored as usize and does not
// alias any Rust reference. gs_data is a Box<GsSegmentData> (plain u64 fields).
unsafe impl Send for MpkThreadInfo {}
unsafe impl Sync for MpkThreadInfo {}


#[derive(Debug)]
pub struct MpkCageThreadInfo {
    pub thread_info: Arc<MpkThreadInfo>,
    /// The grate whose vmmap backs this thread's stack.
    pub grate_cage_id: u64,
    ///This is the stack the thread will use inside the grate.
    pub stack_addr: usize,
    pub stack_base: usize,
    pub stack_size: usize,
}

impl MpkCageThreadInfo {
    /// Allocates this thread's grate stack out of the grate's own vmmap
    /// (`grate_cage_id`), rather than an independent mmap. The grate's
    /// backing memory is already reserved via its 4 GB MAP_NORESERVE region,
    /// so only the vmmap bookkeeping and the guard-page protection need to
    /// be set up here.
    pub fn allocate_grate_stack(&mut self) -> anyhow::Result<()> {
        if self.stack_addr != 0 {
            return Ok(());
        }

        let allocation_size = MPK_GRATE_STACK_GUARD + MPK_GRATE_STACK_SIZE;
        let npages = allocation_size >> PAGESHIFT;
        let guard_pages = MPK_GRATE_STACK_GUARD >> PAGESHIFT;

        let grate = get_cage(self.grate_cage_id)
            .ok_or_else(|| anyhow::anyhow!("grate cage {} not found", self.grate_cage_id))?;
        let mut vmmap = grate.vmmap.write();

        let space = vmmap.find_map_space(npages, 1).ok_or_else(|| {
            anyhow::anyhow!(
                "no space in grate {} vmmap for grate stack",
                self.grate_cage_id
            )
        })?;

        let guard_start = space.start();
        let stack_start = guard_start + guard_pages;

        // Guard page: reserved in the vmmap, no access.
        vmmap.add_entry_with_overwrite(
            guard_start,
            guard_pages,
            PROT_NONE,
            PROT_NONE,
            0,
            MemoryBackingType::Anonymous,
            0,
            0,
            self.grate_cage_id,
        )?;

        // Usable stack region.
        vmmap.add_entry_with_overwrite(
            stack_start,
            npages - guard_pages,
            PROT_READ | PROT_WRITE,
            PROT_READ | PROT_WRITE,
            0,
            MemoryBackingType::Anonymous,
            0,
            0,
            self.grate_cage_id,
        )?;

        let stack_base = vmmap.page_num_to_sys(guard_start);
        drop(vmmap);

        if unsafe { libc::mprotect(stack_base as *mut c_void, MPK_GRATE_STACK_GUARD, libc::PROT_NONE) } != 0 {
            anyhow::bail!(
                "mprotect for MPK grate stack guard failed: {}",
                std::io::Error::last_os_error()
            );
        }

        self.stack_base = stack_base;
        self.stack_size = allocation_size;
        self.stack_addr = self.stack_base + self.stack_size;
        Ok(())
    }
}

impl Drop for MpkCageThreadInfo {
    fn drop(&mut self) {
        // The stack lives inside the grate's vmmap/backing memory, which is
        // owned and torn down by the grate itself; only release the vmmap
        // bookkeeping and restore the guard page's protection here.
        if self.stack_base != 0 && self.stack_size != 0 {
            if let Some(grate) = get_cage(self.grate_cage_id) {
                let mut vmmap = grate.vmmap.write();
                let start_page = vmmap.sys_to_page_num(self.stack_base);
                let npages = self.stack_size >> PAGESHIFT;
                let _ = vmmap.remove_entry(start_page, npages);
            }
            unsafe {
                libc::mprotect(
                    self.stack_base as *mut c_void,
                    MPK_GRATE_STACK_GUARD,
                    libc::PROT_READ | libc::PROT_WRITE,
                );
            }
        }
    }
}

/// Runtime-specific information for MPK (Memory Protection Keys) based cages.
///
/// This structure stores the dlmopen handles and function pointers needed
/// to manage isolated native .so execution. Fields are set once during
/// execute_mpk initialization and then only read (e.g., during fork/clone).
/// The Cage's RwLock<Box<dyn RuntimeInfo>> provides synchronization.
#[derive(Debug)]
pub struct MPKRuntimeInfo {
    /// Handle to the guest .so loaded via dlmopen
    pub loader_cage_handle: *mut c_void,
    /// Handle to the custom libc loaded in the isolated namespace
    pub loader_libc_handle: *mut c_void,
    /// Function pointer to __enable_syscall_interpose in custom libc
    pub enable_interpose_fn: EnableInterposeF,
    /// OS-level process ID of this cage's process.
    /// 0 if the cage runs in the main lind process; non-zero for forked children.
    pub pid: libc::pid_t,
    /// Base address of the 4 GB MAP_NORESERVE region backing this cage's vmmap.
    /// Null if no mapping has been established yet.
    pub memory_base: *mut c_void,
    /// Size (in bytes) of the memory region at memory_base.
    pub memory_size: usize,
    /// Next thread ID to assign for threads created in this cage.
    pub next_thread_id: RwLock<i32>,
    /// Per-thread supervisor stack state, keyed by OS thread ID (gettid()).
    /// Each in-process thread that runs guest code registers its MpkThreadInfo here
    /// so its supervisor stack and GsSegmentData are tracked and freed on exit.
    /// Empty for forked child cages (their resources live in their own address space).
    pub threads: RwLock<HashMap<pid_t, MpkCageThreadInfo>>,
}

impl MPKRuntimeInfo {
    /// Creates a new MPKRuntimeInfo.
    ///
    /// `initial_thread` is the `MpkThreadInfo` for the thread calling this constructor.
    /// Pass `Some(thread_info)` for in-process cages (execute_mpk / exec_mpk_internal);
    /// pass `None` for the parent-side record of a forked child cage (the child's thread
    /// resources live in the child's address space and are managed there).
    pub fn new(
        cage_handle: *mut c_void,
        libc_handle: *mut c_void,
        enable_interpose: EnableInterposeF,
        pid: libc::pid_t,
        memory_base: *mut c_void,
        memory_size: usize,
        next_thread_id: i32,
        initial_thread: Option<(pid_t, MpkCageThreadInfo)>,
    ) -> Self {
        let mut thread_map = HashMap::new();
        if let Some((tid, info)) = initial_thread {
            thread_map.insert(tid, info);
        }
        MPKRuntimeInfo {
            loader_cage_handle: cage_handle,
            loader_libc_handle: libc_handle,
            enable_interpose_fn: enable_interpose,
            pid,
            memory_base,
            memory_size,
            next_thread_id: RwLock::new(next_thread_id),
            threads: RwLock::new(thread_map),
        }
    }
}

impl RuntimeInfo for MPKRuntimeInfo {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Safety: Raw pointers in MPKRuntimeInfo point to dlmopen handles and mmap'd regions
// that remain valid for the cage's lifetime. Access is synchronized through the Cage's
// RwLock<Box<dyn RuntimeInfo>>, ensuring no data races. The handles are opaque library
// objects and the function pointer is extern "C" and statically determined, making them
// safe to share. The threads map is protected by its own RwLock.
unsafe impl Send for MPKRuntimeInfo {}
unsafe impl Sync for MPKRuntimeInfo {}