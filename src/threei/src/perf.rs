//! lind-perf counters for threei (perf builds only).
use lind_perf::Counter;

/// Whole `make_syscall`.
pub static MAKE_SYSCALL: Counter = Counter::new("threei::make_syscall");
/// First dead/exiting target-cage check at the top of `make_syscall`.
pub static DEAD_CAGE_CHECK: Counter = Counter::new("threei::make_syscall::dead_cage_check");
/// `_get_handler`: handler-table lookup for (self_cageid, syscall_num).
pub static HANDLER_LOOKUP: Counter = Counter::new("threei::make_syscall::handler_lookup");
/// RawPOSIX branch: second dead-cage check + call through the RawPOSIX function pointer.
pub static RAWPOSIX_DISPATCH: Counter = Counter::new("threei::make_syscall::rawposix_dispatch");
/// Grate branch: registering the call as in flight on the grate (`grate_inflight += 1`, dead check).
pub static GRATE_INFLIGHT_ENTER: Counter =
    Counter::new("threei::make_syscall::grate_inflight_enter");
/// Grate branch: `grate_inflight -= 1` after the grate returns.
pub static GRATE_INFLIGHT_EXIT: Counter =
    Counter::new("threei::make_syscall::grate_inflight_exit");
/// 3i's own calls (register_handler, copy_data_between_cages, copy_handler_table_to_cage):
/// call through the THREEI_CAGEID function pointer.
pub static THREEI_DISPATCH: Counter = Counter::new("threei::make_syscall::threei_dispatch");
/// Whole `copy_data_between_cages` (checks + the copy), as used by grates to read and write
/// cage memory.
pub static COPY_DATA_BETWEEN_CAGES: Counter = Counter::new("threei::copy_data_between_cages");
/// Whole `_call_grate_func`.
pub static CALL_GRATE_FUNC: Counter = Counter::new("threei::call_grate_func");
/// `get_cage_runtime` + `get_runtime_trampoline` inside `_call_grate_func`.
pub static GRATE_RUNTIME_LOOKUP: Counter =
    Counter::new("threei::call_grate_func::runtime_lookup");

pub static ALL_COUNTERS: &[&Counter] = &[
    &MAKE_SYSCALL,
    &DEAD_CAGE_CHECK,
    &HANDLER_LOOKUP,
    &RAWPOSIX_DISPATCH,
    &THREEI_DISPATCH,
    &COPY_DATA_BETWEEN_CAGES,
    &GRATE_INFLIGHT_ENTER,
    &GRATE_INFLIGHT_EXIT,
    &CALL_GRATE_FUNC,
    &GRATE_RUNTIME_LOOKUP,
];
