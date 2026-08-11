# cow-tests

Tests and benchmarks for the linear-memory COW fork effort
(`/cow-design.md`, `/cow-implementation-plan.md`). Kept as a dedicated
top-level directory rather than folded into `tests/unit-tests/` since
`scripts/test/harnesses/wasmtestreport.py` hardcodes its discovery root to
`tests/unit-tests/` and these tests have their own lifecycle (written
before implementation starts, re-run at every milestone, eventually
retired or promoted once COW ships) that doesn't fit that harness's
model. Both subfolders below have their own small, self-contained
runner script that reuses the same `lind_compile`/`lind_run` tools
(and, for benchmarks, `tests/benchmarks/bench.c`) the main test suite
uses.

## Layout

```
cow-tests/
  common/cow_test.h            shared fill/verify helpers for correctness tests
  correctness/                 deterministic tests, run_correctness_tests.py
  benchmarks/                  bench.h-based benchmarks, run_benchmarks.py
  baseline/                    recorded benchmark numbers + analysis (main branch, eager copy)
```

## Correctness tests (`correctness/`)

```
./run_correctness_tests.py                   # run all
./run_correctness_tests.py m1_write_child_first m2_recursive_chain
```

Each test compiles natively and via `lind_compile`, runs both, and
requires exit code 0 + matching stdout on both sides (same convention as
`tests/unit-tests/*/deterministic/`). Native fork already has real COW at
the OS level, so it's a correct oracle for these tests regardless of how
Lind's fork is currently implemented -- none of these tests can observe
*whether* a copy happened internally, only whether the POSIX
inheritance/isolation semantics fork() must provide are preserved
(`cow_test.h`'s top comment explains this in more detail).

These tests are written to pass **today**, under the current eager-copy
fork, as a regression baseline. They should keep passing, unmodified,
through every milestone of `cow-implementation-plan.md` -- if one starts
failing after a COW change, that's a real correctness regression, not an
expected/needed update to the test.

Tests are named by which milestone first requires them to matter (though
all of them run and must pass from day one, since they encode invariants
`cow-design.md` §24 says must hold at all times, not just from some
milestone onward):

| File | Milestone | What it checks |
|---|---|---|
| `m1_read_only_scan.c` | 1 | child read-only scan of inherited memory sees identical data, doesn't perturb parent (`cow-design.md` §8) |
| `m1_write_child_first.c` | 1 | child diverges first; parent's original data survives (§9, Invariant 3) |
| `m1_write_parent_first.c` | 1 | parent diverges first (pipe-ordered); child still sees pre-divergence data (§10 -- "no special parent-owns-the-original-page rule") |
| `m1_write_different_pages.c` | 1 | parent and child dirty disjoint sub-ranges of the same inherited buffer without cross-contaminating |
| `m1_fork_exec.c` | 1 / 3.4 | fork() then exec() in the child; parent's memory intact afterward (§14, §26 case A -- the headline workload) |
| `m2_recursive_chain.c` | 2 (narrow) | A->B->C, each diverging a disjoint 5-10% via direct writes only; grandchild still sees the *original* ancestor data for untouched ranges (§12) |
| `m2_diamond_siblings.c` | 2 (narrow) | A->B and A->C from the same unmodified parent state, sequential, no cross-contamination |
| `m3_munmap_after_fork.c` | 3.2 | guest `munmap()` of a still-shared range after fork, before any direct write |
| `m3_mmap_fixed_overwrite_after_fork.c` | 3.2 | guest `mmap(MAP_FIXED)` replacing part of a still-shared range |
| `m3_mprotect_toggle_after_fork.c` | 3.2 / Milestone 0 item 1 | guest `mprotect()` toggling R/RW on a still-shared range before writing |
| `m3_growth_not_inherited_by_existing_child.c` | 3 (Invariant 6, broadened) | memory mapped by the parent *after* an earlier fork isn't visible to that already-existing child, and a later fork correctly inherits it |
| `m3_threaded_cage_fork.c` | 3.1 | same-cage sibling thread touching a disjoint range while the main thread diverges a post-fork shared range (deliberately not an unsynchronized data race -- see the file's comment) |
| `m3_concurrent_sibling_writers.c` | 3.3 | 3 children forked back-to-back (no barrier) all diverging the same original backing; parent's view stays correct regardless of interleaving |

**Milestone 4 (grates)** intentionally has no dedicated test here.
`cow-implementation-plan.md` Milestone 4 item 4.5 calls for reusing the
existing `fdtables-test-grate` suite
(`/home/lind/lind-wasm-example-grates`, `test-rust-fdtables` branch) as
the grate regression check under `LIND_FORK_MEMORY=cow` -- that suite
already lives in a separate repo with its own build/run tooling, and item
4.4 specifically calls for reusing it (with an `eager` control run) to
distinguish new COW bugs from the pre-existing #961 Store-aliasing issue,
rather than a new bespoke test here.

## Benchmarks (`benchmarks/`)

```
./run_benchmarks.py                          # run all, print a table (dynamic/dylink build)
./run_benchmarks.py --static                 # same, but -s (static/non-dylink) build
./run_benchmarks.py --out results.csv        # also write CSV
./run_benchmarks.py cow_fork_exec            # run one
```

Each benchmark sweeps `{0, 4, 32, 128, 512}` MiB of parent memory made
resident before forking (`0` isolates the fixed per-fork floor from the
cost of copying touched memory) and prints tab-delimited rows via
`tests/benchmarks/bench.c`'s `emit_result`, same convention as
`tests/benchmarks/`.

**Run both `--static` and the default (dynamic) mode** -- they have very
different fixed floors. The default dynamic/dylink build is what every
other test in this repo other than `static_tests/` uses; the `-s` static
build is the one that unconditionally maps the ~256 MiB grate-worker-stack
arena `cow-design.md` §0.5 flags (`instance.rs`'s non-dylink instantiation
path), so `--static` is the mode that actually exercises
`cow-implementation-plan.md` Milestone 3.8. See `baseline/README.md`
finding #2 for the measured gap (dynamic ~27-29ms floor vs. static
~66-70ms floor at MiB=0/4, i.e. ~2.3-4.4x).

| File | `cow-design.md` §26 case | What it measures |
|---|---|---|
| `cow_fork_exec.c` | A, F | fork() + exec() in the child |
| `cow_fork_read.c` | B | fork() + child reads 100%, writes nothing |
| `cow_fork_write.c` | C, D | fork() + child writes 1% / 100% (two rows per size) |
| `cow_fork_recursive.c` | E | A->B->C, each generation dirtying 5% |

**Read `baseline/README.md` before trusting any benchmark number from
this suite**, especially finding #1: a naive `memset`-based touch gets
silently eliminated by the compiler since the destination is never read
back afterward, which produced a completely flat (and wrong) result
during initial calibration. `cow_bench.h`'s `cow_bench_touch` works around
this with `volatile` stores; if that helper is ever changed, re-verify
that cost still scales with size before trusting new numbers.

## Baseline (`baseline/`)

Benchmark results recorded against `main` (commit `0ca74ce77`) before any
COW implementation work, using the current eager-copy fork
(`fork_vmmap()`, `cow-design.md` §3, §7). See `baseline/README.md` for the
full numbers, environment details, and five specific findings to keep in
mind when comparing a future COW-enabled run against this baseline --
most importantly, that `ForkWrite1pct` and `ForkWrite100pct` cost almost
exactly the same today (~1.00x ratio), which is the clearest single
before/after signal for whether a COW implementation is actually working.
