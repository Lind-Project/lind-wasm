//! Wasm handling of the clone, exec, and exit syscalls.

use crate::lifecycle::SyscallRuntime;
use std::marker::PhantomData;
use wasmtime_lind_multi_process::{clone_syscall, exec_syscall, exit_syscall, LindHost};

/// The Wasm runtime's clone, exec, and exit handlers.
///
/// Wasmtime's multi-process support is generic over the launcher's
/// per-instance host state `T` and launch configuration `U` (lind-boot's
/// `HostCtx` and `CliOptions`), so the launcher names those types when it
/// instantiates this handler.
pub struct WasmLifecycle<T, U>(PhantomData<fn() -> (T, U)>);

impl<T, U> WasmLifecycle<T, U> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> Default for WasmLifecycle<T, U> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, U> SyscallRuntime for WasmLifecycle<T, U>
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    // arg1 = clone_args, arg2 = parent cage, arg3 = child cage (0 for a
    // thread clone).
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
    ) -> i32 {
        clone_syscall::<T, U>(
            cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        )
    }

    // arg1 = path, arg2 = argv, arg3 = envp. Re-instantiates the module in
    // the cage.
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
    ) -> i32 {
        exec_syscall::<T, U>(
            cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        )
    }

    // arg1 = exit code. The last thread to exit finalizes the cage.
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
    ) -> i32 {
        exit_syscall::<T, U>(
            cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        )
    }
}
