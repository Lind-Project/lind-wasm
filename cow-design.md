# Lind COW Linear Memory for `fork`

**Status:** Design draft / research proposal (revision 2 — aligned to Lind's actual memory
architecture)
**Target:** Lind-Wasm on Linux
**Primary goal:** Replace eager per-region linear-memory copying at `fork()` with page-granular
copy-on-write (COW), integrated with Lind's existing `vmmap`/RawPOSIX memory-emulation layer
rather than with Wasmtime's generic memory-growth model.

---

## 0. What changed from revision 1

Revision 1 of this document modeled Lind's linear memory as a fairly ordinary Wasmtime/WASI
memory: one backing allocation that grows via `memory.grow`, pluggable via Wasmtime's
`MemoryCreator`/`LinearMemory` traits, touched only at `fork()`, `memory.grow`, and
`exec()`/exit(). That model does not match Lind's actual architecture, and building against it
directly would have produced a subsystem that quietly desynced from reality on ordinary program
behavior. The corrections, in order of impact:

1. **Lind's linear memory is a live emulated Unix address space, not a Wasm bump allocator.**
   The guest's own `mmap`/`munmap`/`mprotect`/`brk` syscalls are emulated by RawPOSIX
   (`src/rawposix/src/fs_calls.rs`) against `cage`-crate's `vmmap`
   (`src/cage/src/memory/vmmap.rs`), which issues **real host `mprotect`/`mmap(MAP_FIXED)`**
   calls at 4 KiB granularity on the exact same host VA range Wasmtime's memory object occupies.
   This happens continuously during normal execution (any `malloc`/`mmap`-heavy program), not
   just at fork. Any COW scheme has to hook into these paths or its backing-object metadata will
   drift from what the host mappings actually look like. Revision 1 didn't mention this at all.
2. **Lind does not use Wasmtime's `MemoryCreator` trait, and doesn't need to.** No lind crate
   implements it (`with_host_memory` in `config.rs:1561` is unused by any lind code). Lind
   directly forked `MmapMemory` in
   `src/wasmtime/crates/wasmtime/src/runtime/vm/memory/mmap.rs`, and the whole 4 GiB + guard
   region is already pre-allocated `PROT_READ|PROT_WRITE` at the host level and handed to vmmap,
   which then applies its own logical `PROT_NONE` on top. There is no `memory.grow` boundary to
   defend in Lind — the "don't leak future growth to an existing child" concern is really about
   vmmap's active-region set, which already includes `mmap`/`brk`/`mprotect`, not just growth.
3. **`VmmapEntry` already has a `backing: MemoryBackingType` field**
   (`Anonymous | SharedMemory(u64) | FileDescriptor(u64)`, `vmmap.rs:36-41`). This is the natural
   place to record COW backing identity — a new `MemoryBackingType::Cow(BackingId)` variant —
   instead of inventing a parallel `MemoryMap`/`Extent` structure that has to be kept in sync
   with vmmap by hand.
4. **The current fork path is already smarter than a bulk `memcpy`.** `fork_vmmap()`
   (`src/cage/src/memory/memory.rs:85-165`) already skips `PROT_NONE` entries, already does
   zero-copy `mremap` for guest `MAP_SHARED` entries, and copies the rest with one
   `process_vm_writev` syscall per entry. COW's job is narrower than revision 1 implied: it only
   needs to replace the `process_vm_writev` path for `MAP_PRIVATE` entries. The `MAP_SHARED`
   path is unaffected and shouldn't be touched.
5. **A real, measurable, and much cheaper-to-fix inefficiency exists today independent of COW.**
   Every non-dylink cage unconditionally maps ~256 MiB of `MAP_PRIVATE` RW grate-worker-stack
   arena at cage creation (`instance.rs:470-594`, `MAX_GRATE_WORKERS(32) × (GRATE_STACK_SLOT_SIZE
   8 MiB + GRATE_STACK_GUARD_SIZE 4 KiB)`, constants in
   `src/sysdefs/src/constants/lind_platform_const.rs:136,145,157`), regardless of whether any
   grate worker ever runs. That's `process_vm_writev`'d on every fork of every static cage today.
   This is called out as a **separate, lower-risk Phase 0** below, since it lets fork+exec
   benchmarks be interpreted correctly (how much of the win is COW vs. just not copying an unused
   arena).
6. **Cage threads share the exact same base pointer with no host-level synchronization beyond
   guest-emitted Wasm atomics** (`SharedMemory`'s `RwLock` in
   `src/wasmtime/crates/wasmtime/src/runtime/vm/threads/shared_memory.rs` only guards `grow()`,
   not ordinary loads/stores; `ClonedMemory::Thread` attaches new threads to the same
   `SharedMemory` with zero extra synchronization, `linker.rs:176-179,583-585`). Same-cage
   sibling-thread safety during remap-on-write is promoted from "later-phase nice to have" to a
   **Phase-1 blocking requirement**.
7. There is **no existing sibling-thread quiescing mechanism at fork** in Lind today (no
   `quiesce`/`suspend_thread`/`pause_thread`/`stop_the_world` hits anywhere in
   `lind-multi-process`, `cage`, or `rawposix`). `fork()` in POSIX only duplicates the calling
   thread, so this is a pre-existing gap shared by the current eager-copy path, not something COW
   introduces — and COW likely narrows the exposure window rather than widening it, since
   establishing shared backing + write-protection is far cheaper than a full data copy. This
   design treats "quiesce parent's other threads during the fork snapshot" as **out of scope**,
   inherited unchanged from today's behavior, not a COW prerequisite.

Everything else from revision 1 — the shared-backing-object rationale, the argument against
`UFFD_MISSING` and plain `MAP_PRIVATE`+host-fork, the concurrent-writer handling, the VMA
fragmentation risk, the extent-based mitigation — remains structurally correct and is carried
forward, adjusted to plug into vmmap instead of a standalone Wasmtime-level subsystem.

---

## 1. Motivation

For a parent with `N` bytes of active `MAP_PRIVATE` vmmap memory, `fork()` today costs
approximately `O(N)` in `process_vm_writev` calls (one syscall per vmmap entry, not per byte —
see `fork_vmmap`, `src/cage/src/memory/memory.rs:121-157`) plus `O(number of MAP_SHARED entries)`
essentially-free `mremap`s. Even if the child immediately calls `exec()`, or only touches a small
working set, that `N`-byte copy has already been paid before the child gets a chance to diverge or
give up its inherited memory.

```text
before fork:

    parent VA  ──────────> backing page P


after fork:

    parent VA  ─────┐
                    ├────> backing page P
    child VA   ─────┘

reads:
    no copy

first write by one side:
    copy P -> P'
    writer -> P'
    other  -> P
```

Requirements (unchanged from revision 1):

1. **No bulk memory copy at fork time** for `MAP_PRIVATE` regions (the current fastest path,
   `MAP_SHARED` via `mremap`, already meets this and is left alone).
2. **No copy on inherited read.**
3. **Copy only on first write to a shared page/chunk.**
4. **No compiler instrumentation on every Wasm load/store.**
5. **Support repeated Lind forks** without materializing full memory at each generation.
6. **Preserve vmmap's existing permission-emulation invariants** — real host `mprotect` state at
   any address must always match what the guest's own memory syscalls established, whether or not
   COW is involved underneath.

---

## 2. Non-goals

- reproduce Linux kernel `fork()` internally;
- rely on host `fork()` to obtain COW (see §29 — this breaks recursive Lind fork);
- intercept every Wasm load/store in generated code;
- change anything about how guest `MAP_SHARED` vmmap entries are forked — that path
  (`libc::mremap(MREMAP_MAYMOVE|MREMAP_FIXED)` in `fork_vmmap`) already achieves zero-copy sharing
  and is out of scope;
- go through Wasmtime's public `MemoryCreator`/`LinearMemory` trait — Lind already owns
  `MmapMemory` in-tree (§3.3), so there is no reason to route through the pluggable-backend API
  meant for out-of-tree consumers;
- make COW portable to macOS/Windows in the first implementation;
- optimize every VM edge case from the start (THP, NUMA, huge pages, page dedup);
- solve sibling-thread quiescing at fork — that's a pre-existing gap, not a COW-specific one
  (§0.7);
- snapshot registers/stacks/globals/tables/runtime bookkeeping — unchanged, handled by existing
  Asyncify-based fork.

The Linux implementation should still live behind an internal abstraction so a fallback
(eager-copy) path is trivial to select, but that abstraction is now internal to the
`cage`/`rawposix` crates, not a Wasmtime-facing trait (see §22).

---

## 3. Lind's actual memory model

### 3.1 Layering

```text
Wasmtime MmapMemory / SharedMemory
  (one big host mmap: base .. base+4GiB+guard, PROT_READ|WRITE at creation)
              |
              | mprotect(base, 4GiB, PROT_NONE)   [linker.rs:602-604 / lib.rs attach_shared_memory]
              v
     cage-crate vmmap (src/cage/src/memory/vmmap.rs)
       interval tree of VmmapEntry (nodit crate, Cargo.toml:9), 4 KiB granularity
       (PAGESHIFT = 12, sysdefs/src/constants/fs_const.rs:164-165)
              |
              | driven by guest mmap/munmap/mprotect/brk syscalls
              v
     rawposix syscall handlers (src/rawposix/src/fs_calls.rs)
       mmap_syscall / munmap_syscall / mprotect_syscall / brk emulation
       issue REAL host mmap(MAP_FIXED)/munmap/mprotect on the SAME base range
```

`VmmapEntry` (`vmmap.rs:46-57`) already carries:

```rust
pub struct VmmapEntry {
    pub page_num: u32,
    pub npages: u32,
    pub prot: i32,
    pub maxprot: i32,
    pub flags: i32,
    pub removed: bool,
    pub file_offset: i64,
    pub file_size: i64,
    pub cage_id: u64,
    pub backing: MemoryBackingType,
}

pub enum MemoryBackingType {
    None,
    Anonymous,
    SharedMemory(u64),   // shmid, guest SysV shm — already shared by guest intent
    FileDescriptor(u64),
}
```

This is the design's most important grounding fact: **Lind already has a per-region "what backs
this range" tag on the exact same interval-tree structure that drives real host permission calls.**
COW backing identity should be a new variant on this enum, not a separate metadata layer.

### 3.2 What Wasmtime already does, unmodified

`MmapMemory::new` (`mmap.rs:48-141`) is lind-specific in-tree code, not the stock Wasmtime path:
`is_lind_guest_memory = ty.shared` forces `maximum = MAX_MEMORY_SIZE` (4 GiB,
`memory.rs:114`), and the whole region including guard is allocated
`PROT_READ|PROT_WRITE` up front via `Mmap::accessible_reserved` (mmap.rs:131) — the comment there
is explicit: *"make_accessible is a no-op because rawposix manages wasm memory permissions... Guard
regions being host-accessible is safe because lind-wasm relies on explicit bounds checks, not
SIGSEGV-on-PROT_NONE."* `grow_to` is effectively called exactly once, from inside
`attach_shared_memory`, before the PROT_NONE reset.

This means: from the moment a cage is created, its **entire 4 GiB reservation is already one
big host mapping with a stable base pointer** — Wasmtime does no further growth, no further
protection changes, and no further mapping churn of its own for wasm linear memory after startup.
Everything that subdivides that region into differently-permissioned/backed sub-ranges is vmmap,
acting through real `mprotect`/`mmap(MAP_FIXED)`/`munmap` calls issued by RawPOSIX syscall
handlers (e.g. `mprotect_syscall`, `fs_calls.rs:4527-4593`, which does the real
`libc::mprotect` at `fs_calls.rs:4573` and then updates `cage.vmmap.write().change_prot(...)` at
`fs_calls.rs:4582-4589`).

**Consequence:** a COW implementation does not need a new Wasmtime memory backend at all. It
needs vmmap and the RawPOSIX syscall handlers that mutate it to become COW-aware, because they
already own the mechanism (real `mmap`/`mprotect`/`munmap` on the shared base range) that COW
needs to reuse.

### 3.3 Existing precedent for `MAP_SHARED` file-backed sub-mappings

Lind already has working code that maps a file-backed `MAP_SHARED` region at a fixed address
inside a cage's linear memory: SysV shared-memory emulation,
`ShmFile::new` (`src/cage/src/memory/shared.rs:57-78`, creates-then-unlinks a temp file — the
same "anonymous but shareable" trick `memfd_create` provides) and `ShmSegment::map_shm`
(`shared.rs:137-163`, `libc::mmap(addr, size, prot, MAP_SHARED|MAP_FIXED, fd, 0)`). This is
directly reusable prior art for the `PageStore` backing (§4.2) — no new host-level mmap primitive
needs to be invented, just generalized from single-purpose (guest `shmat`) to internal
(COW backing).

---

## 4. Main components

### 4.1 `MemoryBackingType::Cow` — fold into vmmap, not a parallel structure

```rust
pub enum MemoryBackingType {
    None,
    Anonymous,
    SharedMemory(u64),
    FileDescriptor(u64),
    Cow(BackingId),   // NEW — this entry's contents live in a Lind COW backing slot
}
```

`BackingId` identifies a slot in the page store (§4.2). When an entry's `backing` is
`Cow(id)`, its host mapping is a `MAP_SHARED` mapping of that slot rather than plain anonymous
memory, and the manager (§4.3) tracks a logical reference count for `id` alongside vmmap rather
than inside it (vmmap entries are per-cage; the refcount is cross-cage).

Extents fall out for free: since `VmmapEntry` already coalesces adjacent same-permission,
same-backing pages into one interval-tree node (that's what the `nodit` interval tree is for),
an entry with `backing: Cow(id)` already **is** revision 1's `Extent` concept
(`guest_offset, len, backing, backing_offset`) — `page_num`/`npages` give the extent's guest
range, and a small `(store_id, file_offset)` pair per `BackingId` gives the rest. No separate
`Extent`/`MemoryMap` type is needed.

### 4.2 `PageStore`

Modeled directly on `ShmFile` (§3.3) rather than invented fresh:

```rust
struct PageStore {
    file: File,        // unlinked temp file / memfd — same pattern as ShmFile::new
    next_offset: AtomicU64,
}

struct BackingMeta {
    id: BackingId,
    file_offset: u64,
    len: usize,
    logical_refcount: AtomicUsize,
}
```

One `PageStore` per Lind runtime instance (not per cage) is sufficient; slots from different
cages can coexist in the same backing file.

### 4.3 `CowManager`

```rust
struct CowManager {
    page_store: PageStore,
    fault_manager: UserfaultManager,
    backings: DashMap<BackingId, BackingMeta>,
}
```

Lives alongside vmmap (`cage` crate) rather than as a Wasmtime-level object, and is called from
two places:

1. **RawPOSIX syscall handlers** (`mmap_syscall`, `munmap_syscall`, `mprotect_syscall`, `brk`
   emulation, `mremap` emulation) — synchronously, *before* they issue the real host syscall, to
   eagerly resolve any COW state on the range being mutated (§9).
2. **The userfaultfd pager thread** — asynchronously, servicing WP faults from direct guest
   loads/stores (§8).

Both call paths funnel into the same underlying "materialize this range as uniquely owned"
routine — see §9.

---

## 5. Host virtual-memory layout

No change needed here versus what exists today (§3.2): `MmapMemory::new` already reserves
`base .. base + 4GiB + guard` up front via `Mmap::accessible_reserved`, and the base is already
stable for the cage's lifetime. COW does not add a new reservation step; it operates entirely
within the already-established range by having vmmap-driven syscalls install `MAP_SHARED`
file-backed sub-mappings (`mmap(MAP_FIXED, ...)` onto the page store, same primitive as
`ShmSegment::map_shm`) in place of plain anonymous ones for entries that become COW-eligible.

---

## 6. Normal execution before `fork()`

Unchanged conceptually: a `VmmapEntry` with `backing: Anonymous` (the common case today, and the
case for anything that has never been part of a fork's shared range) behaves exactly as it does
today — real private RW mapping, real `mprotect`-enforced permissions, zero COW-related overhead.
`backing: Cow(id)` only appears on entries that have actually been shared across a fork boundary.
The fast path for non-forked cages, and for entries a forked cage has already fully diverged on
(refcount back down to 1, unprotected — §9.5), is unchanged from today.

---

## 7. `fork()` operation — mapped onto `fork_vmmap`'s actual branches

`fork_vmmap()` (`src/cage/src/memory/memory.rs:85-165`) already walks `parent_vmmap.entries` and
branches per entry. COW changes exactly one of the three existing branches:

```text
for entry in parent_vmmap.entries:
    match entry.prot / entry.backing:
        PROT_NONE                    -> skip (unchanged — already free)
        MAP_SHARED (guest shm/mmap)  -> mremap MAYMOVE|FIXED (unchanged — already zero-copy)
        MAP_PRIVATE (backing =
          Anonymous | Cow(_))        -> **NEW: COW-share instead of process_vm_writev**
```

For the `MAP_PRIVATE` branch:

1. If `entry.backing == Anonymous`, lazily promote it: allocate a `BackingId`, copy the
   entry's current contents into the page store once (this is the *only* remaining bulk copy,
   and it happens once per entry the very first time it's shared, not once per fork), remap the
   parent's host range onto the new `MAP_SHARED` slot, set `entry.backing = Cow(id)`,
   `logical_refcount = 1`.
2. If `entry.backing == Cow(id)` already (parent has been COW-shared before, e.g. it's itself a
   fork child that never diverged this range), skip straight to step 3.
3. Increment `logical_refcount` for `id`.
4. Create the child's corresponding `VmmapEntry` with the same `Cow(id)` backing,
   `mmap(MAP_FIXED, MAP_SHARED, fd=page_store, offset)` at the child's host address for that
   range.
5. Register/write-protect (`UFFDIO_WRITEPROTECT`) both parent's and child's host ranges for `id`.

```text
before:                              after fork's COW-share step:

A.entry -> Anonymous                 A.entry -> Cow(P)   ref=2
                                      B.entry -> Cow(P)   ref=2
```

No `MAP_PRIVATE` entry's bytes are bulk-copied at fork time anymore (beyond the one-time
promotion in step 1, which future forks of the same lineage don't repeat). Cost becomes
`O(number of vmmap entries)` instead of `O(bytes)`, matching the original design's goal — but
now expressed against vmmap's real entry set instead of an idealized memory-region list.

The sibling-thread-quiescing question from revision 1's §7.1 is explicitly **not** solved here —
see §0.7. This step is not a new correctness requirement introduced by COW; it's inherited
unchanged from today's `fork_vmmap`.

---

## 8. Read path

Unchanged from revision 1 — this remains the core advantage over `UFFD_MISSING`:

```text
B native load -> B virtual page -> same MAP_SHARED page-store page A also maps
```

No fault, no copy. A child scanning a large inherited region stays physically shared with the
parent.

---

## 9. The write path has two triggers now, not one

Revision 1 only considered the direct-load/store → UFFD-WP-fault trigger. Because vmmap-driven
syscalls (`mmap`, `munmap`, `mprotect`, `mremap`, `brk`) can also mutate a `Cow(id)`-backed range
— and they do so via **real host syscalls that know nothing about `logical_refcount`** — a second
trigger is required: **eager materialization before any vmmap-mutating syscall touches a
`Cow`-backed range.**

Both triggers resolve through the same core routine:

```text
materialize_unique(cage, range):
    lock backing/version for range
    re-read backing metadata (another racer may have already resolved it)
    if backing.logical_refcount == 1:
        unprotect (remove UFFD-WP) if currently registered
        return   # already unique, nothing to copy
    else:
        allocate new BackingId P'
        copy old backing extent -> P'          # exactly one COW unit copied
        remap this cage's range onto P'  (MAP_FIXED)
        entry.backing = Cow(P')
        old_backing.logical_refcount -= 1
        P'.logical_refcount = 1
        unprotect this cage's range
```

### 9.1 Trigger 1 — direct guest write (userfaultfd WP fault)

Unchanged from revision 1: CPU hits a write-protected PTE, kernel delivers a UFFD-WP event to
Lind's pager thread, pager resolves fault VA → cage → vmmap entry (reusing vmmap's existing
address-translation lookup, `translate_vmmap_addr`/`check_and_convert_addr_ext`,
`memory.rs:199-242` — the same interval-tree lookup already used for every syscall's address
validation, not a new lookup structure) → `BackingId`, then runs `materialize_unique`, then wakes
the faulting thread so the CPU retries the store against the now-private page.

### 9.2 Trigger 2 — vmmap-mutating syscall (NEW, not in revision 1)

At the top of `mmap_syscall` (`MAP_FIXED` over an existing range), `munmap_syscall`,
`mprotect_syscall`, `mremap` emulation, and `brk`/heap-shrink emulation, before the handler's
existing real-host-syscall call:

```text
if the target range overlaps any vmmap entry with backing == Cow(_):
    materialize_unique(cage, overlapping sub-range)
proceed with the syscall's existing logic unchanged
```

This bounds the integration surface to **one guard call per handler**, reusing the exact same
`materialize_unique` the fault path uses, rather than teaching each handler bespoke COW-aware
mmap/munmap/mprotect/brk logic. Concretely:

- `munmap` of a `Cow`-backed range: materialize first (now `refcount == 1` on this cage's copy),
  then the existing `munmap` path runs exactly as today and correctly drops that cage's only
  reference; the *other* side (parent or sibling fork) still legitimately holds `refcount == 1`
  on its own copy and is unaffected.
- `mmap(MAP_FIXED)` replacing a `Cow`-backed range: materialize first so the old backing's
  refcount is decremented cleanly, then the existing overwrite logic installs whatever new
  mapping the guest asked for.
- `mprotect` changing R/W bits on a `Cow`-backed range: **does not need to materialize** — a real
  host `mprotect(PROT_WRITE)` and a `UFFDIO_WRITEPROTECT`-registered page compose correctly at the
  kernel level (the VMA's own protection bits and the UFFD-WP soft-dirty-style bit are enforced
  together; a write to a `PROT_WRITE` + UFFD-WP page still faults into UFFD, a write to a
  `PROT_READ` page still SIGSEGVs the guest normally, entirely independent of UFFD-WP). This
  needs experimental confirmation on the target kernel (§15) but should not require touching
  `materialize_unique` at all — it's the one vmmap-mutating syscall that's expected to be a
  no-op for COW purposes.
- `brk`/heap growth: newly committed pages are fresh, never previously shared —
  `backing: Anonymous`, never touches `CowManager`. `brk` shrink (or `munmap` of a shrinking
  heap tail) that happens to overlap a `Cow`-backed range goes through the `munmap` case above.

**Cost in the common case (no COW involved):** one `backing == Cow(_)` check against the looked-up
`VmmapEntry` (`vmmap.rs` already looks the entry up for every one of these syscalls to validate
the address and check permissions) — effectively free, since it's an extra `match` arm on data
already in hand, not an extra lookup.

---

## 10. Parent-first write

Unchanged — symmetric with §9.1, same `materialize_unique` routine.

---

## 11. Concurrent writers

Unchanged from revision 1 — `materialize_unique`'s lock-and-revalidate structure already handles
N-way races (A, B, C all sharing `P`; whichever of A/B faults or syscalls first materializes and
decrements; the second racer revalidates under the lock and finds `refcount` already reduced).

---

## 12. Repeated and recursive Lind `fork()`

Unchanged, and still the central argument for this design over plain `MAP_PRIVATE`+host-fork
(§29): because `BackingId`s are Lind-owned and independent of any live host `fork()`, a cage that
has already diverged (`B.entry -> Cow(Q2)`, its own private backing from an earlier
`materialize_unique`) can itself be the parent of a further Lind fork — `B forks C` just runs the
same `fork_vmmap` COW-share branch again, now sharing `Q2` (refcount 2) instead of the original
page. No full materialization needed at any generation boundary.

---

## 13. Heap / mmap growth (replaces revision 1's "`memory.grow`" section)

Revision 1 framed this around Wasm `memory.grow`, which Lind doesn't use post-startup (§0.2). The
real analog is: **anything the guest maps or grows via `brk`/`mmap` after a fork must not become
visible to an already-existing sibling.** This falls out automatically from the vmmap model with
no extra bookkeeping: `brk`/new `mmap` calls create brand-new `VmmapEntry`s with
`backing: Anonymous` in *that cage's own* vmmap only; they have no `Cow` identity and nothing to
share, so there is nothing for a sibling to inherit. If that cage later forks again, the new
region goes through the normal §7 promotion path like any other private entry at that later fork
point.

---

## 14. `exec()` and process exit

Unchanged in spirit from revision 1, now expressed against vmmap: when a cage's vmmap is torn
down (`exec()` replacing the address space, or process exit), every entry with
`backing: Cow(id)` must decrement `id`'s `logical_refcount` and recycle the page-store slot at
zero — this is a new hook into whatever code path currently walks/drops a cage's vmmap on
exec/exit (not independently verified in this revision; flagged in §32 as an implementation-time
lookup rather than assumed). For the common `fork -> small child setup -> exec` pattern, most
inherited entries are dropped here having never been written, so §7's promotion cost (the one
remaining real copy, on first-ever share) is also avoided whenever the *parent* side never writes
either — the design still wins even when only exec-soon children are considered, since the parent
frequently never diverges that range at all.

---

## 15. `userfaultfd` integration

Unchanged from revision 1 in mechanism (dedicated pager thread, `UFFD_USER_MODE_ONLY`,
`UFFDIO_API` negotiation, fail over to eager-copy if unavailable), with two additions specific to
Lind's layering:

- **Must be validated to compose with vmmap's own real `mprotect` calls**, specifically that
  `mprotect_syscall`'s direct `libc::mprotect` (`fs_calls.rs:4573`) does not clear a range's
  `UFFDIO_WRITEPROTECT` registration as a side effect. This is standard Linux behavior (VMA
  protection and UFFD-WP PTE bits are independent) but should be confirmed experimentally against
  the kernel Lind targets, per revision 1's own §15.3 caution — now specifically in the context of
  RawPOSIX's existing `mprotect_syscall`, not a hypothetical caller.
- **Registration/re-protection must be re-established correctly by `materialize_unique`
  regardless of which trigger (§9.1 fault vs §9.2 syscall-guard) invoked it** — both paths replace
  the same kind of mapping (`MAP_FIXED` swap to a new page-store slot) and must leave UFFD state
  consistent either way.

---

## 16. Pager / `materialize_unique` reentrancy requirement (NEW)

Revision 1 assumed `materialize_unique`-equivalent logic (there, "the pager") only ever runs on a
dedicated background thread servicing UFFD events, and warned it must avoid depending on locks a
faulting guest thread might hold. That still holds for §9.1. But §9.2 requires the *same* routine
to run **synchronously, on the calling RawPOSIX syscall-handler thread, reentrantly with whatever
locks that handler already holds** (e.g. `cage.vmmap.write()` — `mprotect_syscall` already takes
this lock at `fs_calls.rs:4582`). `materialize_unique` must therefore:

- take only fine-grained per-`BackingId` locks, never a global vmmap or Lind lock, so it can be
  called while a vmmap write-lock for the *calling* cage is already or about to be held without
  deadlocking against itself or the pager thread;
- avoid re-entering any RawPOSIX syscall path (no calling back into `mmap_syscall` etc. from
  inside `materialize_unique`) — it should only ever call the same low-level `mmap`/`mprotect`
  primitives the handlers use directly, not go back through syscall dispatch.

This reentrancy requirement is new relative to revision 1 and is a direct consequence of choosing
the syscall-guard trigger (§9.2) to keep vmmap-mutation integration cheap; it should be treated as
a first-class design constraint on `CowManager`'s locking, not an afterthought.

---

## 17. Threads inside one cage — promoted to a Phase-1 blocking requirement

Confirmed in code, not just theorized: `SharedMemory`'s `RwLock`
(`shared_memory.rs:23-26,204-206`) only guards `grow()` (`shared_memory.rs:97-131`); ordinary
JIT-generated loads/stores go through the raw base pointer in `LongTermVMMemoryDefinition`
(explicitly `unsafe impl Send + Sync`, lines 196-206) with **zero host-level synchronization**.
`ClonedMemory::Thread` (`linker.rs:176-179,583-585`) attaches a new pthread to the identical
`SharedMemory` object with no reset and no extra locking. This means a `materialize_unique` remap
on one thread of cage B can race with a sibling thread of the *same* cage B mid-load/store on the
range being swapped — a hazard revision 1 flagged as a to-be-prototyped risk (§17 there) but
placed after concurrency (Phase 3) and recursive fork (Phase 4) in its own plan.

**Given this is confirmed rather than speculative, revision 2's plan (§25) treats it as a scope
boundary from the start**: Phase 1–2 are explicitly restricted to single-threaded cages (no
`pthread_create` before the fork under test), and multi-threaded-cage correctness is a named,
early Phase 3 rather than a deferred stretch goal.

---

## 18. COW granularity

Unchanged tradeoff (4 KiB vs 64 KiB), now with one grounding fact: vmmap already operates at 4 KiB
host-page granularity (`PAGESHIFT = 12`, `fs_const.rs:164-165`) via the `nodit` interval tree, so
a 4 KiB COW unit aligns 1:1 with existing `VmmapEntry` page accounting with no extra
sub-page bookkeeping, while 64 KiB still reduces the number of distinct backing objects/VMAs at
the cost of copy-amplification on sparse writes. Recommend benchmarking both, as revision 1 did;
4 KiB has a slight implementation-simplicity edge here specifically because it needs no new
granularity concept beyond what vmmap already tracks.

---

## 19. Major risk: VMA fragmentation

Unchanged from revision 1, with an added observation: vmmap's own interval tree already has to
cope with fragmentation from ordinary `mmap`/`munmap`/`mprotect` churn (that's what `nodit` is
for), so COW-induced fragmentation is an additional instance of a problem vmmap already manages,
not a wholly new category. Any coalescing/extent-allocation mitigation (revision 1 §20, carried
forward unchanged below) should be co-designed with however vmmap already merges adjacent
compatible entries, rather than built as an independent mechanism.

---

## 20. Suggested extent-oriented backing layout

Unchanged from revision 1 in mechanism — allocate backing generations contiguously so that a
cage's dirtied-range entries can coalesce into one `VmmapEntry` rather than many. This now maps
directly onto `VmmapEntry` coalescing (an entry's `page_num..page_num+npages` range with a
contiguous `(store_id, file_offset..)` range in `Cow(id)` is already exactly what the interval
tree wants to represent as one node), rather than a bespoke `Extent` type layered above vmmap.

---

## 21. Optional zero-page optimization

Unchanged from revision 1 — not needed to prove the design, worth revisiting once the core
mechanism works.

---

## 22. Wasmtime integration details (revised — much smaller than revision 1 assumed)

Revision 1 assumed the integration point was Wasmtime's `MemoryCreator`/`LinearMemory` trait
pair. That's not how Lind's memory allocation works (§3.2, §0.2): no lind code implements
`MemoryCreator`, and Lind already directly owns and has already hand-modified `MmapMemory` in
`mmap.rs`. Given that, and given that vmmap already performs all post-creation mapping/protection
changes via raw host syscalls against Wasmtime's base pointer, **the COW subsystem needs
little-to-no new Wasmtime-crate code**:

```text
Wasmtime MmapMemory / SharedMemory   (unchanged: one big reservation, stable base pointer,
                                       already exposes the base pointer rawposix already uses)
        |
        | (no new Wasmtime-level object needed)
        v
cage crate: vmmap + VmmapEntry::backing::Cow(BackingId)   <- NEW
rawposix crate: mmap/munmap/mprotect/brk/mremap syscall handlers,
                each gains one materialize_unique() guard call                <- NEW
cage crate: CowManager (page store, backing refcounts, UFFD pager)            <- NEW
```

The only place genuinely new Wasmtime-crate work might be needed is if the base pointer or
reservation-size accessor rawposix currently uses isn't already sufficient for installing
`MAP_FIXED` `MAP_SHARED` sub-mappings the same way `ShmSegment::map_shm` already does within a
cage's range — this should be confirmed to already be adequate before any Wasmtime-crate changes
are considered, since precedent (§3.3) suggests it already is.

This significantly de-risks the project relative to revision 1: there's no need to satisfy
Wasmtime's public trait stability contract, no need for a "Phase 0: introduce a custom memory
backend with no behavior change" step (revision 1's plan), and no new invariant to prove about
guard-region inaccessibility (Lind's guard region is already host-accessible by design, §3.2).

---

## 23. Failure/fallback strategy

Unchanged from revision 1:

```text
if Linux + required memfd/mmap/userfaultfd features available:
    use Cow-backed vmmap entries
else:
    use current eager-copy (process_vm_writev) path
```

`LIND_FORK_MEMORY=eager|cow` debug switch, as before.

---

## 24. Correctness invariants

Invariants 1-5 and 7 from revision 1 carry forward unchanged (snapshot equality, read-sharing
doesn't mutate, first-divergent-write preserves old readers, unique backing is freely writable,
VA/base stability, no half-installed version). Two are revised/added:

**Invariant 6 (revised):** A cage cannot inherit a sibling's or ex-parent's post-fork vmmap
mutations of any kind — not just growth. This must hold for `mmap`, `munmap`, `mprotect`, `brk`,
and `mremap` alike, since all of them can create, destroy, or resize entries after the fork point,
not only `memory.grow`-shaped growth.

**Invariant 8 (new):** At every point where RawPOSIX is about to issue a real host `mmap`/
`munmap`/`mprotect`/`mremap` syscall against a range, that range's `Cow` refcount (if any) must
already reflect reality — i.e. `materialize_unique` (§9.2) must run to completion, and vmmap's
`backing` field must be updated, strictly before the real host syscall proceeds. This is the
invariant that closes the gap identified in §0.1 — without it, `CowManager`'s bookkeeping and the
real host mapping state can silently diverge on ordinary (non-fork) guest memory activity.

---

## 25. Prototype implementation plan (revised phasing)

### Phase 0 (NEW, separable, recommended first): static-cage grate-arena fork cost

Independent of COW: the ~256 MiB unconditional `MAP_PRIVATE` grate-worker-stack arena
(`instance.rs:470-594`) is copied on every static-cage fork today regardless of whether it's
used. Lazily reserving it (allocate/`mprotect` on first actual grate-worker use) or marking it
`MAP_SHARED`/zero-fill-on-demand removes a large, fixed, easily-measured chunk of today's fork
cost with none of COW's risk surface. Doing this first also makes later COW benchmarks (§26)
interpretable — otherwise this fixed floor muddies the before/after comparison.

### Phase 1: single-threaded fork share, no writes, `VmmapEntry::backing::Cow` plumbing

- Add `MemoryBackingType::Cow(BackingId)`, `PageStore` (modeled on `ShmFile`), `CowManager` with
  only the fork-time promotion + share logic (§7).
- No UFFD yet; temporarily reject/trap writes to `Cow`-backed ranges for bring-up, as revision 1
  suggested.
- **Scope-limited to cages with no live sibling pthreads at fork time** (§17) — this restriction
  is explicit, not incidental.

Primary test (unchanged from revision 1): parent initializes N MiB of `MAP_PRIVATE` memory, fork,
child scans all N MiB, verify data correctness and that fork does not `process_vm_writev` N MiB.

### Phase 2: UFFD-WP + single parent/child split (§9.1 only)

Implement `materialize_unique` for the direct-write/fault trigger only. Tests: child writes
first, parent writes first, both write different pages, both write the same page — byte-for-byte
verified against the eager path, still single-threaded-cage scope.

### Phase 3: same-cage multi-threaded correctness (moved up from revision 1's later phase)

Prove/implement a mapping-replacement sequence that's safe against sibling threads of the same
cage racing a `materialize_unique` remap (§17). This blocks lifting the single-threaded
restriction from Phases 1-2 and must land before Phase 4 can be considered general-purpose.

### Phase 4: vmmap-mutating-syscall trigger (§9.2) — NOT present in revision 1's plan

Wire `materialize_unique` guard calls into `mmap_syscall`, `munmap_syscall`, `mprotect_syscall`,
`mremap` emulation, and `brk` shrink. Tests: guest `munmap`s a `Cow`-shared range on one side only
(other side must be unaffected and correctly retain its reference); guest `mmap(MAP_FIXED)`
overwrites a `Cow`-shared range; guest `mprotect`s a `Cow`-shared range between `PROT_READ` and
`PROT_READ|WRITE` and confirm UFFD-WP registration survives (§15) and a subsequent write still
splits correctly.

### Phase 5: concurrency

N-way concurrent writers across processes (parent + multiple fork children) to the same backing,
as revision 1's Phase 3.

### Phase 6: recursive fork

`A -> B -> C`, `A -> B` and `A -> C`, multiple generations of divergence — verify metadata/backing
references propagate without materialization, as revision 1's Phase 4.

### Phase 7: `exec()`/exit teardown

Hook whatever code path drops a cage's vmmap on `exec()`/exit to decrement `Cow` refcounts and
recycle page-store slots (§14) — needs the actual teardown call site identified during
implementation, not assumed here.

### Phase 8: VMA/extent optimization

As revision 1's Phase 6 — measure real workloads, implement extent coalescing (§20) if the
fragmentation risk (§19) proves real in practice.

---

## 26. Benchmark plan

Same cases as revision 1 (fork+immediate exec, fork+read-100%, fork+write-1%, fork+write-100%,
recursive fork), plus one new case:

### F. Grate-arena isolation (NEW)

Fork+exec a static cage that touches **no** user heap/stack memory beyond startup, measured
**before and after Phase 0** independently of whether COW is enabled. This isolates how much of
any observed fork-time improvement is attributable to Phase 0 (removing the fixed 256 MiB
`MAP_PRIVATE` floor) versus Phases 1+ (COW itself) — without this control, a combined benchmark
can't distinguish the two.

Same metrics as revision 1 (fork latency, bytes physically copied, COW fault count, backing
allocations, pager time, RSS/PSS, VMA count, `exec()` latency, application-level benchmark), plus:
**count of `materialize_unique` invocations triggered via §9.2 (syscall guard) vs §9.1 (UFFD
fault)** — this distinguishes "guest touched its own address space a lot" workloads from "guest
wrote to shared data directly" workloads, which have different tuning implications.

---

## 27. Expected performance characteristics (recalibrated baseline)

| Workload | Current (per-region `process_vm_writev` + `mremap` for shared) | Proposed COW |
|---|---:|---:|
| fork + exec, static cage | full `MAP_PRIVATE` copy incl. ~256 MiB grate arena (Phase 0 removes the arena piece regardless of COW) | ~no content copy |
| fork + exec, dylink cage | full `MAP_PRIVATE` copy of small (few-MiB) startup region | ~no content copy |
| child reads all memory | full copy already paid | no content copy |
| child writes small working set | full copy | copy dirty COW units only |
| parent writes small working set | full copy | copy dirty COW units only |
| child writes everything | full copy once | eventually full copy + fault/remap overhead |
| guest `mmap`/`munmap`/`mprotect`/`brk` churn on non-shared memory | unaffected today | unaffected — `backing != Cow`, one cheap match-arm check per call, no lock/copy |
| guest `mmap`/`munmap`/`mprotect`/`brk` touching a still-shared `Cow` range | N/A (nothing is shared today) | one `materialize_unique` call — bounded, not free, but rare relative to non-shared traffic |
| recursive fork | full copy each fork | share existing backing versions |

The last two rows are new relative to revision 1's table and are the honest cost of closing the
gap identified in §0.1 — the design is not literally free outside of fork(); it adds a bounded,
usually-cheap check to every vmmap-mutating syscall, in exchange for avoiding the bulk copy.

---

## 28. Alternative considered: `UFFD_MISSING` child memory

Unchanged from revision 1 — still rejected because child reads would cause copying, defeating the
main goal.

---

## 29. Alternative considered: `memfd + MAP_PRIVATE`

Unchanged from revision 1 — still rejected as a general solution because a `MAP_PRIVATE`-dirtied
page has no Lind-level identity a later Lind fork can re-share; still noted as a reasonable
simpler comparison point if most children `exec()` quickly and single-generation fork dominates
real workloads.

---

## 30. Open questions / risks (revised and expanded)

Carried forward from revision 1: VMA fragmentation at scale (§19), exact concurrent-remap-safety
sequence for same-cage threads (§17), UFFD registration lifetime under `MAP_FIXED` replacement
(§15), fault-handler allocation/preallocation strategy, 4 KiB vs 64 KiB granularity (§18),
multiple Wasm memories (Lind currently focuses on memory 0), page-store growth strategy, THP/huge
pages (defer), cross-platform backend abstraction.

New, specific to the vmmap-integration approach:

1. **Does `mprotect_syscall`'s real `libc::mprotect` call actually preserve `UFFDIO_WRITEPROTECT`
   PTE state on the target kernel(s) Lind runs on?** (§9.2, §15) — needs an isolated experiment
   before Phase 4, independent of the rest of the design.
2. **Is `materialize_unique`'s fine-grained-lock, no-global-lock, no-syscall-reentry constraint
   (§16) actually achievable given `mprotect_syscall` already holds `cage.vmmap.write()` when it
   would need to call it?** — needs a concrete lock-ordering plan, not just "use fine-grained
   locks," since `materialize_unique` itself needs to mutate the *same* cage's vmmap entry it was
   called from inside of.
3. **What's the actual call site that tears down a cage's vmmap on `exec()`/exit** (§14, §25 Phase
   7)? Not identified in this revision — needs a direct code lookup before Phase 7 can be scoped
   precisely.
4. Should SysV shared-memory entries (`MemoryBackingType::SharedMemory(shmid)`, already
   intentionally shared by guest request) ever become `Cow`-eligible, or are they categorically
   out of scope since the guest already asked for real sharing semantics, not fork-COW semantics?
   (Recommend: categorically excluded — `Cow` only applies to what would otherwise be
   `Anonymous`/`MAP_PRIVATE`.)
5. Given there's no sibling-thread quiescing at fork today (§0.7), does `fork_vmmap`'s existing
   per-entry loop actually run with any other consistency guarantee against concurrent parent
   writes from sibling threads, or is today's implementation already relying on real-world
   fork-call patterns (usually single-threaded-at-fork-time) to avoid ever hitting this? Worth an
   explicit answer, since it bounds how much new exposure (if any) COW's smaller "snapshot window"
   actually removes vs. today.

---

## 31. Proposed Lind-facing abstraction (revised — internal, not Wasmtime-facing)

Revision 1 proposed a `ForkableLinearMemory` trait meant to sit at the Wasmtime boundary. Since
the real integration point is `cage`/`rawposix`, not Wasmtime (§22), the abstraction belongs
there instead — e.g. as the boundary between `fork_vmmap`'s `MAP_PRIVATE` branch and whichever
backend resolves it:

```rust
trait ForkSnapshotBackend {
    /// Called from fork_vmmap's MAP_PRIVATE branch for one VmmapEntry.
    /// Should avoid copying contents when COW is available and enabled.
    fn share_or_copy(&self, parent_entry: &VmmapEntry, child_cage: CageId) -> VmmapEntry;
}
```

Linux/COW implementation: promote-and-share per §7. Fallback implementation: today's
`mprotect`+`process_vm_writev` path, unchanged, selected by `LIND_FORK_MEMORY=eager` (§23). This
keeps the same separation-of-concerns goal as revision 1's trait, just anchored to the layer that
actually owns fork's memory handling today.

---

## 32. Summary

The core mechanism from revision 1 — Lind-owned shareable backing objects plus userfaultfd
first-write interception, chosen specifically because it (unlike `MAP_PRIVATE`+host-fork) survives
recursive Lind forks — remains the right approach. What revision 1 got wrong was treating Lind's
linear memory as a mostly-Wasmtime-owned, `memory.grow`-shaped object reachable through a generic
plugin trait. It is actually a `cage`/`rawposix`-owned, continuously-mutated emulated Unix address
space, and Wasmtime's role is limited to providing one stable, already-fully-allocated 4 GiB+guard
host mapping that vmmap subdivides via real syscalls.

Concretely, that means:

- the integration point is `VmmapEntry::backing` (already has the right shape —
  `MemoryBackingType`) and the RawPOSIX syscall handlers that mutate vmmap, not a new Wasmtime
  `MemoryCreator`;
- COW only needs to replace the `process_vm_writev` branch of `fork_vmmap`'s existing per-entry
  logic — the `PROT_NONE`-skip and `MAP_SHARED`-`mremap` branches are already effectively doing
  what COW wants and should be left alone;
- every vmmap-mutating syscall (`mmap`, `munmap`, `mprotect`, `mremap`, `brk`) needs one cheap
  guard call into the same `materialize_unique` routine the UFFD-WP fault path uses, or COW
  metadata will silently desync from real host mapping state on ordinary guest program behavior
  — this is the gap that made revision 1 incomplete, and closing it (§9, §16, Phase 4) is now the
  design's central new piece of work, on top of the originally-scoped fork/read/write/recursive-
  fork mechanics;
- same-cage multi-threaded correctness (§17) is confirmed as a real hazard from the actual
  `SharedMemory` implementation and is moved to an early, blocking phase rather than a deferred
  concern;
- a cheaper, independent, and separable fix exists for a large chunk of today's actual fork cost
  (the static-cage grate-worker arena, §0.5, Phase 0) and should be measured separately so later
  COW benchmarks aren't confounded by it.

The expected payoff is still largest for exactly the workloads revision 1 targeted: large
initialized `MAP_PRIVATE` working sets that `fork()` and then either `exec()` or touch only a
small fraction of what they inherited. The two most important risks to prototype early are now:
**(1)** whether `materialize_unique` can satisfy the reentrancy/locking constraint imposed by
already-locking RawPOSIX syscall handlers (§16, §30.2), and **(2)** same-cage multi-threaded remap
safety (§17) — both new relative to revision 1's top risks (VMA fragmentation, generic concurrent
remap safety), which remain real but are no longer the most structurally uncertain pieces.

---

## References

Primary documentation used for this design:

1. Linux kernel documentation, **Userfaultfd**
   https://docs.kernel.org/admin-guide/mm/userfaultfd.html
2. Linux `memfd_create(2)` manual page
   https://man7.org/linux/man-pages/man2/memfd_create.2.html
3. Linux `mmap(2)` manual page (`MAP_SHARED`, `MAP_PRIVATE`, `MAP_FIXED`)
   https://man7.org/linux/man-pages/man2/mmap.2.html
4. Wasmtime `MemoryCreator` API (confirmed unused by Lind, kept for context)
   https://docs.wasmtime.dev/api/wasmtime/trait.MemoryCreator.html
5. Wasmtime `LinearMemory` API
   https://docs.wasmtime.dev/api/wasmtime/trait.LinearMemory.html
6. Wasmtime custom memory test/example
   https://github.com/bytecodealliance/wasmtime/blob/main/tests/all/memory_creator.rs

Lind-internal grounding for this revision (file paths as of this writing; line numbers will drift
with the codebase and should be re-checked before implementation):

- `src/cage/src/memory/vmmap.rs` — `VmmapEntry`, `MemoryBackingType`, interval-tree structure
- `src/cage/src/memory/memory.rs` — `fork_vmmap`, address translation/validation helpers
- `src/cage/src/memory/shared.rs` — `ShmFile`, `ShmSegment` (SysV shm; `MAP_SHARED` precedent)
- `src/rawposix/src/fs_calls.rs` — `mprotect_syscall` and other memory-syscall emulation
- `src/wasmtime/crates/wasmtime/src/runtime/vm/memory/mmap.rs` — lind-modified `MmapMemory`
- `src/wasmtime/crates/wasmtime/src/runtime/vm/threads/shared_memory.rs` — `SharedMemory`,
  thread-sharing model
- `src/wasmtime/crates/wasmtime/src/runtime/instance.rs` — child instantiation, grate-worker
  arena sizing
- `src/wasmtime/crates/wasmtime/src/runtime/linker.rs` — `ClonedMemory::New`/`Thread`
- `src/sysdefs/src/constants/{fs_const.rs,lind_platform_const.rs}` — `PAGESHIFT`,
  `MAX_GRATE_WORKERS`, `GRATE_STACK_SLOT_SIZE`, `GRATE_STACK_GUARD_SIZE`
- `docs/internal/memory.md` — existing internal vmmap documentation
