//! Runtime-specific handling of the clone, exec, and exit syscalls.
//!
//! The boot shims dispatch these syscalls to the cage's runtime through
//! `SyscallRuntime`. The implementations are expected to grow into the Lind-IF
//! lifecycle operations (`init_cage`, `duplicate_cage`, `terminate_cage`,
//! thread operations) driven by Core; see the comments in lind-boot's shims.

/// Runtime-specific handlers for clone, exec, and exit, using the 3i calling
/// convention: one cage ID plus six (argument, argument cage ID) pairs.
#[allow(clippy::too_many_arguments)]
pub trait SyscallRuntime: Send + Sync {
    /// Handle clone/fork.
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32;

    /// Handle exec.
    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32;

    /// Handle the exit of a single thread.
    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32;
}
