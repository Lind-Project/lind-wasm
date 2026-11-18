pub mod clone;
pub mod exec;
pub mod execution;
pub mod exit;
pub mod fork;
pub mod memory;
pub mod runtime;
pub mod signal;

pub use execution::LindCageManager;
pub use runtime::{launch_cage, runtime_of, MAIN_THREAD_ID};

/// Final teardown of a cage after its last thread exited: release the runtime's cage
/// metadata (Lind-IF §4 TerminateCage), publish the zombie, and drop 3i routing.
pub fn finalize_cage(cageid: u64) {
    if let Ok(runtime) = runtime_of(cageid) {
        if let Err(error) = runtime.terminate_cage(cageid) {
            eprintln!("[lind-core] TerminateCage({cageid}) failed: {error:?}");
        }
    }
    cage::cage_finalize(cageid);
    threei::handler_table::_rm_cage_from_handler(cageid);
    threei::remove_cage_runtime(cageid);
}
