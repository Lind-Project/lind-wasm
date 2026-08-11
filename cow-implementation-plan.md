# Lind COW Fork — Implementation Plan

## Milestone 1 implementation notes (post-implementation)

Milestone 1 (single-threaded, no intervening guest memory syscalls) is implemented and passing:
`src/cage/src/memory/cow.rs` (new), wired into `fork_vmmap` (`memory.rs`), `MemoryBackingType::Cow`
added to `vmmap.rs`, instrumentation counters exposed via `LIND_COW_STATS=1`. All 13
`tests/cow-tests/correctness/*.c` pass under `LIND_FORK_MEMORY=cow`, byte-identical to the eager
baseline; the full `tests/unit-tests/` suite (246 tests) passes identically in both modes (same 2
pre-existing environment-only native failures, unrelated to COW). Three things discovered during
implementation that matter for later milestones:

1. **UFFD write-protect requires shmem-backed memory, not a plain regular-file `MAP_SHARED`
   mapping.** The `PageStore` design in `cow-design.md` §4.2/§3.3 (modeled on `ShmFile`'s
   create-then-unlink regular-file pattern) fails `UFFDIO_REGISTER` with `EINVAL` when used for
   COW's page store — `ShmFile`'s pattern works for plain `MAP_SHARED` sharing (SysV shm) but not
   for UFFD-WP specifically. Fixed by backing `PageStore` with `memfd_create()` instead (shmem, not
   filesystem-backed). Any future backing-store code should stay on `memfd_create`.
2. **The wasm stack+guard region (the vmmap entry starting at page_num 0) must not be COW-shared.**
   Sharing it breaks Asyncify's fork/rewind process (the child never reaches user code). Excluded by
   an explicit `page_num == 0` check in `fork_share_entry`. This also has no real COW upside — a
   stack diverges between parent and child almost immediately regardless.
3. **Small vmmap entries (roughly sub-1-MiB — wasm data segment, dylink GOT/table copies, and
   similar) are also unsafe to COW-share as implemented**, distinct from the stack issue: forking a
   *second* time from a cage that had already COW-shared one of these small regions crashes (a raw
   SIGSEGV, not caught by any check), even in scenarios with no writes at all. Root cause not fully
   diagnosed — worked around with a conservative `addr_len >= 1 MiB` floor in `fork_share_entry`,
   which is also a reasonable permanent design choice on its own merits (COW's benefit only matters
   for large regions; eager-copying a few hundred KB is already cheap, so there's no performance
   reason to accept the risk). If this floor is ever lowered, re-investigate the second-fork crash
   first rather than assuming it was purely a Milestone-1-scope artifact.

Known simplifications carried into later milestones (documented in `cow.rs`'s module doc comment):
whole-vmmap-entry COW granularity (not per-4KB-page — a write anywhere in a shared entry
materializes the whole entry; correct but not maximally efficient, see `cow-design.md` §18/§20 and
plan Phase 8). Benchmarked consequence: fork+exec/fork+read-only are large wins (~70%/~44% latency
reduction at 512 MiB touched), but fork+write is currently *slower* than eager (~2-2.5x at 512 MiB)
because every write, however small, re-copies the whole entry on top of the promotion copy already
paid at fork time -- fixing this is the highest-value follow-up, tracked as Phase 8.

## Milestone 2 implementation notes (post-implementation)

Milestone 2 (recursive fork, narrow pass) is done: `m2_recursive_chain.c` and `m2_diamond_siblings.c`
both pass under `LIND_FORK_MEMORY=cow`. The plan's exit criterion also asked for an audit confirming
no leaked/never-decremented refcounts across generations -- implemented as `CowManager::audit()`
(walks every live cage's vmmap, counts `Cow(id)` references, compares against each backing's
recorded `refcount`), exposed via `LIND_COW_AUDIT=1` alongside `LIND_COW_STATS=1`.

Building that audit surfaced a real gap, now fixed: `cage::cage_finalize` removes a cage from
`CAGE_MAP` (dropping its `Vmmap`) as soon as that cage truly exits -- not just at whole-process
shutdown, but per-cage, immediately. Milestone 1 had no hook into this at all, so a backing's
`refcount` only ever grew across repeated forks, never reflecting cages that had already exited --
worse than the "just a leak" framing in the Milestone 1 notes above, since it also left
`fault_routes` entries pointing at host address ranges the OS is free to reuse for an unrelated
later cage. Fixed with `CowManager::on_cage_exit`, called from `cage_finalize` right before
`remove_cage` (needs the cage's vmmap still present). This is still not full Milestone 3 teardown
(doesn't run for `exec()`, never recycles a page-store slot once its refcount hits 0), but it keeps
`refcount`/`fault_routes` accurate as cages come and go, which is what Milestone 2's audit needed to
be meaningful at all.

With that fix, the audit shows the mechanism working correctly: for both the recursive-chain and
diamond-siblings scenarios, 2 of the 3 backing objects created reach `refcount == 0` (matching zero
live references) once their owning cages (the fork children) exit. The one remaining "mismatch" at
full-process end is the top-level cage's own backing, and is a benign artifact of hook placement,
not a bookkeeping bug: `dump_stats_if_requested` (called from `lind-boot/main.rs` right after
`execute_wasmtime` returns) runs slightly before that same cage's own `cage_finalize` completes --
a timing detail of the runtime's exit sequence, unrelated to COW. Re-verify this doesn't hide a real
bug if `dump_stats_if_requested`'s call site ever moves.

Full existing test suite (`process_tests`, both `LIND_FORK_MEMORY=eager` and `=cow`) re-run clean
after the `cage_finalize` change: 55/57 in both modes, same 2 pre-existing environment-only
failures, no regression from touching a shared exit path used by every fork/exit test.

---

Companion to `cow-design.md` (revision 2). This document sequences the work into four milestones,
matching the shape proposed in discussion:

1. Basic COW working for the basic fork scenario
2. Recursive fork works
3. Fix remaining bugs in lind core scenarios
4. Fix grates if affected

The ordering is sound as a milestone structure — it front-loads validation of the design's core
differentiator (recursive fork survives without host-fork-level COW) before spending time on
hardening, and defers grates since they mostly bottom out through the same RawPOSIX code paths
any cage uses. Three refinements are folded in below, called out where they apply:

- **Milestone 1 is explicitly single-threaded-cage scope.** `SharedMemory` shares one raw base
  pointer across all of a cage's threads with no host-level synchronization beyond guest-emitted
  atomics (`shared_memory.rs`, design doc §17) — a COW remap racing a sibling thread's direct
  load/store is a real, confirmed hazard, not a hypothetical one. Multi-threaded-cage safety is
  its own gated item in Milestone 3.
- **"Recursive fork works" (Milestone 2) is split into a narrow pass and a realistic pass.** The
  narrow pass (synthetic, direct-write-only children) validates the refcounted-backing-object
  architecture and can land right after Milestone 1. The realistic pass — recursive fork where
  intermediate generations actually run normal guest code (`malloc`, more `mmap`, `mprotect`)
  before forking again — depends on Milestone 3's syscall-guard integration and is listed there
  as an exit criterion, not assumed done in Milestone 2.
- **Milestone 3 names the syscall-guard integration explicitly** (design doc §9.2) rather than
  leaving it implicit under "bugs" — it's the largest net-new engineering piece in the whole
  project: without it, any guest program that calls `malloc`/`mmap`/`mprotect`/`brk` after a fork
  (i.e. almost all of them) can desync COW metadata from real host mapping state.

Each milestone lists: scope/exclusions, concrete steps, exit criteria, and tests. Section
references (`§N`) point into `cow-design.md`.

---

## Milestone 0 (parallel, non-blocking): spike the open blocking questions

These three items from design doc §30 should be answered experimentally *before* committing to
the Milestone 1-3 approach, because a negative answer to the first one forces a redesign of §9.2:

1. **Does `mprotect_syscall`'s real `libc::mprotect()` call preserve `UFFDIO_WRITEPROTECT` PTE
   state?** Write a standalone test: `mmap(MAP_SHARED)` a memfd page, `UFFDIO_WRITEPROTECT`
   register it, `mprotect(PROT_READ)` then `mprotect(PROT_READ|WRITE)` on the same range from
   outside UFFD, confirm a subsequent write still faults into UFFD instead of just succeeding.
   Run on whatever kernel version(s) Lind's CI/target actually run.
2. **Lock-ordering for `materialize_unique` called reentrantly from inside `mprotect_syscall`**
   (which already holds `cage.vmmap.write()`, design doc §16). Sketch the actual lock hierarchy
   (`BackingMeta` per-id lock vs. `cage.vmmap` per-cage lock) and confirm no path can deadlock —
   this needs to be settled before Milestone 3, but the answer shapes how `CowManager`'s API is
   shaped from Milestone 1 onward, so do this early.
3. **Identify the actual code path that tears down a cage's vmmap on `exec()`/exit.** Needed for
   Milestone 3's refcount-release hook (§14); a quick codebase lookup, not a design question.

None of these block starting Milestone 1's plumbing work, but all three should have answers
before Milestone 1's exit criteria are signed off.

---

## Milestone 1: Basic COW for the basic fork scenario

**Scope.** Single-threaded cages only. No guest `mmap`/`munmap`/`mprotect`/`brk`/`mremap` calls
touching a COW-shared range between fork and either side's first write or exit (i.e. §9.2's
syscall-guard trigger is not implemented yet — if a test needs it, it belongs in Milestone 3).
`LIND_FORK_MEMORY=eager` remains the default; COW is opt-in via env var throughout this milestone.

### Steps

1.1. **Data model.** Add `MemoryBackingType::Cow(BackingId)` to `vmmap.rs`. Add `PageStore`
     (modeled on `ShmFile`, `shared.rs:57-78`) and `CowManager`/`BackingMeta` skeleton in the
     `cage` crate (design doc §4).

1.2. **Fallback switch.** `LIND_FORK_MEMORY=eager|cow` env var. At startup, if `cow` is requested,
     probe for `memfd_create`/`userfaultfd(UFFD_USER_MODE_ONLY)`/`UFFDIO_WRITEPROTECT` support;
     fall back to `eager` with a log line if any probe fails (§23).

1.3. **Fork-time promotion + share, no writes yet.** Modify `fork_vmmap`'s `MAP_PRIVATE` branch
     (`memory.rs:121-157`) to lazily promote `Anonymous` entries to `Cow(id)` on first share
     (one-time bulk copy into the page store, §7 steps 1-2), then share into the child via
     `mmap(MAP_FIXED, MAP_SHARED)` (§7 steps 3-4). Leave the `PROT_NONE`-skip and
     `MAP_SHARED`-`mremap` branches untouched. For this step, **write-protect and trap** on any
     write to a `Cow`-backed range (no real UFFD split logic yet) — this isolates "is the sharing
     itself correct" from "does the split work" as two separable bugs.

1.4. **Read-path test.** Parent initializes N MiB of private memory, forks, child scans (reads
     only) all N MiB. Assert: data identical to parent; `fork_vmmap` does not call
     `process_vm_writev` for the promoted range (instrument with a counter); no crash from the
     write-trap since nothing writes.

1.5. **UFFD-WP write path.** Implement `materialize_unique` (§9) for the direct-write/fault
     trigger only (§9.1): pager thread, `UFFDIO_API` negotiation, fault → vmmap-entry lookup via
     the existing `translate_vmmap_addr`/`check_and_convert_addr_ext` path
     (`memory.rs:199-242`) → `BackingId` → copy-on-shared/unprotect-on-unique → remap → wake.

1.6. **Write-path test matrix.** Child writes first / parent writes first / both write different
     pages / both write the same page — byte-for-byte diff against the `eager` baseline for every
     case. Also verify: after a full split (`refcount` back to 1 on both sides), no further UFFD
     faults occur for that range (i.e. unprotection actually happened, §9.5).

1.7. **Instrumentation.** Add counters (bytes copied at promotion time, `materialize_unique`
     invocation count, fork wall-clock) so Milestone 1's benchmark comparison (design doc §26
     cases A-D) is measurable, not just pass/fail.

### Exit criteria

- `LIND_FORK_MEMORY=cow` passes the full test matrix in 1.4/1.6 for single-threaded cages with no
  intervening memory-syscall activity.
- Fork+exec and fork+read-100% (§26 cases A/B) show measurably reduced copied-bytes and fork
  latency vs. `eager`, isolated from the grate-arena floor (see Milestone 3 item 3.8 — if that
  hasn't landed yet, note the confound explicitly in the benchmark writeup rather than let it
  pollute the comparison).
- Milestone 0 items 1 and 2 have concrete answers, since Milestone 3 depends on them.

---

## Milestone 2: Recursive fork works (narrow / architecture-validation pass)

**Scope.** Still single-threaded cages, still no intervening vmmap-mutating syscalls between
forks — this milestone exists specifically to validate that Lind-owned backing-object identity
(as opposed to relying on host `fork()`/`MAP_PRIVATE` COW, §29) actually survives multiple
generations, which is the core reason this design was chosen over the simpler alternatives.

### Steps

2.1. Test: `A` writes to X% of its private memory (synthetic direct writes only — no
     `malloc`/`mmap` calls in between), `A` forks `B`, `B` writes to a further X% (direct writes),
     `B` forks `C`. Verify: `A`, `B`, `C` see mutually-consistent, correct data at every generation;
     no full materialization occurs at the `B → C` fork step for ranges `B` never wrote to
     (assert via the invocation counters from 1.7).

2.2. Test: diamond/fork-tree shape — `A → B`, `A → C` (siblings, not a chain). Verify sibling
     forks of the same parent correctly share/independently diverge without cross-contaminating
     each other's `refcount`/backing state.

2.3. Test: a grandchild that never diverges a given range still reads the *original* backing
     object correctly, even after intermediate generations have each independently diverged
     different ranges (validates §12's core claim: "every page, including `Q2`, still has a Lind
     page-store identity").

### Exit criteria

- 2.1-2.3 pass under `LIND_FORK_MEMORY=cow`, matching `eager`-mode output exactly.
- Backing-object/refcount bookkeeping is confirmed correct across ≥3 generations and a
  non-linear fork tree, with no leaked/never-decremented refcounts (add an assertion or
  end-of-test audit that walks `CowManager`'s backing table and checks it against live vmmap
  entries).

**Do not treat this milestone as validating recursive fork under realistic workloads** — that
requires Milestone 3's syscall-guard integration and is re-tested there (item 3.5).

---

## Milestone 3: Fix remaining bugs in lind core scenarios

This is the largest milestone; it's where the design closes the gap between "works for a
synthetic direct-write-only test" and "works for real POSIX programs." Sub-items are listed in
recommended order, not strict dependency order except where noted.

### 3.1 Same-cage multi-thread safety (blocks lifting the Milestone 1 single-thread restriction)

Prove/implement a mapping-replacement sequence for `materialize_unique`'s `MAP_FIXED` remap step
that's safe against sibling threads of the same cage concurrently reading/writing the range being
swapped (§17). This is the item flagged as most structurally uncertain in the design doc, alongside
3.2's lock-reentrancy question — prototype and stress-test in isolation before wiring it into the
full pipeline.

Test: pthread'd cage, N threads hammering reads on a range while another thread/process triggers a
split on it; TSan or equivalent race detector run if feasible.

### 3.2 vmmap-mutating-syscall integration (§9.2) — the core new piece

Wire the `materialize_unique` guard call into `mmap_syscall` (`MAP_FIXED` case), `munmap_syscall`,
`mprotect_syscall`, `mremap` emulation, and `brk` shrink, per design doc §9.2. Confirm the
reentrancy/locking answer from Milestone 0 item 2 holds in the real handler code, not just the
sketch.

Tests:
- Guest `munmap`s a `Cow`-shared range on one side only; verify the other side is unaffected and
  retains a correct, still-shared reference.
- Guest `mmap(MAP_FIXED)` overwrites a `Cow`-shared range; verify old backing's refcount drops
  correctly and the new mapping behaves like any fresh private mapping.
- Guest `mprotect`s a `Cow`-shared range between `PROT_READ` and `PROT_READ|WRITE`; verify (per
  Milestone 0 item 1's answer) UFFD-WP registration survives and a subsequent write still splits
  correctly.
- Guest `brk` growth/shrink interacting with a `Cow`-shared heap tail.

### 3.3 Concurrent multi-cage writers

N-way concurrent writers (parent + multiple fork children) racing on the same backing (§11);
confirm `materialize_unique`'s lock-and-revalidate structure resolves correctly under real
scheduling, not just the two-party case already covered in Milestone 1.

### 3.4 `exec()`/exit() teardown

Hook the call site identified in Milestone 0 item 3 to decrement `Cow` refcounts and recycle
page-store slots (§14). Test: `fork()` immediately followed by `exec()` in the child releases the
parent's shared range's refcount correctly without the parent ever seeing a change; verify no
page-store slot leak across repeated fork+exec cycles (long-running loop + RSS/slot-count check).

### 3.5 Recursive fork, realistic pass

Re-run Milestone 2's scenarios (2.1-2.3), but replace the synthetic direct-write generations with
real guest workloads — programs that call `malloc`/`free`/`mmap`/`mprotect` between forks. This is
where "recursive fork really works" gets validated for anything resembling actual Lind workloads,
and it's gated on 3.2 being correct.

### 3.6 Full core test-suite sweep

Run the existing `make test` / lind core test suite (per `tool_testing_ci` conventions already
used in this project) under `LIND_FORK_MEMORY=cow` end-to-end. Triage and fix regressions. This is
the literal "fix remaining bugs in lind core scenario" step — everything above is what makes this
sweep meaningful rather than immediately red.

### 3.7 VMA fragmentation measurement

Under the workloads exercised in 3.5/3.6, measure `/proc/<pid>/maps` VMA counts (§19) for
long-running, heavily-forking, heavily-diverging processes. Decide whether extent-coalescing
(§20) is needed now or can be deferred to a later optimization pass — don't build it speculatively
before there's a measured problem.

### 3.8 (parallel, non-blocking, cheap win) Grate-worker-arena fork-cost fix

Independent of COW: lazily reserve or otherwise avoid unconditionally copying the ~256 MiB
static-cage grate-worker-stack arena on every fork (design doc §0.5, "Phase 0"). Can be done any
time — recommend landing it early/in parallel with Milestone 1 or 2, purely because it removes a
large confound from every benchmark comparison run afterward. Not a correctness dependency for
any other milestone.

### Exit criteria

- 3.1-3.4 individually pass their targeted tests.
- 3.5 passes with realistic (non-synthetic) recursive-fork workloads.
- 3.6's full core suite is green under `LIND_FORK_MEMORY=cow`.
- 3.7 has a documented VMA-count result and an explicit decision (needed now / defer) rather than
  an unmeasured assumption.
- At this point `LIND_FORK_MEMORY=cow` is a candidate for becoming the default, pending Milestone
  4 (grates are part of "lind core" broadly, but isolated here since they have their own
  known-issue surface, see below).

---

## Milestone 4: Fix grates if affected

Grates have no architectural distinction from regular cages (any cage can register as a grate),
so most grate behavior should reduce to "a cage that forks," already covered by Milestones 1-3.
The items below are the specific places grates diverge from a plain cage and need direct
verification rather than being assumed fine by analogy.

### 4.1 Confirm backup-VMContext pooling doesn't go through `fork_vmmap` at all

Grate backup VMContexts are (per existing investigation of issue #961) created by instantiating
multiple times into the *same* Wasmtime `Store` at grate startup (`run.rs`), not by forking. If
confirmed, COW is simply not in the picture for pooling and this reduces Milestone 4's scope —
verify by reading the actual `run.rs` setup path rather than assuming.

### 4.2 Grate cage self-fork

Test a grate cage itself calling `fork()` (e.g. spawning a child grate). Should reduce to the
same path already covered by Milestones 1-3; confirm with an explicit test rather than inferring
from the regular-cage tests passing.

### 4.3 Audit cross-cage memory primitives that bypass the normal syscall path

3i's `copy_data_between_cages` (used by grates acting on another cage's behalf) writes directly
into a destination cage's memory range outside the `mmap`/`munmap`/`mprotect`/`brk` syscall
handlers Milestone 3.2 instrumented. If its destination can be a `Cow`-shared range, it needs its
own `materialize_unique` guard call — check whether this is reachable in practice (e.g. a grate
writing into a cage's buffer that also happens to still be COW-shared with a fork sibling) and add
the guard if so.

### 4.4 Distinguish COW-induced races from the known #961 Store-aliasing bug

Issue #961 (grate `Store` aliasing under concurrent callback dispatch) produces corruption
symptoms that could superficially resemble a COW-related race. When triaging any new grate
concurrency failure under `LIND_FORK_MEMORY=cow`, first reproduce with `LIND_FORK_MEMORY=eager`
as a control — if it still fails, it's #961 or a pre-existing issue, not new COW-related work.

### 4.5 Regression run: existing grate stress tests

Run the existing `fdtables-test-grate` suite (`/home/lind/lind-wasm-example-grates`,
`test-rust-fdtables` branch — the canonical grate concurrency/fork stress test used to validate
the #961 fix) under `LIND_FORK_MEMORY=cow` as a regression check.

### Exit criteria

- 4.1's assumption is confirmed or the pooling path is brought into scope with its own plan.
- 4.2-4.5 pass, with 4.4's control methodology documented so future grate bug reports can be
  triaged consistently.

---

## Cross-cutting practices (apply throughout, not tied to one milestone)

- **Keep `eager` as the default** until Milestone 3's exit criteria are met; only flip the default
  after that, and keep the env var available indefinitely as a bisection tool (§23).
- **Benchmark at every milestone boundary**, not just at the end (§26) — catching a regression
  introduced in Milestone 2 is much cheaper than finding it after Milestone 4.
- **Re-run Milestone 0's kernel-behavior spike** if the target kernel version ever changes (CI
  image bump, deployment environment change) — its answer is an environmental fact, not a
  one-time constant.
