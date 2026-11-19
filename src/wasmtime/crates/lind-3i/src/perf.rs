//! lind-perf counters for lind-3i grate calls (perf builds only).
use lind_perf::Counter;

// Span counters that start in 3i and end here (see threei::perf).
pub use threei::perf::{SPAN_3I_TO_GRATE_WASM, SPAN_GRATE_WASM_TO_3I};

/// Whole `GrateHandler::submit`.
pub static SUBMIT: Counter = Counter::new("lind_3i::submit");
/// Getting a worker: serialization gate (serialized mode only) + `take_worker_blocking`.
pub static WORKER_ACQUIRE: Counter = Counter::new("lind_3i::worker_acquire");
/// Whole `GrateWorker::run`: stack-pointer reset + the Wasm call.
pub static WORKER_RUN: Counter = Counter::new("lind_3i::worker_run");
/// `reset_worker_stack()`: setting the instance's `__stack_pointer` to the top of its stack slot.
pub static WORKER_RESET_STACK: Counter = Counter::new("lind_3i::worker_run::reset_stack");
/// Just the typed `pass_fptr_to_wt` call, from right before `func.call(…)` to right after it
/// returns: enter the grate's Wasm, dispatch, run the grate's handler, return.
pub static PASS_FPTR_CALL: Counter = Counter::new("lind_3i::worker_run::pass_fptr_to_wt_call");
/// Returning the worker to the pool (`WorkerLease` drop -> `return_worker`).
pub static WORKER_RELEASE: Counter = Counter::new("lind_3i::worker_release");

pub static ALL_COUNTERS: &[&Counter] = &[
    &SUBMIT,
    &WORKER_ACQUIRE,
    &WORKER_RUN,
    &WORKER_RESET_STACK,
    &PASS_FPTR_CALL,
    &WORKER_RELEASE,
];
