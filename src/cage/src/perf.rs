//! lind-perf counter for the cage crate (perf builds only).
use lind_perf::Counter;

/// `fork_vmmap`: copying every mapped region of the parent's memory into the child
/// (`process_vm_writev` for private regions, `mremap` for shared ones).
pub static FORK_COPY_MEMORY: Counter = Counter::new("fork::child_setup::instantiate::copy_memory");
