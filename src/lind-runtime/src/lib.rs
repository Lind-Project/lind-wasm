//! Lind-IF: the contract between Lind (Core, RawPOSIX, 3i) and an execution runtime.
//!
//! See `Lind-IF.md` at the repository root. This crate is a leaf: it only defines the
//! operations, their types, the runtime registry, and runtime-independent helpers. Core
//! (`lind-core`) owns the transactions that compose these operations.

pub mod abi;
pub mod host_memory;
pub mod registry;
pub mod testing;
mod types;

pub use registry::{get_runtime, register_runtime, registered_runtimes, runtime_for_executable};
pub use types::*;

/// Operations Lind invokes on a runtime (Lind-IF §2 and §4–§7).
///
/// Every method is required; a backend that cannot support an operation must fail it
/// explicitly (typically with `ENOSYS`) rather than silently succeed.
pub trait LindRuntime: Send + Sync {
    /// 3i runtime identifier recorded per cage (e.g. `RUNTIME_TYPE_WASMTIME`).
    fn runtime_id(&self) -> RuntimeId;

    // §2 discovery and address translation
    fn belongs_to_runtime(&self, exe_path: &str) -> LindResult<bool>;
    fn get_host_address(&self, cage: CageId, guest: GuestAddress) -> LindResult<HostAddress>;
    fn get_guest_address(&self, cage: CageId, host: HostAddress) -> LindResult<GuestAddress>;
    fn get_cage_abi(&self, cage: CageId) -> LindResult<AbiDescriptor>;

    // §4 cage lifecycle
    fn init_cage(
        &self,
        cage: CageId,
        exe_path: &str,
        init_args: LindInitArgs,
        is_staging: bool,
    ) -> LindResult<()>;
    fn apply_staging_cage(&self, staging: CageId, target: CageId) -> LindResult<()>;
    fn duplicate_cage(&self, cage: CageId, new_cage: CageId) -> LindResult<()>;
    fn terminate_cage(&self, cage: CageId) -> LindResult<()>;

    // §5 thread lifecycle
    fn create_thread(&self, cage: CageId, thread: ThreadId, args: LindCloneArgs)
        -> LindResult<()>;
    fn duplicate_thread(
        &self,
        source_cage: CageId,
        source_thread: ThreadId,
        destination_cage: CageId,
        new_thread: ThreadId,
        args: LindThreadDuplicateArgs,
    ) -> LindResult<()>;
    fn terminate_thread(&self, cage: CageId, thread: ThreadId) -> LindResult<()>;

    // §6 signals and handler invocation
    fn send_signal(&self, cage: CageId, thread: ThreadId, info: LindSigInfo) -> LindResult<()>;
    fn run_handler(&self, cage: CageId, args: LindHandlerArgs) -> LindResult<HandlerReturn>;

    // §7 memory mappings (host addresses)
    fn do_mmap(
        &self,
        cage: CageId,
        args: LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress>;
    fn do_mprotect(&self, cage: CageId, args: LindMprotectArgs) -> LindResult<()>;
    fn do_munmap(&self, cage: CageId, args: LindMunmapArgs) -> LindResult<()>;
}
