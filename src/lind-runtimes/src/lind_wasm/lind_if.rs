//! Wasm implementation of the draft Lind-IF.
//!
//! Core drives clone (`duplicate_cage`/`duplicate_thread`), exec
//! (`init_cage`, `create_thread`, `apply_staging_cage`) and exit
//! (`terminate_thread`, `terminate_cage`) through this interface. The initial
//! cage is still booted by lind-boot (`exec_wasm`).

use crate::lifecycle::{errno_error, take_thread_exit_status, StagingTable};
use crate::{memory, signal, unsupported};
use lind_if::{
    AbiDescriptor, CageId, ConstLocalBuffer, Errno, GuestAddress, HandlerReturn, HostAddress,
    LindAbiConversion, LindArgVec, LindCloneArgs, LindError, LindHandlerCall, LindInitArgs,
    LindIovecAccess, LindMmapArgs, LindMprotectArgs, LindMunmapArgs, LindResult, LindRuntime,
    LindThreadDuplicateArgs, LocalBuffer, ThreadDuplicateMode, ThreadId,
};
use std::marker::PhantomData;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID};
use typemap::datatype_conversion::sc_convert_ptr_to_host;
use wasmtime_lind_multi_process::{
    clone_syscall, exec_apply, exit_thread, prepare_exec, release_cage_contexts, LindHost,
    PreparedExec, THREAD_START_ID,
};

/// Exec images loaded by `init_cage` until `apply_staging_cage` replaces the
/// target's image with them.
static STAGED_EXECS: StagingTable<PreparedExec> = StagingTable::new();

/// The Wasm runtime.
///
/// Wasmtime's multi-process support is generic over the launcher's
/// per-instance host state `T` and launch configuration `U` (lind-boot's
/// `HostCtx` and `CliOptions`), so the launcher names those types when it
/// instantiates the runtime.
pub struct WasmRuntime<T, U>(PhantomData<fn() -> (T, U)>);

impl<T, U> WasmRuntime<T, U> {
    pub const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<T, U> Default for WasmRuntime<T, U> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, U> LindRuntime for WasmRuntime<T, U>
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    fn belongs_to_runtime(&self, _exe_path: &str) -> LindResult<bool> {
        unsupported()
    }

    fn get_host_address(
        &self,
        _cage_id: CageId,
        _guest_address: GuestAddress,
    ) -> LindResult<HostAddress> {
        unsupported()
    }

    fn get_guest_address(
        &self,
        _cage_id: CageId,
        _host_address: HostAddress,
    ) -> LindResult<GuestAddress> {
        unsupported()
    }

    fn get_cage_abi(&self, _cage_id: CageId) -> LindResult<AbiDescriptor> {
        unsupported()
    }

    fn init_cage(
        &self,
        cage_id: CageId,
        exe_path: &str,
        init_args: &LindInitArgs,
        is_staging: bool,
    ) -> LindResult<()> {
        if !is_staging {
            // The initial cage is booted by lind-boot (`exec_wasm`), which
            // creates the Store and runs the entry point directly.
            // Implementation steps:
            // 1. Load and instantiate `exe_path` for `cage_id` without running
            //    its entry point, reserving its memory through 3i.
            // 2. Keep the instance pending for `create_thread`, which runs the
            //    entry point on a new host thread.
            return unsupported();
        }
        let (LindArgVec::Guest(argv), LindArgVec::Guest(envp)) = (&init_args.argv, &init_args.envp)
        else {
            // Host vectors (built by Core, e.g. for the initial cage) need a
            // parser for host byte strings; exec only passes guest vectors.
            return unsupported();
        };
        // Load the module now so that a failing exec returns its errno to the
        // old image, which stays untouched.
        let image = prepare_exec(
            exe_path.to_owned(),
            sc_convert_ptr_to_host(argv.address, argv.cage_id),
            sc_convert_ptr_to_host(envp.address, envp.cage_id),
        )
        .map_err(|errno| errno_error(-errno))?;
        STAGED_EXECS.insert(cage_id, image)
    }

    fn apply_staging_cage(&self, staging_id: CageId, target_cage_id: CageId) -> LindResult<()> {
        let staged = STAGED_EXECS
            .take(staging_id)
            .ok_or(LindError::Other(Errno::EINVAL))?;
        if staged.main_thread.is_none() {
            return Err(LindError::Other(Errno::EINVAL));
        }
        // Point of no return: unwinds the target's old image on the calling
        // thread (its main thread context) and, once the unwind finished,
        // instantiates the staged module and runs its entry point as the
        // initial thread. The target must be a Wasm cage; switching runtimes
        // on exec is not supported (see lind-boot's exec shim).
        let ret = exec_apply::<T, U>(target_cage_id, staged.image);
        if ret < 0 {
            return Err(errno_error(ret));
        }
        Ok(())
    }

    fn duplicate_cage(&self, cage_id: CageId, new_cage_id: CageId) -> LindResult<()> {
        if cage_id == new_cage_id {
            return Err(LindError::Other(Errno::EINVAL));
        }
        // Materialization is deferred: `fork_call` builds the child Store,
        // instance, memory copy, and vmmap in its OnCalled callback after the
        // source thread's Asyncify unwind, which `duplicate_thread` starts.
        // TODO: move the cage-level part (child linker/Store creation and the
        // memory copy) out of `fork_call` and register it here as pending
        // state for `new_cage_id`, so `duplicate_thread` only captures the
        // continuation.
        Ok(())
    }

    fn terminate_cage(&self, cage_id: CageId) -> LindResult<()> {
        // A staging cage only holds its loaded module.
        if STAGED_EXECS.take(cage_id).is_some() {
            return Ok(());
        }
        // Core calls this for the cage's last thread once it exited
        // (`LindCore::thread_exited`), so only the metadata remains.
        // Stopping threads that still run (exit_group's epoch_kill_all) is
        // done by RawPOSIX before they exit.
        release_cage_contexts(cage_id);
        Ok(())
    }

    fn create_thread(
        &self,
        cage_id: CageId,
        thread_id: ThreadId,
        _args: &LindCloneArgs,
    ) -> LindResult<()> {
        // Only the initial thread of a staged exec image. The new instance's
        // entry point runs as THREAD_START_ID on the execing host thread once
        // `apply_staging_cage` unwound the old image; entry and stack come
        // from the module.
        if thread_id != THREAD_START_ID as ThreadId {
            return Err(LindError::Other(Errno::EINVAL));
        }
        if STAGED_EXECS.set_main_thread(cage_id, thread_id)? {
            return Ok(());
        }
        // Threads of running cages are created by `duplicate_thread`.
        // Implementation steps for initial threads outside exec:
        // 1. Take the instance prepared by a non-staging `init_cage`.
        // 2. Spawn a host thread that registers the thread with Core
        //    (`lind_signal_init`) and runs the entry point.
        unsupported()
    }

    fn duplicate_thread(
        &self,
        source_cage_id: CageId,
        source_thread_id: ThreadId,
        destination_cage_id: CageId,
        _new_thread_id: ThreadId,
        args: &LindThreadDuplicateArgs,
    ) -> LindResult<()> {
        // `clone_syscall` derives fork vs. thread from the clone flags again and
        // treats a zero child cage as a thread clone.
        let child_cageid = match args.mode {
            ThreadDuplicateMode::SyscallFork if destination_cage_id != source_cage_id => {
                destination_cage_id
            }
            ThreadDuplicateMode::PthreadClone if destination_cage_id == source_cage_id => 0,
            ThreadDuplicateMode::SyscallFork | ThreadDuplicateMode::PthreadClone => {
                return Err(LindError::Other(Errno::EINVAL));
            }
            ThreadDuplicateMode::RawInterrupted => {
                // Implementation steps:
                // 1. Require the source thread to be stopped by Core outside a
                //    syscall, with its interrupted Wasm state (epoch/signal
                //    preemption point) saved by the runtime.
                // 2. Snapshot the source memory and globals at that point
                //    (the same copy `fork_call` makes after an unwind).
                // 3. Build the child instance and resume it at the saved
                //    interruption point without injecting fork return values.
                // 4. Leave the source's saved state untouched so Core can
                //    restore its prior stop state.
                return unsupported();
            }
        };
        // Deviations until Core owns the IDs and TID stores:
        // - `new_thread_id` is unused: `pthread_create_call` allocates the TID
        //   and the forked child's main thread is always thread 1.
        // - `set_parent_tid`/`set_child_tid` are unused: the thread path
        //   stores the new TID at `clone_args.child_tid` itself (glibc's
        //   CLONE_CHILD_CLEARTID address) and CLONE_PARENT_SETTID is not yet
        //   implemented.
        // - Both continuations get their result on the Asyncify rewind (child
        //   0, parent the child cage ID or new TID), so no clone result is
        //   recorded here.
        // - The fork itself runs after this returns, in the parent's deferred
        //   unwind callback (`fork_call`), so a CLONE_VFORK parent is held
        //   there via `cage::vfork_wait` rather than here.
        let ret = clone_syscall::<T, U>(
            WASMTIME_CAGEID,
            args.clone_args.address,
            args.clone_args.cage_id,
            source_cage_id,
            source_thread_id as u64,
            child_cageid,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        );
        if ret < 0 {
            return Err(errno_error(ret));
        }
        Ok(())
    }

    fn terminate_thread(&self, cage_id: CageId, thread_id: ThreadId) -> LindResult<()> {
        // `thread_id` must be the calling thread. `exit_thread` commits it
        // to exit by starting an Asyncify unwind and returns. The scheduling
        // point is the end of the unwind: there the thread is retired with
        // Core (`LindCore::thread_exited`) and its host thread returns the
        // status.
        // Terminating another thread is not supported. Implementation steps:
        // 1. Mark the thread for termination and interrupt it through its
        //    epoch (as exit_group's epoch_kill_all does).
        // 2. The interrupted thread unwinds and completes its exit as above.
        // 3. Wait until Core no longer lists the thread.
        exit_thread::<T, U>(cage_id, thread_id as u64, take_thread_exit_status());
        Ok(())
    }

    fn notify_signal(&self, cage_id: CageId, thread_id: ThreadId) -> LindResult<()> {
        signal::notify_epoch_thread(cage_id, thread_id)
    }

    fn run_handler(&self, _cage_id: CageId, _call: &LindHandlerCall) -> LindResult<HandlerReturn> {
        unsupported()
    }

    fn do_mmap(
        &self,
        _cage_id: CageId,
        args: &LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress> {
        if is_reservation {
            return unsupported();
        }
        memory::mmap_result(unsafe {
            libc::mmap(
                args.address as *mut libc::c_void,
                args.length as usize,
                args.prot,
                args.flags,
                args.fd,
                args.offset,
            )
        })
    }

    fn do_mprotect(&self, _cage_id: CageId, args: &LindMprotectArgs) -> LindResult<()> {
        memory::mprotect(args)
    }

    fn do_munmap(&self, _cage_id: CageId, args: &LindMunmapArgs) -> LindResult<()> {
        memory::munmap(args)
    }
}

impl<T, U, S> LindAbiConversion<S> for WasmRuntime<T, U> {
    fn get_runtime_from_lind(
        &self,
        _abi: AbiDescriptor,
        _input: *const S,
        _output: LocalBuffer,
    ) -> LindResult<()> {
        unsupported()
    }

    fn get_lind_from_runtime(
        &self,
        _abi: AbiDescriptor,
        _input: ConstLocalBuffer,
        _output: *mut S,
    ) -> LindResult<()> {
        unsupported()
    }
}

impl<T, U> LindIovecAccess for WasmRuntime<T, U> {
    fn set_iovec_base(
        &self,
        _abi: AbiDescriptor,
        _iovec: LocalBuffer,
        _index: usize,
        _new_guest_address: GuestAddress,
    ) -> LindResult<()> {
        unsupported()
    }

    fn set_guest_iovec_base(
        &self,
        _abi: AbiDescriptor,
        _owner_cage_id: CageId,
        _iovec_address: GuestAddress,
        _index: usize,
        _new_guest_address: GuestAddress,
    ) -> LindResult<()> {
        unsupported()
    }
}
