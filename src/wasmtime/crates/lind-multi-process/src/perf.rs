//! lind-perf counters for fork and exit (perf builds only).
//!
//! fork and exit don't run inside a timed benchmark syscall, and fork builds the child on a new
//! thread, so these counters use `timer_any` / `span_start_any`: they record whenever the
//! counter is the one enabled for the run.
//!
//! A fork, in order: the parent sets up (`parent_setup`), its Wasm stack unwinds through Asyncify
//! (`parent_unwind`), the unwind callback starts the child's thread and waits for it
//! (`parent_callback`), and the parent's stack rewinds back into `fork()` (`parent_rewind`). The
//! child thread builds a store, a linker and an instance with a copy of the parent's memory
//! (`child_setup`), then rewinds into `fork()` itself.
use lind_perf::Counter;

pub use cage::perf::FORK_COPY_MEMORY;

/// Parent: `fork_call`, up to arming the unwind (snapshots of the linker and globals).
pub static FORK_PARENT_SETUP: Counter = Counter::new("fork::parent_setup");
/// Parent: the linker and global snapshots taken for the child.
pub static FORK_PARENT_SNAPSHOTS: Counter = Counter::new("fork::parent_setup::snapshots");
/// Span, parent: from `fork_call` returning to the unwind callback starting: the parent's Wasm
/// stack unwinding through Asyncify.
pub static SPAN_FORK_PARENT_UNWIND: Counter = Counter::new("fork::parent_unwind");
/// Parent: the unwind callback: start the child's thread, wait for it, start the rewind.
pub static FORK_PARENT_CALLBACK: Counter = Counter::new("fork::parent_callback");
/// Parent: creating the OS thread for the child.
pub static FORK_PARENT_SPAWN: Counter = Counter::new("fork::parent_callback::thread_spawn");
/// Parent: waiting at the barrier until the child has its own copy of the memory.
pub static FORK_PARENT_WAIT: Counter = Counter::new("fork::parent_callback::wait_for_child");
/// Span, parent: from the callback returning to `clone` returning in the parent (lind-common):
/// the parent's Wasm stack rewinding through Asyncify.
pub static SPAN_FORK_PARENT_REWIND: Counter = Counter::new("fork::parent_rewind");
/// Child thread: from its start to the barrier: everything the parent waits for.
pub static FORK_CHILD_SETUP: Counter = Counter::new("fork::child_setup");
/// Child: a new Wasmtime store (and the child's host context).
pub static FORK_CHILD_STORE: Counter = Counter::new("fork::child_setup::new_store");
/// Child: the child's linker, from the parent's snapshot.
pub static FORK_CHILD_LINKER: Counter = Counter::new("fork::child_setup::new_child_linker");
/// Child: instantiating the module, including the copy of the parent's memory.
pub static FORK_CHILD_INSTANTIATE: Counter = Counter::new("fork::child_setup::instantiate");
/// Span, child: from calling the child's `_start` (which rewinds into `fork()`) to its `exit`.
/// In the fork benchmark the child exits at once, so this is mostly the child's rewind.
pub static SPAN_FORK_CHILD_TO_EXIT: Counter = Counter::new("fork::child_rewind_to_exit");
/// `exit_call`: arming the Asyncify unwind for exit.
pub static EXIT_CALL: Counter = Counter::new("exit::exit_call");
/// Span: from `exit_call` returning to the exit callback starting: the Wasm stack unwinding.
pub static SPAN_EXIT_UNWIND: Counter = Counter::new("exit::unwind");
/// The exit callback: thread exit, cage finalize (which lets the parent's wait return),
/// removing the VMContext.
pub static EXIT_CLEANUP: Counter = Counter::new("exit::cleanup");
/// `cage_finalize` inside the exit callback.
pub static EXIT_FINALIZE: Counter = Counter::new("exit::cleanup::cage_finalize");

pub static ALL_COUNTERS: &[&Counter] = &[
    &FORK_PARENT_SETUP,
    &FORK_PARENT_SNAPSHOTS,
    &SPAN_FORK_PARENT_UNWIND,
    &FORK_PARENT_CALLBACK,
    &FORK_PARENT_SPAWN,
    &FORK_PARENT_WAIT,
    &SPAN_FORK_PARENT_REWIND,
    &FORK_CHILD_SETUP,
    &FORK_CHILD_STORE,
    &FORK_CHILD_LINKER,
    &FORK_CHILD_INSTANTIATE,
    &FORK_COPY_MEMORY,
    &SPAN_FORK_CHILD_TO_EXIT,
    &EXIT_CALL,
    &SPAN_EXIT_UNWIND,
    &EXIT_CLEANUP,
    &EXIT_FINALIZE,
];
