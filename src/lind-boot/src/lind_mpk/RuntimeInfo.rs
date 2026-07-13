use libc::c_void;
use std::any::Any;
use cage::RuntimeInfo;

/// Type alias for the __enable_syscall_interpose function pointer.
/// This function is provided by the custom glibc loaded in the dlmopen
/// namespace and is used to register syscall interposition handlers.
pub type EnableInterposeF = unsafe extern "C" fn(
    handler: Option<unsafe extern "C" fn(i64, i64, i64, i64, i64, i64, i64, i32) -> i64>,
) -> libc::c_int;

/// Runtime-specific information for MPK (Memory Protection Keys) based cages.
///
/// This structure stores the dlmopen handles and function pointers needed
/// to manage isolated native .so execution. Fields are set once during
/// execute_mpk initialization and then only read (e.g., during fork/clone).
/// The Cage's RwLock<Box<dyn RuntimeInfo>> provides synchronization.
#[derive(Debug)]
pub struct MPKRuntimeInfo {
    /// Handle to the guest .so loaded via dlmopen
    pub loader_cage_handle: *mut c_void,
    /// Handle to the custom libc loaded in the isolated namespace
    pub loader_libc_handle: *mut c_void,
    /// Function pointer to __enable_syscall_interpose in custom libc
    pub enable_interpose_fn: EnableInterposeF,
    /// OS-level process ID of this cage's process
    /// This is relevant for forked off child cages that run in a different process
    /// this field is 0 if the cage runs in the main lind process
    pub pid: libc::pid_t,
}

impl MPKRuntimeInfo {
    /// Creates a new MPKRuntimeInfo with the given handles and function pointer.
    /// Called during execute_mpk setup after dlmopen and symbol resolution.
    pub fn new(
        cage_handle: *mut c_void,
        libc_handle: *mut c_void,
        enable_interpose: EnableInterposeF,
        pid: libc::pid_t,
    ) -> Self {
        MPKRuntimeInfo {
            loader_cage_handle: cage_handle,
            loader_libc_handle: libc_handle,
            enable_interpose_fn: enable_interpose,
            pid,
        }
    }
}

impl RuntimeInfo for MPKRuntimeInfo {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Safety: Raw pointers in MPKRuntimeInfo point to dlmopen handles that remain
// valid for the cage's lifetime. Access is synchronized through the Cage's
// RwLock<Box<dyn RuntimeInfo>>, ensuring no data races. The handles are opaque
// library objects, not direct memory pointers, making them safe to share.
// The function pointer is extern "C" and statically determined, thus thread-safe.
unsafe impl Send for MPKRuntimeInfo {}
unsafe impl Sync for MPKRuntimeInfo {}