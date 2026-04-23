use crate::lind_wasmtime::host::DylinkMetadata;
use crate::lind_wasmtime::host::{
    cleanup_grate_handler, init_grate_pool, register_grate_handler_for_cage,
    unregister_grate_handler,
};
use crate::{cli::CliOptions, lind_wasmtime::host::HostCtx, lind_wasmtime::trampoline::*};
use anyhow::{Context, Result, anyhow, bail};
use cage::signal::{lind_signal_init, signal_may_trigger};
use cfg_if::cfg_if;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use std::{ffi::c_void, sync::Mutex};
use sysdefs::constants::lind_platform_const::{
    INSTANCE_NUMBER, RAWPOSIX_CAGEID, UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID,
};
use sysdefs::constants::syscall_const::{CLONE_SYSCALL, EXEC_SYSCALL, EXIT_SYSCALL};
use sysdefs::constants::{DEFAULT_STACKSIZE, DylinkErrorCode, GUARD_SIZE, TABLE_START_INDEX};
use sysdefs::logging::lind_debug_panic;
use threei::threei_const;
use wasmtime::{
    AsContextMut, Engine, Export, Func, InstantiateType, Linker, Module, Precompiled, SharedMemory,
    Store, Val, ValType, WasmBacktraceDetails,
};
use wasmtime_lind_3i::*;
use wasmtime_lind_common::LindEnviron;
use wasmtime_lind_dylink::DynamicLoader;
use wasmtime_lind_multi_process::{
    CAGE_START_ID, LindCtx, THREAD_START_ID, attach_shared_memory, early_init_stack,
};
use wasmtime_lind_utils::symbol_table::SymbolMap;
use wasmtime_lind_utils::{LindCageManager, LindGOT};

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
/// After initialization, the function attaches all host-side APIs (Lind common,
/// WASI threads, and Lind multi-process contexts) to the wasmtime linker,
/// instantiates the module into the starting cage, and runs the program's
/// entrypoint. On successful completion it waits for all cages to exit before
/// shutting down RawPOSIX, ensuring runtime-wide cleanup happens only after the
/// last process terminates.
pub fn execute_wasmtime(lindboot_cli: CliOptions) -> anyhow::Result<i32> {
    // -- Initialize Lind + RawPOSIX + 3i runtime --
    // Initialize the Lind cage counter
    let lind_manager = Arc::new(LindCageManager::new(0));
    // new cage is created
    lind_manager.increment();

    let grate_cleanup_funcptr = cleanup_grate_handler as *const () as usize as u64;
    // Initialize trampoline entry function pointer for wasmtime runtime.
    // This is for grate calls to re-enter wasmtime runtime.
    threei::register_trampoline(
        threei_const::RUNTIME_TYPE_WASMTIME,
        grate_callback_trampoline,
        grate_cleanup_funcptr,
    );

    // Register syscall handlers (clone/exec/exit) with 3i
    if !register_wasmtime_syscall_entry() {
        panic!("[lind-boot] register syscall handlers (clone/exec/exit) with 3i failed");
    }

    // initialize the vmctx pool for exit/exec/clone reentry into wasmtime runtime
    init_vmctx_pool();

    // -- Initialize the Wasmtime execution environment --
    let wasm_file_path = Path::new(lindboot_cli.wasm_file());
    let wt_config =
        make_wasmtime_config(lindboot_cli.wasmtime_backtrace, lindboot_cli.enable_fpcast);
    let engine = Engine::new(&wt_config).context("failed to create execution engine")?;
    let module = read_wasm_or_cwasm(&engine, wasm_file_path)?;

    // -- Run the first module in the first cage --
    let result = execute_with_lind(
        lindboot_cli,
        lind_manager.clone(),
        engine,
        module,
        CAGE_START_ID as u64,
    );

    match result {
        Ok(ref ret_vals) => {
            // we wait until all other cage exits
            lind_manager.wait();
            // Interpret the first return value of the Wasm entry point
            // as the process exit code. If the module does not explicitly
            // return an i32, we treat it as a successful exit (code = 0).
            let exit_code = match ret_vals.first() {
                Some(Val::I32(code)) => *code,
                _ => 0,
            };
            // Propagate the exit code to the main, which will translate it
            // into the host process exit status.
            Ok(exit_code)
        }
        Err(e) => {
            // Initial cage crashed.  Do the same cleanup as the
            // fork-crash and signal-handler error paths so child
            // cages see proper termination and resources are freed.
            let cageid = CAGE_START_ID as u64;
            cage::cage_record_exit_status(cageid, cage::ExitStatus::Exited(1));
            if let Some(c) = cage::get_cage(cageid) {
                c.is_dead.store(true, std::sync::atomic::Ordering::Release);
            }
            threei::EXITING_TABLE.insert(cageid);
            threei::handler_table::_rm_grate_from_handler(cageid);
            cage::signal::lind_thread_exit(cageid, THREAD_START_ID as u64);
            cage::cage_finalize(cageid);
            lind_manager.decrement();
            lind_manager.wait();
            return Err(e);
        }
    }
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
    engine: Engine,
    module: Module,
    cageid: u64,
) -> Result<Vec<Val>> {
    // -- Initialize the Wasmtime execution environment --
    let args = lind_boot.args.clone();
    let host = HostCtx::default();
    let mut wstore = Store::new(&engine, host);

    // -- Attach host APIs --
    let mut linker = Arc::new(Mutex::new(Linker::new(&engine)));

    let dylink_enabled = module.dylink_meminfo().is_some();
    let mut dylink_metadata = DylinkMetadata::new(dylink_enabled);

    if dylink_metadata.dylink_enabled {
        // create Global Offset Table for dynamic loading
        dylink_metadata.got = Some(Arc::new(Mutex::new(LindGOT::new())));

        let mut linker = linker.lock().unwrap();
        // Determine the minimal table size required by the main module
        // from its table import declaration.
        let mut main_module_table_size = 0;

        for import in module.imports() {
            if let wasmtime::ExternType::Table(table) = import.ty() {
                main_module_table_size = table.minimum();
            }
        }

        // calculate the stack address for main module
        let stack_low = GUARD_SIZE as i32; // reserve first 1024 bytes for guard page
        let stack_high = stack_low + DEFAULT_STACKSIZE as i32; // 8 MB of default stack size
        let memory_base = stack_high + GUARD_SIZE as i32; // memory base starts after the stack space (plus guard)
        let table_base = TABLE_START_INDEX as i32;

        #[cfg(feature = "debug-dylink")]
        {
            println!("[debug] main module table size: {}", main_module_table_size);
            println!(
                "[debug] main module stack pointer starts from {} to {}",
                stack_low, stack_high
            );
        }

        // Allocate the main module's indirect function table with
        // the minimal required size.
        let table_inner = linker
            .attach_function_table(&mut wstore, main_module_table_size)
            .expect("failed to create table");
        linker
            .attach_stack_imports(&mut wstore, stack_low, stack_high)
            .expect("failed to attach stack imports");
        let _ = linker
            .attach_memory_base(&mut wstore, memory_base)
            .expect("failed to attach memory base");
        linker
            .attach_table_base(&mut wstore, table_base)
            .expect("failed to attach table base");
        linker
            .attach_asyncify(&mut wstore)
            .expect("failed to attach asyncify imports");
        let epoch = linker
            .attach_epoch(&mut wstore)
            .expect("failed to attach epoch");

        wstore.as_context_mut().set_stack_base(stack_high as u64);
        wstore.as_context_mut().set_stack_top(stack_low as u64);

        dylink_metadata.table = Some(table_inner);
        dylink_metadata.epoch_handler = Some(epoch);
    }

    // Load the preload wasm modules.
    let mut modules = Vec::new();
    modules.push((String::new(), String::new(), module.clone()));
    for (name, path) in lind_boot.preloads.iter() {
        // Read the wasm module binary either as `*.wat` or a raw binary
        let module = read_wasm_or_cwasm(&engine, path)?;
        modules.push((
            name.clone(),
            path.to_string_lossy().to_string(),
            module.clone(),
        ));
    }

    attach_api(
        &mut wstore,
        &mut linker,
        dylink_metadata.got.clone(),
        &modules,
        lind_manager.clone(),
        lind_boot.clone(),
        cageid as i32,
        &dylink_metadata,
    )?;

    if dylink_metadata.dylink_enabled {
        let lind_got = dylink_metadata.got.as_ref().unwrap();

        early_init_stack(
            cageid,
            GUARD_SIZE as i32,
            (GUARD_SIZE + DEFAULT_STACKSIZE) as i32,
        )
        .unwrap();

        // For each module (including the main module),
        // register its GOT imports with the shared LindGOT instance.
        //
        // This installs placeholder globals for unresolved GOT entries so that
        // the library modules can be instantiated first and have their symbols
        // patched later during relocation/export processing.
        //
        // We skip the first module because it is the main module, which was
        // already processed earlier.
        for (name, _path, module) in modules.iter() {
            let mut linker = linker.lock().unwrap();

            let mut got_guard = lind_got.lock().unwrap();
            linker.define_GOT_dispatcher(&mut wstore, &module, &mut *got_guard);
        }

        // Add the module's functions to the linker.
        for (name, path, module) in modules.iter().skip(1) {
            #[cfg(feature = "debug-dylink")]
            println!("[debug] link module {}.{}", name, path);
            let mut lib_linker = linker.lock().unwrap();
            let mut table_inner = dylink_metadata.table.as_mut().unwrap();

            // Read dylink metadata for this preloaded (library) module.
            // This contains the module's declared table/memory requirements.
            let dylink_info = module.dylink_meminfo();
            let dylink_info = dylink_info
                .as_ref()
                .expect("library does not contain dylink.0 section");
            // Append this library's function table region to the shared table.
            // `table_start` is the starting index of the library's reserved range
            // within the global indirect function table.
            let table_start = table_inner.size(&mut wstore) as i32;

            #[cfg(feature = "debug-dylink")]
            println!(
                "[debug] library table_start: {}, grow: {}",
                table_start, dylink_info.table_size
            );
            // Grow the shared indirect function table by the amount requested by the
            // library (as recorded in its dylink section). New slots are initialized
            // to null funcref.
            table_inner.grow(
                &mut wstore,
                dylink_info.table_size,
                wasmtime::Ref::Func(None),
            );

            // Link the library instance into the main linker namespace.
            // The linker records the module under `name` and uses `table_start`
            // to relocate/interpret the library's function references into the
            // shared table. GOT entries are patched through the shared LindGOT.
            #[cfg(feature = "debug-dylink")]
            println!("[debug] library {} instantiate", name);
            let mut got_guard = lind_got.lock().unwrap();
            lib_linker
                .module_with_preload(
                    &mut wstore,
                    cageid,
                    &name,
                    &module,
                    &mut table_inner,
                    table_start,
                    &got_guard,
                    path.clone(),
                )
                .context(format!("failed to process preload `{}`", name,))?;
        }

        // Resolve any remaining unknown imports to trap stubs so the library can
        // instantiate even when it has optional/unused imports.
        let mut linker_guard = linker.lock().unwrap();
        linker_guard.define_unknown_imports_as_traps(&module);

        // after all preloaded library are attached to the linker, update the linker in LindCtx
        // so that newly forked cage could use the Linker with necessary library loaded
        let mut ctx = wstore.data_mut().lind_fork_ctx.as_mut().unwrap();
        ctx.attach_linker(linker_guard.clone());

        drop(linker_guard);

        #[cfg(feature = "debug-dylink")]
        {
            // Emit warnings for any GOT slots that remain unresolved after processing
            // preloads and defining trap stubs.
            let mut got_guard = lind_got.lock().unwrap();
            got_guard.warning_undefined();
        }
    }

    // -- Run the module in the cage --
    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(
            &mut wstore,
            &mut linker,
            &module,
            cageid as u64,
            &args,
            dylink_metadata,
        )
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
        UNUSED_ID,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        CLONE_SYSCALL as u64,                // clone syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        WASMTIME_CAGEID,                     // handler function is in the 3i
        clone_call_u64,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );

    // Register exec trampoline (syscall 59).
    let fp_exec = exec_syscall_entry;
    let exec_call_u64: u64 = fp_exec as *const () as usize as u64;
    let exec_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        EXEC_SYSCALL as u64,                 // exec syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        WASMTIME_CAGEID,                     // handler function is in the 3i
        exec_call_u64,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );

    // Register exit trampoline (syscall 60).
    let fp_exit = exit_syscall_entry;
    let exit_call_u64: u64 = fp_exit as *const () as usize as u64;
    let exit_ret = threei::register_handler(
        UNUSED_ID,
        WASMTIME_CAGEID,                     // target cageid for this syscall handler
        RAWPOSIX_CAGEID,                     // cage to modify: current cageid
        EXIT_SYSCALL as u64,                 // exit syscall number
        threei_const::RUNTIME_TYPE_WASMTIME, // runtime id
        WASMTIME_CAGEID,                     // handler function is in the 3i
        exit_call_u64,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
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
/// This function constructs the host interface that the guest expects and stores
/// it inside `HostCtx`. It wires three subsystems into the Wasmtime instance:
///
/// 1. **Lind common** — syscall dispatch, debug helpers, signal hooks, and the
///    4 argv/environ functions that glibc's `_start()` needs.
/// 2. **WASI threads** — pthread-like thread creation within the guest.
/// 3. **Lind multi-process** (`LindCtx`) — fork/exec semantics.
///
/// The `cageid` parameter allows this function to be used both for the initial
/// boot (where no cage override is needed) and for exec-ed cages (where the
/// target cage is explicitly specified).
fn attach_api(
    wstore: &mut Store<HostCtx>,
    mut linker: &mut Arc<Mutex<Linker<HostCtx>>>,
    got_table: Option<Arc<Mutex<LindGOT>>>,
    modules: &Vec<(String, String, Module)>,
    lind_manager: Arc<LindCageManager>,
    lindboot_cli: CliOptions,
    cageid: i32,
    dylink_metadata: &DylinkMetadata,
) -> Result<()> {
    // Initialize argv/environ data and attach all Lind host functions
    // (syscall dispatch, debug, signals, and argv/environ) to the linker.
    wstore.data_mut().lind_environ = Some(LindEnviron::new(&lindboot_cli.args, &lindboot_cli.vars));

    // Build a dynamic loader closure that reads the current cage's linker and GOT
    // at dlopen call time. This ensures the correct per-cage linker is used
    // rather than a snapshot captured at cage creation.
    let dynamic_loader = {
        if dylink_metadata.dylink_enabled {
            let dynamic_loader: DynamicLoader<HostCtx> =
                Arc::new(move |caller, cageid, library_name, mode| {
                    let lind_ctx = caller.data().lind_fork_ctx.as_ref().unwrap();
                    let linker = lind_ctx.linker.clone().unwrap();
                    let got_table = lind_ctx.got_table.clone().unwrap();

                    if lind_ctx.had_threads() {
                        lind_debug_panic("dlopen within threads is currently not supported!");
                    }

                    load_library_module(caller, linker, got_table, cageid, library_name, mode)
                });
            Some(dynamic_loader)
        } else {
            None
        }
    };

    let mut linker_guard = linker.lock().unwrap();
    let _ = wasmtime_lind_common::add_to_linker::<HostCtx, _>(
        &mut linker_guard,
        |s: &HostCtx| {
            s.lind_environ
                .as_ref()
                .expect("lind_environ must be initialized")
        },
        dynamic_loader,
    )?;

    let main_module = &modules.get(0).unwrap().2;

    // attach SharedMemory to the instance
    attach_shared_memory(&mut *wstore, &mut linker_guard, &main_module, true, cageid)?;

    // attach Lind-Multi-Process-Context to the host
    let _ = wstore.data_mut().lind_fork_ctx = Some(LindCtx::new(
        modules.clone(),
        linker_guard.clone(),
        got_table,
        lind_manager.clone(),
        lindboot_cli.clone(),
        cageid,
        |host| host.lind_fork_ctx.as_mut().unwrap(),
        |host| host.fork(),
        |lindboot_cli, path, args, engine, module, cageid, lind_manager, envs| {
            let mut new_lindboot_cli = lindboot_cli.clone();
            new_lindboot_cli.args = vec![String::from(path)];
            // new_lindboot_cli.wasm_file = path.to_string();
            if let Some(envs) = envs {
                new_lindboot_cli.vars = envs.clone();
            }
            for arg in args.iter().skip(1) {
                new_lindboot_cli.args.push(String::from(arg));
            }

            execute_with_lind(
                new_lindboot_cli,
                lind_manager.clone(),
                engine,
                module,
                cageid as u64,
            )
        },
    )?);

    drop(linker_guard);

    Ok(())
}

/// This function takes a compiled module, instantiates it with the current store and linker,
/// and executes its entry point. This is the point where the Wasm "process" actually starts
/// executing.
fn load_main_module(
    mut store: &mut Store<HostCtx>,
    linker: &mut Arc<Mutex<Linker<HostCtx>>>,
    module: &Module,
    cageid: u64,
    args: &[String],
    mut dylink_metadata: DylinkMetadata,
) -> Result<Vec<Val>> {
    let mut linker_guard = linker.lock().unwrap();

    // todo:
    // I don't setup `epoch_handler` since it seems not being used by our previous implementation.
    // Not sure if this is related to our thread exit problem
    let linker = linker_guard.clone();
    let (instance, stack_arena_base, cage_instanceid) = linker
        .instantiate_with_lind(
            &mut *store,
            &module,
            InstantiateType::InstantiateFirst(cageid),
        )
        .context(format!("failed to instantiate"))?;
    drop(linker);

    // Register the main module so get_global_snapshot can find it by name.
    if let Some(name) = module.name() {
        store
            .as_context_mut()
            .register_named_instance(name.to_string(), cage_instanceid);
    }

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

    if !dylink_metadata.dylink_enabled {
        let stack_low = instance.get_stack_low(store.as_context_mut()).unwrap();
        let stack_pointer = instance.get_stack_pointer(store.as_context_mut()).unwrap();
        store.as_context_mut().set_stack_base(stack_pointer as u64);
        store.as_context_mut().set_stack_top(stack_low as u64);
    }

    // apply the relocations for the main module if dylink is enable
    if dylink_metadata.dylink_enabled {
        let got = dylink_metadata.got.as_mut().unwrap();
        let mut got_guard = got.lock().unwrap();
        let table = dylink_metadata.table.as_ref().unwrap();
        let memory_base = GUARD_SIZE + DEFAULT_STACKSIZE + GUARD_SIZE;
        let fpcast_enabled = linker_guard.engine().fpcast_enabled();
        instance.apply_GOT_relocs(
            &mut store,
            Some(&got_guard),
            table,
            Some(memory_base),
            fpcast_enabled,
        );
    }

    cfg_if! {
        // The disable_signals feature allows Wasmtime to run Lind binaries without inserting an epoch.
        // It sets the signal pointer to 0, so any signals will trigger a fault in RawPOSIX.
        // This is intended for debugging only and should not be used in production.
        if #[cfg(feature = "disable_signals")] {
            let epoch_handler = 0;
        } else {
            let epoch_handler = if dylink_metadata.dylink_enabled {
                dylink_metadata.epoch_handler.unwrap()
            } else {
                let lind_epoch = instance
                    .get_export(&mut *store, "epoch")
                    .and_then(|export| export.into_global())
                    .expect("Failed to find epoch global export!");

                // retrieve the handler (underlying pointer) for the epoch global
                lind_epoch.get_handler_as_u64(&mut *store) as u64
            };
        }
    }

    // initialize the signal for the main thread of the cage
    lind_signal_init(
        cageid,
        epoch_handler as *mut u64,
        THREAD_START_ID,
        true, /* this is the main thread */
    );

    // see comments at signal_may_trigger for more details
    signal_may_trigger(cageid);

    // See more details in [lind-3i/src/lib.rs]
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

    // Grate calls only supports static linking for now, so we only initialize the grate pool and register
    // grate workers when dylink is not enabled.
    if !dylink_metadata.dylink_enabled {
        // 4) register grate workers for this cage
        let grate_template = GrateTemplate {
            engine: module.engine().clone(),
            module: module.clone(),
            linker: linker_guard.clone(),
        };
        let host = store.data().clone();

        // initialize the grate pool for later use in grate calls and
        // other syscalls that require re-entry into wasmtime runtime.
        init_grate_pool();
        unregister_grate_handler(cageid);

        register_grate_handler_for_cage(&grate_template, host, cageid)
            .with_context(|| format!("failed to register grate workers for cage {}", cageid))?;
    }

    // 5) Notify threei of the cage runtime type
    threei::set_cage_runtime(cageid, threei_const::RUNTIME_TYPE_WASMTIME);

    let mut linker = linker_guard.clone();
    linker.define_weak_imports_as_traps(&module);

    // must drop linker before jump into wasm
    drop(linker);
    drop(linker_guard);

    let ret = match func {
        Some(func) => invoke_func(store, func, &args),
        None => Ok(vec![]),
    };

    ret
}

/// Dynamically load and instantiate a library module into a running main module.
///
/// This function implements Lind's runtime dynamic loading path (subroutine of `dlopen`).
/// It loads the library using the same engine as the main module, reserves space in
/// the shared indirect function table, installs GOT placeholders for symbol resolution,
/// and then instantiates the library in the context of the running instance.
///
/// The library's functions are appended to the shared table, and relocations are
/// resolved relative to the current table size and shared memory base.
///
/// Returns an integer handle representing the identifier of the loaded library instance,
/// which is used by dlsym symbol lookup
fn load_library_module(
    mut main_module: &mut wasmtime::Caller<HostCtx>,
    mut linker: Linker<HostCtx>,
    mut lind_got: Arc<Mutex<LindGOT>>,
    cageid: i32,
    library_name: &str,
    dlopen_mode: i32,
) -> i32 {
    // Use the same wasmtime engine as the main module so that
    // the library shares compilation configuration and runtime state.
    let engine = main_module.engine();

    let library_path = Path::new(library_name);

    // retrieve inode of the file as the unique identifier of the library
    // TODO: should redirect to threei
    let metadata = match std::fs::metadata(library_path) {
        Ok(data) => data,
        Err(_) => return -(DylinkErrorCode::EOPEN as i32),
    };
    let inode = metadata.ino();

    if let Some(handler) = main_module.check_library_loaded(inode) {
        return handler;
    }

    // Load and compile the library module (either wasm or cwasm format).
    let lib_module = match read_wasm_or_cwasm(&engine, Path::new(library_path)) {
        Ok(module) => module,
        Err(_) => return -(DylinkErrorCode::ETYPE as i32), // library is not a valid wasm module
    };

    // Extract dylink metadata from the library.
    // This includes table and memory requirements declared by the toolchain.
    let dylink_info = match lib_module.dylink_meminfo() {
        Some(info) => info,
        None => return -(DylinkErrorCode::EDYLINKINFO as i32), // dylink section is not found
    };

    // Record the current size of the shared indirect function table.
    // The library's functions will be appended starting from this index.
    let table_size = main_module.get_table_size();
    match main_module.grow_table_lib(dylink_info.table_size, wasmtime::Ref::Func(None)) {
        Ok(_) => {}
        Err(_) => return -(DylinkErrorCode::EINTERNAL as i32),
    };

    // Grow the shared function table to reserve space for this library's
    // function entries, as declared in its dylink section.
    let mut got_guard = lind_got.lock().unwrap();

    // Install placeholder GOT globals for this library's imports.
    // These placeholders allow instantiation to succeed before
    // relocations are applied and symbols are fully resolved.
    linker.define_GOT_dispatcher(&mut main_module, &lib_module, &mut *got_guard);

    let mut symbol_map = SymbolMap::new(dlopen_mode, inode);

    // Instantiate the library module in the context of the running main module.
    // `table_size` is passed as the base index into the shared table so that
    // the library's function references can be relocated correctly.
    //
    // The GOT is used to patch symbol addresses/indices after instantiation.
    let ret = match linker.module_with_caller(
        &mut main_module,
        cageid as u64,
        library_name,
        &lib_module,
        table_size as i32,
        &*got_guard,
        symbol_map,
        library_name.to_string(),
    ) {
        Ok(handle) => handle as i32,
        Err(_) => {
            #[cfg(feature = "debug-dylink")]
            println!("failed to process library `{}`", library_name);
            -(DylinkErrorCode::EINTERNAL as i32) // consider as internal error for now
        }
    };

    let lind_ctx = main_module.data_mut().lind_fork_ctx.as_mut().unwrap();
    lind_ctx.attach_linker(linker);
    lind_ctx.append_module(library_name.to_string(), lib_module);

    ret
}

/// AOT-compile a `.wasm` file to a `.cwasm` artifact on disk.
///
/// This only needs a Wasmtime `Engine` — no runtime, cages, or 3i. The output
/// path is the input path with the extension replaced by `.cwasm`.
pub fn precompile_module(cli: &CliOptions) -> Result<()> {
    let wasm_path = Path::new(cli.wasm_file());
    let cwasm_path = wasm_path.with_extension("cwasm");

    let wt_config = make_wasmtime_config(cli.wasmtime_backtrace, false);
    let engine = Engine::new(&wt_config).context("failed to create engine")?;
    let wasm_bytes = std::fs::read(wasm_path)
        .with_context(|| format!("failed to read {}", wasm_path.display()))?;
    let cwasm_bytes = engine
        .precompile_module(&wasm_bytes)
        .with_context(|| format!("failed to precompile module {}", wasm_path.display()))?;
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
        Ok(_) => unsafe { Module::deserialize_file(engine, path) }.with_context(|| {
            format!(
                "failed to deserialize precompiled module {}",
                path.display()
            )
        }),
        Err(_) => Module::from_file(engine, path)
            .with_context(|| format!("failed to compile module {}", path.display())),
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

    // Unwind in case of an error, this allows us to pretty-print the WasmBacktrace context when option
    // is enabled.
    func.call(&mut *store, &values, &mut results)
        .with_context(|| format!("failed to invoke command default"))?;

    Ok(results)
}

/// Generates a wasmtime config based on the whether or not the --wasmtime-backtrace flag was
/// provided to lind-boot.
fn make_wasmtime_config(backtrace: bool, enable_fpcast: bool) -> wasmtime::Config {
    let mut wt_config = wasmtime::Config::new();
    wt_config.wasm_backtrace(backtrace);
    wt_config.fpcast_enabled(enable_fpcast);

    let details = if backtrace {
        WasmBacktraceDetails::Enable
    } else {
        WasmBacktraceDetails::Disable
    };

    wt_config.wasm_backtrace_details(details);

    // Enable compilation cache — compiled .wasm artifacts are stored on disk
    // so subsequent runs skip compilation. Best-effort: if config loading
    // fails (e.g. no home dir), caching is simply disabled.
    if let Err(e) = wt_config.cache_config_load_default() {
        eprintln!("[lind-boot] warning: failed to enable wasm cache: {e}");
    }

    wt_config
}
