# COW fork baseline: main branch (eager copy)

Baseline benchmark numbers for `tests/cow-tests/benchmarks/`, recorded against
`main` **before** any COW implementation work (`cow-design.md`,
`cow-implementation-plan.md`) has landed. This is the reference eager-copy
implementation described in `cow-design.md` §0.4/§3 (`fork_vmmap()`,
`src/cage/src/memory/memory.rs:85-165`): per-vmmap-entry `process_vm_writev`
for `MAP_PRIVATE` regions, zero-copy `mremap` for `MAP_SHARED` regions.

Recorded in **both build modes**, since they behave very differently (see
finding #2 below) -- run with:

```
python3 tests/cow-tests/run_benchmarks.py --out results.csv            # dynamic/dylink build (default)
python3 tests/cow-tests/run_benchmarks.py --static --out results.csv   # static/non-dylink build (-s)
```

## Environment

| | |
|---|---|
| Commit | `0ca74ce771a913199e00d190cd0a322a2b6501d4` (main) |
| Date | 2026-08-11 |
| OS | Linux 6.8.0-124-generic (Ubuntu 22.04 base image), x86_64 |
| CPUs | 24 |
| RAM | 62 GiB |
| Toolchain | prebuilt `scripts/bin/lind_compile` / `lind_run` already present under `build/` (not rebuilt for this baseline) |

Two independent runs are recorded per build mode
(`main_0ca74ce77_dynamic_run{1,2}.csv`, `main_0ca74ce77_static_run{1,2}.csv`)
plus merged/averaged files (`main_0ca74ce77_{dynamic,static}_avg.csv`) to give
a sense of run-to-run noise (roughly 5-15%, consistent with microbenchmark
variance from process/scheduling jitter -- not a signal in itself).

## How to compare a future COW run against this baseline

Once `LIND_FORK_MEMORY=cow` (or equivalent) exists (`cow-design.md` §23),
re-run the identical command **in the same build mode** and diff against the
matching `_avg.csv`. The benchmark code itself must not change between runs
-- if a benchmark needs to change to accommodate the COW implementation,
re-run baseline first on the unmodified `main` code so the comparison stays
apples-to-apples. Since dynamic and static builds have very different fixed
floors (finding #2), never compare a dynamic run against a static baseline
or vice versa.

## Key findings from this baseline (read before trusting any future comparison)

### 1. A naive `memset`-based touch is silently dead-store-eliminated -- do not "simplify" `cow_bench_touch`

Initial calibration used a plain `memset(buf, val, size)` before the timed
fork loop, with nothing ever reading `buf` afterward. That produced a
**flat ~13-16ms fork cost regardless of size, from 0 MiB up to 512 MiB** --
the compiler (LLVM's dead-store-elimination, which specifically understands
`memset`/`llvm.memset` as an eliminable store when the destination
provably has no further observable use) removed the fill entirely, so
"512 MiB touched" and "0 MiB touched" were measuring the same untouched
memory. Switching to a `volatile`-pointer, one-store-per-page touch
(`cow_bench_touch` in `cow_bench.h`) fixed this immediately. **If
`cow_bench_touch` is ever rewritten to use `memset` "for speed," re-verify
with a quick fork-loop probe that cost still scales with size before
trusting the result.**

### 2. Static (`-s`) builds have a ~2.3-4.4x higher fixed floor than dynamic/dylink builds -- this is the grate-worker-arena effect, and it does NOT show up in a dynamic-mode run

This correction supersedes an earlier draft of this document, which
(incorrectly) assumed the default `lind_compile` invocation used by these
benchmarks was the one affected by the ~256 MiB static-cage grate-worker
arena described in `cow-design.md` §0.5. It is not -- that arena is mapped
on the **non-dylink (`instance.rs` "static-module") instantiation path**,
which the `-s`/`--static` `lind_compile` flag selects. The default
invocation (no `-s`, what every other test/benchmark in this repo other
than `static_tests/` uses) passes `-Wl,-pie` and produces a dylink-enabled
module, which takes the much smaller region-setup path
(`cow-design.md`'s Explore-agent grounding: "For dylink-enabled builds, the
equivalent initial region is much smaller... likely low single-digit MiB").

Measured floor (MiB=0/4, i.e. before the linear-in-size component becomes
significant):

| Build mode | ForkExec floor (~0-4 MiB) | ForkReadAll (4 MiB) |
|---|---:|---:|
| dynamic (default) | ~27-29ms | ~15.7ms |
| static (`-s`) | ~66ms | ~69.6ms |

That's a **~2.3-4.4x fixed-cost gap purely from build mode**, before either
side has touched any user memory, which lines up with `cow-design.md`
§0.5's claim almost exactly (the arena is copied unconditionally on every
static-cage fork regardless of whether it's ever used). **Anyone using
these benchmarks to evaluate `cow-implementation-plan.md` Milestone 3.8
(the grate-arena fix) must use the `--static` baseline** -- the dynamic-mode
numbers won't show that fix's effect at all, since dynamic cages never
allocate that arena in the first place.

Full comparison at every size (mean of 2 runs, nanoseconds):

| Size | ForkExec dynamic | ForkExec static | static/dynamic ratio |
|---:|---:|---:|---:|
| 0 MiB | 27,270,671 | 66,318,616 | 2.43x |
| 4 MiB | 28,665,245 | 66,031,103 | 2.30x |
| 32 MiB | 35,475,054 | 80,610,690 | 2.27x |
| 128 MiB | 63,118,905 | 110,814,886 | 1.76x |
| 512 MiB | 162,565,535 | 241,867,352 | 1.49x |

The ratio shrinks as size grows because the fixed arena cost becomes a
smaller fraction of the total once real user data dominates -- consistent
with "the arena is a constant added on top," not a multiplier.

### 3. Fork cost is dominated by a large, fairly flat overhead up to tens of MiB, then grows close to linearly (both build modes)

Dynamic-mode `ForkExec`: 0 MiB -> 4 MiB barely moves (27.3ms -> 28.7ms);
4 MiB -> 512 MiB grows ~5.7x. Static-mode `ForkExec`: 0 MiB -> 4 MiB is
flat (66.3ms -> 66.0ms, within noise); 4 MiB -> 512 MiB grows ~3.7x. In
both modes, **the smallest-size row is the practical measurement of that
mode's fixed per-fork floor with this toolchain/build**, and should not be
attributed to "COW-relevant" cost when evaluating a future COW run -- COW
targets the linear-in-touched-memory component, not this floor (static
mode's floor specifically is Milestone 3.8's target, per finding #2).

### 4. `ForkWrite1pct` ~= `ForkWrite100pct` today, in both build modes -- this is the number that should change most under COW

Dynamic mode (mean, ns):

| Size | ForkWrite1pct | ForkWrite100pct | ratio |
|---:|---:|---:|---:|
| 4 MiB | 15,795,673 | 16,434,765 | 1.04x |
| 32 MiB | 24,590,786 | 25,005,535 | 1.02x |
| 128 MiB | 50,737,858 | 49,183,745 | 0.97x |
| 512 MiB | 157,596,961 | 163,354,035 | 1.04x |

Static mode (mean, ns):

| Size | ForkWrite1pct | ForkWrite100pct | ratio |
|---:|---:|---:|---:|
| 4 MiB | 67,067,095 | 65,316,137 | 0.97x |
| 32 MiB | 74,969,252 | 76,829,381 | 1.02x |
| 128 MiB | 104,797,317 | 98,566,981 | 0.94x |
| 512 MiB | 219,253,318 | 203,713,768 | 0.93x |

Under eager copy, `fork()` copies the entire inherited region regardless of
how much the child subsequently writes, so writing 1% vs. 100% costs about
the same either way (`cow-design.md` §27, "child writes small working set:
full copy" vs. "child writes everything: full copy once" -- both "full
copy", hence ~1.0x here in both modes). **This ~1.00x ratio is the single
most direct benchmark signal for whether COW is working**: a correct COW
implementation should make `ForkWrite1pct` cost close to `ForkReadAll`
(dominated by the fixed floor + a small materialize cost) while
`ForkWrite100pct` should stay close to today's eager numbers (writing
everything eventually copies everything either way, `cow-design.md` §26
case D, "may be slower than eager copy" once fault/remap overhead is
included).

### 5. `ForkWrite*` write step uses a non-vectorized volatile loop, not `memset`

For the same dead-store-elimination reason as finding #1, the child's
write step in `cow_fork_write.c` also uses `cow_bench_touch` rather than
`memset`. This makes the absolute `ForkWrite*` numbers slower than a real
program's optimized `memset` would be -- **do not compare `ForkWrite*`
against `ForkReadAll` or `ForkExec` as if the write itself were free with a
real memset; only compare `ForkWrite*` between an eager run and a future
COW run of the identical binary**, where the write-loop cost is a wash.

### 6. Recursive fork cost roughly doubles-to-triples single-fork cost, as expected for a 2-hop chain

Dynamic mode: `ForkRecursiveChain` (A->B->C, each diverging 5%) costs
roughly 1.9-2.9x a single `ForkExec`/`ForkReadAll` of the same size (e.g.
512 MiB: 466ms vs. 163ms `ForkExec`). Static mode shows the same pattern
scaled up by the fixed-floor gap from finding #2 (e.g. 512 MiB: 594ms vs.
242ms `ForkExec`). This is consistent with two full-size copies happening,
once per fork. Under COW this should NOT scale with the number of
generations once B's divergence no longer requires materializing A's full
range again for C (`cow-design.md` §12); a future COW run showing
`ForkRecursiveChain` still scaling roughly linearly with generation count
in either build mode would indicate Milestone 2/3's recursive-fork sharing
isn't working as designed.

## Files

- `main_0ca74ce77_dynamic_run{1,2}.csv`, `main_0ca74ce77_dynamic_avg.csv` -- default (dylink) build
- `main_0ca74ce77_static_run{1,2}.csv`, `main_0ca74ce77_static_avg.csv` -- `-s` (static/non-dylink) build
