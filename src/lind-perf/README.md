# lind-perf

Per-layer timing of the lind-wasm syscall path: how much of one syscall is spent in the
guest, the Wasm/host transition, lind-common, 3i, RawPOSIX, fdtables, and (for interposed
calls) the grate trampoline and lind-3i worker pool.

## Normal builds contain none of this

Everything is behind the `lind_perf` cargo feature of lind-boot:

- `lind-perf` is an **optional** dependency of every crate it instruments, so a normal build
  doesn't compile or link it.
- Timing sites use `perf_timer!` / `perf_stop!`, which each crate defines twice: with the
  feature they start/stop a timer, without it they expand to **nothing**.
- The CLI flags (`--perf`, `--perf-filter`, `--perf-scope`, `--perf-top`), the per-counter
  runner and the benchmark
  syscalls (2001-2004) are all `#[cfg(feature = "lind_perf")]`.
- Guest wrappers for the benchmark syscalls live in `tests/benchmarks/lind-perf/`, not glibc.

Check a normal build:

```bash
cargo tree --manifest-path src/lind-boot/Cargo.toml -i lind-perf   # "package ID specification ... did not match"
strings build/lind-boot | grep -E 'lind_perf|lind_3i::|threei::make_syscall' # no output
build/lind-boot --perf x.cwasm                                     # clap: unexpected argument '--perf'
```

## Building and running

```bash
make lind-boot          # normal binary, used by lind_compile to precompile
make lind-boot-perf     # build/lind-boot-perf (separate target dir: src/lind-boot/target-perf)

scripts/test/run_microbench.sh            # nanoseconds
scripts/test/run_microbench.sh tsc        # CPU cycles
scripts/test/run_microbench.sh --perf-filter=threei::,lind_3i::
```

To compare machines, repeat the benchmarks and record the setup:

```bash
scripts/test/collect_microbench.sh -n 5 -c 2 -l my-server     # 5 runs pinned to CPU 2
```

It saves each run, `env.txt` (CPU, frequency settings, kernel mitigations, container and seccomp
status, cgroup limits, load) and `summary.txt`/`summary.csv` (median, min and max of every counter
across the runs) under `~/lind-perf-results/`. A large spread means a run was disturbed.

Grates must be built static (`lind_compile -s --compile-grate`, as in lind-wasm-example-grates):
lind-boot only creates a grate's worker pool for statically linked modules. Cages can be dynamic.

Or directly:

```bash
sudo build/lind-boot-perf --preload env=/lib/libc.cwasm --preload env=/lib/libm.cwasm \
    --perf[=clock|tsc] [--perf-filter=PREFIX,...] prog.cwasm [args...]
```

A perf binary run **without** `--perf` behaves like a normal run. Every timer is off, but each
timer site still costs one atomic load and a branch, so don't use the perf binary for
end-to-end measurements.

## Profiling real programs and grates (`--perf-scope=all`)

By default only the benchmark syscalls are timed. `--perf-scope=all` times **every** syscall
and adds a per-syscall table, so real programs and grates can be profiled:

```bash
# all counters (one run each), per-syscall table with the top 25 rows
sudo build/lind-boot-perf --perf --perf-scope=all usr/local/bin/grep foo /tmp/big.txt

# quicker: only the entry and 3i counters, top 40 rows
sudo build/lind-boot-perf --perf --perf-scope=all --perf-top=40 \
    --perf-filter=lind_common::syscall_entry,threei::make_syscall \
    lmbench/bin/lat_pipe -W 0 -N 1

# a grate: cage calls, the grate's own calls and what it forwards are separated
sudo build/lind-boot-perf --perf --perf-scope=all \
    grates/imfs-grate.cwasm lmbench/bin/bw_file_rd -P 1 -W 0 -N 1 512K open2close /tmp/f
```

The `PER SYSCALL` table has one row per (syscall, origin), sorted by total time in the
syscall entry:

- **origin** `cage` is a program's own call; `grate` is a grate's own call (its startup,
  fork, wait...); `fwd` is a call made *while another call is being serviced*: a grate
  forwarding a call down, `copy_data_between_cages`, etc. Grate calls run on the calling
  thread, so these show up as nested entries.
- **entry** is the whole host-side cost of the call; **3i** the 3i dispatch; **3i lookup**
  the handler-table lookup; **rawposix** the RawPOSIX handler (incl. the kernel call);
  **3i call** 3i's own calls such as `copy_data_between_cages`; **grate** the whole call
  into a grate; **grate wasm** the grate's Wasm code. `-` means the path wasn't taken.
- A cage call handled by a grate includes the grate's `fwd` calls in its **grate** column;
  the `fwd` rows show those calls on their own.

The per-layer tables above it mix all syscalls and origins, and the derived costs are
skipped (they assume one benchmark path). The program still runs once per counter, so it
must run to completion and behave the same each time (batch jobs, tests, lmbench); use
`--perf-filter` to cut the number of runs.

## How a `--perf` run works

1. The parent lind-boot starts a **fresh lind-boot process per counter**, with the same
   arguments plus `LIND_PERF_COUNTER`, `LIND_PERF_TIMER`, `LIND_PERF_RESULT`.
2. That child enables only its counter, runs the program normally, and writes
   `<total> <calls>` to the result file just before exiting.
3. The parent prints all counters grouped by layer, then the **derived** costs.

One counter per process means timers never measure each other, and every sample starts from
clean global state. If a run dies without a result (e.g. a panic), the parent stops: the other
runs execute the same program and would fail the same way.

**Timer overhead.** Every measurement includes the cost of reading the clock itself. Each run
measures that first (lowest median of 20 batches of 10k empty timings) and the report shows it, a raw `avg/call`,
and `net/call` = avg minus overhead. Derived costs use `net/call`, so subtracting several nested
timers doesn't subtract the overhead several times.

**Which syscalls are timed.** The `make-syscall` entry in lind-common records, per thread,
which syscall is being serviced and whether it is timed: with the default `--perf-scope=bench`
only syscalls 2001..=2999 (except 2004), with `--perf-scope=all` every syscall. Timers in every
layer only record while that is set, and credit their time to that syscall. Grate calls run
synchronously on the calling thread, so this covers the whole grate path. In the default scope,
startup syscalls, `printf`, `exit` and so on are never counted.

## Benchmark syscalls (perf builds only)

| # | Name | Handled in | Behaviour |
|---|---|---|---|
| 2001 | `libc_syscall` | RawPOSIX `bench_calls.rs` | argument checks + `geteuid()` in the kernel |
| 2002 | `fdtables_syscall` | RawPOSIX `bench_calls.rs` | argument checks + `close_virtualfd(-1)` (rejected by fdtables, no kernel) |
| 2003 | `null_syscall` | lind-common entry, before 3i | returns 0: guest timing = Wasm->host->Wasm round trip |
| 2004 | guest report | lind-common entry, before 3i | guest passes its loop time/iterations -> `guest::per_call` |

2001/2002 can be interposed by a grate; 2003/2004 never reach 3i.

`fork_wait.c` is not a benchmark syscall: it times `fork()` + `waitpid()` of a child that exits
at once (200 times), reports that as `guest::per_call`, and is run with only the `fork::` and
`exit::` counters. `run_microbench.sh` gives every benchmark only the counters on its path
unless you pass `--perf-filter`.

Two benchmark grates in `tests/benchmarks/lind-perf/` interpose on 2001/2002:

| Grate | Handler | Path measured | Compare with |
|---|---|---|---|
| `grate_syscall.c` | returns 0 | cage -> grate -> back (RawPOSIX and the kernel never run) | the cost of reaching a handler in a grate |
| `grate_forward.c` | forwards the call to RawPOSIX with `make_threei_call`, as a real grate does | cage -> grate -> RawPOSIX -> kernel/fdtables | the plain 2001/2002 runs: the difference in `guest::per_call` is the cost of having a grate in the path |

With `grate_forward.c` both the cage's call and the grate's forwarded call are timed, so the
report adds the per-syscall table (origin `cage` vs `fwd`) and skips the derived lines.

## What is timed

| Counter | Where | Covers |
|---|---|---|
| `guest::per_call` | guest program (`lind_perf_run`) | loop time / iterations, measured inside Wasm; always ns |
| `lind_common::syscall_entry` | lind-common `make-syscall` host function | whole host-side entry, incl. 3i and below |
| `threei::make_syscall` | `make_syscall` | whole 3i dispatch |
| `threei::make_syscall::dead_cage_check` | `make_syscall` | first dead/exiting target-cage check |
| `threei::make_syscall::handler_lookup` | `make_syscall` | `_get_handler` |
| `threei::make_syscall::rawposix_dispatch` | `make_syscall`, RawPOSIX branch | second dead-cage check + call into the RawPOSIX handler |
| `threei::make_syscall::grate_inflight_enter` | `make_syscall`, grate branch | `grate_inflight += 1` + grate-dead check |
| `threei::make_syscall::grate_inflight_exit` | `make_syscall`, grate branch | `grate_inflight -= 1` |
| `threei::make_syscall::threei_dispatch` | `make_syscall`, 3i branch | 3i's own calls: register_handler, copy_data_between_cages, copy_handler_table_to_cage |
| `threei::copy_data_between_cages` | `copy_data_between_cages` | whole cross-cage copy (checks + copy) |
| `threei::call_grate_func` | `_call_grate_func` | whole grate dispatch |
| `threei::call_grate_func::runtime_lookup` | `_call_grate_func` | `get_cage_runtime` + `get_runtime_trampoline` |
| `rawposix::libc_call` | `bench_calls::libc_syscall` | arg checks + `geteuid()` |
| `rawposix::fdtables_call` | `bench_calls::fdtables_syscall` | arg checks + `close_virtualfd` + errno |
| `fdtables::close_virtualfd` | call site in `fdtables_syscall` | the fdtables call only |
| `lind_boot::grate_callback_trampoline` | lind-boot `trampoline.rs` | whole trampoline |
| `lind_boot::grate_callback_trampoline::get_grate_handler` | `submit_grate_request` | handler lookup in `GRATE_POOL` |
| `lind_3i::submit` | `GrateHandler::submit` | whole submit |
| `lind_3i::worker_acquire` | `submit_serialized/parallel` | serial gate (serialized mode) + `take_worker_blocking` |
| `lind_3i::worker_run` | `GrateWorker::run` | stack reset + Wasm call |
| `lind_3i::worker_run::reset_stack` | `GrateWorker::run` | `reset_worker_stack()`: the instance's stack pointer set to the top of its slot |
| `lind_3i::worker_run::pass_fptr_to_wt_call` | `GrateWorker::run` | just `func.call(…)`, the typed `pass_fptr_to_wt` call: enter grate Wasm, dispatch, handler, return |
| `lind_3i::worker_release` | `WorkerLease` drop | `return_worker` |
| `span::3i_trampoline_call_to_grate_wasm_call` | span: 3i `_call_grate_func` → lind-3i `GrateWorker::run` | from 3i calling the grate trampoline (`(trampoline)(…)`) to lind-3i about to call the grate's Wasm (`func.call(…)`): the way into a grate |
| `span::grate_wasm_return_to_3i` | span: lind-3i `GrateWorker::run` → 3i `_call_grate_func` | from the grate's Wasm call returning to 3i getting the trampoline's result: the way back out |

**fork and exit** (lind-multi-process, and `copy_memory` in the cage crate). These record on
every fork and exit while their counter is enabled, not only during benchmark syscalls (fork
builds the child on a new thread, which the syscall entry never sees). The `fork_wait` benchmark
runs them.

| Counter | Where | What it covers |
|---|---|---|
| `fork::parent_setup` | `fork_call` | parent: up to arming the Asyncify unwind |
| `fork::parent_setup::snapshots` | `fork_call` | the linker and global snapshots taken for the child |
| `fork::parent_unwind` | span: `fork_call` → unwind callback | the parent's Wasm stack unwinding |
| `fork::parent_callback` | unwind callback | start the child's thread, wait for it, start the rewind |
| `fork::parent_callback::thread_spawn` | unwind callback | creating the child's OS thread |
| `fork::parent_callback::wait_for_child` | unwind callback | waiting until the child has its copy of the memory |
| `fork::parent_rewind` | span: unwind callback → lind-common `clone` rewind | the parent's Wasm stack rewinding into `fork()` |
| `fork::child_setup` | child thread | from thread start to the barrier: what the parent waits for |
| `fork::child_setup::new_store` | child thread | new Wasmtime store and host context |
| `fork::child_setup::new_child_linker` | child thread | the child's linker from the parent's snapshot |
| `fork::child_setup::instantiate` | child thread | instantiating the module for the child |
| `fork::child_setup::instantiate::copy_memory` | cage `fork_vmmap` | copying the parent's mapped memory into the child |
| `fork::child_rewind_to_exit` | span: child `_start` → `exit_call` | the child rewinding into `fork()` and running until `exit` |
| `exit::exit_call` | `exit_call` | arming the Asyncify unwind for exit |
| `exit::unwind` | span: `exit_call` → exit callback | the Wasm stack unwinding |
| `exit::cleanup` | exit callback | thread exit, `cage_finalize`, removing the VMContext |
| `exit::cleanup::cage_finalize` | exit callback | zombie record and SIGCHLD (lets the parent's `waitpid` return), fdtable and cage removal |

The report skips counters with no calls (e.g. grate counters in a direct run).

## Derived costs

Printed after the table when all their counters were measured, as outer avg minus nested avgs:

- guest side (glibc wrapper + Wasm<->host transition) = `guest::per_call` - `syscall_entry`
- lind-common glue = `syscall_entry` - `make_syscall`
- 3i own work (direct / grate path) = `make_syscall` - its timed parts
- 3i->RawPOSIX hop = `rawposix_dispatch` - `libc_call` / `fdtables_call`
- RawPOSIX fdtables handler excl. fdtables = `fdtables_call` - `close_virtualfd`
- `call_grate_func` own work, trampoline own work, lind-3i submit bookkeeping,
  worker run excl. Wasm call
- fork: parent setup excl. snapshots, parent callback excl. spawn and waiting, child setup
  excl. store/linker/instantiate, instantiate excl. copying memory; exit cleanup excl.
  `cage_finalize`

Caveats: each counter comes from a separate run, so differences include run-to-run noise
(small negative values mean "within noise of zero"); with `tsc`, lines mixing
`guest::per_call` (ns) with cycle counters are skipped.

**Spans** time an interval that starts in one function or crate and ends in another, on
the same thread: `perf_span_start!(COUNTER)` at the start and `perf_span_end!(COUNTER)` at the
end (the counter is defined once, e.g. in threei, and re-exported by the other crate's `perf`
module). Grate calls run on the calling thread from 3i down to the grate's Wasm, so spans across
3i, lind-boot and lind-3i are safe. One span is open per thread at a time. For paths outside a
benchmark syscall (fork, exit) use `perf_timer_any!` / `perf_span_start_any!`, which record
whenever the counter is the enabled one.

## Adding a timer

1. In the crate's `perf.rs`, add `pub static FOO: Counter = Counter::new("<layer>::foo");`
   and list it in `ALL_COUNTERS`.
2. At the site: `perf_timer!(_t_foo = FOO);` (stops at end of scope) and, to stop earlier,
   `perf_stop!(_t_foo);`.

For a new crate: add `lind-perf = { ..., optional = true }`, a `lind_perf = ["dep:lind-perf"]`
feature, the two macros at the top of `lib.rs` (copy from threei) plus `#[cfg(feature =
"lind_perf")] pub mod perf;`, enable it from lind-boot's `lind_perf` feature, and add its
`ALL_COUNTERS` to `all_counters()` in `src/lind-boot/src/perf.rs`.
