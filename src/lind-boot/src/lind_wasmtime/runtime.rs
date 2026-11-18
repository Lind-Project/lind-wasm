//! The Wasmtime backend of Lind-IF: an execution service plus the `LindRuntime` operations.

use crate::cli::CliOptions;
use crate::lind_wasmtime::execute::{execute_with_lind, read_main_wasm_or_cwasm};
use crate::lind_wasmtime::host::HostCtx;
use crate::lind_wasmtime::trampoline::grate_callback_trampoline;
use anyhow::{Context, Result, anyhow};
use lind_core::LindCageManager;
use lind_runtime::*;
use std::collections::HashMap;
use std::ffi::CString;
use std::io::Read;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID};
use threei::threei_const::RUNTIME_TYPE_WASMTIME;
use wasmtime::{Engine, Module, Val};
use wasmtime_lind_multi_process as multi_process;

type Work = Box<dyn FnOnce() + Send + 'static>;

pub(crate) struct WasmExecutionService {
    work: Sender<Work>,
    worker: JoinHandle<()>,
}

impl WasmExecutionService {
    pub(crate) fn new(stack_size: usize) -> Result<Self> {
        let (work, pending) = mpsc::channel::<Work>();
        let worker = thread::Builder::new()
            .name("lind-wasm-runtime".to_owned())
            .stack_size(stack_size)
            .spawn(move || {
                while let Ok(entry) = pending.recv() {
                    entry();
                }
            })
            .context("failed to start Wasm runtime worker")?;

        Ok(Self { work, worker })
    }

    pub(crate) fn submit<R: Send + 'static>(
        &self,
        entry: impl FnOnce() -> R + Send + 'static,
    ) -> Result<Receiver<R>> {
        let (result, completed) = mpsc::sync_channel(1);
        self.work
            .send(Box::new(move || {
                if result.send(entry()).is_err() {
                    eprintln!("Wasm runtime execution result has no receiver");
                }
            }))
            .map_err(|_| anyhow!("Wasm runtime worker is not running"))?;
        Ok(completed)
    }

    pub(crate) fn shutdown(self) -> Result<()> {
        let Self { work, worker } = self;
        drop(work);
        worker
            .join()
            .map_err(|_| anyhow!("Wasm runtime worker panicked"))?;
        Ok(())
    }
}

/// Images prepared by `InitCage` that are waiting for `CreateThread` / `ApplyStagingCage`.
enum PreparedImage {
    Boot {
        module: Module,
    },
    Exec {
        exe_path: String,
        argv: UserPtr,
        envp: UserPtr,
        thread_created: bool,
    },
}

/// Lind-IF backend for Wasmtime.
///
/// Fork, pthread clone, exec and exit still run through the Asyncify-based paths in
/// `wasmtime-lind-multi-process`; the operations below adapt them to the Lind-IF shape.
pub(crate) struct WasmRuntime {
    service: Mutex<Option<WasmExecutionService>>,
    boot_cli: CliOptions,
    engine: Engine,
    lind_manager: Arc<LindCageManager>,
    prepared: Mutex<HashMap<CageId, PreparedImage>>,
    root_result: Mutex<Option<Receiver<Result<i32>>>>,
}

fn errno(errno: Errno) -> LindError {
    LindError::Other(errno)
}

fn legacy_result(ret: i32) -> LindResult<()> {
    from_syscall_ret(ret as i64).map(|_| ())
}

impl WasmRuntime {
    pub(crate) fn new(
        boot_cli: CliOptions,
        engine: Engine,
        lind_manager: Arc<LindCageManager>,
    ) -> Result<Self> {
        let service = WasmExecutionService::new(boot_cli.thread_stack_size)?;
        Ok(Self {
            service: Mutex::new(Some(service)),
            boot_cli,
            engine,
            lind_manager,
            prepared: Mutex::new(HashMap::new()),
            root_result: Mutex::new(None),
        })
    }

    /// Completion of the initial cage's guest entry (its exit code), available after the
    /// initial `CreateThread`.
    pub(crate) fn take_root_result(&self) -> Option<Receiver<Result<i32>>> {
        self.root_result.lock().unwrap().take()
    }

    pub(crate) fn shutdown(&self) -> Result<()> {
        match self.service.lock().unwrap().take() {
            Some(service) => service.shutdown(),
            None => Ok(()),
        }
    }

    fn start_initial_thread(&self, cage: CageId, module: Module) -> LindResult<()> {
        let cli = self.boot_cli.clone();
        let engine = self.engine.clone();
        let manager = Arc::clone(&self.lind_manager);
        let service = self.service.lock().unwrap();
        let service = service.as_ref().ok_or(errno(Errno::EAGAIN))?;
        let completed = service
            .submit(move || {
                execute_with_lind(cli, manager, engine, module, cage, None).map(|ret_vals| {
                    match ret_vals.first() {
                        Some(Val::I32(code)) => *code,
                        _ => 0,
                    }
                })
            })
            .map_err(|_| errno(Errno::EAGAIN))?;
        *self.root_result.lock().unwrap() = Some(completed);
        Ok(())
    }
}

impl LindRuntime for WasmRuntime {
    fn runtime_id(&self) -> RuntimeId {
        RUNTIME_TYPE_WASMTIME
    }

    /// Accepts Wasm modules, precompiled `.cwasm` (ELF) modules and shebang scripts, matching
    /// what the exec loader can handle. Unreadable files are left to the loader to reject.
    fn belongs_to_runtime(&self, exe_path: &str) -> LindResult<bool> {
        let path = std::path::Path::new(exe_path);
        if !path.exists() {
            return Err(errno(Errno::ENOENT));
        }
        let mut magic = [0u8; 4];
        let read = std::fs::File::open(path).and_then(|mut file| file.read(&mut magic));
        match read {
            Ok(n) if n >= 2 && &magic[..2] == b"#!" => Ok(true),
            Ok(4) => Ok(&magic == b"\0asm" || &magic == b"\x7fELF"),
            Ok(_) => Ok(false),
            Err(_) => Ok(true),
        }
    }

    /// Guest addresses are offsets into the cage's linear memory.
    fn get_host_address(&self, cage: CageId, guest: GuestAddress) -> LindResult<HostAddress> {
        let cage = cage::get_cage(cage).ok_or(errno(Errno::ESRCH))?;
        let base = cage.vmmap.read().base_address.ok_or(errno(Errno::EFAULT))?;
        Ok(base as u64 + guest)
    }

    fn get_guest_address(&self, cage: CageId, host: HostAddress) -> LindResult<GuestAddress> {
        let cage = cage::get_cage(cage).ok_or(errno(Errno::ESRCH))?;
        let base = cage.vmmap.read().base_address.ok_or(errno(Errno::EFAULT))? as u64;
        host.checked_sub(base).ok_or(errno(Errno::EFAULT))
    }

    fn get_cage_abi(&self, cage: CageId) -> LindResult<AbiDescriptor> {
        match threei::get_cage_runtime(cage) {
            Some(RUNTIME_TYPE_WASMTIME) => Ok(AbiDescriptor::Wasm32),
            _ => Err(errno(Errno::ENOENT)),
        }
    }

    /// Boot: compile the module now so load errors surface before any guest runs.
    /// Exec staging: record the request; the legacy exec path loads the module during apply.
    /// The linear-memory reservation is created by Wasmtime itself, not via `DoMmap`.
    fn init_cage(
        &self,
        cage: CageId,
        exe_path: &str,
        init_args: LindInitArgs,
        is_staging: bool,
    ) -> LindResult<()> {
        let image = if is_staging {
            let (LindArgVector::User(argv), LindArgVector::User(envp)) =
                (init_args.argv, init_args.envp)
            else {
                return Err(errno(Errno::EINVAL));
            };
            PreparedImage::Exec {
                exe_path: exe_path.to_owned(),
                argv,
                envp,
                thread_created: false,
            }
        } else {
            // argv/env come from the launcher's CLI configuration, which built `init_args`.
            let module = read_main_wasm_or_cwasm(&self.engine, &self.boot_cli).map_err(|e| {
                eprintln!("{e:?}");
                errno(Errno::ENOEXEC)
            })?;
            PreparedImage::Boot { module }
        };
        let mut prepared = self.prepared.lock().unwrap();
        if prepared.contains_key(&cage) {
            return Err(errno(Errno::EEXIST));
        }
        prepared.insert(cage, image);
        Ok(())
    }

    /// Runs the legacy exec path on the target's main thread. It returns 0 once the Asyncify
    /// unwind towards the replacement has started (committed) and a negative errno while the
    /// old image is still intact.
    fn apply_staging_cage(&self, staging: CageId, target: CageId) -> LindResult<()> {
        let image = self.prepared.lock().unwrap().remove(&staging);
        let Some(PreparedImage::Exec {
            exe_path,
            argv,
            envp,
            thread_created: true,
        }) = image
        else {
            return Err(errno(Errno::EINVAL));
        };
        let path = CString::new(exe_path).map_err(|_| errno(Errno::ENOENT))?;
        legacy_result(multi_process::exec_syscall::<HostCtx, CliOptions>(
            WASMTIME_CAGEID,
            path.as_ptr() as u64,
            target,
            argv.addr,
            argv.cage,
            envp.addr,
            envp.cage,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        ))
    }

    /// The child's linear memory is copied when the duplicated thread starts after the parent's
    /// Asyncify unwind (the unwind state lives in linear memory), so nothing happens here yet.
    fn duplicate_cage(&self, _cage: CageId, _new_cage: CageId) -> LindResult<()> {
        Ok(())
    }

    /// Releases staging images. Live cages release their Wasmtime state in the legacy exit path,
    /// which is also where Core finalizes them.
    fn terminate_cage(&self, cage: CageId) -> LindResult<()> {
        self.prepared.lock().unwrap().remove(&cage);
        Ok(())
    }

    fn create_thread(&self, cage: CageId, thread: ThreadId, _args: LindCloneArgs) -> LindResult<()> {
        if thread != lind_core::MAIN_THREAD_ID {
            return Err(errno(Errno::ENOSYS));
        }
        let mut prepared = self.prepared.lock().unwrap();
        match prepared.get_mut(&cage) {
            // Staging threads stay held until ApplyStagingCage.
            Some(PreparedImage::Exec { thread_created, .. }) => {
                *thread_created = true;
                Ok(())
            }
            Some(PreparedImage::Boot { .. }) => {
                let Some(PreparedImage::Boot { module }) = prepared.remove(&cage) else {
                    unreachable!()
                };
                drop(prepared);
                self.start_initial_thread(cage, module)
            }
            None => Err(errno(Errno::ESRCH)),
        }
    }

    fn duplicate_thread(
        &self,
        source_cage: CageId,
        source_thread: ThreadId,
        destination_cage: CageId,
        _new_thread: ThreadId,
        args: LindThreadDuplicateArgs,
    ) -> LindResult<()> {
        let child_cage = match args.mode {
            ThreadDuplicateMode::SyscallFork => destination_cage,
            ThreadDuplicateMode::PthreadClone => 0,
            ThreadDuplicateMode::RawInterrupted => return Err(errno(Errno::ENOSYS)),
        };
        // The clone flags in `clone_args` select fork vs. thread inside the legacy path.
        legacy_result(multi_process::clone_syscall::<HostCtx, CliOptions>(
            WASMTIME_CAGEID,
            args.clone_args.addr,
            args.clone_args.cage,
            source_cage,
            source_thread as u64,
            child_cage,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        ))
    }

    /// Only self-termination is supported: the legacy exit path unwinds the calling thread.
    fn terminate_thread(&self, cage: CageId, thread: ThreadId) -> LindResult<()> {
        let status = lind_core::exit::take_thread_exit_status(cage, thread).unwrap_or(0);
        legacy_result(multi_process::exit_syscall::<HostCtx, CliOptions>(
            WASMTIME_CAGEID,
            status as u64,
            cage,
            thread as u64,
            UNUSED_ARG,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        ))
    }

    fn send_signal(&self, cage: CageId, thread: ThreadId, info: LindSigInfo) -> LindResult<()> {
        if thread != ANY_THREAD {
            return Err(errno(Errno::ENOSYS));
        }
        if cage::signal::lind_send_signal(cage, info.signo) {
            Ok(())
        } else {
            Err(errno(Errno::ESRCH))
        }
    }

    fn run_handler(&self, cage: CageId, args: LindHandlerArgs) -> LindResult<HandlerReturn> {
        let [a1, a2, a3, a4, a5, a6] = args.args;
        Ok(grate_callback_trampoline(
            args.handler,
            cage,
            a1.0,
            a1.1,
            a2.0,
            a2.1,
            a3.0,
            a3.1,
            a4.0,
            a4.1,
            a5.0,
            a5.1,
            a6.0,
            a6.1,
        ))
    }

    fn do_mmap(
        &self,
        _cage: CageId,
        args: LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress> {
        if is_reservation {
            return Err(errno(Errno::ENOSYS));
        }
        host_memory::mmap(args)
    }

    fn do_mprotect(&self, _cage: CageId, args: LindMprotectArgs) -> LindResult<()> {
        host_memory::mprotect(args)
    }

    fn do_munmap(&self, _cage: CageId, args: LindMunmapArgs) -> LindResult<()> {
        host_memory::munmap(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn submitted_execution_runs_without_blocking_the_caller() {
        let runtime = WasmExecutionService::new(2 * 1024 * 1024).unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let completed = runtime
            .submit(move || {
                started_tx.send(thread::current().id()).unwrap();
                release_rx.recv().unwrap();
                42
            })
            .unwrap();

        let worker_id = started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_ne!(worker_id, thread::current().id());
        assert!(matches!(
            completed.try_recv(),
            Err(mpsc::TryRecvError::Empty)
        ));
        release_tx.send(()).unwrap();
        assert_eq!(completed.recv_timeout(Duration::from_secs(5)).unwrap(), 42);

        let next = runtime.submit(|| 7).unwrap();
        assert_eq!(next.recv_timeout(Duration::from_secs(5)).unwrap(), 7);
        runtime.shutdown().unwrap();
    }

    #[test]
    fn worker_failure_is_reported() {
        let runtime = WasmExecutionService::new(2 * 1024 * 1024).unwrap();
        let completed = runtime.submit(|| panic!("worker failed")).unwrap();
        assert!(completed.recv_timeout(Duration::from_secs(5)).is_err());
        let deadline = Instant::now() + Duration::from_secs(5);
        while !runtime.worker.is_finished() {
            assert!(Instant::now() < deadline, "worker did not finish");
            thread::yield_now();
        }
        assert!(runtime.submit(|| ()).is_err());
        assert!(runtime.shutdown().is_err());
    }
}
