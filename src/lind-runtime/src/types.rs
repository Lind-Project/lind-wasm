//! Lind-IF §1 types and the provisional argument structures used by §4–§7.

use sysdefs::constants::err_const::Errno;

pub type GuestAddress = u64;
pub type HostAddress = u64;
pub type CageId = u64;
pub type ThreadId = u32;
pub type RuntimeId = u64;

pub type LindResult<T> = Result<T, LindError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl LindError {
    /// Build an error from a positive or negative Linux errno. Unknown values map to `EIO`.
    pub fn from_errno(errno: i64) -> Self {
        let value = errno.unsigned_abs() as i32;
        LindError::Other(Errno::from_discriminant(value).unwrap_or(Errno::EIO))
    }

    /// Negative Linux errno for syscall-style callers. A memory fault becomes `-EFAULT`; callers
    /// that must raise a cage fault instead (e.g. `rt_sigreturn`) must match on the variant.
    pub fn to_syscall_ret(self) -> i64 {
        match self {
            LindError::Other(errno) => -(errno as i64),
            LindError::MemoryFault(_) => -(Errno::EFAULT as i64),
        }
    }
}

impl From<Errno> for LindError {
    fn from(errno: Errno) -> Self {
        LindError::Other(errno)
    }
}

/// Convert a syscall-style result (`>= 0` success, `< 0` negative errno) into a `LindResult`.
pub fn from_syscall_ret(ret: i64) -> LindResult<u64> {
    if ret < 0 {
        Err(LindError::from_errno(ret))
    } else {
        Ok(ret as u64)
    }
}

/// A user pointer passed through Lind-IF together with the cage that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserPtr {
    pub addr: u64,
    pub cage: CageId,
}

/// ABI identity of a cage. Only the Wasm32 ABI used by lind-glibc exists today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AbiDescriptor {
    Wasm32,
}

/// Argument or environment vector for `InitCage`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LindArgVector {
    /// Supervisor-owned strings (initial launch).
    Owned(Vec<String>),
    /// NULL-terminated pointer array inside a cage (exec).
    User(UserPtr),
}

/// Provisional `LindInitArgs` (Lind-IF §4): argv/envp plus entry configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LindInitArgs {
    pub argv: LindArgVector,
    pub envp: LindArgVector,
}

/// Provisional `LindCloneArgs` (Lind-IF §5). Empty for an initial thread from a prepared image.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LindCloneArgs {
    pub flags: u64,
    pub stack: Option<UserPtr>,
    pub stack_size: u64,
    pub tls: Option<UserPtr>,
}

/// The duplication flavours Lind-IF §5 distinguishes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadDuplicateMode {
    /// fork/clone without `CLONE_VM`: child result zero, both continuations held.
    SyscallFork,
    /// libc pthread clone (`CLONE_VM`): child resumes with result zero, new stack and TLS.
    PthreadClone,
    /// Asynchronous on-behalf-of duplication of an interrupted continuation.
    RawInterrupted,
}

/// `newThreadId` value asking the runtime to assign the thread ID itself. Core does not own the
/// thread ID namespace yet (Wasmtime allocates pthread IDs per cage).
pub const RUNTIME_ASSIGNED_THREAD: ThreadId = 0;

/// Provisional `LindThreadDuplicateArgs` (Lind-IF §5). `clone_args` is the guest's raw clone
/// argument structure until the continuation fields are specified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindThreadDuplicateArgs {
    pub mode: ThreadDuplicateMode,
    pub clone_args: UserPtr,
}

/// Process-directed signal target for `SendSignal` (thread targeting conventions are open).
pub const ANY_THREAD: ThreadId = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindSigInfo {
    pub signo: i32,
    pub sender_cage: CageId,
}

/// Arguments of a 3i grate call, as forwarded by `RunHandler`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindHandlerArgs {
    pub handler: u64,
    pub args: [(u64, u64); 6],
}

pub type HandlerReturn = i64;

/// Host-side mapping request (Lind-IF §7). `addr` is a host address; `fd` is a host descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMmapArgs {
    pub addr: HostAddress,
    pub len: u64,
    pub prot: i32,
    pub flags: i32,
    pub fd: i32,
    pub offset: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMprotectArgs {
    pub addr: HostAddress,
    pub len: u64,
    pub prot: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LindMunmapArgs {
    pub addr: HostAddress,
    pub len: u64,
}
