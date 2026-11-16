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
pub static GRATE_INFLIGHT_EXIT: Counter = Counter::new("threei::make_syscall::grate_inflight_exit");
/// 3i's own calls (register_handler, copy_data_between_cages, copy_handler_table_to_cage):
/// call through the THREEI_CAGEID function pointer.
pub static THREEI_DISPATCH: Counter = Counter::new("threei::make_syscall::threei_dispatch");
/// Whole `copy_data_between_cages` (checks + the copy), as used by grates to read and write
/// cage memory.
pub static COPY_DATA_BETWEEN_CAGES: Counter = Counter::new("threei::copy_data_between_cages");
/// Whole `_call_grate_func`.
pub static CALL_GRATE_FUNC: Counter = Counter::new("threei::call_grate_func");
/// `get_cage_runtime` + `get_runtime_trampoline` inside `_call_grate_func`.
pub static GRATE_RUNTIME_LOOKUP: Counter = Counter::new("threei::call_grate_func::runtime_lookup");

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

/// Span: from 3i calling the grate trampoline (`(trampoline)(…)` in `_call_grate_func`) to lind-3i
/// about to call the grate's Wasm (`func.call(…)` in `GrateWorker::run`). The way into a grate.
pub static SPAN_3I_TO_GRATE_WASM: Counter =
    Counter::new("span::3i_trampoline_call_to_grate_wasm_call");
/// Span: from the grate's Wasm call returning in lind-3i back to 3i, right after the trampoline
/// call returns. The way back out of a grate.
pub static SPAN_GRATE_WASM_TO_3I: Counter = Counter::new("span::grate_wasm_return_to_3i");

pub static SPAN_COUNTERS: &[&Counter] = &[&SPAN_3I_TO_GRATE_WASM, &SPAN_GRATE_WASM_TO_3I];
