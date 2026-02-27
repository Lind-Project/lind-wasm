/// lind-boot's perf file binds together every other module's perf file.
///
/// This involves:
/// - Reading their COUNTERS
/// - Initializing them
/// - Combining all the COUNTERS into one list to iterate over and sequentially enable
/// - Printing a combined lind-perf report.
use crate::cli::CliOptions;
use lind_perf::{Counter, TimerKind};

// These are counters defined within lind-boot.
pub static GRATE_CALLBACK_TRAMPOLINE: Counter =
    Counter::new("lind_boot::grate_callback_trampoline");
pub static TYPED_FUNC_CALL: Counter = Counter::new("lind_boot::typed_func_call");

// Counter list used by the perf runner in `main.rs`. Each benchmark iteration
// enables exactly one counter name from this list.
pub static LIND_BOOT_COUNTERS: &[&Counter] = &[&GRATE_CALLBACK_TRAMPOLINE, &TYPED_FUNC_CALL];

pub static ALL_COUNTERS: &[&Counter] = [LIND_BOOT_COUNTERS, threei::perf::ALL_COUNTERS].concat();

/// Initialize counters for all modules, involves setting the TimerKind and resetting the
/// counts.
pub fn perf_init(kind: TimerKind) {
    // Configure timer backend (Clock or TSC) for all local counters.
    lind_perf::set_timer(ALL_COUNTERS, kind);
    // Reset all accumulated measurements before benchmark runs begin.
    lind_perf::reset_all_counters(ALL_COUNTERS);
}

/// Finds a counter by it's name and searches for it across modules to enable it. Disables all
/// other counters.
pub fn enable_one_counter(name: &str) {
    lind_perf::enable_counter_by_name(ALL_COUNTERS, name);
}

/// Get a list of all counter names.
pub fn all_counter_names() -> Vec<&'static str> {
    ALL_COUNTERS.iter().filter_map(|c| c.get_name()).collect()
}

/// Print a report for every module.
pub fn perf_report() {
    // Note: `lind_perf::report*` are no-ops when lind-perf is built without
    // its internal `enabled` feature.
    lind_perf::report_header(format!("LIND-BOOT"));
    lind_perf::report(LIND_BOOT_COUNTERS);

    lind_perf::report_header(format!("THREE-I"));
    lind_perf::report(threei::perf::ALL_COUNTERS);
}
