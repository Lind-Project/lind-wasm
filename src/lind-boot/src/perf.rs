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
pub static TRAMPOLINE_GET_VMCTX: Counter = Counter::new("lind_boot::trampoline::get_vmctx");

pub static LIND_BOOT_COUNTERS: &[&Counter] = &[&GRATE_CALLBACK_TRAMPOLINE, &TRAMPOLINE_GET_VMCTX];

/// Initialize counters for all modules, involves setting the TimerKind and resetting the
/// counts.
pub fn perf_init(kind: TimerKind) {
    lind_perf::set_timer(LIND_BOOT_COUNTERS, kind);
    lind_perf::reset_all(LIND_BOOT_COUNTERS);
}

/// Finds a counter by it's name and searches for it across modules to enable it. Disables all
/// other counters.
pub fn enable_one_counter(name: &str) {
    lind_perf::enable_name(LIND_BOOT_COUNTERS, name);
}

/// Get a list of all counter names.
pub fn all_counter_names() -> Vec<&'static str> {
    LIND_BOOT_COUNTERS
        .iter()
        .filter_map(|c| c.get_name())
        .collect()
}

/// Print a report for every module.
pub fn perf_report() {
    lind_perf::report_header(format!("LIND-BOOT"));
    lind_perf::report(LIND_BOOT_COUNTERS);
}
