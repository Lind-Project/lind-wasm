//! Lind COW (copy-on-write) fork memory backing.
//!
//! Implements the shared-backing-object + userfaultfd write-protect design
//! from `/cow-design.md`, scoped to `/cow-implementation-plan.md` Milestone 1:
//! `fork_vmmap`'s `MAP_PRIVATE` branch shares memory via a zero-copy
//! `MAP_SHARED` mapping of a Lind-owned page-store slot instead of an eager
//! `process_vm_writev` copy, splitting into a private copy only on the first
//! write to a still-shared range (detected via `userfaultfd` write-protect).
//!
//! # Scope of this implementation (Milestone 1)
//!
//! - Single-threaded cages, no intervening guest `mmap`/`munmap`/`mprotect`/
//!   `brk` between fork and first write. `cow-design.md` §9.2's
//!   vmmap-mutating-syscall guard is Milestone 3 work and is NOT implemented
//!   here; only §9.1's direct-write/UFFD-WP-fault trigger is.
//! - **Whole-vmmap-entry granularity, not per-4KB-page.** A write anywhere
//!   in a shared entry's range materializes the *entire* entry, not just the
//!   touched page. This is a deliberate simplification explicitly allowed by
//!   `cow-design.md` §18 ("start coarse for implementation simplicity") --
//!   it keeps materialization from ever needing to split a `VmmapEntry`
//!   (mirroring the entry's range 1:1 with a single `BackingId`), at the
//!   cost of not showing a benchmark win for small partial writes to a large
//!   entry until page/extent-level granularity is added later (tracked as
//!   follow-up work, see `cow-design.md` §20 / plan Phase 8). Correctness is
//!   unaffected by this choice -- see the reasoning in `materialize_unique`.
//! - No `exec()`/exit() refcount teardown yet (plan Milestone 3 item 3.4) --
//!   backing slots are never freed in this build. Fine for the fork-scoped
//!   correctness/benchmark scenarios this milestone targets; would leak
//!   page-store space in a long-running multi-fork process.

use crate::cage::get_cage;
use crate::memory::vmmap::{MemoryBackingType, Vmmap, VmmapOps};
use nodit::{interval::ie, Interval, NoditMap};
use parking_lot::Mutex;
use std::ffi::c_void;
use std::fs::File;
use std::os::unix::fs::FileExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, OnceLock};
use sysdefs::constants::fs_const::{MAP_FIXED, MAP_SHARED, PAGESIZE};
use sysdefs::lind_log;
use userfaultfd::{Event, FaultKind, FeatureFlags, RegisterMode, Uffd, UffdBuilder};

/// Identifies one backing slot in the page store.
pub type BackingId = u64;

/// One backing slot: a byte range in the page-store file that some set of
/// cages currently share (refcount > 1), or that exactly one cage now
/// privately owns going forward without needing further COW protection
/// (refcount == 1, `cow-design.md` Invariant 4). Guarded entirely by the
/// `DashMap` shard lock in `CowManager::backings` -- every access to
/// `refcount` goes through a `get`/`get_mut` on that map, never bypassed,
/// which is what makes the check-and-maybe-decrement in
/// `materialize_unique` atomic (`cow-design.md` §9.2 "take a lock...
/// re-read the mapping metadata after acquiring the lock").
struct BackingMeta {
    file_offset: u64,
    len: usize,
    refcount: usize,
}

/// Where a currently write-protected host address range routes to: which
/// cage owns that mapping and which backing it currently points at. Keyed
/// by host address in `CowManager::fault_routes` so the pager thread can
/// resolve a raw UFFD fault address without scanning every cage's vmmap.
#[derive(Clone, Copy)]
struct FaultRoute {
    cageid: u64,
    backing_id: BackingId,
}

/// Page-store backing file. Follows the same create-then-unlink pattern as
/// `cage::memory::shared::ShmFile` -- a shareable-but-anonymous `MAP_SHARED`
/// backing without needing `memfd_create` specifically (`cow-design.md`
/// §3.3 notes this as existing, working precedent in this codebase).
struct PageStore {
    file: File,
    next_offset: AtomicU64,
}

fn round_up_page(len: usize) -> usize {
    let page = PAGESIZE as usize;
    (len + page - 1) / page * page
}

impl PageStore {
    /// Backed by `memfd_create()`, not a create-then-unlink regular file
    /// like `cage::memory::shared::ShmFile` uses for SysV shm. Discovered
    /// empirically (cow-implementation-plan.md Milestone 1 notes):
    /// `UFFDIO_REGISTER` with `WRITE_PROTECT` mode fails with `EINVAL` on a
    /// `MAP_SHARED` mapping of a plain regular-filesystem-backed file --
    /// Linux's UFFD write-protect support requires shmem-backed memory
    /// (anonymous or `memfd`/tmpfs), not arbitrary filesystem-backed pages.
    /// `memfd_create` gives a shmem-backed anonymous file, which does not
    /// have this restriction.
    fn new() -> std::io::Result<Self> {
        let name = std::ffi::CString::new(format!("cow-pagestore-{}", std::process::id())).unwrap();
        let fd = unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let f = unsafe { File::from_raw_fd(fd) };
        Ok(PageStore {
            file: f,
            next_offset: AtomicU64::new(0),
        })
    }

    fn raw_fd(&self) -> i32 {
        self.file.as_raw_fd()
    }

    /// Bump-allocate `len` bytes (rounded up to page size) in the store
    /// file. Milestone 1 never frees/recycles slots -- refcount-driven
    /// recycling on exec()/exit() is plan Milestone 3 item 3.4
    /// (`cow-design.md` §14).
    fn allocate(&self, len: usize) -> std::io::Result<u64> {
        let aligned = round_up_page(len) as u64;
        let offset = self.next_offset.fetch_add(aligned, Ordering::SeqCst);
        self.file.set_len(offset + aligned)?;
        Ok(offset)
    }

    /// Copy `len` bytes from a live host memory range into the store at
    /// `file_offset`. This is the one remaining real bulk copy in the COW
    /// design -- it happens once per entry, the first time it is shared or
    /// split, not once per fork (`cow-design.md` §7 step 1, §9.4).
    ///
    /// # Safety
    /// `src` must point to at least `len` readable bytes for the duration
    /// of the call. Callers pass a currently-live host mapping (either the
    /// entry being promoted, or a still-shared page being split), which is
    /// always safe to read regardless of its current refcount (the read
    /// path never mutates state, `cow-design.md` §8/Invariant 2).
    unsafe fn write_from(&self, file_offset: u64, src: *const u8, len: usize) -> std::io::Result<()> {
        let bytes = std::slice::from_raw_parts(src, len);
        self.file.write_all_at(bytes, file_offset)
    }
}

pub struct CowManager {
    page_store: PageStore,
    backings: dashmap::DashMap<BackingId, BackingMeta>,
    next_backing_id: AtomicU64,
    uffd: OnceLock<Uffd>,
    enabled: AtomicBool,
    fault_routes: Mutex<NoditMap<usize, Interval<usize>, FaultRoute>>,

    // Milestone 1.7 instrumentation -- see cow-implementation-plan.md
    /// Number of vmmap entries lazily promoted from Anonymous -> Cow (i.e.
    /// the one-time bulk copy into the page store on first share).
    pub promote_count: AtomicU64,
    /// Bytes copied during promotion (the "N bytes" fork used to pay on
    /// every fork; now paid once per entry lineage instead).
    pub promote_bytes: AtomicU64,
    /// Number of entries zero-copy-shared into a child at fork time
    /// (promotions + subsequent-generation shares of already-Cow entries).
    pub share_count: AtomicU64,
    /// Number of write faults that triggered `materialize_unique`.
    pub materialize_count: AtomicU64,
    /// Bytes copied by `materialize_unique` (the write-triggered split
    /// cost, as opposed to promotion's fork-triggered cost).
    pub materialize_bytes: AtomicU64,
    /// Subset of `materialize_count` that hit the fast path (refcount was
    /// already 1, no copy needed -- `cow-design.md` §9.3).
    pub materialize_fast_path_count: AtomicU64,
}

impl CowManager {
    fn new() -> Self {
        let page_store = PageStore::new().expect("cow: failed to create page store backing file");
        let mgr = CowManager {
            page_store,
            backings: dashmap::DashMap::new(),
            next_backing_id: AtomicU64::new(1),
            uffd: OnceLock::new(),
            enabled: AtomicBool::new(false),
            fault_routes: Mutex::new(NoditMap::new()),
            promote_count: AtomicU64::new(0),
            promote_bytes: AtomicU64::new(0),
            share_count: AtomicU64::new(0),
            materialize_count: AtomicU64::new(0),
            materialize_bytes: AtomicU64::new(0),
            materialize_fast_path_count: AtomicU64::new(0),
        };
        mgr.try_enable();
        mgr
    }

    /// `LIND_FORK_MEMORY=eager|cow` fallback switch (`cow-design.md` §23,
    /// plan Milestone 1 step 1.2). Defaults to `eager`. If `cow` is
    /// requested but userfaultfd write-protect isn't available on this
    /// kernel, falls back to `eager` with a log line rather than failing.
    fn try_enable(&self) {
        let requested = std::env::var("LIND_FORK_MEMORY").unwrap_or_else(|_| "eager".to_string());
        if requested != "cow" {
            return;
        }

        match UffdBuilder::new()
            .require_features(FeatureFlags::PAGEFAULT_FLAG_WP)
            .close_on_exec(true)
            .non_blocking(false)
            .create()
        {
            Ok(uffd) => {
                if self.uffd.set(uffd).is_err() {
                    lind_log!(Default, "cow: uffd already initialized, ignoring duplicate init");
                    return;
                }
                self.enabled.store(true, Ordering::SeqCst);
                spawn_pager_thread();
                lind_log!(
                    Default,
                    "cow: LIND_FORK_MEMORY=cow, userfaultfd write-protect available, COW fork enabled"
                );
            }
            Err(e) => {
                lind_log!(
                    Default,
                    "cow: LIND_FORK_MEMORY=cow requested but userfaultfd unavailable ({:?}); falling back to eager-copy fork",
                    e
                );
            }
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::SeqCst)
    }

    fn uffd(&self) -> &Uffd {
        self.uffd.get().expect("cow: uffd accessed while disabled")
    }

    fn register_route(&self, host_start: usize, host_len: usize, cageid: u64, backing_id: BackingId) {
        let mut routes = self.fault_routes.lock();
        let _ = routes.insert_overwrite(
            ie(host_start, host_start + host_len),
            FaultRoute { cageid, backing_id },
        );
    }

    fn unregister_route(&self, host_start: usize, host_len: usize) {
        let mut routes = self.fault_routes.lock();
        let _ = routes.remove_overlapping(ie(host_start, host_start + host_len));
    }

    fn lookup_route(&self, addr: usize) -> Option<(usize, usize, FaultRoute)> {
        let routes = self.fault_routes.lock();
        let result = routes
            .overlapping(ie(addr, addr + 1))
            .next()
            .map(|(interval, route)| (interval.start(), interval.end() - interval.start() + 1, *route));
        result
    }

    /// Share one `vmmap` entry's range into a child at fork time, avoiding
    /// the bulk copy in `fork_vmmap`'s `MAP_PRIVATE` branch
    /// (`cow-design.md` §7). Returns `true` if handled (caller should skip
    /// the eager `process_vm_writev` path for this entry), `false` if COW
    /// is disabled and the caller should fall back to eager copy unchanged.
    ///
    /// `parent_vmmap`/`child_vmmap` are passed as `&mut` because this
    /// mutates each side's `VmmapEntry::backing` field to record the
    /// (possibly newly allocated) `BackingId` -- see the module doc comment
    /// on why this is done at whole-entry granularity.
    #[allow(clippy::too_many_arguments)]
    pub fn fork_share_entry(
        &self,
        parent_vmmap: &mut Vmmap,
        child_vmmap: &mut Vmmap,
        parent_cageid: u64,
        child_cageid: u64,
        page_num: u32,
        npages: u32,
        prot: i32,
        current_backing: MemoryBackingType,
        parent_st: usize,
        child_st: usize,
        addr_len: usize,
    ) -> bool {
        if !self.enabled() {
            return false;
        }
        // Only COW-share entries above a conservative size floor. Small
        // entries (a handful of KB to a few hundred KB) are almost always
        // critical low-level runtime structures -- the wasm data segment,
        // dylink GOT/table copies, and similar -- not real user heap
        // allocations, and empirically (cow-implementation-plan.md
        // Milestone 1 notes) sharing them via the MAP_FIXED remap this
        // function does causes crashes on a cage's *second* fork, not its
        // first (consistent with something in Asyncify/Wasmtime's runtime
        // bookkeeping for these regions not tolerating the mapping-type
        // change). There's no real COW upside to sharing them anyway --
        // the whole point of COW is avoiding the cost of copying *large*
        // regions, and eager-copying a few hundred KB is already cheap.
        // Leaving them on the eager path sidesteps the risk entirely.
        const COW_MIN_ENTRY_BYTES: usize = 1024 * 1024;
        if addr_len < COW_MIN_ENTRY_BYTES {
            return false;
        }
        // The entry starting at page_num 0 is the wasm stack+guard region
        // set up by early_init_stack/the static-build initial mmap (both
        // always start the stack at address 0 in Lind's layout). Fork's
        // Asyncify unwind/rewind writes into this region as an intrinsic
        // part of resuming the child, and empirically (see
        // cow-implementation-plan.md Milestone 1 notes) sharing it via the
        // MAP_FIXED remap this function does breaks that rewind. It also
        // provides no real COW benefit anyway -- a stack diverges between
        // parent and child essentially immediately on any fork, so it would
        // materialize right away regardless. Fall back to the eager copy
        // path for it, same as before this change.
        if page_num == 0 {
            return false;
        }

        let backing_id = match current_backing {
            MemoryBackingType::Cow(id) => {
                // Already shared by an earlier fork in this lineage --
                // no copy needed, just add another reference
                // (cow-design.md §12, the recursive-fork win).
                id
            }
            MemoryBackingType::Anonymous => {
                // First time this entry is ever shared: promote it.
                let offset = match self.page_store.allocate(addr_len) {
                    Ok(o) => o,
                    Err(e) => {
                        lind_log!(Default, "cow: page store allocation failed during promotion: {:?}", e);
                        return false;
                    }
                };
                if let Err(e) = unsafe { self.page_store.write_from(offset, parent_st as *const u8, addr_len) } {
                    lind_log!(Default, "cow: page store copy failed during promotion: {:?}", e);
                    return false;
                }
                self.promote_count.fetch_add(1, Ordering::Relaxed);
                self.promote_bytes.fetch_add(addr_len as u64, Ordering::Relaxed);

                let new_id = self.next_backing_id.fetch_add(1, Ordering::SeqCst);
                self.backings.insert(
                    new_id,
                    BackingMeta {
                        file_offset: offset,
                        len: addr_len,
                        refcount: 1,
                    },
                );

                // Remap the parent's own range onto the new MAP_SHARED
                // backing -- parent transitions from private-anon to
                // Cow-shared, in place, at the same host address
                // (cow-design.md §7.2).
                let ret = unsafe {
                    libc::mmap(
                        parent_st as *mut c_void,
                        addr_len,
                        prot,
                        (MAP_SHARED | MAP_FIXED) as i32,
                        self.page_store.raw_fd(),
                        offset as i64,
                    )
                };
                if ret == libc::MAP_FAILED {
                    lind_log!(Default, "cow: mmap MAP_SHARED failed while promoting parent range");
                    return false;
                }
                set_entry_backing(parent_vmmap, page_num, npages, MemoryBackingType::Cow(new_id));
                new_id
            }
            // SharedMemory/FileDescriptor/None entries never reach here --
            // fork_vmmap only calls this for the MAP_PRIVATE branch.
            _ => return false,
        };

        // Map the same backing into the child, and bump the shared
        // refcount (cow-design.md §7.4).
        let child_ret = unsafe {
            libc::mmap(
                child_st as *mut c_void,
                addr_len,
                prot,
                (MAP_SHARED | MAP_FIXED) as i32,
                self.page_store.raw_fd(),
                self.backings.get(&backing_id).unwrap().file_offset as i64,
            )
        };
        if child_ret == libc::MAP_FAILED {
            lind_log!(Default, "cow: mmap MAP_SHARED failed while sharing into child");
            return false;
        }
        {
            let mut meta = self.backings.get_mut(&backing_id).expect("cow: backing vanished during share");
            meta.refcount += 1;
        }
        set_entry_backing(child_vmmap, page_num, npages, MemoryBackingType::Cow(backing_id));

        // Write-protect both sides (cow-design.md §7.2/§7.4). Idempotent
        // with respect to our own bookkeeping: register_route always
        // overwrites, and we only skip the actual UFFD ioctl if our own
        // index says this exact range is already registered (e.g. the
        // parent side, in the "already Cow" branch, if it was never
        // unprotected since an earlier fork).
        self.ensure_write_protected(parent_st, addr_len, parent_cageid, backing_id);
        self.ensure_write_protected(child_st, addr_len, child_cageid, backing_id);

        self.share_count.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn ensure_write_protected(&self, host_start: usize, host_len: usize, cageid: u64, backing_id: BackingId) {
        let already = self.lookup_route(host_start).is_some();
        if !already {
            if let Err(e) = self
                .uffd()
                .register_with_mode(host_start as *mut c_void, host_len, RegisterMode::WRITE_PROTECT)
            {
                lind_log!(Default, "cow: uffd register failed: {:?}", e);
                return;
            }
        }
        if let Err(e) = self.uffd().write_protect(host_start as *mut c_void, host_len) {
            lind_log!(Default, "cow: uffd write_protect failed: {:?}", e);
            return;
        }
        self.register_route(host_start, host_len, cageid, backing_id);
    }

    /// Handle a UFFD write-protect fault (`cow-design.md` §9.1/§9). Runs on
    /// the dedicated pager thread.
    fn handle_write_fault(&self, addr: usize) {
        let Some((host_start, host_len, route)) = self.lookup_route(addr) else {
            lind_log!(Default, "cow: write fault at {:#x} has no registered route, waking defensively", addr);
            let _ = self.uffd().wake(addr as *mut c_void, PAGESIZE as usize);
            return;
        };
        self.materialize_count.fetch_add(1, Ordering::Relaxed);

        // Atomically decide fast-vs-shared under the backing's own
        // DashMap-shard lock, then release it before doing any I/O
        // (cow-design.md §9.2 "take a lock... re-read the mapping metadata
        // after acquiring the lock"; §16 avoiding holding a lock across
        // syscalls/allocation is a Milestone 3 concern once this needs to
        // be called reentrantly from RawPOSIX handlers -- not applicable
        // here since the pager thread holds no other lock).
        let is_unique = {
            let mut meta = self
                .backings
                .get_mut(&route.backing_id)
                .expect("cow: unknown backing id in fault route");
            if meta.refcount == 1 {
                true
            } else {
                meta.refcount -= 1;
                false
            }
        };

        if is_unique {
            // Fast path (cow-design.md §9.3): this cage's mapping is
            // already the only reference to this backing -- no copy.
            self.materialize_fast_path_count.fetch_add(1, Ordering::Relaxed);
            let _ = self.uffd().remove_write_protection(host_start as *mut c_void, host_len, true);
            self.unregister_route(host_start, host_len);
            return;
        }

        // Shared path (cow-design.md §9.4-9.5): allocate a new slot, copy
        // the current (still valid to read) contents, remap just this
        // cage's range onto it at the same host address.
        let new_offset = match self.page_store.allocate(host_len) {
            Ok(o) => o,
            Err(e) => {
                lind_log!(Default, "cow: page store allocation failed in materialize: {:?}", e);
                let _ = self.uffd().wake(host_start as *mut c_void, host_len);
                return;
            }
        };
        if let Err(e) = unsafe { self.page_store.write_from(new_offset, host_start as *const u8, host_len) } {
            lind_log!(Default, "cow: page store copy failed in materialize: {:?}", e);
            let _ = self.uffd().wake(host_start as *mut c_void, host_len);
            return;
        }
        self.materialize_bytes.fetch_add(host_len as u64, Ordering::Relaxed);

        let new_id = self.next_backing_id.fetch_add(1, Ordering::SeqCst);
        self.backings.insert(
            new_id,
            BackingMeta {
                file_offset: new_offset,
                len: host_len,
                refcount: 1,
            },
        );

        let existing_prot = current_prot_hint(route.cageid, host_start);
        let ret = unsafe {
            libc::mmap(
                host_start as *mut c_void,
                host_len,
                existing_prot,
                (MAP_SHARED | MAP_FIXED) as i32,
                self.page_store.raw_fd(),
                new_offset as i64,
            )
        };
        if ret == libc::MAP_FAILED {
            lind_log!(Default, "cow: mmap MAP_SHARED failed in materialize");
            let _ = self.uffd().wake(host_start as *mut c_void, host_len);
            return;
        }
        update_cage_vmmap_backing(route.cageid, host_start, host_len, MemoryBackingType::Cow(new_id));
        // This range is unique now; drop its UFFD-WP registration. The
        // mmap(MAP_FIXED) above implicitly unmapped/replaced the
        // previously-registered range, which is expected to drop its UFFD
        // registration as a side effect of the unmap -- unregister_route
        // just keeps our own reverse index consistent with that. We still
        // issue an explicit wake below in case the faulting thread's
        // pending fault wasn't already resolved by the implicit unmap.
        self.unregister_route(host_start, host_len);
        let _ = self.uffd().wake(host_start as *mut c_void, host_len);
    }

    /// Milestone 2 exit criterion (cow-implementation-plan.md): confirm
    /// backing-object/refcount bookkeeping stays correct across multiple
    /// fork generations and non-linear fork trees, with no leaked/
    /// never-decremented refcounts. Walks every cage's vmmap and counts how
    /// many `VmmapEntry::backing == Cow(id)` occurrences exist for each
    /// `id`, then compares that live count against `backings[id].refcount`.
    ///
    /// This is an O(cages * entries) scan, not something to run on a hot
    /// path -- it's a diagnostic for tests/benchmarking (see
    /// `dump_stats_if_requested`), not part of normal fork/materialize
    /// operation.
    ///
    /// Note: since Milestone 1 doesn't implement exec()/exit() refcount
    /// teardown yet (plan Milestone 3 item 3.4), an exited cage's vmmap
    /// entries are never cleared, so they still count as "live" references
    /// here -- that's intentional and correct for what this audit checks:
    /// whether fork-time sharing/materialize bookkeeping is internally
    /// consistent, not whether backings get recycled promptly (a separate,
    /// already-documented Milestone 1 limitation).
    pub fn audit(&self) -> CowAuditReport {
        let max_cageid = sysdefs::constants::lind_platform_const::MAX_CAGEID as u64;
        let mut live_counts: std::collections::HashMap<BackingId, usize> = std::collections::HashMap::new();
        for cageid in 0..max_cageid {
            let Some(cage) = get_cage(cageid) else {
                continue;
            };
            let vmmap = cage.vmmap.read();
            for (_interval, entry) in vmmap.entries.iter() {
                if let MemoryBackingType::Cow(id) = entry.backing {
                    *live_counts.entry(id).or_insert(0) += 1;
                }
            }
        }

        let mut mismatches = Vec::new();
        let mut total_recorded_refcount = 0usize;
        for item in self.backings.iter() {
            let id = *item.key();
            let recorded = item.value().refcount;
            let live = live_counts.get(&id).copied().unwrap_or(0);
            total_recorded_refcount += recorded;
            if recorded != live {
                mismatches.push(format!(
                    "backing {}: recorded refcount={} but live vmmap references={}",
                    id, recorded, live
                ));
            }
        }
        for (id, count) in &live_counts {
            if !self.backings.contains_key(id) {
                mismatches.push(format!(
                    "backing {} referenced by {} live vmmap entries but missing from backings table",
                    id, count
                ));
            }
        }

        CowAuditReport {
            total_backings: self.backings.len(),
            total_recorded_refcount,
            total_live_references: live_counts.values().sum(),
            mismatches,
        }
    }

    /// Best-effort refcount release when a cage is finalized.
    ///
    /// Discovered empirically while building the Milestone 2 audit above:
    /// `cage::cage_finalize` removes a cage from `CAGE_MAP` (and drops its
    /// `Cage`/`Vmmap`) as soon as that cage truly finishes running -- not
    /// just at process shutdown, but per-cage, as each one exits. Without
    /// this hook, a `Cow` backing's `refcount` would only ever grow across
    /// repeated forks, never reflecting cages that have already exited,
    /// which both leaks page-store space and (worse) leaves stale
    /// `fault_routes` entries pointing at host address ranges the OS is
    /// free to hand to an unrelated later cage.
    ///
    /// This is still not full Milestone 3 teardown (plan item 3.4): it
    /// does not run for `exec()` (only whatever calls this is wired to,
    /// currently just `cage_finalize`), and it never recycles/frees a
    /// page-store slot even once its refcount reaches 0 -- that requires
    /// also knowing no *other* cage's mapping still points at that file
    /// offset via `mmap`, which is more bookkeeping than this narrow fix
    /// needs to take on. It closes the specific gap this session found:
    /// keeping `refcount` and `fault_routes` accurate as cages come and go.
    pub fn on_cage_exit(&self, cageid: u64) {
        if !self.enabled() {
            return;
        }
        let Some(cage) = get_cage(cageid) else {
            return;
        };
        let vmmap = cage.vmmap.read();
        for (_interval, entry) in vmmap.entries.iter() {
            if let MemoryBackingType::Cow(id) = entry.backing {
                if let Some(mut meta) = self.backings.get_mut(&id) {
                    meta.refcount = meta.refcount.saturating_sub(1);
                }
                let addr_st = (entry.page_num << sysdefs::constants::fs_const::PAGESHIFT) as u32;
                let addr_len = (entry.npages << sysdefs::constants::fs_const::PAGESHIFT) as usize;
                let host_addr = vmmap.user_to_sys(addr_st);
                self.unregister_route(host_addr, addr_len);
            }
        }
    }
}

/// Result of `CowManager::audit`. `mismatches.is_empty()` is the pass/fail
/// signal; the counts are there for diagnostics even when it passes.
pub struct CowAuditReport {
    pub total_backings: usize,
    pub total_recorded_refcount: usize,
    pub total_live_references: usize,
    pub mismatches: Vec<String>,
}

fn set_entry_backing(vmmap: &mut Vmmap, page_num: u32, npages: u32, backing: MemoryBackingType) {
    if let Some(entry) = vmmap.find_page_mut(page_num) {
        debug_assert_eq!(entry.page_num, page_num);
        debug_assert_eq!(entry.npages, npages);
        entry.backing = backing;
    } else {
        lind_log!(Default, "cow: set_entry_backing found no vmmap entry at page {}", page_num);
    }
}

/// Best-effort protection lookup for the fault path: re-derive the
/// prot the guest expects for this range from the owning cage's vmmap,
/// so the freshly materialized mapping keeps the same logical permission
/// the shared mapping had (e.g. PROT_READ|PROT_WRITE).
fn current_prot_hint(cageid: u64, host_addr: usize) -> i32 {
    let Some(cage) = get_cage(cageid) else {
        return sysdefs::constants::fs_const::PROT_READ | sysdefs::constants::fs_const::PROT_WRITE;
    };
    let vmmap = cage.vmmap.read();
    let user_addr = vmmap.sys_to_user(host_addr);
    let page_num = user_addr >> sysdefs::constants::fs_const::PAGESHIFT;
    vmmap
        .find_page(page_num)
        .map(|e| e.prot)
        .unwrap_or(sysdefs::constants::fs_const::PROT_READ | sysdefs::constants::fs_const::PROT_WRITE)
}

fn update_cage_vmmap_backing(cageid: u64, host_addr: usize, host_len: usize, backing: MemoryBackingType) {
    let Some(cage) = get_cage(cageid) else {
        lind_log!(Default, "cow: update_cage_vmmap_backing: cage {} not found", cageid);
        return;
    };
    let mut vmmap = cage.vmmap.write();
    let user_addr = vmmap.sys_to_user(host_addr);
    let page_num = user_addr >> sysdefs::constants::fs_const::PAGESHIFT;
    let npages = (host_len as u32) >> sysdefs::constants::fs_const::PAGESHIFT;
    set_entry_backing(&mut vmmap, page_num, npages, backing);
}

fn spawn_pager_thread() {
    std::thread::Builder::new()
        .name("lind-cow-pager".to_string())
        .spawn(|| loop {
            let event = COW_MANAGER.uffd().read_event();
            match event {
                Ok(Some(Event::Pagefault { kind, addr, .. })) => {
                    if matches!(kind, FaultKind::WriteProtected) {
                        COW_MANAGER.handle_write_fault(addr as usize);
                    } else {
                        lind_log!(Default, "cow: pager thread saw unexpected fault kind {:?} at {:p}", kind, addr);
                    }
                }
                Ok(Some(_other_event)) => {
                    // Fork/Remap/Remove/Unmap events: not expected in
                    // Milestone 1's scope (no intervening guest syscalls on
                    // Cow-managed ranges); ignore defensively rather than
                    // crash the pager thread.
                }
                Ok(None) => continue,
                Err(e) => {
                    lind_log!(Default, "cow: uffd read_event failed, pager thread exiting: {:?}", e);
                    break;
                }
            }
        })
        .expect("cow: failed to spawn pager thread");
}

pub static COW_MANAGER: LazyLock<CowManager> = LazyLock::new(CowManager::new);

pub fn is_cow_enabled() -> bool {
    COW_MANAGER.enabled()
}

/// Call from `cage_finalize` (or any other cage-teardown path) before the
/// cage is removed from `CAGE_MAP`, so `CowManager::on_cage_exit` can still
/// read its vmmap. See that method's doc comment for what this does and
/// does not cover.
pub fn on_cage_exit(cageid: u64) {
    COW_MANAGER.on_cage_exit(cageid);
}

/// Milestone 1 step 1.7 instrumentation: print the fork-copy counters if
/// `LIND_COW_STATS=1` is set, so a benchmark run can directly confirm "no
/// bulk copy occurred" instead of only inferring it from wall-clock time
/// (cow-implementation-plan.md Milestone 1 exit criteria). No-op if COW was
/// never enabled (LazyLock hasn't necessarily been forced in that case, but
/// `enabled()` forces it safely and returns false without side effects).
pub fn dump_stats_if_requested() {
    if std::env::var("LIND_COW_STATS").as_deref() != Ok("1") {
        return;
    }
    if !COW_MANAGER.enabled() {
        eprintln!("[cow-stats] COW disabled (LIND_FORK_MEMORY != cow, or userfaultfd unavailable)");
        return;
    }
    eprintln!(
        "[cow-stats] promotions={} promoted_bytes={} fork_shares={} materializations={} materialized_bytes={} materialize_fast_path={}",
        COW_MANAGER.promote_count.load(Ordering::Relaxed),
        COW_MANAGER.promote_bytes.load(Ordering::Relaxed),
        COW_MANAGER.share_count.load(Ordering::Relaxed),
        COW_MANAGER.materialize_count.load(Ordering::Relaxed),
        COW_MANAGER.materialize_bytes.load(Ordering::Relaxed),
        COW_MANAGER.materialize_fast_path_count.load(Ordering::Relaxed),
    );

    if std::env::var("LIND_COW_AUDIT").as_deref() == Ok("1") {
        let report = COW_MANAGER.audit();
        eprintln!(
            "[cow-audit] total_backings={} total_recorded_refcount={} total_live_references={} mismatches={}",
            report.total_backings,
            report.total_recorded_refcount,
            report.total_live_references,
            report.mismatches.len(),
        );
        for m in &report.mismatches {
            eprintln!("[cow-audit] MISMATCH: {}", m);
        }
    }
}
