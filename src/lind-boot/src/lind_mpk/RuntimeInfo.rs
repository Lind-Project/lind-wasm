use libc::{c_void, pid_t};
use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use cage::{get_cage, MemoryBackingType, RuntimeInfo, RwLock, VmmapOps};
use sysdefs::constants::fs_const::{PAGESHIFT, PROT_NONE, PROT_READ, PROT_WRITE};

const MPK_GRATE_STACK_SIZE: usize = 8 * 1024 * 1024;
const MPK_GRATE_STACK_GUARD: usize = 4096;

fn align_up_to_pages(value: usize) -> usize {
    let page_size = 1usize << PAGESHIFT;
    (value + page_size - 1) & !(page_size - 1)
}

/// Allocates a guard-page + usable stack region out of `cage_id`'s own vmmap.
/// The cage's backing memory is already reserved via its 4 GB MAP_NORESERVE
/// region, so only the vmmap bookkeeping and the guard-page protection need
/// to be set up here. Returns `(guard_start_sys_addr, stack_top_sys_addr)`.
pub fn allocate_stack_in_vmmap(
    cage_id: u64,
    guard_size: usize,
    usable_size: usize,
) -> anyhow::Result<(usize, usize)> {
    let allocation_size = guard_size + usable_size;
    let npages = allocation_size >> PAGESHIFT;
    let guard_pages = guard_size >> PAGESHIFT;

    let cage = get_cage(cage_id).ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    let mut vmmap = cage.vmmap.write();

    let space = vmmap
        .find_map_space(npages, 1)
        .ok_or_else(|| anyhow::anyhow!("no space in cage {} vmmap for stack", cage_id))?;

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
        cage_id,
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
        cage_id,
    )?;

    let guard_start_sys = vmmap.page_num_to_sys(guard_start);
    drop(vmmap);

    if unsafe { libc::mprotect(guard_start_sys as *mut c_void, guard_size, libc::PROT_NONE) } != 0 {
        anyhow::bail!(
            "mprotect for stack guard failed: {}",
            std::io::Error::last_os_error()
        );
    }

    Ok((guard_start_sys, guard_start_sys + allocation_size))
}

/// Releases a stack region previously allocated by `allocate_stack_in_vmmap`,
/// removing its vmmap bookkeeping and restoring the guard page to read/write.
pub fn free_stack_in_vmmap(cage_id: u64, guard_start_sys: usize, allocation_size: usize, guard_size: usize) {
    if let Some(cage) = get_cage(cage_id) {
        let mut vmmap = cage.vmmap.write();
        let start_page = vmmap.sys_to_page_num(guard_start_sys);
        let npages = allocation_size >> PAGESHIFT;
        let _ = vmmap.remove_entry(start_page, npages);
    }
    unsafe {
        libc::mprotect(
            guard_start_sys as *mut c_void,
            guard_size,
            libc::PROT_READ | libc::PROT_WRITE,
        );
    }
}

/// Allocates a writable region out of `cage_id`'s vmmap and returns
/// `(region_base_sys_addr, region_size)`.
fn allocate_region_in_vmmap(cage_id: u64, size: usize) -> anyhow::Result<(usize, usize)> {
    if size == 0 {
        anyhow::bail!("cannot allocate zero-sized vmmap region");
    }

    let allocation_size = align_up_to_pages(size);
    let npages = allocation_size >> PAGESHIFT;
    let cage = get_cage(cage_id).ok_or_else(|| anyhow::anyhow!("cage {} not found", cage_id))?;
    let mut vmmap = cage.vmmap.write();

    let space = vmmap
        .find_map_space(npages, 1)
        .ok_or_else(|| anyhow::anyhow!("no space in cage {} vmmap for region", cage_id))?;
    let start_page = space.start();
    vmmap.add_entry_with_overwrite(
        start_page,
        npages,
        PROT_READ | PROT_WRITE,
        PROT_READ | PROT_WRITE,
        0,
        MemoryBackingType::Anonymous,
        0,
        0,
        cage_id,
    )?;
    let region_base = vmmap.page_num_to_sys(start_page);
    drop(vmmap);

    unsafe {
        std::ptr::write_bytes(region_base as *mut u8, 0, allocation_size);
    }

    Ok((region_base, allocation_size))
}

/// Releases a region previously allocated by `allocate_region_in_vmmap`.
fn free_region_in_vmmap(cage_id: u64, base_sys: usize, allocation_size: usize) {
    if base_sys == 0 || allocation_size == 0 {
        return;
    }
    if let Some(cage) = get_cage(cage_id) {
        let mut vmmap = cage.vmmap.write();
        let start_page = vmmap.sys_to_page_num(base_sys);
        let npages = allocation_size >> PAGESHIFT;
        let _ = vmmap.remove_entry(start_page, npages);
    }
}

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

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct LindAllocateTlsResult {
    pub stackaddr: *mut c_void,
    pub fs_base: *mut c_void,
    pub stacktop: *mut c_void,
    pub dtv: *mut c_void,
    pub dtv_size: usize,
}

//int _lind_dl_allocate_tls (struct lind_dl_tls_result *result)
pub type DlAllocateTlsF = unsafe extern "C" fn(result: *mut LindAllocateTlsResult) -> libc::c_int;

/// Maximum number of nested MPK contexts in a thread's GS segment.
pub const LIND_MPK_MAX_CONTEXTS: usize = 16;

/// A saved MPK execution context.
/// NOTE: The field offsets are used in assembly. Changes here need to be reflected in the corresponding assembly code.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct MPKSupervisorContext {
    pub rsp: u64,
    pub fs_base: usize,
    pub cage_id: u64, // 0 for supervisor
    pub pkru: u32,
}


/// GS segment data layout for the syscall interposition assembly.
///
/// The GS base points at this struct:
///   - gs:0  — current context index
///   - gs:8  — OS thread ID
///   - gs:16 — fixed-capacity saved-context array
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


/// Per-thread supervisor stack state for MPK syscall interposition.
///
/// Each in-process thread needs its own `MPKSupervisorCtxStack` and supervisor stack.
/// Instances are stored in `MPKRuntimeInfo::threads`, keyed by OS thread ID
/// (`gettid()`), and are freed when removed from that map or when the cage exits.
#[derive(Debug)]
pub struct MpkThreadInfo {
    /// GS segment data for this thread, stored in the mapped context page and
    /// installed via arch_prctl(ARCH_SET_GS, ...).
    /// The GS data is stable accross all cages for this thread and only accessible to 
    /// the supervisor.
    pub gs_data: *mut MPKSupervisorCtxStack,
    /// Mapping containing the GS segment data.
    pub context_pages: usize,
    pub context_pages_size: usize,
    /// Base address of the mmap'd supervisor stack region (includes guard page).
    /// Stored as `usize` rather than a raw pointer so that `MpkThreadInfo` is `Send`.
    pub supervisor_stack_base: usize,
    /// Total allocation size of the supervisor stack region in bytes (guard + usable).
    pub supervisor_stack_size: usize,
    /// Cage IDs whose thread maps contain an MpkCageThreadInfo for this OS thread.
    pub cage_ids: Mutex<HashSet<u64>>,
    /// Host-thread stack pointer captured near thread start, used as a safe stack
    /// to run exit cleanup without touching the supervisor stack allocation.
    pub exit_stack: AtomicUsize,
    /// Pointer to child_tid in guest memory to atomically clear and futex wake on thread exit.
    pub child_tid: AtomicU64,
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
        // The context page is unmapped after dropping its initialized GS data.
    }
}

// SAFETY: MpkThreadInfo is owned by MPKRuntimeInfo::threads, which is protected
// by a RwLock. Its pointers refer to mapped context/stack regions and are not
// accessed through Rust references concurrently.
unsafe impl Send for MpkThreadInfo {}
unsafe impl Sync for MpkThreadInfo {}

// ── Deferred cleanup for still-running supervisor stacks ────────────────────
//
// A cage's exit handler runs on its own OS thread's supervisor stack. If that
// call drops the last `Arc<MpkThreadInfo>` synchronously, `MpkThreadInfo::drop`
// munmaps the very stack the code is executing on. Instead, the last reference
// is queued here and only dropped by `run_thread_info_reaper` once the OS
// thread has actually terminated.
static PENDING_THREAD_REAP: OnceLock<Mutex<Vec<(pid_t, Arc<MpkThreadInfo>)>>> = OnceLock::new();

fn pending_thread_reap() -> &'static Mutex<Vec<(pid_t, Arc<MpkThreadInfo>)>> {
    PENDING_THREAD_REAP.get_or_init(|| Mutex::new(Vec::new()))
}

/// Queues `thread_info` to be dropped once OS thread `tid` has exited.
pub fn queue_thread_info_for_reap(tid: pid_t, thread_info: Arc<MpkThreadInfo>) {
    pending_thread_reap().lock().unwrap().push((tid, thread_info));
}

/// Returns whether OS thread `tid` is still alive in this process.
fn os_thread_is_alive(tid: pid_t) -> bool {
    let ret = unsafe { libc::syscall(libc::SYS_tgkill, libc::getpid(), tid, 0) };
    ret == 0
}

/// Periodically drops queued `MpkThreadInfo`s whose OS thread has exited,
/// until `stop_thread_info_reaper` is called. Spawned once from `init_mpk`.
pub fn run_thread_info_reaper() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(20));
        pending_thread_reap()
            .lock()
            .unwrap()
            .retain(|(tid, _)| os_thread_is_alive(*tid));
    }
}


#[derive(Debug)]
pub struct MpkCageThreadInfo {
    pub thread_info: Arc<MpkThreadInfo>,
    /// The grate whose vmmap backs this thread's stack.
    pub grate_cage_id: u64,
    ///This is the stack the thread will use inside the grate.
    pub stack_addr: AtomicUsize,
    pub stack_base: usize,
    pub stack_size: usize,
    pub fs_base: usize,
    pub dtv: *mut c_void,
    pub dtv_size: usize,
}

impl MpkCageThreadInfo {
    const DTV_BUFFER_SIZE: usize = 4 * 1024;

    /// Allocates this thread's grate stack out of the grate's own vmmap
    /// (`grate_cage_id`), rather than an independent mmap. The grate's
    /// backing memory is already reserved via its 4 GB MAP_NORESERVE region,
    /// so only the vmmap bookkeeping and the guard-page protection need to
    /// be set up here.
    pub fn allocate_grate_stack(&mut self) -> anyhow::Result<()> {
        if self.stack_addr.load(Ordering::Relaxed) != 0 {
            return Ok(());
        }

        let (stack_base, stack_addr) = allocate_stack_in_vmmap(
            self.grate_cage_id,
            MPK_GRATE_STACK_GUARD,
            MPK_GRATE_STACK_SIZE,
        )?;

        self.stack_base = stack_base;
        self.stack_size = MPK_GRATE_STACK_GUARD + MPK_GRATE_STACK_SIZE;
        self.stack_addr.store(stack_addr, Ordering::Relaxed);
        Ok(())
    }

    pub fn initialize_grate_tls(&mut self, dl_allocate_tls: DlAllocateTlsF) -> anyhow::Result<()> {
        let stack_addr = self.stack_addr.load(Ordering::Relaxed);
        if stack_addr == 0 {
            anyhow::bail!("grate stack is not initialized");
        }
        if self.dtv.is_null() {
            let (dtv_base, dtv_size) = allocate_region_in_vmmap(self.grate_cage_id, Self::DTV_BUFFER_SIZE)?;
            self.dtv = dtv_base as *mut c_void;
            self.dtv_size = dtv_size;
        }

        let mut tls_result = LindAllocateTlsResult {
            stackaddr: stack_addr as *mut c_void,
            fs_base: std::ptr::null_mut(),
            stacktop: std::ptr::null_mut(),
            dtv: self.dtv,
            dtv_size: self.dtv_size,
        };
        let ret = unsafe { dl_allocate_tls(&mut tls_result as *mut LindAllocateTlsResult) };
        if ret != 0 {
            anyhow::bail!(
                "_lind_dl_allocate_tls failed: {}",
                std::io::Error::last_os_error()
            );
        }
        if tls_result.fs_base.is_null() || tls_result.stacktop.is_null() {
            anyhow::bail!("_lind_dl_allocate_tls returned null fs_base or stacktop");
        }

        self.fs_base = tls_result.fs_base as usize;
        self.stack_addr
            .store(tls_result.stacktop as usize, Ordering::Relaxed);
        Ok(())
    }
}

impl Drop for MpkCageThreadInfo {
    fn drop(&mut self) {
        // The stack lives inside the grate's vmmap/backing memory, which is
        // owned and torn down by the grate itself; only release the vmmap
        // bookkeeping and restore the guard page's protection here.
        if self.stack_base != 0 && self.stack_size != 0 {
            free_stack_in_vmmap(
                self.grate_cage_id,
                self.stack_base,
                self.stack_size,
                MPK_GRATE_STACK_GUARD,
            );
        }
        if !self.dtv.is_null() {
            free_region_in_vmmap(self.grate_cage_id, self.dtv as usize, self.dtv_size);
            self.dtv = std::ptr::null_mut();
            self.dtv_size = 0;
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
    /// Function pointer to _lind_dl_allocate_tls in custom libc, needed for thread-local storage allocation.
    pub dl_allocate_tls: DlAllocateTlsF,
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
    /// so its supervisor stack and GS context data are tracked and freed on exit.
    /// Empty for forked child cages (their resources live in their own address space).
    pub threads: RwLock<HashMap<pid_t, MpkCageThreadInfo>>,
    /// MPK cages do not have a Wasmtime epoch, so lifecycle transitions are
    /// kept explicitly and observed at MPK entry points.
    pub stopped: AtomicBool,
    pub killed: AtomicBool,
    pub state_lock: Mutex<()>,
    pub state_cv: Condvar,
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
        dl_allocate_tls: DlAllocateTlsF,
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
            dl_allocate_tls,
            pid,
            memory_base,
            memory_size,
            next_thread_id: RwLock::new(next_thread_id),
            threads: RwLock::new(thread_map),
            stopped: AtomicBool::new(false),
            killed: AtomicBool::new(false),
            state_lock: Mutex::new(()),
            state_cv: Condvar::new(),
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