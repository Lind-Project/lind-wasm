//! Rust representation of the draft Lind-IF.
//!
//! The signatures and several argument types are provisional, not a stable ABI.
//! See `Lind-IF.md` for the lifecycle, memory, signal, and error-handling
//! contracts. This crate depends only on `sysdefs`, so Core (`cage`) can hold
//! registered runtimes without depending on their implementations.

pub use sysdefs::constants::Errno;

use std::sync::OnceLock;

pub type GuestAddress = u64;
pub type HostAddress = u64;
pub type CageId = u64;
pub type ThreadId = u32;
pub type RuntimeId = u64;

pub type LindResult<T> = Result<T, LindError>;

#[derive(Debug, PartialEq, Eq)]
pub enum LindError {
    MemoryFault(MemoryFaultInfo),
    Other(Errno),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryFaultInfo {
    pub address: GuestAddress,
    pub cause: MemoryFaultCause,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFaultCause {
    Unmapped,
    PermissionDenied,
}

/// A guest pointer and the cage that owns the pointed-to memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserPtr {
    pub address: GuestAddress,
    pub cage_id: CageId,
}

/// Selects the target cage's ABI, independently of the pointer owner's cage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiDescriptor {
    Wasm32,
    LinuxX86_64,
}

/// A buffer in the current context. Its address may still be untrusted.
#[derive(Debug, Clone, Copy)]
pub struct LocalBuffer {
    pub address: *mut u8,
    pub len: usize,
}

/// A read-only buffer in the current context. Its address may still be untrusted.
#[derive(Debug, Clone, Copy)]
pub struct ConstLocalBuffer {
    pub address: *const u8,
    pub len: usize,
}

/// Structure-specific versions of GetRuntimeXYZfromLindXYZ and its inverse.
/// Implementations must contain faults from invalid local pointers; raw pointers
/// must not be turned into Rust references before they are safely copied.
pub trait LindAbiConversion<T> {
    fn get_runtime_from_lind(
        &self,
        abi: AbiDescriptor,
        input: *const T,
        output: LocalBuffer,
    ) -> LindResult<()>;

    fn get_lind_from_runtime(
        &self,
        abi: AbiDescriptor,
        input: ConstLocalBuffer,
        output: *mut T,
    ) -> LindResult<()>;
}

/// Optional iovec accessors. The guest variant must use cross-cage copying.
pub trait LindIovecAccess {
    fn set_iovec_base(
        &self,
        abi: AbiDescriptor,
        iovec: LocalBuffer,
        index: usize,
        new_guest_address: GuestAddress,
    ) -> LindResult<()>;

    fn set_guest_iovec_base(
        &self,
        abi: AbiDescriptor,
        owner_cage_id: CageId,
        iovec_address: GuestAddress,
        index: usize,
        new_guest_address: GuestAddress,
    ) -> LindResult<()>;
}

//TODO: Define accessors for other struct types such as stat etc.

/// Provisional source of an argument or environment vector.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LindArgVec {
    Host(Vec<Vec<u8>>),
    Guest(UserPtr),
}

/// Provisional initial-image arguments;
///NOTE: This definition is supposed to beextended with explicit fields during the implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LindInitArgs {
    pub argv: LindArgVec,
    pub envp: LindArgVec,
}

/// Provisional initial-thread arguments; the initial-entry requirements remain open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindCloneArgs {
    pub flags: u64,
    pub stack: Option<UserPtr>,
    pub tls: Option<UserPtr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadDuplicateMode {
    SyscallFork,
    PthreadClone,
    RawInterrupted, //WHAT is this?
}

/// Provisional continuation input and clone TID-store requests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindThreadDuplicateArgs {
    pub mode: ThreadDuplicateMode,
    pub clone_args: UserPtr,
    /// CLONE_PARENT_SETTID address in the source cage.
    pub set_parent_tid: Option<GuestAddress>,
    /// CLONE_CHILD_SETTID address in the destination cage.
    pub set_child_tid: Option<GuestAddress>,
}

/// The existing 3i argument convention pairs each value with an owner cage ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindSyscallArg {
    pub value: u64,
    pub owner_cage_id: CageId,
}

/// The handler entry and six arguments from the 3i trampoline convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindHandlerCall {
    pub handler: u64,
    pub args: [LindSyscallArg; 6],
}

pub type HandlerReturn = i64;
pub type SyscallResult = i64;

/// Mapping addresses are host addresses and `fd` is a host backing object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMmapArgs {
    pub address: HostAddress,
    pub length: u64,
    pub prot: i32,
    pub flags: i32,
    pub fd: i32,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMprotectArgs {
    pub address: HostAddress,
    pub length: u64,
    pub prot: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMunmapArgs {
    pub address: HostAddress,
    pub length: u64,
}

/// The Lind-to-runtime boundary. Memory operations are wired through the
/// temporary 3i shims; the remaining operations are still provisional.
pub trait LindRuntime: Send + Sync {
    fn belongs_to_runtime(&self, exe_path: &str) -> LindResult<bool>;

    /// Translation alone does not establish that the address is safe to access.
    fn get_host_address(
        &self,
        cage_id: CageId,
        guest_address: GuestAddress,
    ) -> LindResult<HostAddress>;

    fn get_guest_address(
        &self,
        cage_id: CageId,
        host_address: HostAddress,
    ) -> LindResult<GuestAddress>;

    /// An unknown cage must result in ENOENT.
    fn get_cage_abi(&self, cage_id: CageId) -> LindResult<AbiDescriptor>;

    /// Prepare an image without creating a thread, reserving memory through 3i.
    fn init_cage(
        &self,
        cage_id: CageId,
        exe_path: &str,
        init_args: &LindInitArgs,
        is_staging: bool,
    ) -> LindResult<()>;

    /// Exec-only replacement. The representation of errors after the point of
    /// no return remains unresolved in the draft.
    fn apply_staging_cage(&self, staging_id: CageId, target_cage_id: CageId) -> LindResult<()>;

    /// Duplicate cage state without creating a thread. Materialization may be
    /// deferred until the scheduling point described in Lind-IF section 5.
    fn duplicate_cage(&self, cage_id: CageId, new_cage_id: CageId) -> LindResult<()>;

    /// Tear down cage execution and runtime metadata synchronously.
    fn terminate_cage(&self, cage_id: CageId) -> LindResult<()>;

    /// Register an initial thread without waiting for it to execute or exit.
    /// Staging and logical stop state still gate execution.
    fn create_thread(
        &self,
        cage_id: CageId,
        thread_id: ThreadId,
        args: &LindCloneArgs,
    ) -> LindResult<()>;

    /// Hold guest continuations through handler unwind and attempt requested
    /// TID stores at the prescribed scheduling point; store faults are ignored.
    fn duplicate_thread(
        &self,
        source_cage_id: CageId,
        source_thread_id: ThreadId,
        destination_cage_id: CageId,
        new_thread_id: ThreadId,
        args: &LindThreadDuplicateArgs,
    ) -> LindResult<()>;

    fn terminate_thread(&self, cage_id: CageId, thread_id: ThreadId) -> LindResult<()>;

    /// Core queued a signal for `cage_id`, or a queued signal may have become
    /// deliverable after an unblock, and selected `thread_id` to observe it.
    /// Core notifies on every queue insertion, including blocked signals. The
    /// runtime must consult Core's bookkeeping, and once a signal is
    /// deliverable arrange for that thread to obtain it from Core (e.g.
    /// `lind_get_first_signal`) and deliver it. Blocked signals must not be
    /// delivered. This only notifies; it must not dequeue or deliver
    /// synchronously.
    fn notify_signal(&self, cage_id: CageId, thread_id: ThreadId) -> LindResult<()>;

    /// Invoke a grate handler synchronously in the specified cage.
    fn run_handler(&self, cage_id: CageId, call: &LindHandlerCall) -> LindResult<HandlerReturn>;

    /// `is_reservation` is reserved for supervisor-controlled bootstrap.
    fn do_mmap(
        &self,
        cage_id: CageId,
        args: &LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress>;

    fn do_mprotect(&self, cage_id: CageId, args: &LindMprotectArgs) -> LindResult<()>;

    /// Release host pages. Callers that must retain a cage reservation remap
    /// the range with `PROT_NONE` through `do_mmap` instead.
    fn do_munmap(&self, cage_id: CageId, args: &LindMunmapArgs) -> LindResult<()>;
}

/// The runtime-to-Core lifecycle services (Lind-IF section 8). Core registers
/// its implementation once at boot with [`register_core`], before any cage
/// runs, so runtimes do not depend on Core's implementation.
pub trait LindCore: Send + Sync {
    /// The runtime retired `thread_id` of `cage_id`: after `TerminateThread`
    /// committed it to exit, or after it faulted, it runs no more guest code
    /// and the call is made off its guest and handler stacks. Core removes the
    /// thread; for the cage's last thread it waits for in-flight grate calls
    /// into the cage, has the runtime release the cage (`TerminateCage`) and
    /// finalizes it (zombie, SIGCHLD). Called exactly once per thread exit.
    /// Returns whether this was the cage's last thread.
    fn thread_exited(&self, cage_id: CageId, thread_id: ThreadId) -> bool;
}

static CORE: OnceLock<&'static dyn LindCore> = OnceLock::new();

/// Registers Core's services. Later registrations are ignored.
pub fn register_core(core: &'static dyn LindCore) {
    let _ = CORE.set(core);
}

/// Core's registered services.
///
/// # Panics
/// If Core has not registered with [`register_core`].
pub fn core() -> &'static dyn LindCore {
    *CORE.get().expect("Lind-IF: Core services are not registered")
}
