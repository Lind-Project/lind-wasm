//! lind-perf integration for lind-boot (perf builds only).
//!
//! `lind-boot --perf[=clock|tsc] [--perf-filter=...] prog.cwasm [args]` makes this process the
//! *parent*. For each selected counter it starts a fresh lind-boot *child* with the same
//! arguments and three environment variables naming the counter, the clock and a result file.
//! The child measures the timer's own overhead, enables only that counter, runs the program
//! normally, and writes `<total> <calls> <overhead>` to the result file just before exiting.
//! The parent then prints one report.
//!
//! One fresh process per counter means:
//! - only one timer is active per run, so timers never measure each other;
//! - every sample starts from clean global state, exactly like a normal run.

use crate::cli::{CliOptions, PerfScopeArg, PerfTimer};
use lind_perf::{Counter, Derived, TimerKind};
use std::ffi::OsString;
use std::fs::File;
use std::io::Write;
use std::process::Command;

use rawposix::perf as rp;
use threei::perf as ti;
use wasmtime_lind_3i::perf as l3i;
use wasmtime_lind_common::perf as lc;

/// Whole `grate_callback_trampoline` (3i's entry into the Wasmtime runtime for a grate call).
pub static GRATE_CALLBACK_TRAMPOLINE: Counter =
    Counter::new("lind_boot::grate_callback_trampoline");
/// `get_grate_handler` in `submit_grate_request`: finding the grate's handler in `GRATE_POOL`.
pub static GRATE_HANDLER_LOOKUP: Counter =
    Counter::new("lind_boot::grate_callback_trampoline::get_grate_handler");

static LIND_BOOT_COUNTERS: &[&Counter] = &[&GRATE_CALLBACK_TRAMPOLINE, &GRATE_HANDLER_LOOKUP];

/// Every counter, in call-path order. The report groups them by the name prefix (layer).
fn all_counters() -> Vec<&'static Counter> {
    [
        lc::ALL_COUNTERS,
        ti::ALL_COUNTERS,
        rp::ALL_COUNTERS,
        LIND_BOOT_COUNTERS,
        l3i::ALL_COUNTERS,
        ti::SPAN_COUNTERS,
    ]
    .iter()
    .flat_map(|group| group.iter().copied())
    .collect()
}

/// Costs derived by subtracting nested timers. A line is printed only when all of its
/// counters were measured in this benchmark, so direct-path and grate-path lines appear
/// in the matching runs only.
static DERIVED: &[Derived] = &[
    Derived {
        label: "guest side: glibc wrapper + Wasm<->host transition",
        outer: &lc::GUEST_PER_CALL,
        inner: &[&lc::SYSCALL_ENTRY],
    },
    Derived {
        label: "lind-common: syscall entry glue (excl. 3i)",
        outer: &lc::SYSCALL_ENTRY,
        inner: &[&ti::MAKE_SYSCALL],
    },
    Derived {
        label: "3i: make_syscall own work, direct path",
        outer: &ti::MAKE_SYSCALL,
        inner: &[
            &ti::DEAD_CAGE_CHECK,
            &ti::HANDLER_LOOKUP,
            &ti::RAWPOSIX_DISPATCH,
        ],
    },
    Derived {
        label: "3i -> RawPOSIX dispatch hop (excl. handler), libc",
        outer: &ti::RAWPOSIX_DISPATCH,
        inner: &[&rp::LIBC_CALL],
    },
    Derived {
        label: "3i -> RawPOSIX dispatch hop (excl. handler), fdtables",
        outer: &ti::RAWPOSIX_DISPATCH,
        inner: &[&rp::FDTABLES_CALL],
    },
    Derived {
        label: "RawPOSIX fdtables handler excl. close_virtualfd",
        outer: &rp::FDTABLES_CALL,
        inner: &[&rp::CLOSE_VIRTUALFD],
    },
    Derived {
        label: "3i: make_syscall own work, grate path",
        outer: &ti::MAKE_SYSCALL,
        inner: &[
            &ti::DEAD_CAGE_CHECK,
            &ti::HANDLER_LOOKUP,
            &ti::GRATE_INFLIGHT_ENTER,
            &ti::CALL_GRATE_FUNC,
            &ti::GRATE_INFLIGHT_EXIT,
        ],
    },
    Derived {
        label: "3i: call_grate_func own work (excl. lookup + trampoline)",
        outer: &ti::CALL_GRATE_FUNC,
        inner: &[&ti::GRATE_RUNTIME_LOOKUP, &GRATE_CALLBACK_TRAMPOLINE],
    },
    Derived {
        label: "lind-boot: trampoline own work (excl. lookup + submit)",
        outer: &GRATE_CALLBACK_TRAMPOLINE,
        inner: &[&GRATE_HANDLER_LOOKUP, &l3i::SUBMIT],
    },
    Derived {
        label: "lind-3i: submit bookkeeping (excl. acquire/run/release)",
        outer: &l3i::SUBMIT,
        inner: &[&l3i::WORKER_ACQUIRE, &l3i::WORKER_RUN, &l3i::WORKER_RELEASE],
    },
    Derived {
        label: "lind-3i: worker run excl. stack reset and Wasm call",
        outer: &l3i::WORKER_RUN,
        inner: &[&l3i::WORKER_RESET_STACK, &l3i::PASS_FPTR_CALL],
    },
];

const ENV_COUNTER: &str = "LIND_PERF_COUNTER";
const ENV_TIMER: &str = "LIND_PERF_TIMER";
const ENV_RESULT: &str = "LIND_PERF_RESULT";
const ENV_SCOPE: &str = "LIND_PERF_SCOPE";

/// Columns of the per-syscall table (`--perf-scope=all`), after the syscall-entry column.
static SYSCALL_COLUMNS: &[(&str, &Counter)] = &[
    ("3i", &ti::MAKE_SYSCALL),
    ("3i lookup", &ti::HANDLER_LOOKUP),
    ("rawposix", &ti::RAWPOSIX_DISPATCH),
    ("3i call", &ti::THREEI_DISPATCH),
    ("grate", &ti::CALL_GRATE_FUNC),
    ("grate wasm", &l3i::PASS_FPTR_CALL),
];

/// State of a child run: the one enabled counter and where to write its result.
pub struct PerfChild {
    counter: &'static Counter,
    out: File,
    overhead: u64,
}

/// If this process was started by a `--perf` parent, enable its counter and open the result
/// file. Must run before the chroot, since the result file lives outside lindfs.
pub fn child_init() -> Option<PerfChild> {
    let name = std::env::var(ENV_COUNTER).ok()?;

    let kind = match std::env::var(ENV_TIMER).as_deref() {
        Ok("tsc") => TimerKind::Rdtsc,
        _ => TimerKind::Clock,
    };
    lind_perf::set_timer_kind(kind);
    if std::env::var(ENV_SCOPE).as_deref() == Ok("all") {
        lind_perf::set_perf_scope(lind_perf::PerfScope::All);
    }

    let counter = all_counters()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("[lind-perf] unknown counter {:?}", name));
    // Measure the timer's own cost now, before the program runs (guest timing needs none).
    let overhead = match counter.unit {
        lind_perf::Unit::Timer => lind_perf::calibrate_overhead(),
        lind_perf::Unit::Nanos => 0,
    };
    counter.enable();

    let path = std::env::var(ENV_RESULT)
        .unwrap_or_else(|_| panic!("[lind-perf] {} is not set", ENV_RESULT));
    let out = File::create(&path)
        .unwrap_or_else(|e| panic!("[lind-perf] cannot create result file {}: {}", path, e));

    Some(PerfChild {
        counter,
        out,
        overhead,
    })
}

/// Write the child's result just before the process exits: a first line
/// `<total> <calls> <overhead>`, then one `s <slot> <total> <calls>` line per slot (syscall x
/// origin) that recorded anything.
pub fn child_finish(child: Option<PerfChild>) {
    if let Some(mut c) = child {
        let mut text = format!(
            "{} {} {}\n",
            c.counter.total(),
            c.counter.calls(),
            c.overhead
        );
        for (slot, total, calls) in c.counter.nonzero_slots() {
            text.push_str(&format!("s {} {} {}\n", slot, total, calls));
        }
        if let Err(e) = c.out.write_all(text.as_bytes()) {
            eprintln!(
                "[lind-perf] failed to write result for {}: {}",
                c.counter.name, e
            );
        }
    }
}

/// Parse a child's result file and store it in `counter`. Returns false if it is missing or
/// malformed.
fn load_result(counter: &Counter, text: &str) -> bool {
    let mut lines = text.lines();
    let Some(first) = lines.next() else {
        return false;
    };
    let mut it = first.split_whitespace().map(str::parse::<u64>);
    let (Some(Ok(total)), Some(Ok(calls)), Some(Ok(overhead))) = (it.next(), it.next(), it.next())
    else {
        return false;
    };
    counter.set_result(total, calls, overhead);
    for line in lines {
        let mut f = line.split_whitespace();
        if f.next() != Some("s") {
            continue;
        }
        let nums: Vec<u64> = f.filter_map(|x| x.parse().ok()).collect();
        if let [slot, total, calls] = nums[..] {
            counter.set_slot(slot as usize, total, calls);
        }
    }
    true
}

/// Parent side of `--perf`: run one child per selected counter, collect, report.
/// Returns the process exit code.
pub fn run_parent(cli: &CliOptions, timer: PerfTimer) -> i32 {
    let (timer_env, kind) = match timer {
        PerfTimer::Clock => ("clock", TimerKind::Clock),
        PerfTimer::Tsc => ("tsc", TimerKind::Rdtsc),
    };
    // The parent never times anything; this only sets the units used by the report.
    lind_perf::set_timer_kind(kind);
    let scope_all = cli.perf_scope == PerfScopeArg::All;

    let filters: Vec<&str> = cli
        .perf_filter
        .as_deref()
        .map(|f| {
            f.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let selected: Vec<&'static Counter> = all_counters()
        .into_iter()
        .filter(|c| filters.is_empty() || filters.iter().any(|f| c.name.starts_with(f)))
        .collect();
    if selected.is_empty() {
        eprintln!("[lind-perf] no counters match --perf-filter");
        return 2;
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[lind-perf] cannot locate the lind-boot executable: {}", e);
            return 1;
        }
    };
    // Same command line for every child; the child ignores `--perf` because of ENV_COUNTER.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    let mut failed = false;
    for (i, counter) in selected.iter().enumerate() {
        eprintln!(
            "[lind-perf] run {}/{}: {}",
            i + 1,
            selected.len(),
            counter.name
        );
        let result_path =
            std::env::temp_dir().join(format!("lind-perf-{}-{}.txt", std::process::id(), i));

        match Command::new(&exe)
            .args(&args)
            .env(ENV_COUNTER, counter.name)
            .env(ENV_TIMER, timer_env)
            .env(ENV_RESULT, &result_path)
            .env(ENV_SCOPE, if scope_all { "all" } else { "bench" })
            .status()
        {
            Ok(status) if status.success() => {}
            Ok(status) => {
                eprintln!("[lind-perf] {}: run exited with {}", counter.name, status);
                failed = true;
            }
            Err(e) => {
                eprintln!("[lind-perf] {}: failed to start run: {}", counter.name, e);
                failed = true;
                continue;
            }
        }

        let loaded = std::fs::read_to_string(&result_path)
            .map(|text| load_result(counter, &text))
            .unwrap_or(false);
        let _ = std::fs::remove_file(&result_path);
        if !loaded {
            // The run died before reporting (e.g. a panic). The other runs execute the same
            // program, so they would almost certainly fail the same way: stop here.
            eprintln!(
                "[lind-perf] {}: no result recorded; the program did not finish. \
                 Stopping the remaining {} run(s). Check the program runs without --perf.",
                counter.name,
                selected.len() - i - 1
            );
            return 1;
        }
    }

    // The timer's real cost is the same in every run (same machine, same binary); a higher
    // calibration in one run means it was disturbed (core migration, frequency change). Use the
    // lowest measured value for every counter, and say what the runs measured.
    let timed: Vec<&&Counter> = selected
        .iter()
        .filter(|c| c.calls() > 0 && c.unit == lind_perf::Unit::Timer)
        .collect();
    if let (Some(lo), Some(hi)) = (
        timed.iter().map(|c| c.overhead()).min(),
        timed.iter().map(|c| c.overhead()).max(),
    ) {
        if hi != lo {
            println!(
                "\ntimer overhead measured {} .. {} across runs; using the lowest ({}) for all \
                 counters (unit: {})",
                lo,
                hi,
                lo,
                if kind == TimerKind::Rdtsc {
                    "cycles"
                } else {
                    "ns"
                }
            );
        }
        for c in &timed {
            c.set_result(c.total(), c.calls(), lo);
        }
    }
    if scope_all {
        println!(
            "\nscope: all syscalls. Per-layer totals below mix all syscalls and origins; \
             see the PER SYSCALL table for the breakdown."
        );
    }
    lind_perf::print_report(&selected);
    if scope_all {
        if lc::SYSCALL_ENTRY.calls() > 0 {
            lind_perf::print_syscall_table(&lc::SYSCALL_ENTRY, SYSCALL_COLUMNS, cli.perf_top);
        } else {
            println!(
                "\n(no per-syscall table: include lind_common::syscall_entry in --perf-filter)"
            );
        }
    } else {
        // Derived costs assume one benchmark syscall path, so only for the benchmark scope.
        lind_perf::print_derived(DERIVED);
    }

    if failed { 1 } else { 0 }
}
