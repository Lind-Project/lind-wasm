//! Lind COW (copy-on-write) fork memory backing.
//!
//! Implements the shared-backing-object + userfaultfd write-protect design
//! from `/cow-design.md`, scoped to `/cow-implementation-plan.md` Milestones
//! 1-2: `fork_vmmap`'s `MAP_PRIVATE` branch shares memory via a zero-copy
//! `MAP_SHARED` mapping of a Lind-owned page-store slot instead of an eager
//! `process_vm_writev` copy, splitting into a private copy only on the first
//! write to a still-shared range (detected via `userfaultfd` write-protect).
//!
//! # Chunk granularity (not per-vmmap-entry, not per-4KB-page)
//!
//! A `VmmapEntry` shared via COW is internally divided into fixed-size
//! `COW_CHUNK_SIZE` (1 MiB) chunks, each independently tracked (own
//! `BackingId`, own refcount, own materialize-on-write). A write anywhere in
//! a chunk only copies *that chunk*, not the whole entry -- this replaces an
//! earlier whole-entry-granularity version that, while simpler, made every
//! write cost proportional to the entry's size rather than to how much
//! actually diverged (confirmed empirically: a 4 KiB write into a 128 MiB
//! entry cost the same as writing all of it). 1 MiB (not 4 KiB) was chosen
//! deliberately: `cow-design.md` §18 frames 4 KiB vs 64 KiB as a real
//! tradeoff (minimum copy-on-write vs. fewer backing objects/mmap calls per
//! fork); at 4 KiB, sharing a 512 MiB entry would need up to ~131,000
//! separate chunk records, which risks the VMA-fragmentation problem §19
//! warns about being the largest scalability risk in this whole design. 1
//! MiB keeps that count in the hundreds while still cutting materialize cost
//! by orders of magnitude for realistic sparse-write patterns. `VmmapEntry`
//! itself is never split -- `VmmapEntry::backing` is just a `Cow` marker
//! ("this entry has chunk-level COW state, look it up here") rather than a
//! single `BackingId`; the real per-chunk bookkeeping lives entirely in
//! `CowManager::chunk_owners`, keyed by host address.
//!
//! `fork_share_entry`'s mmap calls into the child still coalesce contiguous
//! same-backing-run chunks into one `mmap` call each, so a freshly-promoted
//! (never-diverged) entry costs one `mmap` call at fork time either way, not
//! one per chunk -- only an entry that has already partially diverged pays
//! more than one child-side `mmap` call, proportional to how fragmented it
//! already is.
//!
//! # Scope of this implementation (Milestones 1-2)
//!
//! - Single-threaded cages, no intervening guest `mmap`/`munmap`/`mprotect`/
//!   `brk` between fork and first write. `cow-design.md` §9.2's
//!   vmmap-mutating-syscall guard is Milestone 3 work and is NOT implemented
//!   here; only §9.1's direct-write/UFFD-WP-fault trigger is.
//! - Small entries (`< 1 MiB` total) and the wasm stack region (the vmmap
//!   entry starting at page_num 0) are excluded from COW and stay on the
//!   eager path -- both for correctness reasons found empirically during
//!   Milestone 1 bring-up (see `cow-implementation-plan.md`'s Milestone 1
//!   notes): sharing either broke Asyncify's fork/rewind on a cage's second
//!   fork, and neither has any real COW upside anyway (stacks diverge
//!   immediately; small entries are cheap to eager-copy regardless).
//! - Exit-time refcount release (`on_cage_exit`, wired into
//!   `cage::cage_finalize`) keeps `chunk_owners`/refcounts accurate as cages
//!   exit, but this is still not full Milestone 3 `exec()`/exit() teardown
//!   (plan item 3.4): it doesn't run for `exec()`, and a backing whose
//!   refcount reaches 0 is never recycled/freed from the page store (would
//!   need to also confirm no *other* mapping still points at that file
//!   offset, which is more bookkeeping than this narrow fix takes on).

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
use sysdefs::constants::fs_const::{MAP_FIXED, MAP_SHARED, PAGESHIFT, PAGESIZE};
use sysdefs::lind_log;
use userfaultfd::{Event, FaultKind, FeatureFlags, RegisterMode, Uffd, UffdBuilder};

/// Identifies one backing slot in the page store. Now one per *chunk*, not
/// one per shared `VmmapEntry` -- see the module doc comment.
pub type BackingId = u64;

/// Fixed COW unit size. See the module doc comment for why 1 MiB.
const COW_CHUNK_SIZE: usize = 1024 * 1024;

/// `VmmapEntry::backing` value used for any entry with chunk-level COW
/// state. The real per-chunk identity lives in `CowManager::chunk_owners`,
/// not in this field -- `0` is never a real `BackingId` (`next_backing_id`
/// starts at 1), so it's a safe, unambiguous marker.
const CHUNKED_ENTRY_MARKER: u64 = 0;

/// One backing slot: a `COW_CHUNK_SIZE`-ish byte range in the page-store
/// file (the last chunk of an entry may be shorter) that some set of cages
/// currently share (refcount > 1), or that exactly one cage now privately
/// owns going forward without needing further COW protection (refcount ==
/// 1, `cow-design.md` Invariant 4). Guarded entirely by the `DashMap` shard
/// lock in `CowManager::backings` -- every access to `refcount` goes through
/// a `get`/`get_mut` on that map, never bypassed, which is what makes the
/// check-and-maybe-decrement in `handle_write_fault` atomic
/// (`cow-design.md` §9.2 "take a lock... re-read the mapping metadata after
/// acquiring the lock").
struct BackingMeta {
    file_offset: u64,
    len: usize,
    refcount: usize,
}

/// Which cage a given host-address chunk belongs to and which backing it
/// currently points at. Keyed by host address in `CowManager::chunk_owners`,
/// one entry per chunk, so the pager thread can resolve a raw UFFD fault
/// address to exactly the chunk it fell in without scanning any vmmap.
///
/// Unlike Milestone 1's `fault_routes` (which this replaces), entries here
/// are **persistent**: they are not removed when a chunk becomes unique/
/// unprotected, only when its owning cage exits (`on_cage_exit`). This is
/// required for chunk granularity to work at all -- `VmmapEntry::backing` is
/// now just a generic marker, so `chunk_owners` is the *only* record of
/// "does this specific chunk have COW identity, and if so which backing,"
/// needed by a later fork of the same entry even after some of its chunks
/// have already gone unique in the meantime.
#[derive(Clone, Copy)]
struct ChunkOwner {
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

    /// Bump-allocate `len` bytes (rounded up to host page size) in the store
    /// file. Milestone 1-2 never free/recycle slots -- refcount-driven
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
    /// design for a given chunk -- it happens once per chunk, the first
    /// time that chunk is shared or split, not once per fork
    /// (`cow-design.md` §7 step 1, §9.4).
    ///
    /// # Safety
    /// `src` must point to at least `len` readable bytes for the duration
    /// of the call. Callers pass a currently-live host mapping (either the
    /// entry being promoted, or a still-shared chunk being split), which is
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
    chunk_owners: Mutex<NoditMap<usize, Interval<usize>, ChunkOwner>>,

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
    /// Number of write faults that triggered chunk materialization.
    pub materialize_count: AtomicU64,
    /// Bytes copied by materialize (the write-triggered split cost, as
    /// opposed to promotion's fork-triggered cost) -- now bounded by
    /// `COW_CHUNK_SIZE` per event instead of the whole entry's size.
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
            chunk_owners: Mutex::new(NoditMap::new()),
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

    fn register_chunk(&self, start: usize, len: usize, cageid: u64, backing_id: BackingId) {
        let mut map = self.chunk_owners.lock();
        let _ = map.insert_overwrite(ie(start, start + len), ChunkOwner { cageid, backing_id });
    }

    fn remove_chunk(&self, start: usize, len: usize) {
        let mut map = self.chunk_owners.lock();
        let _ = map.remove_overlapping(ie(start, start + len));
    }

    /// Returns `(chunk_start, chunk_len, owner)` for the chunk containing
    /// `addr`, if any.
    fn lookup_chunk(&self, addr: usize) -> Option<(usize, usize, ChunkOwner)> {
        let map = self.chunk_owners.lock();
        let result = map
            .overlapping(ie(addr, addr + 1))
            .next()
            .map(|(interval, owner)| (interval.start(), interval.end() - interval.start() + 1, *owner));
        result
    }

    /// First-ever share of an entry: one bulk copy of the whole range into
    /// the page store, then split into `COW_CHUNK_SIZE` chunks each with
    /// their own fresh `BackingId` (`refcount = 1`, all pointing at
    /// contiguous offsets within that one copy). Remaps the parent's own
    /// range onto the new backing in one `mmap` call (still contiguous at
    /// this point, so this doesn't yet pay the per-chunk `mmap` cost that
    /// only applies once an entry has partially diverged).
    fn promote_entry(&self, parent_st: usize, addr_len: usize, prot: i32, parent_cageid: u64) -> bool {
        let base_offset = match self.page_store.allocate(addr_len) {
            Ok(o) => o,
            Err(e) => {
                lind_log!(Default, "cow: page store allocation failed during promotion: {:?}", e);
                return false;
            }
        };
        if let Err(e) = unsafe { self.page_store.write_from(base_offset, parent_st as *const u8, addr_len) } {
            lind_log!(Default, "cow: page store copy failed during promotion: {:?}", e);
            return false;
        }
        self.promote_count.fetch_add(1, Ordering::Relaxed);
        self.promote_bytes.fetch_add(addr_len as u64, Ordering::Relaxed);

        let num_chunks = (addr_len + COW_CHUNK_SIZE - 1) / COW_CHUNK_SIZE;
        let base_id = self.next_backing_id.fetch_add(num_chunks as u64, Ordering::SeqCst);
        let mut chunk_offset = 0usize;
        for i in 0..num_chunks {
            let chunk_len = COW_CHUNK_SIZE.min(addr_len - chunk_offset);
            let id = base_id + i as u64;
            self.backings.insert(
                id,
                BackingMeta {
                    file_offset: base_offset + chunk_offset as u64,
                    len: chunk_len,
                    refcount: 1,
                },
            );
            self.register_chunk(parent_st + chunk_offset, chunk_len, parent_cageid, id);
            chunk_offset += chunk_len;
        }

        // Remap the parent's own range onto the new MAP_SHARED backing --
        // parent transitions from private-anon to Cow-shared, in place, at
        // the same host address (cow-design.md §7.2). One mmap call since
        // all chunks are still contiguous right after this bulk copy.
        let ret = unsafe {
            libc::mmap(
                parent_st as *mut c_void,
                addr_len,
                prot,
                (MAP_SHARED | MAP_FIXED) as i32,
                self.page_store.raw_fd(),
                base_offset as i64,
            )
        };
        if ret == libc::MAP_FAILED {
            lind_log!(Default, "cow: mmap MAP_SHARED failed while promoting parent range");
            return false;
        }
        true
    }

    /// Share every chunk of `[parent_st, parent_st+addr_len)` into the
    /// child at `child_st`, coalescing consecutive chunks whose backing
    /// offsets are contiguous into a single `mmap` call each (so a
    /// never-diverged entry still costs exactly one `mmap` call here, same
    /// as the whole-entry version did -- only a partially-diverged entry
    /// pays more than one, proportional to how fragmented it already is).
    fn share_chunks_into_child(&self, parent_st: usize, child_st: usize, addr_len: usize, prot: i32, child_cageid: u64) -> bool {
        let mut offset = 0usize;
        while offset < addr_len {
            let Some((_, chunk_len, owner)) = self.lookup_chunk(parent_st + offset) else {
                lind_log!(Default, "cow: missing chunk owner at parent offset {}", offset);
                return false;
            };
            let Some(base_file_offset) = self.backings.get(&owner.backing_id).map(|m| m.file_offset) else {
                lind_log!(Default, "cow: backing {} vanished during share", owner.backing_id);
                return false;
            };

            // Extend the run while the next chunk continues contiguously in
            // the page store.
            let mut run_len = chunk_len;
            while offset + run_len < addr_len {
                let Some((_, next_len, next_owner)) = self.lookup_chunk(parent_st + offset + run_len) else {
                    break;
                };
                let next_file_offset = self.backings.get(&next_owner.backing_id).map(|m| m.file_offset);
                if next_file_offset != Some(base_file_offset + run_len as u64) {
                    break;
                }
                run_len += next_len;
            }

            let ret = unsafe {
                libc::mmap(
                    (child_st + offset) as *mut c_void,
                    run_len,
                    prot,
                    (MAP_SHARED | MAP_FIXED) as i32,
                    self.page_store.raw_fd(),
                    base_file_offset as i64,
                )
            };
            if ret == libc::MAP_FAILED {
                lind_log!(Default, "cow: mmap MAP_SHARED failed while sharing chunk run into child");
                return false;
            }

            // Bump refcount and register the child's chunk_owners entry for
            // every chunk in the run just mapped.
            let mut sub = 0usize;
            while sub < run_len {
                let Some((_, clen, o)) = self.lookup_chunk(parent_st + offset + sub) else {
                    break;
                };
                if let Some(mut meta) = self.backings.get_mut(&o.backing_id) {
                    meta.refcount += 1;
                }
                self.register_chunk(child_st + offset + sub, clen, child_cageid, o.backing_id);
                sub += clen;
            }

            offset += run_len;
        }
        true
    }

    /// Share one `vmmap` entry's range into a child at fork time, avoiding
    /// the bulk copy in `fork_vmmap`'s `MAP_PRIVATE` branch
    /// (`cow-design.md` §7). Returns `true` if handled (caller should skip
    /// the eager `process_vm_writev` path for this entry), `false` if COW
    /// is disabled, the entry is excluded (too small, or the stack region),
    /// or the share failed for some reason (e.g. mmap of a page-store slot
    /// failed) -- in all `false` cases the caller falls back to eager copy.
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
        // first. There's no real COW upside to sharing them anyway -- the
        // whole point of COW is avoiding the cost of copying *large*
        // regions, and eager-copying a few hundred KB is already cheap.
        const COW_MIN_ENTRY_BYTES: usize = 1024 * 1024;
        if addr_len < COW_MIN_ENTRY_BYTES {
            return false;
        }
        // The entry starting at page_num 0 is the wasm stack+guard region.
        // Sharing it breaks Asyncify's fork/rewind (cow-implementation-plan.md
        // Milestone 1 notes); it also has no real COW upside since a stack
        // diverges between parent and child essentially immediately.
        if page_num == 0 {
            return false;
        }

        match current_backing {
            MemoryBackingType::Cow(_) => {
                // Already shared by an earlier fork in this lineage -- no
                // bulk copy needed, just add another reference per chunk
                // (cow-design.md §12, the recursive-fork win).
            }
            MemoryBackingType::Anonymous => {
                if !self.promote_entry(parent_st, addr_len, prot, parent_cageid) {
                    return false;
                }
                set_entry_backing(parent_vmmap, page_num, npages, MemoryBackingType::Cow(CHUNKED_ENTRY_MARKER));
            }
            // SharedMemory/FileDescriptor/None entries never reach here --
            // fork_vmmap only calls this for the MAP_PRIVATE branch.
            _ => return false,
        }

        if !self.share_chunks_into_child(parent_st, child_st, addr_len, prot, child_cageid) {
            return false;
        }
        set_entry_backing(child_vmmap, page_num, npages, MemoryBackingType::Cow(CHUNKED_ENTRY_MARKER));

        // Write-protect both sides' full byte range in one ioctl call each
        // (cow-design.md §7.2/§7.4) -- UFFD registration/protection
        // granularity is independent of chunk-backing granularity, so this
        // doesn't need to happen per chunk.
        self.ensure_write_protected(parent_st, addr_len);
        self.ensure_write_protected(child_st, addr_len);

        self.share_count.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn ensure_write_protected(&self, host_start: usize, host_len: usize) {
        if let Err(e) = self
            .uffd()
            .register_with_mode(host_start as *mut c_void, host_len, RegisterMode::WRITE_PROTECT)
        {
            // Non-fatal: likely already registered from an earlier fork of
            // this same entry, or from a defensive re-register after a
            // chunk materialize (see handle_write_fault). UFFDIO_REGISTER
            // on an already-registered range is expected to be redundant
            // here, not a real error -- write_protect below is what
            // actually matters.
            lind_log!(Default, "cow: uffd register (possibly redundant) returned {:?}", e);
        }
        if let Err(e) = self.uffd().write_protect(host_start as *mut c_void, host_len) {
            lind_log!(Default, "cow: uffd write_protect failed: {:?}", e);
        }
    }

    /// Handle a UFFD write-protect fault (`cow-design.md` §9.1/§9). Runs on
    /// the dedicated pager thread. Operates on exactly the chunk the fault
    /// address falls in, not the whole entry it belongs to.
    fn handle_write_fault(&self, addr: usize) {
        let Some((chunk_start, chunk_len, owner)) = self.lookup_chunk(addr) else {
            lind_log!(Default, "cow: write fault at {:#x} has no registered chunk, waking defensively", addr);
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
                .get_mut(&owner.backing_id)
                .expect("cow: unknown backing id in chunk fault");
            if meta.refcount == 1 {
                true
            } else {
                meta.refcount -= 1;
                false
            }
        };

        if is_unique {
            // Fast path (cow-design.md §9.3): this chunk's mapping is
            // already the only reference to this backing -- no copy.
            self.materialize_fast_path_count.fetch_add(1, Ordering::Relaxed);
            let _ = self.uffd().remove_write_protection(chunk_start as *mut c_void, chunk_len, true);
            // chunk_owners keeps pointing at the same (now-unique) backing
            // -- correct, no change needed there.
            return;
        }

        // Shared path (cow-design.md §9.4-9.5): allocate a new chunk-sized
        // slot, copy the current (still valid to read) contents, remap just
        // this chunk onto it at the same host address.
        let new_offset = match self.page_store.allocate(chunk_len) {
            Ok(o) => o,
            Err(e) => {
                lind_log!(Default, "cow: page store allocation failed in materialize: {:?}", e);
                let _ = self.uffd().wake(chunk_start as *mut c_void, chunk_len);
                return;
            }
        };
        if let Err(e) = unsafe { self.page_store.write_from(new_offset, chunk_start as *const u8, chunk_len) } {
            lind_log!(Default, "cow: page store copy failed in materialize: {:?}", e);
            let _ = self.uffd().wake(chunk_start as *mut c_void, chunk_len);
            return;
        }
        self.materialize_bytes.fetch_add(chunk_len as u64, Ordering::Relaxed);

        let new_id = self.next_backing_id.fetch_add(1, Ordering::SeqCst);
        self.backings.insert(
            new_id,
            BackingMeta {
                file_offset: new_offset,
                len: chunk_len,
                refcount: 1,
            },
        );

        let existing_prot = current_prot_hint(owner.cageid, chunk_start);
        let ret = unsafe {
            libc::mmap(
                chunk_start as *mut c_void,
                chunk_len,
                existing_prot,
                (MAP_SHARED | MAP_FIXED) as i32,
                self.page_store.raw_fd(),
                new_offset as i64,
            )
        };
        if ret == libc::MAP_FAILED {
            lind_log!(Default, "cow: mmap MAP_SHARED failed in materialize");
            let _ = self.uffd().wake(chunk_start as *mut c_void, chunk_len);
            return;
        }

        self.register_chunk(chunk_start, chunk_len, owner.cageid, new_id);

        // Defensive re-register: the MAP_FIXED replace above may have
        // dropped UFFD registration for just this chunk's sub-range
        // (cow-design.md §15.3's open question -- Milestone 1 found this
        // happens for a whole-entry replace; assumed to hold for a
        // chunk-sized one too). A redundant register on an unaffected
        // range is harmless (see ensure_write_protected). This chunk is
        // unique now so it is NOT write-protected here -- only
        // re-registered, so a *later* fork sharing this exact chunk again
        // can write_protect it without needing to register first.
        if let Err(e) = self
            .uffd()
            .register_with_mode(chunk_start as *mut c_void, chunk_len, RegisterMode::WRITE_PROTECT)
        {
            lind_log!(Default, "cow: defensive re-register after materialize returned {:?}", e);
        }

        let _ = self.uffd().wake(chunk_start as *mut c_void, chunk_len);
    }

    /// Milestone 2 exit criterion (cow-implementation-plan.md): confirm
    /// backing-object/refcount bookkeeping stays correct across multiple
    /// fork generations and non-linear fork trees, with no leaked/
    /// never-decremented refcounts. Counts how many `chunk_owners` entries
    /// currently point at each `BackingId` and compares that live count
    /// against the backing's recorded `refcount`.
    ///
    /// This is an O(total chunks) scan, not something to run on a hot path
    /// -- it's a diagnostic for tests/benchmarking (see
    /// `dump_stats_if_requested`), not part of normal fork/materialize
    /// operation. `chunk_owners` entries are removed on `on_cage_exit`, so
    /// (unlike counting via `VmmapEntry::backing`, which Milestone 1's
    /// version of this audit did) this reflects only currently-alive
    /// cages' references without needing to walk `CAGE_MAP` at all.
    pub fn audit(&self) -> CowAuditReport {
        let mut live_counts: std::collections::HashMap<BackingId, usize> = std::collections::HashMap::new();
        {
            let map = self.chunk_owners.lock();
            for (_interval, owner) in map.iter() {
                *live_counts.entry(owner.backing_id).or_insert(0) += 1;
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
                    "backing {}: recorded refcount={} but live chunk references={}",
                    id, recorded, live
                ));
            }
        }
        for (id, count) in &live_counts {
            if !self.backings.contains_key(id) {
                mismatches.push(format!(
                    "backing {} referenced by {} live chunk_owners entries but missing from backings table",
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
    /// Discovered empirically while building the Milestone 2 audit:
    /// `cage::cage_finalize` removes a cage from `CAGE_MAP` (and drops its
    /// `Cage`/`Vmmap`) as soon as that cage truly finishes running -- not
    /// just at process shutdown, but per-cage, as each one exits. Without
    /// this hook, a backing's `refcount` would only ever grow across
    /// repeated forks, never reflecting cages that have already exited,
    /// which both leaks page-store space and (worse) leaves stale
    /// `chunk_owners` entries pointing at host address ranges the OS is
    /// free to hand to an unrelated later cage.
    ///
    /// Walks this cage's own `Cow`-marked vmmap entries, and for each one
    /// steps through its chunks (using whatever chunk boundaries
    /// `chunk_owners` actually has recorded, so this works regardless of
    /// how fragmented the entry has become), decrementing each chunk's
    /// backing refcount and removing that chunk's `chunk_owners` entry.
    ///
    /// This is still not full Milestone 3 teardown (plan item 3.4): it
    /// does not run for `exec()`, and never recycles/frees a page-store
    /// slot even once its refcount reaches 0 -- that requires also knowing
    /// no *other* cage's mapping still points at that file offset via
    /// `mmap`, which is more bookkeeping than this narrow fix needs to
    /// take on.
    pub fn on_cage_exit(&self, cageid: u64) {
        if !self.enabled() {
            return;
        }
        let Some(cage) = get_cage(cageid) else {
            return;
        };
        let vmmap = cage.vmmap.read();
        for (_interval, entry) in vmmap.entries.iter() {
            if !matches!(entry.backing, MemoryBackingType::Cow(_)) {
                continue;
            }
            let addr_st = (entry.page_num << PAGESHIFT) as u32;
            let addr_len = (entry.npages << PAGESHIFT) as usize;
            let host_addr = vmmap.user_to_sys(addr_st);
            let mut offset = 0usize;
            while offset < addr_len {
                let Some((c_start, c_len, owner)) = self.lookup_chunk(host_addr + offset) else {
                    break;
                };
                if let Some(mut meta) = self.backings.get_mut(&owner.backing_id) {
                    meta.refcount = meta.refcount.saturating_sub(1);
                }
                self.remove_chunk(c_start, c_len);
                offset += c_len;
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

/// Best-effort protection lookup for the fault path: re-derive the prot the
/// guest expects for this range from the owning cage's vmmap, so the
/// freshly materialized chunk keeps the same logical permission the shared
/// mapping had (e.g. PROT_READ|PROT_WRITE).
fn current_prot_hint(cageid: u64, host_addr: usize) -> i32 {
    let Some(cage) = get_cage(cageid) else {
        return sysdefs::constants::fs_const::PROT_READ | sysdefs::constants::fs_const::PROT_WRITE;
    };
    let vmmap = cage.vmmap.read();
    let user_addr = vmmap.sys_to_user(host_addr);
    let page_num = user_addr >> PAGESHIFT;
    vmmap
        .find_page(page_num)
        .map(|e| e.prot)
        .unwrap_or(sysdefs::constants::fs_const::PROT_READ | sysdefs::constants::fs_const::PROT_WRITE)
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
                    // Milestone 1-2's scope (no intervening guest syscalls
                    // on Cow-managed ranges); ignore defensively rather
                    // than crash the pager thread.
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
