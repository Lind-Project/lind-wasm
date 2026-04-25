# lind-perf

`lind-perf` is the instrumentation crate used by Lind crates (for example `lind-boot`) to measure hot paths.

This crate is defined in a manner where the callsites remain clean i.e. without needing conditional flags. This is implemented using the `enabled` feature.

The public APIs used remain the same, but if the crate is compiled without the `enabled` feature, each operation is a no-op ensuring that the final binary is not polluted with unused codepaths.

## Build Modes

`lind-perf` supports two compile-time modes via Cargo feature `enabled`:

1. `enabled` on: real counter accumulation + real reporting.
2. `enabled` off (default): API-compatible no-op behavior.

`lind-boot` maps its crate feature `lind_perf` to `lind-perf/enabled`.

## Public API

Main exports:

- `struct Counter` : Responsible for recording information such as cycles spent, calls made for a benchmarking site.
- `TimerKind::{Clock, Rdtsc}` : Clock uses `CLOCK_MONOTONIC_RAW`, Rdtsc: Time Stamp Counter.
- `fn set_timer(...)` : Set timer kind for list of Counters.
- `fn reset_all_counters(...)` : Reset Counters.
- `fn enable_counter_by_name(...)` : Enable Counter that matches the input name, disable the rest.
- `fn report(...)` : Print results from a set of counters.
- `static ENABLED: bool` : Check if `lind-perf` uses the `enabled` feature.

Macro:

- `lind_perf::get_timer!(COUNTER_PATH)` : Used to introduce a timer to a scope and start it.

## Typical Usage

Define a counters:

```rust
pub static MY_COUNTER: lind_perf::Counter = lind_perf::Counter::new("my_crate::my_counter");
```

Use timer to time a scope:

```rust
(|| {
    let _timer = lind_perf::get_timer!(crate::perf::MY_COUNTER); // Starts the timer
    // measured work
})(); // Timer stops when dropped.
```

Timers can also be dropped manually for timing non-scope snippets:

```rust
let _timer = lind_perf::get_timer!(crate::perf::MY_COUNTER); // Starts the timer
// measured work
drop(_timer); // Implicit drop
```

Counters can be enabled or disabled during runtime. The most common use-case for this is to sequentially enable a timer exclusively to avoid performance overheads.

```rust
lind_perf::set_timer(ALL_COUNTERS, lind_perf::TimerKind::Clock);
lind_perf::reset_all(ALL_COUNTERS);
lind_perf::enable_name(ALL_COUNTERS, "my_crate::my_counter");
```

Print report:

```rust
lind_perf::report_header("MY-CRATE".to_string());
lind_perf::report(ALL_COUNTERS);
```

## Disabled Mode Semantics

When `enabled` is not set:

- `Counter` is a lightweight no-op type.
- `get_timer!` returns a no-op scope guard.
- `set_timer/reset_all/enable_name/report*` are no-ops.
- `read_start/read_end` return `0`.

This allows instrumentation to remain in code without `cfg` guards at callsites.

## Timer Backends

- `TimerKind::Clock`: uses `clock_gettime(CLOCK_MONOTONIC_RAW)` in enabled mode.
- `TimerKind::Rdtsc`: uses RDTSC/RDTSCP on `x86_64` (falls back to clock timing on non-`x86_64`).
