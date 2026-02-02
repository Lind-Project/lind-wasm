use crate::{cli::CliOptions, host::HostCtx, trampoline::*};
use anyhow::{Context, Result, anyhow, bail};
use cage::signal::{lind_signal_init, signal_may_trigger};
use rawposix::sys_calls::{rawposix_shutdown, rawposix_start};
use std::ffi::c_void;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, WASMTIME_CAGEID};
use threei::{make_syscall, threei_const};
use wasi_common::sync::Dir;
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::{AsContextMut, Engine, Func, InstantiateType, Linker, Module, Store, Val, ValType};
use wasmtime_lind_3i::{VmCtxWrapper, init_vmctx_pool, rm_vmctx, set_vmctx};
use wasmtime_lind_multi_process::{CAGE_START_ID, LindCtx, THREAD_START_ID};
use wasmtime_lind_utils::{LindCageManager, lind_syscall_numbers::EXIT_SYSCALL};
use wasmtime_wasi_threads::WasiThreadsCtx;

static HOME_DIR_PATH: &str = "/home";

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
/// rebuilding the handler table. Specail needs will be handled per user request in
/// their implementation through `register_handler` via glibc.
///
/// After initialization, the function attaches all host-side APIs (WASI preview1,
/// WASI threads, and Lind contexts) to the linker, instantiates the module into the
/// starting cage, and runs the program's entrypoint. On successful completion it
/// waits for all cages to exit before shutting down RawPOSIX, ensuring runtime-wide
/// cleanup happens only after the last process terminates.
pub fn execute(lindboot_cli: CliOptions) -> anyhow::Result<Vec<Val>> {
    // -- Initialize the Wasmtime execution environment --
    let wasm_file_path = Path::new(&lindboot_cli.wasm_file);
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
    // Initialize RawPOSIX, also registered RawPOSIX syscalls to 3i
    rawposix_start(0);
    // Initialize vmctx pool
    init_vmctx_pool();
    // Initialize trampoline entry function pointer for wasmtime runtime.
    // This is for grate calls to re-enter wasmtime runtime.
    threei::register_trampoline(
        threei_const::RUNTIME_TYPE_WASMTIME,
        grate_callback_trampoline,
    );

    // Register syscall handlers (clone/exec/exit) with 3i
    let fp_clone: extern "C" fn(
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) -> i32 = clone_syscall_entry;
    let clone_call_u64: u64 = fp_clone as usize as u64;
    threei::register_handler(
        clone_call_u64,
        RAWPOSIX_CAGEID,                     // self cageid
        56,                                  // clone syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // target cageid
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    let fp_exec: extern "C" fn(
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) -> i32 = exec_syscall_entry;
    let exec_call_u64: u64 = fp_exec as usize as u64;
    threei::register_handler(
        exec_call_u64,
        RAWPOSIX_CAGEID,                     // self cageid
        59,                                  // exec syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // target cageid
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    let fp_exit: extern "C" fn(
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) -> i32 = exit_syscall_entry;
    let exit_call_u64: u64 = fp_exit as usize as u64;
    threei::register_handler(
        exit_call_u64,
        RAWPOSIX_CAGEID,                     // self cageid
        60,                                  // exit syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        1,                                   // register
        WASMTIME_CAGEID,                     // target cageid
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
    );

    // -- Load module and Attach host APIs --
    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let module = Module::from_file(&engine, wasm_file_path)?;
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
        Ok(ref res) => {
            let mut code = 0;
            let retval: &Val = res.get(0).unwrap();
            if let Val::I32(res) = retval {
                code = *res;
            }

            // we wait until all other cage exits
            lind_manager.wait();
            // after all cage exits, finalize the lind
            rawposix_shutdown();
        }
        Err(e) => {
            // Exit the process if Wasmtime understands the error;
            // otherwise, fall back on Rust's default error printing/return
            // code.
            return Err(wasi_common::maybe_exit_on_error(e));
        }
    }

    println!("Wasm execution in cage {} finished.", CAGE_START_ID);

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
    println!(
        "[execute_with_lind] Starting Wasm execution in cage {}...\n with cliopts args: {:?}",
        cageid, lind_boot.args
    );
    let wasm_file_path = Path::new(&lind_boot.wasm_file);
    println!("[execute_with_lind] Wasm file path: {:?}", wasm_file_path);
    let args = lind_boot.args.clone();
    let mut wt_config = wasmtime::Config::new();
    let engine = Engine::new(&wt_config).context("failed to create execution engine")?;
    let host = HostCtx::default();
    let mut wstore = Store::new(&engine, host);

    // -- Load module and Attach host APIs --
    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let module = Module::from_file(&engine, wasm_file_path)?;
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

    println!("Wasm execution in cage {} finished.", CAGE_START_ID);

    result
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
    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    wasi_common::sync::add_to_linker(&mut linker, |s: &mut HostCtx| {
        AsMut::<wasi_common::WasiCtx>::as_mut(s)
    });

    let mut builder = WasiCtxBuilder::new();
    wstore.data_mut().preview1_ctx = Some(builder.build());

    // Setup WASI-thread
    wasmtime_wasi_threads::add_to_linker(&mut linker, &wstore, &module, |s: &mut HostCtx| {
        s.wasi_threads.as_ref().unwrap()
    });

    // attach Lind common APIs to the linker
    {
        wasmtime_lind_common::add_to_linker::<HostCtx, _>(&mut linker)?;
    }

    // attach Lind-Multi-Process-Context to the host
    {
        wstore.data_mut().lind_fork_ctx = Some(LindCtx::new(
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
                new_lindboot_cli.wasm_file = path.to_string();
                if let Some(envs) = envs {
                    new_lindboot_cli.vars = envs.clone();
                }
                for arg in args.iter().skip(1) {
                    new_lindboot_cli.args.push(String::from(arg));
                }

                execute_with_lind(new_lindboot_cli, lind_manager.clone(), cageid as u64)
            },
        )?);
    }

    wstore.data_mut().wasi_threads = Some(Arc::new(WasiThreadsCtx::new(
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
    // I don't setup `epoch_handler` since it seems to be used for https, which is not required
    // for TriSeal. I'm not fully sure about this, but it works now.

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

    // retrieve the epoch global
    let lind_epoch = instance
        .get_export(&mut *store, "epoch")
        .and_then(|export| export.into_global())
        .expect("Failed to find epoch global export!");

    // retrieve the handler (underlying pointer) for the epoch global
    let pointer = lind_epoch.get_handler(&mut *store);

    // initialize the signal for the main thread of the cage
    lind_signal_init(
        cageid,
        pointer as *mut u64,
        THREAD_START_ID,
        true, /* this is the main thread */
    );

    // // see comments at signal_may_trigger for more details
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

    // 3) Store the vmctx wrapper in the global table for later retrieval during grate calls
    // This function will be called at either the first cage or exec-ed cages.
    set_vmctx(cageid, vmctx_wrapper);

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

    match func {
        Some(func) => invoke_func(store, func, &args),
        None => Ok(vec![]),
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
    func.call(&mut *store, &values, &mut results)
        .with_context(|| format!("failed to invoke command default"));

    Ok(results)
}
