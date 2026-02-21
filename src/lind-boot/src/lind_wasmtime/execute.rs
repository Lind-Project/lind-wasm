use crate::{cli::CliOptions, lind_wasmtime::host::HostCtx, lind_wasmtime::trampoline::*};
use anyhow::{Context, Result, anyhow, bail};
use cage::signal::{lind_signal_init, signal_may_trigger};
use cfg_if::cfg_if;
use sysdefs::constants::LINDFS_ROOT;
use std::{ffi::c_void, sync::Mutex};
use std::path::Path;
use std::ptr::NonNull;
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, WASMTIME_CAGEID};
use threei::threei_const;
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::{
    AsContextMut, Engine, Export, Func, InstantiateType, Linker, Module, Precompiled, Store, Val, ValType
};
use wasmtime_lind_3i::{VmCtxWrapper, init_vmctx_pool, rm_vmctx, set_vmctx, set_vmctx_thread};
use wasmtime_lind_multi_process::{CAGE_START_ID, LindCtx, THREAD_START_ID, get_memory_base};
use wasmtime_lind_utils::{LindCageManager, LindGOT};
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
    
    // create Global Offset Table for dynamic loading
    // #[cfg(feature = "dylink-support")]
    let mut lind_got = Arc::new(Mutex::new(LindGOT::new()));

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
    // let mut linker = Linker::new(&engine);
    let mut linker = Arc::new(Mutex::new(Linker::new(&engine)));

    let mut table;
    {
        let mut linker = linker.lock().unwrap();
        // Determine the minimal table size required by the main module
        // from its table import declaration.
        let mut main_module_table_size = None;
        let memory_size;
        
        for import in module.imports() {
            if let wasmtime::ExternType::Table(table) = import.ty() {
                main_module_table_size = Some(table.minimum());
            }
        }

        // Memory size and alignment are encoded in the dylink section.
        let dylink_info = module.dylink_meminfo();
        let dylink_info = dylink_info.as_ref().unwrap();

        let size = dylink_info.memory_size;
        let mut align = {
            // Enforce minimal alignment requirement for Lind:
            // at least 8 bytes (2^3).
            if dylink_info.memory_alignment < 3 { 3 }
            else { dylink_info.memory_alignment }
        };
        // round up memory size to align
        align = (1 << align) - 1;
        memory_size = (size + align) & !align;
            
        let main_module_table_size = main_module_table_size.unwrap();

        // Allocate the main module's indirect function table with
        // the minimal required size.
        #[cfg(feature = "debug-dylink")]
        println!("[debug] main module table size: {}", main_module_table_size);
        let ty = wasmtime::TableType::new(wasmtime::RefType::FUNCREF, main_module_table_size, None);
        table = wasmtime::Table::new(&mut wstore, ty, wasmtime::Ref::Func(None)).unwrap();
        linker.define(&mut wstore, "env", "__indirect_function_table", table);

        // calculate the stack address for main module
        let stack_low_num = 1024; // reserve first 1024 bytes for guard page
        let stack_high_num = stack_low_num + 8388608; // 8 MB of default stack size
        #[cfg(feature = "debug-dylink")]
        println!("[debug] main module stack pointer starts from {} to {}", stack_low_num, stack_high_num);
        let stack_low = wasmtime::Global::new(&mut wstore, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Var), Val::I32(stack_low_num)).unwrap();
        let stack_high = wasmtime::Global::new(&mut wstore, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Var), Val::I32(stack_high_num)).unwrap();
        linker.define(&mut wstore, "GOT.mem", "__stack_low", stack_low);
        linker.define(&mut wstore, "GOT.mem", "__stack_high", stack_high);

        let stack_pointer = wasmtime::Global::new(&mut wstore, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Var), Val::I32(stack_high_num)).unwrap();
        linker.define(&mut wstore, "env", "__stack_pointer", stack_pointer);

        // For the main module:
        // - Table base starts at 0.
        // - Memory base begins after the stack space (plus padding).
        let memory_base = wasmtime::Global::new(&mut wstore, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Const), Val::I32(1024 + 8388608 + 1024)).unwrap();
        let table_base = wasmtime::Global::new(&mut wstore, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Const), Val::I32(0)).unwrap();
        linker.define(&mut wstore, "env", "__memory_base", memory_base);
        linker.define(&mut wstore, "env", "__table_base", table_base);

        // Define placeholder globals for GOT imports so they can be
        // patched during/after instantiation.
        let mut got_guard = lind_got.lock().unwrap();
        linker.define_GOT_dispatcher(&mut wstore, &module, &mut *got_guard);
        drop(got_guard);
    }

    {
        let cloned_linker = linker.clone();
        let cloned_lind_got = lind_got.clone();

        let dlopen = move |mut caller: wasmtime::Caller<'_, HostCtx>, lib: i32| -> i32 {
            let base = get_memory_base(&mut caller);
            let path = typemap::get_cstr(base + lib as u64).unwrap();
            println!("[debug] dlopen path \"{}\"", path);

            load_library_module(&mut caller, cloned_linker.clone(), cloned_lind_got.clone(), path) as i32
        };

        let cloned_lind_got = lind_got.clone();
        let dlsym = move |mut caller: wasmtime::Caller<'_, HostCtx>, handle: i32, sym: i32| -> i32 {
            let base = get_memory_base(&mut caller);
            let symbol = typemap::get_cstr(base + sym as u64).unwrap();
            println!("[debug] dlsym {}", symbol);
            let lib_symbol = caller.get_library_symbols((handle - 1) as usize).unwrap();
            let val = *lib_symbol.get(&String::from(symbol)).unwrap() as i32;
            println!("[debug] dlsym resolves {} to {}", symbol, val);
            val
        };

        let dlclose = move |mut caller: wasmtime::Caller<'_, HostCtx>, handle: i32| -> i32 {
            println!("[debug] dlclose handle {}", handle);
            // to do: implement dlclose
            0
        };

        let mut linker = linker.lock().unwrap();
        {
            linker.func_wrap(
                "lind",
                "dlopen",
                dlopen,
            ).unwrap();

            linker.func_wrap(
                "lind",
                "dlsym",
                dlsym,
            ).unwrap();

            linker.func_wrap(
                "lind",
                "dlclose",
                dlclose,
            ).unwrap();
        }
    }

    attach_api(
        &mut wstore,
        &mut linker,
        &module,
        lind_manager.clone(),
        lindboot_cli.clone(),
        None,
    )?;


    // Load the preload wasm modules.
    let mut modules = Vec::new();
    modules.push((String::new(), module.clone()));
    println!("[debug]: preload list: {:?}", lindboot_cli.preloads);
    for (name, path) in lindboot_cli.preloads.iter() {
        println!("[debug]: preload {}", name);
        // Read the wasm module binary either as `*.wat` or a raw binary
        let module = read_wasm_or_cwasm(&engine, path)?;
        modules.push((name.clone(), module.clone()));
    }

    // For each additional module (excluding the main module),
    // register its GOT imports with the shared LindGOT instance.
    //
    // This installs placeholder globals for unresolved GOT entries so that
    // the library modules can be instantiated first and have their symbols
    // patched later during relocation/export processing.
    //
    // We skip the first module because it is the main module, which was
    // already processed earlier.
    for (name, module) in modules.iter().skip(1) {
        let mut linker = linker.lock().unwrap();

        let mut got_guard = lind_got.lock().unwrap();
        linker.define_GOT_dispatcher(&mut wstore, &module, &mut *got_guard);
    }

    // Add the module's functions to the linker.
    for (name, module) in modules.iter().skip(1) {
        let mut linker = linker.lock().unwrap();

        // Read dylink metadata for this preloaded (library) module.
        // This contains the module's declared table/memory requirements.
        let dylink_info = module.dylink_meminfo();
        let dylink_info = dylink_info.as_ref().unwrap();
        // Append this library's function table region to the shared table.
        // `table_start` is the starting index of the library's reserved range
        // within the global indirect function table.
        let table_start = table.size(&mut wstore) as i32;

        #[cfg(feature = "debug-dylink")]
        println!("[debug] library table_start: {}, grow: {}", table_start, dylink_info.table_size);
        // Grow the shared indirect function table by the amount requested by the
        // library (as recorded in its dylink section). New slots are initialized
        // to null funcref.
        table.grow(&mut wstore, dylink_info.table_size, wasmtime::Ref::Func(None));

        // Link the library instance into the main linker namespace.
        // The linker records the module under `name` and uses `table_start`
        // to relocate/interpret the library's function references into the
        // shared table. GOT entries are patched through the shared LindGOT.
        {
            let mut guard = lind_got.lock().unwrap();
            linker.module(&mut wstore, &name, &module, &mut table, table_start, &*guard).context(format!(
                "failed to process preload `{}`",
                name,
            ))?;
        }
    }

    {
        let mut linker = linker.lock().unwrap();
        linker.define_unknown_imports_as_traps(&module);
    }

    {
        // Emit warnings for any GOT slots that remain unresolved after processing
        // preloads and defining trap stubs.
        let mut guard = lind_got.lock().unwrap();
        guard.warning_undefined();
    }

    // -- Run the first module in the first cage --
    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(
            &mut wstore,
            &mut linker,
            &module,
            CAGE_START_ID as u64,
            &args,
            lind_got
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
    let mut linker = Arc::new(Mutex::new(Linker::new(&engine)));
    // create Global Offset Table for dynamic loading
    // #[cfg(feature = "dylink-support")]
    let mut lind_got = Arc::new(Mutex::new(LindGOT::new()));

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
        load_main_module(&mut wstore, &mut linker, &module, cageid as u64, &args, lind_got)
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
    mut linker: &mut Arc<Mutex<Linker<HostCtx>>>,
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
    let mut linker_guard = linker.lock().unwrap();

    let mut linker = linker_guard.clone();
    let _ = wasi_common::sync::add_to_linker(&mut linker, |s: &mut HostCtx| {
        AsMut::<wasi_common::WasiCtx>::as_mut(s)
    });
    drop(linker);

    let mut builder = WasiCtxBuilder::new();
    let _ = builder.inherit_stdio().args(&lindboot_cli.args);
    builder.inherit_stdin();
    builder.inherit_stderr();
    wstore.data_mut().preview1_ctx = Some(builder.build());

    let mut linker = linker_guard.clone();
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
    drop(linker);

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
    got: Arc<Mutex<LindGOT>>,
) -> Result<Vec<Val>> {
    let mut linker_guard = linker.lock().unwrap();

    // todo:
    // I don't setup `epoch_handler` since it seems not being used by our previous implementation.
    // Not sure if this is related to our thread exit problem
    let linker = linker_guard.clone();
    let (instance, cage_instanceid) = linker
        .instantiate_with_lind(
            &mut *store,
            &module,
            InstantiateType::InstantiateFirst(cageid),
        )
        .context(format!("failed to instantiate"))?;
    drop(linker);

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
            let pointer = lind_epoch.get_handler_as_u64(&mut *store);
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
        let mut linker = linker_guard.clone();
        linker.define_weak_imports_as_traps(&module);
        let (instance, backup_cage_instanceid) = linker
            .instantiate_with_lind_thread(&mut *store, &module)
            .context(format!("failed to instantiate"))?;
        drop(linker);

        // update GOT entries after main module is instantiated
        // let mut funcs = vec![];
        let mut globals: Vec<(String, wasmtime::Global)> = vec![];
        for export in instance.exports(&mut store) {
            let name = export.name().to_owned();
            match export.into_extern() {
                // I don't think main module should update GOT functions? 
                // Extern::Func(func) => {
                //     funcs.push((name, func));
                // },
                wasmtime::Extern::Global(global) => {
                    globals.push((name, global));
                },
                _ => {}
            }
        }

        // for (name, func) in funcs {
        //     let index = table.grow(&mut store, 1, crate::Ref::Func(Some(func))).unwrap();
        //     if got.update_entry_if_exist(&name, index) {
        //         println!("[debug] update GOT.func.{} to {}", name, index);
        //     }
        // }
        for (name, global) in globals {
            let val = global.get(&mut store);
            // relocate the variable
            let val = val.i32().unwrap() as u32 + 1024 + 8388608 + 1024; // 0 stands for memory base for main module
            let mut guard = got.lock().unwrap();
            if (*guard).update_entry_if_unresolved(&name, val) {
                println!("[debug] main update GOT.mem.{} to {}", name, val);
            }
        }

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

fn load_library_module(
    mut main_module: &mut wasmtime::Caller<HostCtx>,
    mut main_linker: Arc<Mutex<Linker<HostCtx>>>,
    mut lind_got: Arc<Mutex<LindGOT>>,
    library_name: &str,
) -> i32 {
    println!("[debug] load_library_module");

    // use the same engine for the library
    let engine = main_module.engine();
    // let mut store = main_module.as_context_mut();

    let base_path = Path::new(LINDFS_ROOT);
    let library_path = Path::new(library_name);
    let library_path = library_path.strip_prefix("/").unwrap_or(library_path);

    let library_full_path = base_path.join(library_path);
    let library_full_path_str = library_full_path.to_str().unwrap();
    println!("[debug] base_path: {:?}, library_full_path: {:?}", base_path, library_full_path);

    let lib_module = read_wasm_or_cwasm(&engine, Path::new(library_full_path_str)).unwrap();

    let dylink_info = lib_module.dylink_meminfo().unwrap();

    let table_size = main_module.get_table_size();
    main_module.grow_table_lib(dylink_info.table_size, wasmtime::Ref::Func(None));
    let table_base = wasmtime::Global::new(&mut *main_module, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Const), Val::I32(table_size as i32)).unwrap();
    let memory_base = wasmtime::Global::new(&mut *main_module, wasmtime::GlobalType::new(ValType::I32, wasmtime::Mutability::Const), Val::I32(0)).unwrap();

    let handle;
    {
        let mut linker = main_linker.lock().unwrap();
        {
            let mut guard = lind_got.lock().unwrap();
            linker.define_GOT_dispatcher(&mut main_module, &lib_module, &mut *guard);
        }

        {
            let mut guard = lind_got.lock().unwrap();
            handle = linker.module_dyn(&mut main_module, library_full_path_str, &lib_module, table_size as i32, &*guard).context(format!(
                "failed to process library `{}`",
                library_full_path_str
            )).unwrap();
        }
    }
    println!("[debug] dlopen: handle={}", handle);

    handle as i32
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
