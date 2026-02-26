/// lind-boot's perf file binds together every other module's perf file.
///
/// This involves:
/// - Reading their COUNTERS
/// - Initializing them
/// - Combining all the COUNTERS into one list to iterate over and sequentially enable
/// - Printing a combined lind-perf report.
#[cfg(feature = "lind_perf")]
pub mod enabled {
    use lind_perf::{Counter, TimerKind, enable_name, reset_all, set_timer};

    // These are counters defined within lind-boot.
    pub static GRATE_CALLBACK_TRAMPOLINE: Counter =
        Counter::new("lind_boot::grate_callback_trampoline");
    pub static TRAMPOLINE_GET_VMCTX: Counter = Counter::new("lind_boot::trampoline::get_vmctx");
    pub static TRAMPOLINE_GET_PASS_FPTR_TO_WT: Counter =
        Counter::new("lind_boot::trampoline::get_pass_fptr_to_wt");
    pub static TRAMPOLINE_TYPED_DISPATCH_CALL: Counter =
        Counter::new("lind_boot::trampoline::typed_dispatch_call");

    pub static LIND_BOOT_COUNTERS: &[&Counter] = &[
        &GRATE_CALLBACK_TRAMPOLINE,
        &TRAMPOLINE_GET_VMCTX,
        &TRAMPOLINE_GET_PASS_FPTR_TO_WT,
        &TRAMPOLINE_TYPED_DISPATCH_CALL,
    ];

    /// Initialize counters for all modules, involves setting the TimerKind and resetting the
    /// counts.
    pub fn init(kind: TimerKind) {
        set_timer(LIND_BOOT_COUNTERS, kind);

        reset_all(LIND_BOOT_COUNTERS);
    }

    /// Finds a counter by it's name and searches for it across modules to enable it. Disables all
    /// other counters.
    pub fn enable_one(name: &str) {
        enable_name(LIND_BOOT_COUNTERS, name);
    }

    /// Get a list of all counter names.
    pub fn all_counter_names() -> Vec<&'static str> {
        let mut names = Vec::new();
        names.extend(LIND_BOOT_COUNTERS.iter().map(|c| c.name));
        names
    }

    /// Print a report for every module
    pub fn report() {
        lind_perf::report_header(format!("LIND-BOOT"));
        lind_perf::report(LIND_BOOT_COUNTERS);
    }
}

#[cfg(not(feature = "lind_perf"))]
pub mod enabled {}
