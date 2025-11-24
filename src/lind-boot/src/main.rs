use std::env;
use std::fs::File;
use std::os::unix::io::{IntoRawFd, RawFd};
use std::os::fd::FromRawFd;
use std::io::Read;
// ----
use anyhow::{anyhow, bail, Context, Result};
use std::sync::{atomic::AtomicU64, Arc};
use threei::{make_syscall, threei_const, WasmGrateFnEntry};
use rawposix::sys_calls::{rawposix_shutdown, rawposix_start};
use cage::signal::{lind_signal_init, lind_thread_exit, signal_may_trigger};
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME};
use wasi_common::sync::WasiCtxBuilder;
use wasmtime::vm::{VMContext, VMOpaqueContext};
use wasmtime::{
    AsContextMut, Caller, Engine, Func, Instance, InstantiateType, Linker, Module, Store,
    Val, ValType,
};
use wasmtime_lind_common::LindCommonCtx;
use wasmtime_lind_multi_process::{LindCtx, LindHost, CAGE_START_ID, THREAD_START_ID};
use wasmtime_lind_utils::{lind_syscall_numbers::EXIT_SYSCALL, LindCageManager};
use wasmtime_lind_3i::set_gratefn_wasm;
use wasmtime_wasi_threads::WasiThreadsCtx;
use wiggle::tracing::trace_span;
use std::ptr::NonNull;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use wasi_common::sync::Dir;

static HOME_DIR_PATH: &str = "/home";

/// 运行时配置：从命令行和环境变量里拿到的东西
#[derive(Debug, Clone, Default)]
struct Config {
    /// 传给 wasm 程序本身的参数（不包括 lind-boot 自己的参数）
    args: Vec<String>,
    /// 当前环境变量快照
    env: Vec<(String, String)>,
}

/// 要传给 execute() 的包
#[derive(Debug, Clone)]
struct Package {
    /// wasm 可执行文件对应的 RawFd
    wasmbin: Vec<u8>,
    /// 运行配置
    config: Config,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let package = create_package_from_cli()?;
    execute(package);
    Ok(())
}

/// 按照 lind-boot 风格从命令行构造 Package
///
/// 假设命令行是：
///   lind-boot [flags...] wasm_file.wasm -- arg1 arg2 ...
///
/// - 最后一个 `--` 之前的参数的最后一项当作 wasm 文件路径
/// - `--` 之后的所有参数当作传给 wasm 的 args
fn create_package_from_cli() -> Result<Package, Box<dyn std::error::Error>> {
    // 跳过 argv[0]（程序名）
    let raw_args: Vec<String> = env::args().skip(1).collect();

    if raw_args.is_empty() {
        return Err("usage: lind-boot [flags...] wasm_file.wasm -- [args for wasm]".into());
    }

    // 找到 "--" 的位置（如果没有，就认为没有额外 args）
    let sep_pos = raw_args.iter().position(|s| s == "--");

    let (before_sep, after_sep) = match sep_pos {
        Some(pos) => raw_args.split_at(pos),
        None => (raw_args.as_slice(), &[][..]),
    };

    if before_sep.is_empty() {
        return Err("missing wasm file: lind-boot [flags...] wasm_file.wasm -- [args]".into());
    }

    // 按 wasmtime/lind-boot 的习惯：最后一个参数视为 wasm 文件路径
    let wasm_path = before_sep
        .last()
        .expect("we just checked before_sep is not empty");

    // `--` 后面的部分（去掉 "--" 本身）是传给 wasm 的 args
    let wasm_args: Vec<String> = if sep_pos.is_some() {
        after_sep.iter().skip(1).cloned().collect()
    } else {
        Vec::new()
    };

    // 打开 wasm 文件，获取 RawFd
    let file = File::open(wasm_path)?;
    // into_raw_fd() 会把 File 的所有权转移出去，不再自动 close
    let wasmbin_fd: RawFd = file.into_raw_fd();

    let wasmbin_data = read_all_from_fd(wasmbin_fd)?;

    // 收集当前环境变量快照
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
    // from_raw_fd 会接管这个 fd，File drop 的时候会自动 close
    unsafe {
        let mut file = File::from_raw_fd(fd);
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }
}

/// Wraps a raw Wasmtime `VMContext` pointer for cross-boundary use.
///
/// The `VmCtxWrapper` type provides an minimal wrapper around a non-null
/// pointer to a `VMContext`. It allows the pointer to be passed between
/// Wasmtime and 3i without exposing the raw pointer everywhere in the
/// codebase.
struct VmCtxWrapper {
    vmctx: NonNull<c_void>,
}

unsafe impl Send for VmCtxWrapper {}
unsafe impl Sync for VmCtxWrapper {}

/// Holds both the process identifier and the Wasmtime context needed
/// for cross-instance callbacks.
///
/// Each `WasmCallbackCtx` instance corresponds to one Cage or Grate
/// process (`pid`) and its runtime context (`VmCtxWrapper`).
#[repr(C)]
struct WasmCallbackCtx {
    pid: u64,
    vm: VmCtxWrapper,
}

/// The callback function registered with 3i uses a unified Wasm entry
/// function as the single re-entry point into the Wasm executable. It
/// receives an address inside grate that identifies the target handler.
/// When invoked, the callback calls the entry function inside the Wasm
/// module, passing this address as an argument. The entry function then
/// dispatches control to the corresponding per-syscall implementation
/// based on the address provided by `register_handler`.
/// To complete the bridge between host and guest, the system uses
/// `Caller::with()` to re-enter the  Wasmtime runtime context from the
/// host side.
///
/// This function is called by 3i when a syscall is routed to a grate.
pub extern "C" fn grate_callback_trampoline(
    ctx: *mut c_void,
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
    // Never unwind across the C boundary.
    let res = catch_unwind(AssertUnwindSafe(|| unsafe {
        // Validatation check
        if ctx.is_null() {
            return threei_const::GRATE_ERR;
        }

        // Convert back to WasmCallbackCtx
        let ctx = &*(ctx as *const WasmCallbackCtx);
        let opaque: *mut VMOpaqueContext = ctx.vm.vmctx.as_ptr() as *mut VMOpaqueContext;
        let vmctx_raw: *mut VMContext = VMContext::from_opaque(opaque);

        // Re-enter Wasmtime using the stored vmctx pointer
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
    }));

    match res {
        Ok(v) => v,
        Err(_) => threei_const::GRATE_ERR,
    }
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
    lind_common_ctx: Option<LindCommonCtx>,
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

        let forked_lind_common_ctx = match &self.lind_common_ctx {
            Some(ctx) => Some(ctx.fork()),
            None => None,
        };

        // besides preview1_ctx, lind_common_ctx and forked_lind_fork_ctx, we do not
        // care about other context since they are not used by glibc so we can just share
        // them between processes
        let forked_host = Self {
            preview1_ctx: forked_preview1_ctx,
            lind_fork_ctx: forked_lind_fork_ctx,
            lind_common_ctx: forked_lind_common_ctx,
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

/// This function runs the first Wasm module in an Enarx Keep. It parses the Enarx package,
/// generates or attests an identity, sets up the Wasmtime engine, creates the initial store
/// and linker, and injects various contexts (WASI, lind-common, lind-multi-process). The
/// module is instantiated, and the main function is executed via load_main_module. This
/// function is the primary entry point for initial Wasm execution.
pub fn execute(package: Package) -> anyhow::Result<Vec<Val>> {

    let Package { wasmbin, config } = package;
    let Config {
        args,
        env,
    } = &config;

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
    lind_manager.increment();

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
    // are the actual arguments. However, in Enarx, we don’t have access to the original program name since the Wasm
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

    // attach Lind-Common-Context to the host
    let shared_next_cageid = Arc::new(AtomicU64::new(1));
    {
        wasmtime_lind_common::add_to_linker::<HostCtx, Option<Config>>(
            &mut linker,
            |host| host.lind_common_ctx.as_ref().unwrap(),
        )?;
        wstore.data_mut().lind_common_ctx =
            Some(LindCommonCtx::new(shared_next_cageid.clone())?);
    }

    // attach Lind-Multi-Process-Context to the host
    {
        wstore.data_mut().lind_fork_ctx = Some(LindCtx::new(
            module.clone(),
            linker.clone(),
            lind_manager.clone(),
            wasmbin.clone(),
            Some(config.clone()),
            shared_next_cageid.clone(),
            |host| host.lind_fork_ctx.as_mut().unwrap(),
            |host| host.fork(),
            |wasmbin, config, path, args, pid, next_cageid, lind_manager, envs| {
                let mut new_enarx_conf = config.clone();
                let conf = new_enarx_conf.get_or_insert_with(|| Config {
                    args: vec![],
                    ..Default::default()
                });
                conf.args = args.get(1..).map_or(vec![], |s| s.to_vec());

                execute_with_lind(
                    wasmbin.clone(),
                    Some(conf.clone()),
                    lind_manager.clone(),
                    pid as u64,
                    next_cageid.clone(),
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

/// This function is called when a new Wasm module is executed via an exec() syscall inside
/// a Wasm process. It mirrors much of the behavior of execute, but instead of reading
/// configuration from Enarx.toml, it uses an updated or synthetic config passed in at runtime.
/// This config has its args explicitly overridden. A new HostCtx is created, associated with
/// a new PID, and the module is launched in its own cage.
pub fn execute_with_lind(
    // Wasm module
    wasmbin: Vec<u8>,
    // lind-boot configuration
    config: Option<Config>,
    lind_manager: Arc<LindCageManager>,
    pid: u64,
    next_cageid: Arc<AtomicU64>,
) -> Result<Vec<Val>> {
    let Config {
        args,
        env,
    } = &config.clone().unwrap_or_default();

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
    // In WASI, the argv semantics follow the POSIX convention: argv[0] is expected to be the program name, and argv[1..]
    // are the actual arguments. However, in Enarx, we don’t have access to the original program name since the Wasm
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

    // attach Lind-Common-Context to the host
    let shared_next_cageid = Arc::new(AtomicU64::new(1));
    {
        wasmtime_lind_common::add_to_linker::<HostCtx, Option<Config>>(
            &mut linker,
            |host| host.lind_common_ctx.as_ref().unwrap(),
        )?;
        // Create a new lind ctx with the next cage ID since we are going to fork
        wstore.data_mut().lind_common_ctx = Some(LindCommonCtx::new_with_pid(
            pid as i32,
            next_cageid.clone(),
        )?);
    }

    // attach Lind-Multi-Process-Context to the host
    {
        wstore.data_mut().lind_fork_ctx = Some(LindCtx::new_with_pid(
            module.clone(),
            linker.clone(),
            lind_manager.clone(),
            wasmbin.clone(),
            config.clone(),
            pid as i32,
            next_cageid.clone(),
            |host| host.lind_fork_ctx.as_mut().unwrap(),
            |host| host.fork(),
            |wasmbin, config, path, args, pid, next_cageid, lind_manager, envs| {
                let mut new_enarx_conf = config.clone();
                let conf = new_enarx_conf.get_or_insert_with(|| Config {
                    args: vec![],
                    ..Default::default()
                });
                conf.args = args.get(1..).map_or(vec![], |s| s.to_vec());

                execute_with_lind(
                    wasmbin.clone(),
                    Some(conf.clone()),
                    lind_manager.clone(),
                    pid as u64,
                    next_cageid.clone(),
                )
            },
        )?);
    }

    wstore.data_mut().wasi_threads = Some(Arc::new(WasiThreadsCtx::new(
        module.clone(),
        Arc::new(linker.clone()),
    )?));

    let result = wasmtime_wasi::runtime::with_ambient_tokio_runtime(|| {
        load_main_module(&mut wstore, &mut linker, &module, pid as u64, &args)
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
    pid: u64,
    args: &[String],
) -> Result<Vec<Val>> {
    // I don't setup `epoch_handler` since it seems to be used for https, which is not required
    // for TriSeal. I'm not fully sure about this, but it works now.

    let (instance, grate_instanceid) = linker
        .instantiate_with_lind(&mut *store, &module, InstantiateType::InstantiateFirst(pid))
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
        pid,
        pointer as *mut u64,
        THREAD_START_ID,
        true, /* this is the main thread */
    );

    // // see comments at signal_may_trigger for more details
    signal_may_trigger(pid);

    // The main challenge in enabling dynamic syscall interposition between grates and 3i lies in Rust’s
    // strict lifetime and ownership system, which makes retrieving the Wasmtime runtime context across
    // instance boundaries particularly difficult. To overcome this, the design employs low-level context
    // capture by extracting and storing vmctx pointers from Wasmtime’s internal `StoreOpaque` and `InstanceHandler`
    // structures.
    // 1) Get StoreOpaque & InstanceHandler to extract vmctx pointer
    let grate_storeopaque = store.inner_mut();
    let grate_instancehandler = grate_storeopaque.instance(grate_instanceid);

    // 2) Extract vmctx pointer and put in a Send+Sync wrapper
    let vmctx_ptr: *mut c_void = grate_instancehandler.vmctx().cast();
    let ctx = WasmCallbackCtx {
        pid,
        vm: VmCtxWrapper {
            vmctx: NonNull::new(vmctx_ptr).ok_or_else(|| anyhow!("null vmctx"))?,
        },
    };

    // 3) Heap-allocate the context; 3i will keep this pointer until unregister
    let boxed: Box<WasmCallbackCtx> = Box::new(ctx);
    let ctx_ptr: *mut c_void = Box::into_raw(boxed) as *mut c_void;
    // Convert the trampoline to a function pointer
    let fn_ptr: extern "C" fn(
        *mut c_void,
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
    ) -> i32 = grate_callback_trampoline;

    // 4) Build entry and store in [`crates::lind-3i`] table
    let boxed_entry = Box::new(WasmGrateFnEntry { fn_ptr, ctx_ptr });
    let raw_entry: *const WasmGrateFnEntry = Box::into_raw(boxed_entry);
    let rc = set_gratefn_wasm(pid, raw_entry);
    if rc < 0 {
        // reclaim memory on error
        unsafe {
            drop(Box::from_raw(ctx_ptr as *mut WasmCallbackCtx));
        }
        return Err(anyhow!("3i rejected registration"));
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
