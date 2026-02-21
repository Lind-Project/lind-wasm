use crate::{cli::CliOptions, lind_wasmtime::host::HostCtx, lind_wasmtime::trampoline::*};
use anyhow::{Context, Result, anyhow, bail};
use cage::signal::{lind_signal_init, signal_may_trigger};
use cfg_if::cfg_if;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, WASMTIME_CAGEID};
use threei::threei_const;
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::{
    AsContextMut, Engine, Func, InstantiateType, Linker, Module, Precompiled, Store, Val, ValType,
};
use wasmtime_lind_3i::{VmCtxWrapper, init_vmctx_pool, rm_vmctx, set_vmctx, set_vmctx_thread};
use wasmtime_lind_multi_process::{CAGE_START_ID, LindCtx, THREAD_START_ID};
use wasmtime_lind_utils::LindCageManager;
use wasmtime_wasi_threads::WasiThreadsCtx;

/// Boots the Lind + RawPOSIX + 3i runtime and executes the initial Wasm program
/// in the first cage.
///
/// This function is the *only* entry point for the initial launch of `lind-boot`.
/// It performs three high-level tasks.
///
/// First, it initializes the Wasmtime execution environment by creating the
/// engine/store and loading the main module from disk.
///
/// Second, it brings up the Lind runtime by starting RawPOSIX, creating the first
/// cage, and initializing the `VMContext` pool used for later re-entry into Wasmtime
/// during *grate calls*. It also registers the Wasmtime-specific 3i trampoline, which
/// serves as the unified callback path for interposed syscalls routed through 3i.
///
/// Third, it registers the syscall handlers (clone/exec/exit) with 3i exactly once
/// during the initial boot. This is intentional: during `fork()`, RawPOSIX clones
/// the parent process's handler table into the child, so children automatically
/// inherit all registered handlers without additional registration. In contrast,
/// `exec()` replaces the guest program within an existing cage and does not require
/// rebuilding the handler table. Special needs will be handled per user request in
/// their implementation through `register_handler` via glibc.
///
/// After initialization, the function attaches all host-side APIs (WASI preview1,
/// WASI threads, and Lind contexts) to the wasmtime linker, instantiates the module into the
/// starting cage, and runs the program's entrypoint. On successful completion it
/// waits for all cages to exit before shutting down RawPOSIX, ensuring runtime-wide
/// cleanup happens only after the last process terminates.
pub fn execute_wasmtime(lindboot_cli: CliOptions) -> anyhow::Result<Vec<Val>> {
    // -- Initialize the Wasmtime execution environment --
    let wasm_file_path = Path::new(lindboot_cli.wasm_file());
    let args = lindboot_cli.args.clone();
    let wt_config = wasmtime::Config::new();
    let engine = Engine::new(&wt_config).context("failed to create execution engine")?;
    let host = HostCtx::default();
    let mut wstore = Store::new(&engine, host);

    // -- Initialize Lind + RawPOSIX + 3i runtime --
    // Initialize the Lind cage counter
    let lind_manager = Arc::new(LindCageManager::new(0));
    // new cage is created
    lind_manager.increment();

    // Initialize vmctx pool
    init_vmctx_pool();
    // Initialize trampoline entry function pointer for wasmtime runtime.
    // This is for grate calls to re-enter wasmtime runtime.
    threei::register_trampoline(
        threei_const::RUNTIME_TYPE_WASMTIME,
        grate_callback_trampoline,
    );

    // Register syscall handlers (clone/exec/exit) with 3i
    if !register_wasmtime_syscall_entry() {
        panic!("[lind-boot] egister syscall handlers (clone/exec/exit) with 3i failed");
    }

    // -- Load module and Attach host APIs --
    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let module = read_wasm_or_cwasm(&engine, wasm_file_path)?;
    let mut linker = Linker::new(&engine);

    attach_api(
        &mut wstore,
        &mut linker,
        &module,
        lind_manager.clone(),
        lindboot_cli.clone(),
        None,
    )?;

    // -- Run the first module in the first cage --
    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(
            &mut wstore,
            &mut linker,
            &module,
            CAGE_START_ID as u64,
            &args,
        )
        .with_context(|| format!("failed to run main module"))
    });

    match result {
        Ok(ref _res) => {
            // we wait until all other cage exits
            lind_manager.wait();
        }
        Err(e) => {
            // Exit the process if Wasmtime understands the error;
            // otherwise, fall back on Rust's default error printing/return
            // code.
            return Err(wasi_common::maybe_exit_on_error(e));
        }
    }

    result
}

/// Executes a Wasm program *within an existing Lind runtime* as part of an `exec()` path.
///
/// This function is not used for the initial launch. Instead, it is invoked only when
/// the guest issues `exec`, at which point the runtime must load a new Wasm module
/// (or the same module with different args/env) into a target cage while keeping the
/// global Lind/RawPOSIX runtime alive.
///
/// Unlike `execute()`, this function does not call `rawposix_start`, does not create
/// the initial cage manager, and does not register 3i handlers. The handler table is
/// already present: forked processes inherit it via RawPOSIX table cloning, and exec
/// does not require mutating it. The goal here is to perform the minimal work needed
/// to re-create a Wasmtime engine/store, attach host APIs, instantiate the module
/// inside the provided `cageid`, and transfer control to the new guest entrypoint.
pub fn execute_with_lind(
    lind_boot: CliOptions,
    lind_manager: Arc<LindCageManager>,
    cageid: u64,
) -> Result<Vec<Val>> {
    // -- Initialize the Wasmtime execution environment --
    let wasm_file_path = Path::new(lind_boot.wasm_file());
    let args = lind_boot.args.clone();
    let wt_config = wasmtime::Config::new();
    let engine = Engine::new(&wt_config).context("failed to create execution engine")?;
    let host = HostCtx::default();
    let mut wstore = Store::new(&engine, host);

    // -- Load module and Attach host APIs --
    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let module = read_wasm_or_cwasm(&engine, wasm_file_path)?;
    let mut linker = Linker::new(&engine);

    attach_api(
        &mut wstore,
        &mut linker,
        &module,
        lind_manager.clone(),
        lind_boot.clone(),
        Some(cageid as i32),
    )?;

    // -- Run the module in the cage --
    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(&mut wstore, &mut linker, &module, cageid as u64, &args)
            .with_context(|| format!("failed to run main module"))
    });

    result
}

/// Register Wasmtime re-entry trampolines into the 3i handler table.
///
/// During `lind-boot` initialization, we extract raw function pointers for a
/// small set of syscalls whose semantics must be completed inside Wasmtime
/// (e.g., instance/thread creation and termination). These functions act as
/// **Wasmtime re-entry trampolines**:
///
/// ```
///   Wasm
///     -> Wasmtime lind-common trampoline
///     -> 3i dispatch
///     -> RawPOSIX handling
///     -> 3i dispatch
///     -> **back to Wasmtime (registered trampolines)**
/// ```
/// All handlers are registered from the RawPOSIX cage (`RAWPOSIX_CAGEID`)
/// targeting the Wasmtime runtime cage (`WASMTIME_CAGEID`).
///
/// Registered syscalls:
/// - `clone` (56): fork / pthread_create completion in Wasmtime
/// - `exec`  (59): exec completion in Wasmtime (instance replacement / image switch)
/// - `exit`  (60): thread/process termination completion in Wasmtime
fn register_wasmtime_syscall_entry() -> bool {
    // Register clone trampoline (syscall 56).
    let fp_clone = clone_syscall_entry;
    let clone_call_u64: u64 = fp_clone as *const () as usize as u64;
    let clone_ret = threei::register_handler(
        0,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        56,                                  // clone syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // handler function is in the 3i
        clone_call_u64,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    // Register exec trampoline (syscall 59).
    let fp_exec = exec_syscall_entry;
    let exec_call_u64: u64 = fp_exec as *const () as usize as u64;
    let exec_ret = threei::register_handler(
        0,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        59,                                  // exec syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // handler function is in the 3i
        exec_call_u64,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    // Register exit trampoline (syscall 60).
    let fp_exit = exit_syscall_entry;
    let exit_call_u64: u64 = fp_exit as *const () as usize as u64;
    let exit_ret = threei::register_handler(
        0,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        60,                                  // exit syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // handler function is in the 3i
        exit_call_u64,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    // Return false if registration failed
    if (clone_ret | exec_ret | exit_ret) != 0 {
        return false;
    };
    // Succeed
    true
}

/// Attaches all host-side APIs and Lind runtime contexts to the linker and store.
///
/// This function constructs the host interface that the guest expects and stores it
/// inside `HostCtx`. It wires three major subsystems into the Wasmtime instance.
///
/// It first installs WASI Preview1 support and initializes a per-process WASI context
/// (used for args/env and libc-facing interfaces). It then installs WASI threads support,
/// enabling pthread-like execution within the guest.
///
/// Next, it attaches Lind common APIs (for our glibc implementation) and initializes the
/// Lind multi-process context (`LindCtx`) that implements fork/exec semantics.
///
/// The `cageid` parameter allows this function to be used both for the initial boot
/// (where no cage override is needed) and for exec-ed cages (where the target cage is
/// explicitly specified).
fn attach_api(
    wstore: &mut Store<HostCtx>,
    mut linker: &mut Linker<HostCtx>,
    module: &Module,
    lind_manager: Arc<LindCageManager>,
    lindboot_cli: CliOptions,
    cageid: Option<i32>,
) -> Result<()> {
    // Setup WASI-p1
    // --- Why we still attach a WASI preview1 context (WasiCtx) even though we don't use wasi-libc ---
    //
    // Our guest is linked with our customized glibc, whose startup path still follows a WASI-style ABI
    // for *process metadata* (argv/environ). Concretely, our glibc crt1 `_start` expands to:
    //
    //   _start()
    //     -> __wasi_initialize_environ()
    //          -> __wasi_environ_sizes_get()
    //          -> __wasi_environ_get()
    //     -> __main_void()
    //          -> __wasi_args_sizes_get()
    //          -> __wasi_args_get()
    //     -> main(argc, argv, environ)
    //
    // The functions __wasi_* above are thin wrappers around imported WASI preview1 symbols:
    //
    //   __imported_wasi_snapshot_preview1_args_sizes_get  (import "wasi_snapshot_preview1" "args_sizes_get")
    //   __imported_wasi_snapshot_preview1_args_get        (import "wasi_snapshot_preview1" "args_get")
    //   __imported_wasi_snapshot_preview1_environ_sizes_get (import "wasi_snapshot_preview1" "environ_sizes_get")
    //   __imported_wasi_snapshot_preview1_environ_get       (import "wasi_snapshot_preview1" "environ_get")
    //
    // Therefore, even if we bypass wasi-libc and implement syscalls via glibc/RawPOSIX,
    // the guest still expects the *WASI preview1 argument and environment APIs* to exist,
    // otherwise argc/argv/environ cannot be initialized during crt startup (argv[i] becomes NULL,
    // environ becomes empty, or the module traps if the imports are missing).
    //
    // The following two steps are required:
    //   1) Add WASI preview1 functions to the Wasmtime linker (so the imports resolve).
    //   2) Populate a WasiCtx as the backing store for argv/env/std{in,out,err}, so that
    //      args_get/environ_get return meaningful data.
    //
    // Note: This is about process metadata plumbing. Our "real" syscalls are still handled
    // by glibc/RawPOSIX.
    let _ = wasi_common::sync::add_to_linker(&mut linker, |s: &mut HostCtx| {
        AsMut::<wasi_common::WasiCtx>::as_mut(s)
    });

    let mut builder = WasiCtxBuilder::new();
    let _ = builder.inherit_stdio().args(&lindboot_cli.args);
    builder.inherit_stdin();
    builder.inherit_stderr();
    wstore.data_mut().preview1_ctx = Some(builder.build());

    // Setup WASI-thread
    let _ =
        wasmtime_wasi_threads::add_to_linker(&mut linker, &wstore, &module, |s: &mut HostCtx| {
            s.wasi_threads.as_ref().unwrap()
        });

    // attach Lind common APIs to the linker
    let _ = wasmtime_lind_common::add_to_linker::<HostCtx, _>(&mut linker)?;

    // attach Lind-Multi-Process-Context to the host
    let _ = wstore.data_mut().lind_fork_ctx = Some(LindCtx::new(
        module.clone(),
        linker.clone(),
        lind_manager.clone(),
        lindboot_cli.clone(),
        cageid,
        |host| host.lind_fork_ctx.as_mut().unwrap(),
        |host| host.fork(),
        |lindboot_cli, path, args, cageid, lind_manager, envs| {
            let mut new_lindboot_cli = lindboot_cli.clone();
            new_lindboot_cli.args = vec![String::from(path)];
            // new_lindboot_cli.wasm_file = path.to_string();
            if let Some(envs) = envs {
                new_lindboot_cli.vars = envs.clone();
            }
            for arg in args.iter().skip(1) {
                new_lindboot_cli.args.push(String::from(arg));
            }

            execute_with_lind(new_lindboot_cli, lind_manager.clone(), cageid as u64)
        },
    )?);

    let _ = wstore.data_mut().wasi_threads = Some(Arc::new(WasiThreadsCtx::new(
        module.clone(),
        Arc::new(linker.clone()),
    )?));

    Ok(())
}

/// This function takes a compiled module, instantiates it with the current store and linker,
/// and executes its entry point. This is the point where the Wasm "process" actually starts
/// executing.
fn load_main_module(
    store: &mut Store<HostCtx>,
    linker: &mut Linker<HostCtx>,
    module: &Module,
    cageid: u64,
    args: &[String],
) -> Result<Vec<Val>> {
    // todo:
    // I don't setup `epoch_handler` since it seems not being used by our previous implementation.
    // Not sure if this is related to our thread exit problem
    let (instance, cage_instanceid) = linker
        .instantiate_with_lind(
            &mut *store,
            &module,
            InstantiateType::InstantiateFirst(cageid),
        )
        .context(format!("failed to instantiate"))?;

    // If `_initialize` is present, meaning a reactor, then invoke
    // the function.
    if let Some(func) = instance.get_func(&mut *store, "_initialize") {
        func.typed::<(), ()>(&store)?.call(&mut *store, ())?;
    }

    // Look for the specific function provided or otherwise look for
    // "" or "_start" exports to run as a "main" function.
    let func = instance
        .get_func(&mut *store, "")
        .or_else(|| instance.get_func(&mut *store, "_start"));

    let stack_low = instance.get_stack_low(store.as_context_mut()).unwrap();
    let stack_pointer = instance.get_stack_pointer(store.as_context_mut()).unwrap();
    store.as_context_mut().set_stack_base(stack_pointer as u64);
    store.as_context_mut().set_stack_top(stack_low as u64);

    cfg_if! {
        // The disable_signals feature allows Wasmtime to run Lind binaries without inserting an epoch.
        // It sets the signal pointer to 0, so any signals will trigger a fault in RawPOSIX.
        // This is intended for debugging only and should not be used in production.
        if #[cfg(feature = "disable_signals")] {
            let pointer = 0;
        } else {
            // retrieve the epoch global
            let lind_epoch = instance
                .get_export(&mut *store, "epoch")
                .and_then(|export| export.into_global())
                .expect("Failed to find epoch global export!");

            // retrieve the handler (underlying pointer) for the epoch global
            let pointer = lind_epoch.get_handler(&mut *store);
        }
    }

    // initialize the signal for the main thread of the cage
    lind_signal_init(
        cageid,
        pointer as *mut u64,
        THREAD_START_ID,
        true, /* this is the main thread */
    );

    // see comments at signal_may_trigger for more details
    signal_may_trigger(cageid);

    // The main challenge in enabling dynamic syscall interposition between grates and 3i lies in Rust’s
    // strict lifetime and ownership system, which makes retrieving the Wasmtime runtime context across
    // instance boundaries particularly difficult. To overcome this, the design employs low-level context
    // capture by extracting and storing vmctx pointers from Wasmtime’s internal `StoreOpaque` and `InstanceHandler`
    // structures. See more details in [lind-3i/src/lib.rs]
    // 1) Get StoreOpaque & InstanceHandler to extract vmctx pointer
    let cage_storeopaque = store.inner_mut();
    let cage_instancehandler = cage_storeopaque.instance(cage_instanceid);
    let vmctx_ptr: *mut c_void = cage_instancehandler.vmctx().cast();

    // 2) Extract vmctx pointer and put in a Send+Sync wrapper
    let vmctx_wrapper = VmCtxWrapper {
        vmctx: NonNull::new(vmctx_ptr).ok_or_else(|| anyhow!("null vmctx"))?,
    };

    // 3) Store the vmctx wrapper in the global table for later retrieval during grate calls or other syscalls
    // This function will be called at either the first cage or exec-ed cages.
    set_vmctx_thread(cageid, THREAD_START_ID as u64, vmctx_wrapper);

    // 4) Notify threei of the cage runtime type
    threei::set_cage_runtime(cageid, threei_const::RUNTIME_TYPE_WASMTIME);

    // 5) Create backup instances to populate the vmctx pool
    // See more comments in lind-3i/lib.rs
    for _ in 0..9 {
        let (_, backup_cage_instanceid) = linker
            .instantiate_with_lind_thread(&mut *store, &module)
            .context(format!("failed to instantiate"))?;

        // Extract vmctx pointer
        let backup_cage_storeopaque = store.inner_mut();
        let backup_cage_instancehandler = backup_cage_storeopaque.instance(backup_cage_instanceid);
        let backup_vmctx_ptr: *mut c_void = backup_cage_instancehandler.vmctx().cast();

        // Put vmctx in a Send+Sync wrapper
        let backup_vmctx_wrapper = VmCtxWrapper {
            vmctx: NonNull::new(backup_vmctx_ptr).ok_or_else(|| anyhow!("null vmctx"))?,
        };

        // Store the vmctx wrapper in the global table for later retrieval during grate calls
        set_vmctx(cageid, backup_vmctx_wrapper);
    }

    let ret = match func {
        Some(func) => invoke_func(store, func, &args),
        None => Ok(vec![]),
    };

    if !rm_vmctx(cageid) {
        panic!(
            "[lind-boot] Failed to remove existing VMContext for cage_id {}",
            cageid
        );
    }

    ret
}

/// AOT-compile a `.wasm` file to a `.cwasm` artifact on disk.
///
/// This only needs a Wasmtime `Engine` — no runtime, cages, or 3i. The output
/// path is the input path with the extension replaced by `.cwasm`.
pub fn precompile_module(cli: &CliOptions) -> Result<()> {
    let wasm_path = Path::new(cli.wasm_file());
    let cwasm_path = wasm_path.with_extension("cwasm");

    let engine = Engine::new(&wasmtime::Config::new()).context("failed to create engine")?;
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("failed to read {}", wasm_path.display()))?;
    let cwasm_bytes = engine
        .precompile_module(&wasm_bytes)
        .context("failed to precompile module")?;
    std::fs::write(&cwasm_path, cwasm_bytes)
        .with_context(|| format!("failed to write {}", cwasm_path.display()))?;

    eprintln!("OK: {}", cwasm_path.display());
    Ok(())
}

/// Load a Wasm module from disk, supporting both `.wasm` and precompiled `.cwasm` files.
///
/// The function probes the file header via `Engine::detect_precompiled_file`.
/// If the file is a precompiled module it is deserialized directly (skipping
/// compilation). Otherwise it is compiled from source via `Module::from_file`.
fn read_wasm_or_cwasm(engine: &Engine, path: &Path) -> Result<Module> {
    // `detect_precompiled_file` *expects* input to already be an ELF file. It is used to detect
    // whether this ELF matches the current host architecture.
    //
    // When passing in a .wasm file, the ELF parsing unwinds early. (`ElfFile64::parse(&read_cache)?;`)
    // We can therefore not call .context()? on this function since that would unwind and not run the Module::from_file()
    match engine.detect_precompiled_file(path) {
        Ok(_) => unsafe { Module::deserialize_file(engine, path) }
            .context("failed to deserialize precompiled module"),
        Err(_) => Module::from_file(engine, path).context("failed to compile module"),
    }
}

/// This function takes a Wasm function (Func) and a list of string arguments, parses the
/// arguments into Wasm values based on expected types (ValType), and invokes the function
fn invoke_func(store: &mut Store<HostCtx>, func: Func, args: &[String]) -> Result<Vec<Val>> {
    let ty = func.ty(&store);
    if ty.params().len() > 0 {
        eprintln!(
            "warning: using `--invoke` with a function that takes arguments \
                is experimental and may break in the future"
        );
    }
    let mut args = args.iter();
    let mut values = Vec::new();
    for ty in ty.params() {
        let val_str = args
            .next()
            .ok_or_else(|| anyhow!("not enough arguments for invoke"))?;
        let val = match ty {
            ValType::I32 => Val::I32(val_str.parse()?),
            ValType::I64 => Val::I64(val_str.parse()?),
            ValType::F32 => Val::F32(val_str.parse::<f32>()?.to_bits()),
            ValType::F64 => Val::F64(val_str.parse::<f64>()?.to_bits()),
            _ => bail!("unsupported argument type {:?}", ty),
        };
        values.push(val);
    }

    // Invoke the function and then afterwards print all the results that came
    // out, if there are any.
    let mut results = vec![Val::null_func_ref(); ty.results().len()];
    let _ = func
        .call(&mut *store, &values, &mut results)
        .with_context(|| format!("failed to invoke command default"));

    Ok(results)
}
