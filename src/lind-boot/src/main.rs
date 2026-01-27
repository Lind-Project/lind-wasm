use anyhow::{Context, Result, anyhow, bail};
use cage::signal::{lind_signal_init, lind_thread_exit, signal_may_trigger};
use rawposix::sys_calls::{rawposix_shutdown, rawposix_start};
use std::env;
use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::os::fd::FromRawFd;
use std::os::unix::io::{IntoRawFd, RawFd};
use std::ptr::NonNull;
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME};
use threei::{make_syscall, threei_const};
use wasi_common::sync::Dir;
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::vm::{VMContext, VMOpaqueContext};
use wasmtime::{
    AsContextMut, Caller, Engine, Func, Instance, InstantiateType, Linker, Module, Store, Val,
    ValType,
};
use wasmtime_lind_3i::{VmCtxWrapper, get_vmctx, init_vmctx_pool, rm_vmctx, set_vmctx};
use wasmtime_lind_multi_process::{CAGE_START_ID, LindCtx, LindHost, THREAD_START_ID};
use wasmtime_lind_utils::{LindCageManager, lind_syscall_numbers::EXIT_SYSCALL};
use wasmtime_wasi_threads::WasiThreadsCtx;
use wiggle::tracing::trace_span;

static HOME_DIR_PATH: &str = "/home";

#[derive(Debug, Clone, Default)]
struct Config {
    args: Vec<String>,
    env: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
struct Package {
    wasmbin: Vec<u8>,
    config: Config,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = create_package_from_cli()?;
    execute(package);
    Ok(())
}

/// Command line:
///   lind-boot [flags...] wasm_file.wasm -- arg1 arg2 ...
fn create_package_from_cli() -> Result<Package, Box<dyn std::error::Error>> {
    let raw_args: Vec<String> = env::args().skip(1).collect();

    if raw_args.is_empty() {
        return Err("usage: lind-boot [flags...] wasm_file.wasm -- [args for wasm]".into());
    }

    let sep_pos = raw_args.iter().position(|s| s == "--");

    let (before_sep, after_sep) = match sep_pos {
        Some(pos) => raw_args.split_at(pos),
        None => (raw_args.as_slice(), &[][..]),
    };

    if before_sep.is_empty() {
        return Err("missing wasm file: lind-boot [flags...] wasm_file.wasm -- [args]".into());
    }

    let wasm_path = before_sep
        .last()
        .expect("we just checked before_sep is not empty");

    let wasm_args: Vec<String> = if sep_pos.is_some() {
        after_sep.iter().skip(1).cloned().collect()
    } else {
        Vec::new()
    };

    let file = File::open(wasm_path)?;
    let wasmbin_fd: RawFd = file.into_raw_fd();

    let wasmbin_data = read_all_from_fd(wasmbin_fd)?;

    let env_vars: Vec<(String, String)> = env::vars().collect();

    let config = Config {
        args: wasm_args,
        env: env_vars,
    };

    let package = Package {
        wasmbin: wasmbin_data,
        config,
    };

    Ok(package)
}

fn read_all_from_fd(fd: RawFd) -> Result<Vec<u8>> {
    unsafe {
        let mut file = File::from_raw_fd(fd);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// The callback function registered with 3i uses a unified Wasm entry
/// function as the single re-entry point into the Wasm executable.
///
/// When invoked, this function first uses the provided grateid to
/// retrieve the corresponding `VMContext` pointer from lind-3i’s global
/// runtime-state table. The `VMContext` identifies the Wasmtime store and
/// instance associated with the target grate and allows execution to
/// re-enter the correct runtime context.
///
/// This function receives an address inside grate that identifies the target handler.
/// When invoked, the callback calls the entry function inside the Wasm
/// module, passing this address as an argument. The entry function then
/// dispatches control to the corresponding per-syscall implementation
/// based on the address provided by `register_handler`.
///
/// To complete the bridge between host and guest, the system uses
/// `Caller::with()` to re-enter the  Wasmtime runtime context from the
/// host side.
///
/// This function is called by 3i when a syscall is routed to a grate.
///
/// todo: Currently this function is sent to 3i from [run::execute] function.
/// This will be updated to be sent from lind-boot in the future.
pub extern "C" fn grate_callback_trampoline(
    in_grate_fn_ptr_u64: u64,
    cageid: u64,
    arg1: u64,
    arg1cageid: u64,
    arg2: u64,
    arg2cageid: u64,
    arg3: u64,
    arg3cageid: u64,
    arg4: u64,
    arg4cageid: u64,
    arg5: u64,
    arg5cageid: u64,
    arg6: u64,
    arg6cageid: u64,
) -> i32 {
    let vmctx_wrapper: VmCtxWrapper = match get_vmctx(cageid) {
        Some(v) => v,
        None => {
            panic!("no VMContext found for cage_id {}", cageid);
        }
    };

    // Convert back to VMContext
    let opaque: *mut VMOpaqueContext = vmctx_wrapper.as_ptr() as *mut VMOpaqueContext;

    let vmctx_raw: *mut VMContext = unsafe { VMContext::from_opaque(opaque) };

    // Re-enter Wasmtime using the stored vmctx pointer
    let grate_ret = unsafe {
        Caller::with(vmctx_raw, |caller: Caller<'_, HostCtx>| {
            let Caller {
                mut store,
                caller: instance,
            } = caller;

            // Resolve the unified entry function once per call
            let entry_func = instance
                .host_state()
                .downcast_ref::<Instance>()
                .ok_or_else(|| anyhow!("bad host_state Instance"))?
                .get_export(&mut store, "pass_fptr_to_wt")
                .and_then(|f| f.into_func())
                .ok_or_else(|| anyhow!("missing export `pass_fptr_to_wt`"))?;

            let typed_func = entry_func.typed::<(
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
                u64,
            ), i32>(&mut store)?;

            // Call the entry function with all arguments and in grate function pointer
            typed_func.call(
                &mut store,
                (
                    in_grate_fn_ptr_u64,
                    cageid,
                    arg1,
                    arg1cageid,
                    arg2,
                    arg2cageid,
                    arg3,
                    arg3cageid,
                    arg4,
                    arg4cageid,
                    arg5,
                    arg5cageid,
                    arg6,
                    arg6cageid,
                ),
            )
        })
        .unwrap_or(threei_const::GRATE_ERR)
    };
    // Push the vmctx back to the global pool
    set_vmctx(cageid, vmctx_wrapper);
    grate_ret
}

/// The HostCtx host structure stores all relevant execution context objects
/// `preview1_ctx`: the WASI preview1 context (used by glibc and POSIX emulation);
/// `lind_common_ctx`: the context responsible for per-cage state management (e.g., signal handling, cage ID tracking);
/// `lind_fork_ctx`: the multi-process management structure, encapsulating fork/exec state;
/// `wasi_threads`: which manages WASI thread-related capabilities.
#[derive(Default, Clone)]
struct HostCtx {
    preview1_ctx: Option<wasi_common::WasiCtx>,
    wasi_threads: Option<Arc<WasiThreadsCtx<HostCtx>>>,
    lind_fork_ctx: Option<LindCtx<HostCtx, Option<Config>>>,
}

/// This implementation allows HostCtx to be used where a mutable reference to `wasi_common::WasiCtx`
/// is expected.
impl AsMut<wasi_common::WasiCtx> for HostCtx {
    fn as_mut(&mut self) -> &mut wasi_common::WasiCtx {
        self.preview1_ctx
            .as_mut()
            .expect("preview1_ctx must be initialized before use")
    }
}

impl HostCtx {
    /// Performs a partial deep clone of the host context. It explicitly forks the WASI preview1
    /// context(`preview1_ctx`), the lind multi-process context (`lind_fork_ctx`), and the lind common
    /// context (`lind_common_ctx`). Other parts of the context, such as `wasi_threads`, are shared
    /// between forks since they are not required to be process-isolated.
    pub fn fork(&self) -> Self {
        // we want to do a real fork for wasi_preview1 context since glibc uses the environment variable
        // related interface here
        let forked_preview1_ctx = match &self.preview1_ctx {
            Some(ctx) => Some(ctx.fork()),
            None => None,
        };

        // and we also want to fork the lind-common context and lind-multi-process context
        let forked_lind_fork_ctx = match &self.lind_fork_ctx {
            Some(ctx) => Some(ctx.fork()),
            None => None,
        };

        // besides preview1_ctx, lind_common_ctx and forked_lind_fork_ctx, we do not
        // care about other context since they are not used by glibc so we can just share
        // them between processes
        let forked_host = Self {
            preview1_ctx: forked_preview1_ctx,
            lind_fork_ctx: forked_lind_fork_ctx,
            wasi_threads: self.wasi_threads.clone(),
        };

        return forked_host;
    }
}

impl LindHost<HostCtx, Option<Config>> for HostCtx {
    fn get_ctx(&self) -> LindCtx<HostCtx, Option<Config>> {
        self.lind_fork_ctx.clone().unwrap()
    }
}

pub fn execute(package: Package) -> anyhow::Result<Vec<Val>> {
    let Package { wasmbin, config } = package;
    let Config { args, env } = &config;

    let mut wt_config = wasmtime::Config::new();

    let engine = trace_span!("initialize Wasmtime engine")
        .in_scope(|| Engine::new(&wt_config))
        .context("failed to create execution engine")?;

    let host = HostCtx::default();

    let mut wstore =
        trace_span!("initialize Wasmtime store").in_scope(|| Store::new(&engine, host));

    let module = trace_span!("compile Wasm")
        .in_scope(|| Module::from_binary(&engine, &wasmbin))
        .context("failed to compile Wasm module")?;

    let lind_manager = Arc::new(LindCageManager::new(0));
    // Initialize Lind here
    rawposix_start(0);
    // new cage is created
    lind_manager.increment();
    // initialize vmctx pool
    init_vmctx_pool();

    // initialize trampoline entry function pointer for wasmtime runtime.
    // todo: will remove to lind-boot in the future
    threei::register_trampoline(
        threei_const::RUNTIME_TYPE_WASMTIME,
        grate_callback_trampoline,
    );

    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let mut linker = trace_span!("setup linker").in_scope(|| Linker::new(&engine));
    // Setup WASI-p1
    trace_span!("link WASI")
        .in_scope(|| {
            wasi_common::sync::add_to_linker(&mut linker, |s: &mut HostCtx| {
                AsMut::<wasi_common::WasiCtx>::as_mut(s)
            })
        })
        .context("failed to setup linker and link WASI")?;
    let mut builder = WasiCtxBuilder::new();
    // In WASI, the argv semantics follow the POSIX convention: argv[0] is expected to be the program name, and argv[1..]
    // are the actual arguments. However, in lind-boot, we don’t have access to the original program name since the Wasm
    // binary is typically loaded from a file descriptor rather than a path. As a result, we insert a placeholder
    // value as argv[0] when constructing the argument list.
    let mut full_args = vec!["main.wasm".to_string()];
    full_args.extend(args.clone());
    builder.inherit_stdio().args(&full_args);
    builder.inherit_stdin();
    builder.inherit_stderr();

    let dir = Dir::open_ambient_dir(HOME_DIR_PATH, cap_std::ambient_authority())
        .expect(&format!("failed to open {}", HOME_DIR_PATH));
    builder
        .preopened_dir(dir, ".")
        .expect("failed to open current directory");
    wstore.data_mut().preview1_ctx = Some(builder.build());

    // Setup WASI-thread
    trace_span!("link WASI-thread")
        .in_scope(|| {
            wasmtime_wasi_threads::add_to_linker(
                &mut linker,
                &wstore,
                &module,
                |s: &mut HostCtx| s.wasi_threads.as_ref().unwrap(),
            )
        })
        .context("failed to setup linker and link WASI")?;

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
            wasmbin.clone(),
            Some(config.clone()),
            |host| host.lind_fork_ctx.as_mut().unwrap(),
            |host| host.fork(),
            |wasmbin, config, path, args, cageid, lind_manager, envs| {
                let mut new_lind_conf = config.clone();
                let conf = new_lind_conf.get_or_insert_with(|| Config {
                    args: vec![],
                    ..Default::default()
                });
                conf.args = args.get(1..).map_or(vec![], |s| s.to_vec());

                execute_with_lind(
                    wasmbin.clone(),
                    Some(conf.clone()),
                    lind_manager.clone(),
                    cageid as u64,
                )
            },
        )?);
    }

    wstore.data_mut().wasi_threads = Some(Arc::new(WasiThreadsCtx::new(
        module.clone(),
        Arc::new(linker.clone()),
    )?));

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
            // exit the thread
            if lind_thread_exit(CAGE_START_ID as u64, THREAD_START_ID as u64) {
                // Clean up the context from the global table
                if !rm_vmctx(CAGE_START_ID as u64) {
                    panic!(
                        "[wasmtime|run] Failed to remove VMContext for cage_id {}",
                        CAGE_START_ID
                    );
                }

                // we clean the cage only if this is the last thread in the cage
                // exit the cage with the exit code
                // This is a direct underlying RawPOSIX call, so the `name` field will not be used.
                // We pass `0` here as a placeholder to avoid any unnecessary performance overhead.
                make_syscall(
                    1,                     // self cage id
                    (EXIT_SYSCALL) as u64, // syscall num
                    UNUSED_NAME,           // syscall name
                    1,                     // target cage id, should be itself
                    code as u64,           // Exit type
                    1,                     // self cage id
                    UNUSED_ARG,
                    UNUSED_ID,
                    UNUSED_ARG,
                    UNUSED_ID,
                    UNUSED_ARG,
                    UNUSED_ID,
                    UNUSED_ARG,
                    UNUSED_ID,
                    UNUSED_ARG,
                    UNUSED_ID,
                );

                // main cage exits
                lind_manager.decrement();
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

    result
}

pub fn execute_with_lind(
    // Wasm module
    wasmbin: Vec<u8>,
    // lind-boot configuration
    config: Option<Config>,
    lind_manager: Arc<LindCageManager>,
    cageid: u64,
) -> Result<Vec<Val>> {
    let Config { args, env } = &config.clone().unwrap_or_default();

    let mut wt_config = wasmtime::Config::new();

    let engine = trace_span!("initialize Wasmtime engine")
        .in_scope(|| Engine::new(&wt_config))
        .context("failed to create execution engine")?;

    let host = HostCtx::default();

    let mut wstore =
        trace_span!("initialize Wasmtime store").in_scope(|| Store::new(&engine, host));

    let module = trace_span!("compile Wasm")
        .in_scope(|| Module::from_binary(&engine, &wasmbin))
        .context("failed to compile Wasm module")?;

    // Set up the WASI. In lind-wasm, we predefine all the features we need are `thread` and `wasipreview1`
    // so we manually add them to the linker without checking the input
    let mut linker = trace_span!("setup linker").in_scope(|| Linker::new(&engine));
    // Setup WASI-p1
    trace_span!("link WASI")
        .in_scope(|| {
            wasi_common::sync::add_to_linker(&mut linker, |s: &mut HostCtx| {
                AsMut::<wasi_common::WasiCtx>::as_mut(s)
            })
        })
        .context("failed to setup linker and link WASI")?;
    let mut builder = WasiCtxBuilder::new();
    let mut full_args = vec!["main.wasm".to_string()];
    full_args.extend(args.clone());
    builder.inherit_stdio().args(&full_args);
    builder.inherit_stdin();
    builder.inherit_stderr();

    let dir = Dir::open_ambient_dir(HOME_DIR_PATH, cap_std::ambient_authority())
        .expect(&format!("failed to open {}", HOME_DIR_PATH));
    builder
        .preopened_dir(dir, ".")
        .expect("failed to open current directory");
    wstore.data_mut().preview1_ctx = Some(builder.build());

    // Setup WASI-thread
    trace_span!("link WASI-thread")
        .in_scope(|| {
            wasmtime_wasi_threads::add_to_linker(
                &mut linker,
                &wstore,
                &module,
                |s: &mut HostCtx| s.wasi_threads.as_ref().unwrap(),
            )
        })
        .context("failed to setup linker and link WASI")?;

    // attach Lind common APIs to the linker
    {
        wasmtime_lind_common::add_to_linker::<HostCtx, _>(&mut linker)?;
    }
    // attach Lind-Multi-Process-Context to the host
    {
        wstore.data_mut().lind_fork_ctx = Some(LindCtx::new_with_cageid(
            module.clone(),
            linker.clone(),
            lind_manager.clone(),
            wasmbin.clone(),
            config.clone(),
            cageid as i32,
            |host| host.lind_fork_ctx.as_mut().unwrap(),
            |host| host.fork(),
            |wasmbin, config, path, args, cageid, lind_manager, envs| {
                let mut new_lind_conf = config.clone();
                let conf = new_lind_conf.get_or_insert_with(|| Config {
                    args: vec![],
                    ..Default::default()
                });
                conf.args = args.get(1..).map_or(vec![], |s| s.to_vec());

                execute_with_lind(
                    wasmbin.clone(),
                    Some(conf.clone()),
                    lind_manager.clone(),
                    cageid as u64,
                )
            },
        )?);
    }

    wstore.data_mut().wasi_threads = Some(Arc::new(WasiThreadsCtx::new(
        module.clone(),
        Arc::new(linker.clone()),
    )?));

    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(&mut wstore, &mut linker, &module, cageid as u64, &args)
            .with_context(|| format!("failed to run main module"))
    });

    result
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
    // This function will be called at either the first cage or exec-ed cages. If there is already
    // a vmctx stored for this cage id, we remove it first to avoid stale vmctx pointer.
    if cageid != CAGE_START_ID as u64 {
        if !rm_vmctx(cageid) {
            panic!(
                "[wasmtime|run] Failed to remove existing VMContext for cage_id {}",
                cageid
            );
        }
    }
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
