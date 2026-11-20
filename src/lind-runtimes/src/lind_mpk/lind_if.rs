//! MPK implementation of the draft Lind-IF.
//!
//! Core drives clone (`duplicate_cage`/`duplicate_thread`), exec
//! (`init_cage`, `create_thread`, `apply_staging_cage`) and exit
//! (`terminate_thread`, `terminate_cage`) through this interface. The initial
//! cage is still booted by lind-boot (`execute_mpk`).

use super::execute::{exec_mpk_internal, prepare_mpk_exec, GuestPtrArray, PreparedMpkExec};
use super::pkeys;
use super::syscalls::{defer_thread_exit, mpk_clone, release_cage_runtime};
use crate::staging::StagingTable;
use crate::{memory, signal, unsupported};
use lind_if::{
    AbiDescriptor, CageId, ConstLocalBuffer, Errno, GuestAddress, HandlerReturn, HostAddress,
    LindAbiConversion, LindArgVec, LindCloneArgs, LindError, LindHandlerCall, LindInitArgs,
    LindIovecAccess, LindMmapArgs, LindMprotectArgs, LindMunmapArgs, LindResult, LindRuntime,
    LindThreadDuplicateArgs, LocalBuffer, ThreadDuplicateMode, ThreadId, UserPtr,
};
use typemap::datatype_conversion::sc_convert_ptr_to_host;
use wasmtime_lind_multi_process::THREAD_START_ID;

/// Exec images prepared by `init_cage` until `apply_staging_cage` replaces the
/// target's image with them.
static STAGED_EXECS: StagingTable<PreparedMpkExec> = StagingTable::new();

/// A guest argv/envp array. The array and its strings are guest addresses of
/// the owning cage; in a gs-forked cage they are shifted by its logical
/// offset.
fn guest_ptr_array(ptr: &UserPtr) -> GuestPtrArray {
    GuestPtrArray {
        ptr: sc_convert_ptr_to_host(ptr.address, ptr.cage_id) as *const u64,
        cageid: ptr.cage_id,
    }
}

/// The errno of an exec failure: the OS error behind it, if any.
fn exec_error(error: &anyhow::Error, default: Errno) -> LindError {
    let errno = error
        .downcast_ref::<std::io::Error>()
        .and_then(std::io::Error::raw_os_error)
        .and_then(|errno| Errno::from_discriminant(errno).ok());
    LindError::Other(errno.unwrap_or(default))
}

#[derive(Debug, Default)]
pub struct MpkRuntime;

impl LindRuntime for MpkRuntime {
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
            // The initial cage is booted by lind-boot (`execute_mpk`), which
            // loads the program and jumps into it on the calling thread.
            // Implementation steps:
            // 1. Reserve the cage memory through 3i and load ld.so and the
            //    program into the cage vmmap (execute_mpk steps 2-3).
            // 2. Keep the loaded image pending for `create_thread`, which sets
            //    up a supervisor stack on a new OS thread and jumps into ld.so.
            return unsupported();
        }
        let (LindArgVec::Guest(argv), LindArgVec::Guest(envp)) = (&init_args.argv, &init_args.envp)
        else {
            // Host vectors (built by Core, e.g. for the initial cage) are not
            // passed by exec.
            return unsupported();
        };
        // Copy everything out of the old image now; a failure leaves it
        // untouched and returns the errno to it.
        let image = prepare_mpk_exec(exe_path, guest_ptr_array(argv), guest_ptr_array(envp))
            .map_err(|error| exec_error(&error, Errno::EINVAL))?;
        STAGED_EXECS.insert(cage_id, image)
    }

    fn apply_staging_cage(&self, staging_id: CageId, target_cage_id: CageId) -> LindResult<()> {
        let staged = STAGED_EXECS
            .take(staging_id)
            .ok_or(LindError::Other(Errno::EINVAL))?;
        if staged.main_thread.is_none() {
            return Err(LindError::Other(Errno::EINVAL));
        }
        // Tears down the target's namespace and memory, loads the staged
        // program and jumps into it as the initial thread on the calling OS
        // thread; it only returns on failure. The target must be an MPK cage;
        // switching runtimes on exec is not supported (see lind-boot's exec
        // shim).
        //
        // TODO: failures after the old image is torn down (loading the
        // program, setting up its stack) are past the point of no return but
        // are still returned to the destroyed image. Implementation steps:
        // 1. Load ld.so and the program into a fresh reservation before
        //    tearing down the old namespace, so these failures return here.
        // 2. Terminate the cage on any later failure instead of returning.
        match exec_mpk_internal(target_cage_id, staged.image) {
            Ok(_) => Ok(()),
            Err(error) => {
                eprintln!("[lind-mpk] exec failed: {error:#}");
                Err(exec_error(&error, Errno::ENOEXEC))
            }
        }
    }

    fn duplicate_cage(&self, cage_id: CageId, new_cage_id: CageId) -> LindResult<()> {
        if cage_id == new_cage_id {
            return Err(LindError::Other(Errno::EINVAL));
        }
        // `gs_fork` still duplicates the cage-level state (address-space
        // copy, child MPKRuntimeInfo, context stack) together with the thread
        // in `duplicate_thread`, because it copies memory at the selected
        // thread's capture point.
        // TODO: split `gs_fork` so the reservation, MPKRuntimeInfo, and
        // registry entry for `new_cage_id` are created here and only the
        // capture and memory snapshot remain in `duplicate_thread`.
        Ok(())
    }

    fn terminate_cage(&self, cage_id: CageId) -> LindResult<()> {
        // A staging cage only holds its copied arguments.
        if STAGED_EXECS.take(cage_id).is_some() {
            return Ok(());
        }
        // Core calls this for the cage's last thread once it exited
        // (`LindCore::thread_exited`), so only the cage-level state
        // remains. Stopping threads that still run (exit_group's
        // epoch_kill_all) is done by RawPOSIX before they exit.
        release_cage_runtime(cage_id);
        Ok(())
    }

    fn create_thread(
        &self,
        cage_id: CageId,
        thread_id: ThreadId,
        _args: &LindCloneArgs,
    ) -> LindResult<()> {
        // Only the initial thread of a staged exec image. It runs as
        // THREAD_START_ID on the execing OS thread, which jumps into ld.so in
        // `apply_staging_cage`; entry and stack come from the loaded program.
        if thread_id != THREAD_START_ID as ThreadId {
            return Err(LindError::Other(Errno::EINVAL));
        }
        if STAGED_EXECS.set_main_thread(cage_id, thread_id)? {
            return Ok(());
        }
        // Threads of running cages are created by `duplicate_thread`.
        // Implementation steps for initial threads outside exec:
        // 1. Take the image prepared by a non-staging `init_cage`.
        // 2. Spawn an OS thread with a supervisor stack that registers the
        //    thread with Core (`lind_signal_init`) and jumps into ld.so.
        unsupported()
    }

    fn duplicate_thread(
        &self,
        source_cage_id: CageId,
        _source_thread_id: ThreadId,
        destination_cage_id: CageId,
        new_thread_id: ThreadId,
        args: &LindThreadDuplicateArgs,
    ) -> LindResult<()> {
        // The clone entry derives fork vs. thread from the clone flags again
        // and treats a zero child cage as a thread clone.
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
                // 1. Require the source thread to be stopped by Core with its
                //    interrupted register state saved (signal frame), not at
                //    a syscall entry.
                // 2. Build the child as in `gs_fork`, but resume it from the
                //    saved register state on a copy of the guest stack instead
                //    of a syscall return, without injecting a return value.
                // 3. Leave the source's saved state untouched so Core can
                //    restore its prior stop state.
                return unsupported();
            }
        };
        // Deviation: `set_parent_tid`/`set_child_tid` are unused; the thread
        // path performs the CLONE_PARENT_SETTID/CLONE_CHILD_SETTID stores
        // from `clone_args` itself.
        // The parent continuation returns through the calling handlers, where
        // Core returns `parent_result`. Children resume at the guest syscall
        // entry with `child_result` and do not return here.
        let ret = mpk_clone(
            args.clone_args.address,
            args.clone_args.cage_id,
            source_cage_id,
            child_cageid,
            new_thread_id,
            args.child_result,
        );
        if ret < 0 {
            return Err(LindError::from_errno_return(ret));
        }
        match args.mode {
            // The GS-fork child already runs on its own host thread. Hold the
            // parent while Core has it registered as a CLONE_VFORK child.
            // (Not with `mpk-process-fork`: the parent cannot observe that
            // child's exec/exit in this table.)
            ThreadDuplicateMode::SyscallFork if !cfg!(feature = "mpk-process-fork") => {
                cage::vfork_wait(destination_cage_id)
            }
            _ => {}
        }
        Ok(())
    }

    fn terminate_thread(&self, cage_id: CageId, _thread_id: ThreadId, status: i32) -> LindResult<()> {
        // `thread_id` must be the calling thread, which is identified by its
        // OS thread instead. This only commits the thread to exit; it is
        // retired at its next scheduling point, once the syscall handler
        // returned through 3i (`retire_pending_exit`), so locks and guards
        // on the call path are released first.
        // Terminating another thread is not supported. Implementation steps:
        // 1. Mark the thread for termination and interrupt it through its
        //    epoch (as exit_group's epoch_kill_all does).
        // 2. The interrupted thread enters the supervisor and is retired at
        //    that scheduling point as above.
        defer_thread_exit(cage_id, status);
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
        cage_id: CageId,
        args: &LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress> {
        if is_reservation {
            return unsupported();
        }
        memory::mmap_result(unsafe {
            pkeys::cage_mmap_hook(
                cage_id,
                args.address as usize,
                args.length as usize,
                args.prot,
                args.flags,
                args.fd,
                args.offset,
            )
            .unwrap_or_else(|| {
                libc::mmap(
                    args.address as *mut libc::c_void,
                    args.length as usize,
                    args.prot,
                    args.flags,
                    args.fd,
                    args.offset,
                ) as usize
            }) as *mut libc::c_void
        })
    }

    fn do_mprotect(&self, _cage_id: CageId, args: &LindMprotectArgs) -> LindResult<()> {
        memory::mprotect(args)
    }

    fn do_munmap(&self, _cage_id: CageId, args: &LindMunmapArgs) -> LindResult<()> {
        memory::munmap(args)
    }
}

impl<T> LindAbiConversion<T> for MpkRuntime {
    fn get_runtime_from_lind(
        &self,
        _abi: AbiDescriptor,
        _input: *const T,
        _output: LocalBuffer,
    ) -> LindResult<()> {
        unsupported()
    }

    fn get_lind_from_runtime(
        &self,
        _abi: AbiDescriptor,
        _input: ConstLocalBuffer,
        _output: *mut T,
    ) -> LindResult<()> {
        unsupported()
    }
}

impl LindIovecAccess for MpkRuntime {
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
