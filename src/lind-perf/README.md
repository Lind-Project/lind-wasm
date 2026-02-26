# lind-perf

`lind-perf` is a microbenchmarking library for lind-wasm. It generates timing reports for hot
paths in the syscall lifecycle by measuring the total time spent in specific functions across
modules.

Sample output for running `close(-1)`:

```bash
FDTABLE Test    ................
--------------------------------------------LIND-BOOT--------------------------------------------
name                                                              calls        total          avg
-------------------------------------------------------------------------------------------------
lind_boot::load_main_module                                           1    111.482ms    111.482ms
lind_boot::invoke_func                                                1    111.282ms    111.282ms

-------------------------------------------LIND-COMMON-------------------------------------------
name                                                              calls        total          avg
-------------------------------------------------------------------------------------------------
lind_common::add_to_linker::make-syscall                        1000000     94.274ms     94.000ns

---------------------------------------------THREEI----------------------------------------------
name                                                              calls        total          avg
-------------------------------------------------------------------------------------------------
threei::make_syscall                                            1000000     90.815ms     90.000ns

--------------------------------------------RAWPOSIX---------------------------------------------
name                                                              calls        total          avg
-------------------------------------------------------------------------------------------------
rawposix::close_syscall                                         1000000     21.255ms     21.000ns

--------------------------------------------FDTABLES---------------------------------------------
name                                                              calls        total          avg
-------------------------------------------------------------------------------------------------
fdtables::close_virtualfd                                       1000000     14.372ms     14.000ns
```

## Building

`lind-perf` is only included in the final binary if `--features lind_perf` is set during build.

`make lind-boot-perf` is a shorthand for building a `release` version of `lind-boot` with `lind-perf` enabled.

## Running Benchmarks

`lind-perf` will generate a report for any module that is run using `lind-boot` with the
`--perf` or `--perftsc` flag.

e.g. `sudo lind-boot --perf libc_syscall.wasm`

Standard benchmarks can be run using: [`./scripts/run_microbench.sh`](../../scripts/run_microbench.sh)

Flags:
- `--perf`: Uses the default Clock timer (nanoseconds)
- `--perftsc`: Uses the `rdtsc` timer (CPU cycles)

## Internals

### How the timer works
Each benchmark site is a `Counter`. A counter tracks:
- total elapsed time across calls
- number of calls

Timing is scoped. The common pattern is:
1. Create a guard at the start of the function.
2. The guard records the start time immediately.
3. When the function returns, the guard is dropped and records the end time.
4. The elapsed time is added to the counter total and the call count increments.

This means early returns are timed as well. If the guard is dropped before the work
finishes (e.g., because of a `return foo(...)` expression), the measurement will be too
small. Keep the guard alive until after the work:

```rust
let _scope = perf::enabled::YOUR_COUNTER.scope();
let ret = (|| {
    // measured work
    ...
})();
std::hint::black_box(&_scope); // Tells Rust to be pessimistic about optimizing this variable.
ret
```

### Ensuring only one active timer
`lind-boot` runs the benchmark module once per counter. On each run it enables exactly one
counter and disables the rest, then prints a report. This avoids stacked measurement overhead
from multiple counters running at the same time.

The logic for this can be seen in [`lind-boot/src/main.rs`](../lind-boot/src/main.rs)

### Adding a new benchmark site
Suppose we want to add a new timer in `threei` for the `copy_data_between_cages` function. We will need to make the following changes:

1. Add a counter in `src/threei/src/perf.rs` and include it in `ALL_COUNTERS`.
2. Add a scoped timer in `src/threei/src/threei.rs` at the top of the `copy_data_between_cages` function.
3. Keep the guard alive until after the measured work if the function has multiple return paths. This can be done by moving measured work into an unnamed scope, and using the `std::hint::black_box` to avoid the scope being optimized out early.

In case we want to only benchmark a snippet of a function instead of the entire thing, we can `drop` the scope manually:

```rust
let scope = perf::enabled::YOUR_COUNTER.scope();
// measured snippet
drop(scope);
```

### Adding a new crate
Currently the crates that are supported are `wasmtime_lind_common`, `fdtables`, `rawposix`, and `threei`. In order to add support for a new crate, the following changes are needed:

1. Add a `perf.rs` module to the new crate and define counters plus `ALL_COUNTERS`.
2. Export `ALL_COUNTERS` from the crate�~@~Ys `perf` module.
3. Add the crate�~@~Ys counters to `lind-boot` enumeration, enable/reset, and reporting.
4. Rebuild `lind-boot` with `--features lind_perf` to include the new module.
