#![allow(dead_code)]

use cfg_if::cfg_if;

use anyhow::{anyhow, Result};
use std::ffi::c_void;
use std::ptr::NonNull;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID, UNUSED_NAME};
use sysdefs::constants::syscall_const::{EXEC_SYSCALL, EXIT_SYSCALL, FORK_SYSCALL};
use sysdefs::constants::{Errno, MAX_SHEBANG_DEPTH};
use sysdefs::logging::lind_debug_panic;
use sysdefs::{constants::sys_const, data::sys_struct};
use threei::{threei::make_syscall, threei_const};
use wasmtime_lind_3i::{
    get_vmctx, get_vmctx_thread, rm_vmctx, rm_vmctx_thread, set_vmctx, set_vmctx_thread,
    VmCtxWrapper,
};
use wasmtime_lind_utils::{symbol_table::SymbolMap, LindCageManager, LindGOT};

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use wasmtime::vm::{VMContext, VMOpaqueContext};
use wasmtime::{
    AsContext, AsContextMut, AsyncifyState, Caller, ChildLibraryType, Engine, ExternType,
    InstanceId, InstantiateType, Linker, Module, OnCalledAction, SharedMemory, Store, StoreOpaque,
    Val, ValRaw, ValType,
};

use cage::alloc_cage_id;
use cage::signal::{lind_signal_init, lind_thread_exit};
use wasmtime_environ::MemoryIndex;

use crate::shebang::{build_shebang_argv, parse_shebang};
use crate::utils::{parse_argv, parse_env, parse_path};

pub mod signal;

mod shebang;
mod utils;

pub const CAGE_START_ID: i32 = 1; // cage id starts from 1
pub const THREAD_START_ID: i32 = 1; // thread id starts from 1

const ASYNCIFY_START_UNWIND: &str = "asyncify_start_unwind";
const ASYNCIFY_STOP_UNWIND: &str = "asyncify_stop_unwind";
const ASYNCIFY_START_REWIND: &str = "asyncify_start_rewind";
const ASYNCIFY_STOP_REWIND: &str = "asyncify_stop_rewind";

const UNWIND_METADATA_SIZE: u64 = 16;

// Define the trait with the required method
pub trait LindHost<T, U> {
    fn get_ctx(&self) -> LindCtx<T, U>;
    fn get_ctx_mut(&mut self) -> &mut LindCtx<T, U>;
}

// Closures are abused in this file, mainly because the architecture of wasmtime itself does not support
// the sub modules to directly interact with the top level runtime engine. But multi-processing, especially exec syscall,
// would heavily require to do so. So the only convenient way to break the rule and communicate with the
// top level runtime engine is abusing closures.
pub struct LindCtx<T, U> {
    // linker used by the module
    pub linker: Option<Linker<T>>,

    // Global Offset Table of the process
    pub got_table: Option<Arc<Mutex<LindGOT>>>,

    // the module associated with the ctx
    modules: Vec<(String, String, Module)>,

    // Shared list of dynamically loaded modules across all threads of a cage.
    // Each entry is (module_name, path, module, memory_base, symbol_map).
    // symbol_map is a clone taken at dlopen time so cross-thread replay can push
    // it to each thread's symbol table (enabling dlsym in worker threads).
    // Per-process (fork): child gets its own Arc with a copy of the parent's list.
    // Per-thread (pthread_create): all threads of a cage share the same Arc.
    dlopen_modules: Arc<Mutex<Vec<(String, String, Module, i32, SymbolMap)>>>,

    // Index into dlopen_modules tracking how many entries this thread has already
    // replayed. When has_pending_dlopen_replay() returns true, the thread replays
    // entries from dlopen_replay_index to the end and advances the index.
    dlopen_replay_index: usize,

    // cage id
    pub cageid: i32,

    // thread id
    pub tid: i32,

    // next thread id
    next_threadid: Arc<AtomicU32>,

    // used to keep track of how many active cages are running
    lind_manager: Arc<LindCageManager>,

    // from lind-boot, used for exec call
    lindboot_cli: U,

    // get LindCtx from host
    get_cx: Arc<dyn Fn(&mut T) -> &mut LindCtx<T, U> + Send + Sync + 'static>,

    // fork the host
    fork_host: Arc<dyn Fn(&T) -> T + Send + Sync + 'static>,

    // exec the host
    exec_host: Arc<
        dyn Fn(
                &U,
                &str,
                &Vec<String>,
                Engine,
                Module,
                i32,
                &Arc<LindCageManager>,
                &Option<Vec<(String, Option<String>)>>,
            ) -> Result<Vec<Val>>
            + Send
            + Sync
            + 'static,
    >,
}

impl<
        T: Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    > LindCtx<T, U>
{
    // create a new LindContext
    // Function Argument:
    // * module: wasmtime module object, used to fork a new instance
    // * linker: wasmtime function linker. Used to link the imported functions
    // * lind_manager: global lind cage counter. Used to make sure the wasmtime runtime would only exit after all cages have exited
    // * lindboot_cli: used by exec closure below.
    // * cageid: cageid associated with the context
    // * get_cx: get lindContext from Host object
    // * fork_host: closure to fork a host
    // * exec: closure for the exec syscall entry
    pub fn new(
        modules: Vec<(String, String, Module)>,
        linker: Linker<T>,
        got_table: Option<Arc<Mutex<LindGOT>>>,
        lind_manager: Arc<LindCageManager>,
        lindboot_cli: U,
        cageid: i32,
        get_cx: impl Fn(&mut T) -> &mut LindCtx<T, U> + Send + Sync + 'static,
        fork_host: impl Fn(&T) -> T + Send + Sync + 'static,
        exec: impl Fn(
                &U,
                &str,
                &Vec<String>,
                Engine,
                Module,
                i32,
                &Arc<LindCageManager>,
                &Option<Vec<(String, Option<String>)>>,
            ) -> Result<Vec<Val>>
            + Send
            + Sync
            + 'static,
    ) -> Result<Self> {
        // this method should only be called once from run.rs, other instances of LindCtx
        // are supposed to be created from fork() method

        let get_cx = Arc::new(get_cx);
        let fork_host = Arc::new(fork_host);
        let exec_host = Arc::new(exec);

        let tid = THREAD_START_ID;
        let next_threadid = Arc::new(AtomicU32::new(THREAD_START_ID as u32)); // cageid starts from 1
        Ok(Self {
            linker: Some(linker),
            got_table,
            modules: modules.clone(),
            dlopen_modules: Arc::new(Mutex::new(vec![])),
            dlopen_replay_index: 0,
            cageid,
            tid,
            next_threadid,
            lind_manager: lind_manager.clone(),
            lindboot_cli,
            get_cx,
            fork_host,
            exec_host,
        })
    }

    pub fn attach_linker(&mut self, linker: Linker<T>) {
        self.linker = Some(linker);
    }

    // Attach a LindGOT (Global Offset Table) to this context, wrapping it in
    // Arc<Mutex<>> for shared, thread-safe access. The GOT maps symbol names to
    // the addresses of their GOT cells; it is shared across all modules within
    // a cage so that cross-library indirect calls resolve to the correct target.
    pub fn attach_got_table(&mut self, got_table: Option<LindGOT>) {
        if let Some(got) = got_table {
            self.got_table = Some(Arc::new(Mutex::new(got)));
        }
    }

    // Record a dynamically loaded module (from dlopen) into this cage's shared
    // module list. During fork() and pthread_create(), every entry is re-instantiated
    // into the child/thread store so that the child inherits all libraries opened at
    // runtime. symbol_map is a clone of the library's symbol namespace, stored so
    // cross-thread replay can push it to each thread's symbol table (for dlsym).
    pub fn append_module(
        &mut self,
        path: String,
        module: Module,
        memory_base: i32,
        symbol_map: SymbolMap,
    ) {
        let mut list = self.dlopen_modules.lock().unwrap();
        list.push(("env".to_string(), path, module, memory_base, symbol_map));
    }

    // Returns true if there are dlopen'd modules that this thread has not yet replayed.
    pub fn has_pending_dlopen_replay(&self) -> bool {
        let list = self.dlopen_modules.lock().unwrap();
        self.dlopen_replay_index < list.len()
    }

    // Return a snapshot of the dlopen entries this thread has not yet replayed.
    // The lock is released before returning so callers can act without holding it.
    pub fn pending_dlopen_entries(&self) -> Vec<(String, String, Module, i32, SymbolMap)> {
        let list = self.dlopen_modules.lock().unwrap();
        list[self.dlopen_replay_index..].to_vec()
    }

    // Advance the per-thread replay cursor by `count` entries.
    pub fn advance_dlopen_replay(&mut self, count: usize) {
        self.dlopen_replay_index += count;
    }

    // Set the per-thread replay cursor to an absolute position.
    // Used after a startup replay loop that may have consumed more entries than
    // replay_start (set at fork_thread() time) if dlopen ran concurrently.
    pub fn set_dlopen_replay_index(&mut self, index: usize) {
        self.dlopen_replay_index = index;
    }

    // The way multi-processing works depends on Asyncify from Binaryen. Asyncify marks the process into 3 states:
    // normal state, unwind state and rewind state.
    // During the normal state, the process continues its execution as normal.
    // During the unwind state, the process is undergoing quick callstack unwind and the function context are saved.
    // During the rewind state, the process is restoring the callstack it saved.
    // Asyncify is a 2nd stage compilation that adds on top of the 1st stage compilation. After asyncify, the function
    // logic would become something like this:
    // ```
    // function A() {
    //     if current state == rewind {
    //         restore_function_context();
    //     }
    //     if current state == normal {
    //         ... some user code ...
    //         B();
    //     }
    //     if current state == unwind {
    //         save_function_context();
    //         return;
    //     }
    //     if current state == normal {
    //         ... some user code ...
    //     }
    // }
    // function B() {
    //     if current state == rewind {
    //         restore_function_context();
    //     }
    //     if current state == normal {
    //         ... some user code ...
    //         asyncify_start_unwind();
    //     }
    //     if current state == unwind {
    //         save_function_context();
    //         return;
    //     }
    //     if current state == normal {
    //         ... some user code ...
    //     }
    // }
    // ```
    // In this example, function B serves as the function that starts the callstack unwind. After calling
    // asyncify_start_unwind() in function B, it will returns immediately from function B, then function A,
    // with the context of each function saved.
    // There are four Asyncify functions to control the global asyncify state:
    // asyncify_start_unwind: start the callstack unwind. Essentially set the global state to unwind and return
    // asyncify_stop_unwind: stop the callstack unwind. Essentially set the global state to normal and return
    // asyncify_start_rewind: start the callstack rewind. Essentially set the global state to rewind and return
    // asyncify_stop_rewind: stop the callstack rewind. Essentially set the global state to normal and return
    // asyncify_start_unwind and asyncify_start_rewind also take an argument that specifies where to store/retrieve
    // the saved function context

    // check if current process is in rewind state
    // if yes, stop the rewind and return the clone syscall result
    pub fn catch_rewind(&self, mut caller: &mut Caller<'_, T>) -> Option<i32> {
        if let AsyncifyState::Rewind(retval) = caller.as_context().get_asyncify_state() {
            // stop the rewind
            let asyncify_stop_rewind_func = caller.get_asyncify_stop_rewind().unwrap();
            let _res = asyncify_stop_rewind_func.call(&mut caller, ());

            // set asyncify state to normal
            caller
                .as_context_mut()
                .set_asyncify_state(AsyncifyState::Normal);

            return Some(retval);
        }

        None
    }

    // fork syscall. Create a child wasm process that copied memory from parent. It works as follows:
    // 1. fork a wasmtime host
    // 2. call fork_syscall from rawposix to create a forked cage object
    // 3. unwind the parent callstack and save the function context (unwind context)
    // 4. create a new wasm instance from same module
    // 5. fork the memory region to child (including saved unwind context)
    // 6. start the rewind for both parent and child
    pub fn fork_call(&self, mut caller: &mut Caller<'_, T>, child_cageid: u64) -> Result<i32> {
        // get the base address of the memory
        let address = get_memory_base(&mut caller) as *mut u8;

        // main module is the first module in the module list
        let main_module = &self.modules.get(0).unwrap().2;

        // detect if dynamic loading is enabled
        let dylink_enabled = main_module.dylink_meminfo().is_some();

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // get the wasm stack top address
        let stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let unwind_data_start_usr = stack_low_usr;
        let unwind_data_start_sys = address as u64 + unwind_data_start_usr;

        // start unwind
        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // we need to tell two parameters to Asyncify:
        // unwind_data_start: the start address to store the unwind data.
        // unwind_data_end: the end address of the avaliable space Asyncify could work with
        // These two parameters are usually stored on the top of the unwind data.
        // Below is a graph describing the entire user's stack layout
        // -------------------------- <----- unwind_data_start_usr (stack low)
        // |    unwind arguments    | stores where to start and where is the end (16 bytes)
        // -------------------------- <----- unwind_data_start
        // |         .....          | |
        // |   actual unwind data   | | unwind data grow direction
        // |         .....          | V
        // -------------------------- <----- unwind_data_end (user's current stack pointer)
        // |         .....          | ^
        // |         .....          | |
        // |       stack data       | | user's stack grow direction
        // |         .....          | |
        // |         .....          | |
        // -------------------------- <----- stack high
        unsafe {
            // UNWIND_METADATA_SIZE is 16 because it is the size of two u64
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + UNWIND_METADATA_SIZE;
            *(unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        let get_cx = self.get_cx.clone();
        // retrieve the child host
        let mut child_host = (self.fork_host)(caller.data());

        let mut snapshot = {
            let mut parent_host = caller.data_mut();
            let parent_ctx = get_cx(&mut parent_host);
            // has to clone to prevent double mutable reference of caller
            // TODO: may worth a refactor in the future for performance
            let mut parent_linker = parent_ctx.linker.clone().unwrap();

            let snapshot = parent_linker.get_linker_snapshot_for_child(&mut caller, false);

            snapshot
        };

        let global_snapshots = caller.as_context_mut().get_global_snapshot();

        // mark the start of unwind
        let _res = asyncify_start_unwind_func.call(&mut caller, unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to child thread
        let cloned_address = address as u64;

        let parent_cageid = self.cageid;

        // use the same engine for parent and child
        let engine = main_module.engine().clone();

        let parent_stack_snapshots = caller.as_context_mut().get_stack_snapshots();
        let parent_stack_low = caller.as_context().get_stack_top();
        let parent_stack_high = caller.as_context().get_stack_base();

        let symbol_table = caller.get_library_symbol_table().clone();

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        let signal_asyncify_data = store.get_signal_asyncify_data();
        let syscall_asyncify_data = store.get_syscall_asyncify_data();
        let is_parent_thread = store.is_thread();

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // use a barrier to make sure the child has fully copied parent's memory before parent
            // resumes its execution
            let barrier = Arc::new(Barrier::new(2));
            let barrier_clone = Arc::clone(&barrier);

            let builder = thread::Builder::new().name(format!("lind-fork-{}", child_cageid));
            builder
                .spawn(move || {
                    // create a new instance
                    let store_inner = Store::<T>::new_inner(&engine, symbol_table);

                    // get child context
                    let child_ctx = get_cx(&mut child_host);
                    child_ctx.cageid = child_cageid as i32;

                    // main module is the first module in the module list
                    let mut main_module = &mut child_ctx.modules.get_mut(0).unwrap().2;

                    let lind_manager = child_ctx.lind_manager.clone();
                    let module = main_module.clone();
                    let modules = child_ctx.modules.clone();
                    let dlopen_modules = child_ctx.dlopen_modules.clone();
                    let mut store = Store::new_with_inner(&engine, child_host, store_inner);

                    let mut child_got = if dylink_enabled {
                        Some(LindGOT::new())
                    } else {
                        None
                    };

                    let (mut linker, memory_base_table, epoch_handler, child_memory_base) =
                        Linker::new_child_linker(
                            &mut store,
                            &engine,
                            &mut child_got,
                            &snapshot.0,
                            &snapshot.1,
                            &snapshot.2,
                        )
                        .expect("failed to create child linker");

                    // early init vmmap
                    cage::init_vmmap(child_cageid, child_memory_base.unwrap() as usize, None);

                    // update the linker for the child instance, since new linker contains some child-specific defines
                    // e.g. __stack_pointer, __indirect_function_table, etc.

                    let child_table = if dylink_enabled {
                        let mut table_size = 0;
                        for import in module.imports() {
                            if let wasmtime::ExternType::Table(table) = import.ty() {
                                table_size = table.minimum();
                            }
                        }
                        let mut child_table = linker
                            .attach_function_table(&mut store, table_size)
                            .unwrap();

                        linker.attach_asyncify(&mut store).unwrap();

                        for (name, path, module) in modules.iter().skip(1) {
                            // Read dylink metadata for this preloaded (library) module.
                            // This contains the module's declared table/memory requirements.
                            let dylink_info = module.dylink_meminfo();
                            let dylink_info = dylink_info.as_ref().unwrap();
                            // Append this library's function table region to the shared table.
                            // `table_start` is the starting index of the library's reserved range
                            // within the global indirect function table.
                            let table_start = child_table.size(&mut store) as i32;

                            #[cfg(feature = "debug-dylink")]
                            println!(
                                "[debug] library table_start: {}, grow: {}",
                                table_start, dylink_info.table_size
                            );
                            // Grow the shared indirect function table by the amount requested by the
                            // library (as recorded in its dylink section). New slots are initialized
                            // to null funcref.
                            child_table
                                .grow(
                                    &mut store,
                                    dylink_info.table_size,
                                    wasmtime::Ref::Func(None),
                                )
                                .unwrap();

                            let module_name = module
                                .name()
                                .unwrap_or_else(|| lind_debug_panic("module has no name"));
                            let module_memory_base = *memory_base_table
                                .get(module_name)
                                .expect("memory base not found for library");

                            linker.allow_shadowing(true);
                            // Link the library instance into the main linker namespace.
                            // The linker records the module under `name` and uses `table_start`
                            // to relocate/interpret the library's function references into the
                            // shared table. GOT entries are patched through the shared LindGOT.
                            let module_name = module
                                .name()
                                .unwrap_or_else(|| lind_debug_panic("module has no name"));
                            linker
                                .module_with_child(
                                    &mut store,
                                    child_cageid,
                                    &name,
                                    &module,
                                    &mut child_table,
                                    table_start,
                                    module_memory_base,
                                    ChildLibraryType::Process,
                                    global_snapshots
                                        .get(module_name)
                                        .map(Vec::as_slice)
                                        .unwrap_or(&[]),
                                )
                                .unwrap();
                            linker.allow_shadowing(false);
                        }

                        Some(child_table)
                    } else {
                        None
                    };

                    store.set_stack_snapshots(parent_stack_snapshots);

                    // if parent is a thread, so does the child
                    if is_parent_thread {
                        store.set_is_thread(true);
                    }

                    let (instance, grate_instanceid) = linker
                        .instantiate_with_lind(
                            &mut store,
                            &module,
                            InstantiateType::InstantiateChild {
                                parent_cageid: parent_cageid as u64,
                                child_cageid: child_cageid,
                            },
                        )
                        .unwrap();

                    // Global snapshot workflow:
                    // 1. register_named_instance: record the child's main module instance under
                    //    its wasm intrinsic name so that get_global_snapshot (called at the top of
                    //    this fork path) could locate it. Also used for name-collision detection.
                    // 2. apply_global_snapshots: restore the parent's Wasm globals (GOT cell
                    //    addresses, stack pointer, memory base pointers) into the child instance.
                    //    This ensures the child starts with a consistent view of all symbol
                    //    addresses rather than the zero-initialized defaults from instantiation.
                    //    Snapshots are looked up by module name from the HashMap captured before
                    //    the unwind; backup instances are never registered so they are naturally
                    //    excluded from the snapshot map.
                    let main_module_name = module
                        .name()
                        .unwrap_or_else(|| lind_debug_panic("module has no name"));
                    store
                        .as_context_mut()
                        .register_named_instance(main_module_name.to_string(), grate_instanceid);
                    instance.apply_global_snapshots(
                        &mut store,
                        global_snapshots
                            .get(main_module_name)
                            .map(Vec::as_slice)
                            .unwrap_or(&[]),
                    );

                    // Track how many dlopen entries were replayed during startup.
                    // Used to update dlopen_replay_index so the epoch-based replay path
                    // (handle_dlopen_replay in signal.rs) does not double-replay entries
                    // that were already handled here.
                    let mut dlopen_startup_replay_count = 0usize;

                    if dylink_enabled {
                        let mut child_table = child_table.unwrap();
                        instance.apply_GOT_relocs(&mut store, None, &child_table, None, false);

                        // Snapshot the dlopen list before iterating so we don't hold
                        // the lock while calling into Wasm (which could deadlock).
                        let dlopen_snapshot: Vec<_> = dlopen_modules.lock().unwrap().clone();
                        dlopen_startup_replay_count = dlopen_snapshot.len();
                        for (name, _path, module, module_memory_base, symbol_map) in
                            dlopen_snapshot.iter()
                        {
                            // Read dylink metadata for this dlopen'd module.
                            // This contains the module's declared table/memory requirements.
                            let dylink_info = module.dylink_meminfo();
                            let dylink_info = dylink_info.as_ref().unwrap();
                            // Append this library's function table region to the shared table.
                            // `table_start` is the starting index of the library's reserved range
                            // within the global indirect function table.
                            let table_start = child_table.size(&mut store) as i32;

                            #[cfg(feature = "debug-dylink")]
                            println!(
                                "[debug] library table_start: {}, grow: {}",
                                table_start, dylink_info.table_size
                            );
                            // Grow the shared indirect function table by the amount requested by the
                            // library (as recorded in its dylink section). New slots are initialized
                            // to null funcref.
                            child_table
                                .grow(
                                    &mut store,
                                    dylink_info.table_size,
                                    wasmtime::Ref::Func(None),
                                )
                                .unwrap();

                            let module_name = module
                                .name()
                                .unwrap_or_else(|| lind_debug_panic("module has no name"));

                            linker.allow_shadowing(true);
                            // Define GOT entries for this dlopen'd module before instantiating it.
                            // These entries may be absent from the child linker's snapshot when
                            // dlopen was called concurrently with (or after) fork/thread creation.
                            // define_GOT_dispatcher is a no-op for entries already present.
                            if let Some(ref mut got) = child_got {
                                let _ = linker.define_GOT_dispatcher(&mut store, module, got);
                            }
                            // Link the library instance into the main linker namespace.
                            // The linker records the module under `name` and uses `table_start`
                            // to relocate/interpret the library's function references into the
                            // shared table. GOT entries are patched through the shared LindGOT.
                            linker
                                .module_with_child(
                                    &mut store,
                                    child_cageid,
                                    &name,
                                    &module,
                                    &mut child_table,
                                    table_start,
                                    *module_memory_base,
                                    ChildLibraryType::Process,
                                    global_snapshots
                                        .get(module_name)
                                        .map(Vec::as_slice)
                                        .unwrap_or(&[]),
                                )
                                .unwrap();
                            linker.allow_shadowing(false);

                            // Register the library's symbols in the child store's symbol table
                            // so that dlsym(handle, name) works in the forked process.
                            let _ = store
                                .as_context_mut()
                                .push_library_symbols(symbol_map.clone());
                        }
                    }

                    let epoch_pointer = if epoch_handler.is_some() {
                        epoch_handler.unwrap() as *mut u64
                    } else {
                        cfg_if! {
                            // The disable_signals feature allows Wasmtime to run Lind binaries without inserting an epoch.
                            // It sets the signal pointer to 0, so any signals will trigger a fault in RawPOSIX.
                            // This is intended for debugging only and should not be used in production.
                            if #[cfg(feature = "disable_signals")] {
                                &mut 0
                            } else {
                                // retrieve the epoch global
                                let lind_epoch = instance
                                    .get_export(&mut store, "epoch")
                                    .and_then(|export| export.into_global())
                                    .expect("Failed to find epoch global export!");

                                // retrieve the handler (underlying pointer) for the epoch global
                                lind_epoch.get_handler_as_u64(&mut store)
                            }
                        }
                    };

                    // initialize the signal for the main thread of forked cage
                    lind_signal_init(
                        child_cageid,
                        epoch_pointer,
                        THREAD_START_ID,
                        true, /* this is the main thread */
                    );

                    // new cage created, increment the cage counter
                    lind_manager.increment();
                    // The main challenge in enabling dynamic syscall interposition between grates and 3i lies in Rust’s
                    // strict lifetime and ownership system, which makes retrieving the Wasmtime runtime context across
                    // instance boundaries particularly difficult. To overcome this, the design employs low-level context
                    // capture by extracting and storing vmctx pointers from Wasmtime’s internal `StoreOpaque` and `InstanceHandler`
                    // structures. See more details in [lind-3i/src/lib.rs]
                    // 1) Get StoreOpaque & InstanceHandler to extract vmctx pointer
                    let grate_storeopaque = store.inner_mut();
                    let grate_instancehandler = grate_storeopaque.instance(grate_instanceid);
                    let vmctx_ptr: *mut c_void = grate_instancehandler.vmctx().cast();

                    // 2) Extract vmctx pointer and put in a Send+Sync wrapper
                    let vmctx_wrapper = VmCtxWrapper {
                        vmctx: NonNull::new(vmctx_ptr).unwrap(),
                    };

                    // 3) Store the vmctx wrapper in the global table for later retrieval during syscalls
                    let rc = set_vmctx_thread(child_cageid, THREAD_START_ID as u64, vmctx_wrapper);

                    // 4) Notify threei of the cage runtime type
                    threei::set_cage_runtime(child_cageid, threei_const::RUNTIME_TYPE_WASMTIME);

                    // 5) Create backup instances to populate the vmctx pool
                    // See more comments in lind-3i/lib.rs
                    for _ in 0..9 {
                        let (_, backup_cage_instanceid) = linker
                            .instantiate_with_lind_thread(&mut store, &module, false)
                            .unwrap();
                        let backup_cage_storeopaque = store.inner_mut();
                        let backup_cage_instancehandler =
                            backup_cage_storeopaque.instance(backup_cage_instanceid);
                        let backup_vmctx_ptr: *mut c_void =
                            backup_cage_instancehandler.vmctx().cast();

                        let backup_vmctx_wrapper = VmCtxWrapper {
                            vmctx: NonNull::new(backup_vmctx_ptr).unwrap(),
                        };

                        set_vmctx(child_cageid, backup_vmctx_wrapper);
                    }

                    barrier_clone.wait();

                    // update the linker for the child instance, since new linker contains some child-specific defines
                    let mut new_child_host = store.data_mut();
                    let new_child_ctx = get_cx(&mut new_child_host);
                    new_child_ctx.attach_linker(linker);
                    new_child_ctx.attach_got_table(child_got);
                    // Synchronise the replay index with however many entries were replayed
                    // during startup (may be more than replay_start if dlopen ran concurrently).
                    new_child_ctx.set_dlopen_replay_index(dlopen_startup_replay_count);

                    // If dlopen ran concurrently and appended entries after our snapshot
                    // was taken, epoch_dlopen_trigger_others may have missed this forked
                    // process (epoch handler not registered yet). Self-trigger EPOCH_DLOPEN
                    // so the callback fires on the first Wasm function entry.
                    if new_child_ctx.has_pending_dlopen_replay() {
                        let ep = epoch_pointer as *mut u64;
                        unsafe {
                            *ep = cage::signal::EPOCH_DLOPEN;
                        }
                    }

                    // get the asyncify_rewind_start and module start function
                    let child_rewind_start;

                    match instance.get_typed_func::<i32, ()>(&mut store, ASYNCIFY_START_REWIND) {
                        Ok(func) => {
                            child_rewind_start = func;
                        }
                        Err(_error) => {
                            return -1;
                        }
                    };

                    // mark the child to rewind state
                    let _ = child_rewind_start.call(&mut store, unwind_data_start_usr as i32);

                    // set up rewind state and fork return value for child
                    store
                        .as_context_mut()
                        .set_asyncify_state(AsyncifyState::Rewind(0));

                    if store.is_thread() {
                        // fork inside a thread is currently not supported
                        return -1;
                    } else {
                        // main thread calls fork, then we just call _start function
                        let child_start_func = instance
                            .get_func(&mut store, "_start")
                            .ok_or_else(|| anyhow!("no func export named `_start` found"))
                            .unwrap();

                        let ty = child_start_func.ty(&store);

                        let values = Vec::new();
                        let mut results = vec![Val::null_func_ref(); ty.results().len()];

                        store.as_context_mut().set_stack_top(parent_stack_low);
                        store.as_context_mut().set_stack_base(parent_stack_high);
                        store
                            .as_context_mut()
                            .set_signal_asyncify_data(signal_asyncify_data);
                        store
                            .as_context_mut()
                            .set_syscall_asyncify_data(syscall_asyncify_data);

                        let invoke_res = child_start_func.call(&mut store, &values, &mut results);
                        // Wasm instance crashed — perform the same cleanup
                        // as the signal-handler error path so the parent
                        // sees a proper zombie and resources are freed.
                        if let Err(err) = invoke_res {
                            let e = wasi_common::maybe_exit_on_error(err);
                            eprintln!("Child Error: {:?}", e);
                            cage::cage_record_exit_status(
                                child_cageid,
                                cage::ExitStatus::Exited(1),
                            );
                            if let Some(c) = cage::get_cage(child_cageid) {
                                c.is_dead.store(true, std::sync::atomic::Ordering::Release);
                            }
                            threei::EXITING_TABLE.insert(child_cageid);
                            threei::handler_table::_rm_grate_from_handler(child_cageid);
                            cage::signal::lind_thread_exit(child_cageid, THREAD_START_ID as u64);
                            cage::cage_finalize(child_cageid);
                            if !rm_vmctx(child_cageid) {
                                eprintln!(
                                    "[wasmtime|fork-crash] Failed to remove VMContext for cage {}",
                                    child_cageid
                                );
                            }
                            lind_manager.decrement();
                            return 0;
                        }

                        // get the exit code of the module
                        let exit_code = results
                            .get(0)
                            .expect("_start function does not have a return value");
                        match exit_code {
                            Val::I32(val) => {}
                            _ => {
                                eprintln!("unexpected _start function return type!");
                            }
                        }
                    }

                    return 0;
                })
                .unwrap();

            // wait until child has fully copied the memory
            barrier.wait();

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, unwind_data_start_usr as i32);

            // set up asyncify state and fork return value for parent
            store.set_asyncify_state(AsyncifyState::Rewind(child_cageid as i32));

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // set asyncify state to unwind
        store.set_asyncify_state(AsyncifyState::Unwind);

        // The "dual" return from fork is handled directly in the wasm modules through asyncify.
        // We return the newly forked child_cageid so that callers invoking this path through
        // `make_threei_call` are aware of the cageid of the forked process.
        return Ok(child_cageid as i32);
    }

    // shared-memory version of fork syscall, used to create a new thread
    // This is very similar to normal fork syscall, except the memory is not copied
    // and the saved unwind context need to be carefully copied and managed since parent
    // and child are operating two copies to unwind data in the same memory
    // Function Argument:
    // * stack_addr: child's base stack address
    // * stack_size: child's stack size
    // * child_tid: the address of the child's thread id. This should be set by wasmtime
    pub fn pthread_create_call(
        &self,
        mut caller: &mut Caller<'_, T>,
        mut stack_addr: u32,
        stack_size: u32,
        child_tid: u64,
    ) -> Result<i32> {
        // get the base address of the memory
        let parent_address = get_memory_base(&mut caller) as *mut u8;

        // main module is the first module in the module list
        let main_module = self.modules.get(0).unwrap().2.clone();

        // detect if dynamic loading is enabled
        let dylink_enabled = main_module.dylink_meminfo().is_some();

        // get the wasm stack top address
        let parent_stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let parent_unwind_data_start_usr = parent_stack_low_usr;
        let parent_unwind_data_start_sys = parent_address as u64 + parent_unwind_data_start_usr;

        // get the current stack pointer
        let stack_pointer = caller.get_stack_pointer().unwrap();

        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // reference comments in fork_call
        unsafe {
            // UNWIND_METADATA_SIZE is 16 because it is the size of two u64
            *(parent_unwind_data_start_sys as *mut u64) =
                parent_unwind_data_start_usr + UNWIND_METADATA_SIZE;
            *(parent_unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        // set up child_tid
        let next_tid = match self.next_thread_id() {
            Some(val) => val,
            None => {
                println!("running out of thread id!");
                0
            }
        };
        let child_tid = child_tid as *mut u32;
        unsafe {
            *child_tid = next_tid;
        }

        let get_cx = self.get_cx.clone();

        // retrieve a snapshot of the Globals defined in the main module, which will be used to initialize the Globals in child instance.
        let mut snapshot = {
            let mut parent_host = caller.data_mut();
            let parent_ctx = get_cx(&mut parent_host);
            // has to clone to prevent double mutable reference of caller
            // TODO: may worth a refactor in the future for performance
            let mut parent_linker = parent_ctx.linker.clone().unwrap();

            let snapshot = parent_linker.get_linker_snapshot_for_child(&mut caller, true);

            snapshot
        };

        // retrieve the child host
        let mut child_host = caller.data().clone();

        let global_snapshots = caller.as_context_mut().get_global_snapshot();

        // mark the start of unwind
        let _res =
            asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to child thread
        let parent_address_u64 = parent_address as u64;
        let parent_stack_high_usr = caller.as_context().get_stack_base();

        let symbol_table = caller.get_library_symbol_table().clone();

        // get current cageid, child should have the same cageid
        let child_cageid = self.cageid;

        // use the same engine for parent and child
        let engine = main_module.engine().clone();

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        store.set_on_called(Box::new(move |mut store| {
            // once unwind is finished, the first u64 stored on the unwind_data becomes the actual
            // end address of the unwind_data
            let parent_unwind_data_end_usr = unsafe { *(parent_unwind_data_start_sys as *mut u64) };

            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // child's stack low = stack_high - stack_size
            let child_stack_low_usr = stack_addr as u64 - stack_size as u64;
            let child_unwind_data_start_usr = child_stack_low_usr;

            let child_unwind_data_start_sys =
                (parent_address_u64 + child_unwind_data_start_usr) as *mut u8;
            let rewind_total_size =
                (parent_unwind_data_end_usr - parent_unwind_data_start_usr) as usize;

            // copy the unwind data to child stack
            unsafe {
                std::ptr::copy_nonoverlapping(
                    parent_unwind_data_start_sys as *const u8,
                    child_unwind_data_start_sys,
                    rewind_total_size,
                );
            }
            // manage child's unwind context. The unwind context is consumed when the process uses it to rewind the callstack
            // so a seperate copy is needed for child. The unwind context also contains some absolute address that is relative to parent
            // hence we also need to translate it to be relative to child's stack
            unsafe {
                // first 4 bytes in unwind data represent the address of the end of the unwind data
                // we also need to change this for child
                *(child_unwind_data_start_sys as *mut u64) =
                    child_unwind_data_start_usr + rewind_total_size as u64;
            }

            let builder = thread::Builder::new().name(format!("lind-thread-{}", next_tid));
            builder
                .spawn(move || {
                    // create a new instance
                    let store_inner = Store::<T>::new_inner(&engine, symbol_table);

                    // get child context
                    let child_ctx = get_cx(&mut child_host);
                    // set up child cageid
                    child_ctx.cageid = child_cageid;
                    child_ctx.tid = next_tid as i32;

                    // main module is the first module in the module list
                    let mut main_module = &mut child_ctx.modules.get_mut(0).unwrap().2;
                    let lind_manager = child_ctx.lind_manager.clone();

                    let module = main_module.clone();
                    let modules = child_ctx.modules.clone();
                    let dlopen_modules = child_ctx.dlopen_modules.clone();

                    let mut store = Store::new_with_inner(&engine, child_host, store_inner);

                    let mut child_got = if dylink_enabled {
                        Some(LindGOT::new())
                    } else {
                        None
                    };

                    let (mut linker,
                         memory_base_table,
                         epoch_handler,
                         _,
                        ) = Linker::new_child_linker(&mut store,
                                &engine,
                                &mut child_got,
                                &snapshot.0,
                                &snapshot.1,
                                &snapshot.2
                        ).expect("failed to create child linker");

                    let child_table = if dylink_enabled {
                        let mut table_size = 0;
                        for import in module.imports() {
                            if let wasmtime::ExternType::Table(table) = import.ty() {
                                table_size = table.minimum();
                            }
                        }
                        let mut child_table = linker.attach_function_table(&mut store, table_size).unwrap();

                        linker.attach_asyncify(&mut store).unwrap();

                        for (name, path, module) in modules.iter().skip(1) {
                            // Read dylink metadata for this preloaded (library) module.
                            // This contains the module's declared table/memory requirements.
                            let dylink_info = module.dylink_meminfo();
                            let dylink_info = dylink_info.as_ref().unwrap();
                            // Append this library's function table region to the shared table.
                            // `table_start` is the starting index of the library's reserved range
                            // within the global indirect function table.
                            let table_start = child_table.size(&mut store) as i32;

                            #[cfg(feature = "debug-dylink")]
                            println!(
                                "[debug] library table_start: {}, grow: {}",
                                table_start, dylink_info.table_size
                            );
                            // Grow the shared indirect function table by the amount requested by the
                            // library (as recorded in its dylink section). New slots are initialized
                            // to null funcref.
                            child_table.grow(
                                &mut store,
                                dylink_info.table_size,
                                wasmtime::Ref::Func(None),
                            ).unwrap();

                            let module_name = module.name().unwrap_or_else(|| lind_debug_panic("module has no name"));
                            let module_memory_base = *memory_base_table.get(module_name).expect("memory base not found for library");

                            linker.allow_shadowing(true);
                            // Link the library instance into the main linker namespace.
                            // The linker records the module under `name` and uses `table_start`
                            // to relocate/interpret the library's function references into the
                            // shared table. GOT entries are patched through the shared LindGOT.
                            let module_name = module.name().unwrap_or_else(|| lind_debug_panic("module has no name"));
                            linker
                                .module_with_child(
                                    &mut store,
                                    child_cageid as u64,
                                    &name,
                                    &module,
                                    &mut child_table,
                                    table_start,
                                    module_memory_base,
                                    ChildLibraryType::Thread(&mut stack_addr),
                                    global_snapshots.get(module_name).map(Vec::as_slice).unwrap_or(&[]),
                                ).unwrap();
                            linker.allow_shadowing(false);
                        }

                        Some(child_table)
                    } else {
                        None
                    };

                    // mark as thread
                    store.set_is_thread(true);

                    // instantiate the module
                    let (instance, grate_instanceid) = linker
                        .instantiate_with_lind_thread(&mut store, &module, false)
                        .unwrap();

                    // Global snapshot workflow:
                    // 1. register_named_instance: record the thread's main module instance under
                    //    its wasm intrinsic name so that get_global_snapshot (called at the top of
                    //    this thread creation path) could locate it. Also used for name-collision detection.
                    // 2. apply_global_snapshots: restore the parent's Wasm globals (GOT cell
                    //    addresses, stack pointer, memory base pointers) into the thread instance.
                    //    Threads share the cage's linear memory but each have their own Wasmtime
                    //    Store/Instance, so globals must be explicitly synced from the parent's
                    //    snapshot. Snapshots are looked up by module name; backup instances are
                    //    never registered and are therefore naturally excluded.
                    let main_module_name = module.name().unwrap_or_else(|| lind_debug_panic("module has no name"));
                    store.as_context_mut().register_named_instance(main_module_name.to_string(), grate_instanceid);
                    instance.apply_global_snapshots(
                        &mut store,
                        global_snapshots.get(main_module_name).map(Vec::as_slice).unwrap_or(&[]),
                    );

                    // Track how many dlopen entries were replayed during startup.
                    let mut dlopen_startup_replay_count = 0usize;

                    if dylink_enabled {
                        let mut child_table = child_table.unwrap();
                        instance.apply_GOT_relocs(&mut store, None, &child_table, None, false);

                        // Snapshot the dlopen list before iterating so we don't hold
                        // the lock while calling into Wasm (which could deadlock).
                        let dlopen_snapshot: Vec<_> = dlopen_modules.lock().unwrap().clone();
                        dlopen_startup_replay_count = dlopen_snapshot.len();
                        for (name, _path, module, module_memory_base, symbol_map) in dlopen_snapshot.iter() {
                            let dylink_info = module.dylink_meminfo();
                            let dylink_info = dylink_info.as_ref().unwrap();
                            let table_start = child_table.size(&mut store) as i32;

                            #[cfg(feature = "debug-dylink")]
                            println!(
                                "[debug] dlopen library table_start: {}, grow: {}",
                                table_start, dylink_info.table_size
                            );
                            child_table
                                .grow(
                                    &mut store,
                                    dylink_info.table_size,
                                    wasmtime::Ref::Func(None),
                                )
                                .unwrap();

                            let module_name = module.name().unwrap_or_else(|| lind_debug_panic("module has no name"));
                            linker.allow_shadowing(true);
                            // Define GOT entries for this dlopen'd module before instantiating it.
                            // These entries may be absent from the child linker's snapshot when
                            // dlopen was called concurrently with (or after) pthread_create.
                            if let Some(ref mut got) = child_got {
                                let _ = linker.define_GOT_dispatcher(&mut store, module, got);
                            }
                            linker
                                .module_with_child(
                                    &mut store,
                                    child_cageid as u64,
                                    &name,
                                    &module,
                                    &mut child_table,
                                    table_start,
                                    *module_memory_base,
                                    ChildLibraryType::Thread(&mut stack_addr),
                                    global_snapshots.get(module_name).map(Vec::as_slice).unwrap_or(&[]),
                                )
                                .unwrap();
                            linker.allow_shadowing(false);

                            // Register the library's symbols in the thread's symbol table
                            // so that dlsym(handle, name) works in this thread.
                            let _ = store.as_context_mut().push_library_symbols(symbol_map.clone());
                        }
                    }

                    if let Ok(init_tls) = instance.get_typed_func::<i32, ()>(
                        store.as_context_mut(),
                        "__wasm_init_tls",
                    ) {
                        let get_tls_size = instance.get_typed_func::<(), i32>(
                            store.as_context_mut(),
                            "__get_aligned_tls_size",
                        ).unwrap();

                        let tls_size = get_tls_size.call(store.as_context_mut(), ()).unwrap();
                        stack_addr -= tls_size as u32;
                        let _ = init_tls.call(store.as_context_mut(), stack_addr as i32).unwrap();
                    }

                    // we might also want to perserve the offset of current stack pointer to stack bottom
                    // not very sure if this is required, but just keep everything the same from parent seems to be good
                    let offset = parent_stack_high_usr as u32 - stack_pointer;
                    let stack_pointer_setter = instance
                        .get_typed_func::<i32, ()>(&mut store, "set_stack_pointer")
                        .unwrap();
                    let _ = stack_pointer_setter.call(&mut store, (stack_addr - offset) as i32);
                    // TODO: set up __stack_low and __stack_high
                    // TODO: should share the imported wasm global

                    let epoch_pointer = if epoch_handler.is_some() {
                        epoch_handler.unwrap() as *mut u64
                    } else {
                        cfg_if! {
                            // The disable_signals feature allows Wasmtime to run Lind binaries without inserting an epoch.
                            // It sets the signal pointer to 0, so any signals will trigger a fault in RawPOSIX.
                            // This is intended for debugging only and should not be used in production.
                            if #[cfg(feature = "disable_signals")] {
                                &mut 0
                            } else {
                                // retrieve the epoch global
                                let lind_epoch = instance
                                    .get_export(&mut store, "epoch")
                                    .and_then(|export| export.into_global())
                                    .expect("Failed to find epoch global export!");

                                // retrieve the handler (underlying pointer) for the epoch global
                                lind_epoch.get_handler_as_u64(&mut store)
                            }
                        }
                    };

                    // initialize the signal for the thread of the cage
                    lind_signal_init(
                        child_cageid as u64,
                        epoch_pointer,
                        next_tid as i32,
                        false, /* this is not the main thread */
                    );

                    // The main challenge in enabling dynamic syscall interposition between grates and 3i lies in Rust’s
                    // strict lifetime and ownership system, which makes retrieving the Wasmtime runtime context across
                    // instance boundaries particularly difficult. To overcome this, the design employs low-level context
                    // capture by extracting and storing vmctx pointers from Wasmtime’s internal `StoreOpaque` and `InstanceHandler`
                    // structures. See more details in [lind-3i/src/lib.rs]
                    // 1) Get StoreOpaque & InstanceHandler to extract vmctx pointer
                    let grate_storeopaque = store.inner_mut();
                    let grate_instancehandler = grate_storeopaque.instance(grate_instanceid);
                    let vmctx_ptr: *mut c_void = grate_instancehandler.vmctx().cast();

                    // 2) Extract vmctx pointer and put in a Send+Sync wrapper
                    let vmctx_wrapper = VmCtxWrapper {
                        vmctx: NonNull::new(vmctx_ptr).unwrap(),
                    };

                    // 3) Store the vmctx wrapper in the global table for later retrieval during syscalls
                    let rc = set_vmctx_thread(child_cageid as u64, next_tid as u64, vmctx_wrapper);

                    // update the linker for the child instance, since new linker contains some child-specific defines
                    let mut new_child_host = store.data_mut();
                    let new_child_ctx = get_cx(&mut new_child_host);
                    new_child_ctx.attach_linker(linker);
                    new_child_ctx.attach_got_table(child_got);
                    // Synchronise the replay index with however many entries were replayed
                    // during startup (may be more than replay_start if dlopen ran concurrently).
                    new_child_ctx.set_dlopen_replay_index(dlopen_startup_replay_count);

                    // get the asyncify_rewind_start and module start function
                    let child_rewind_start;

                    match instance.get_typed_func::<i32, ()>(&mut store, ASYNCIFY_START_REWIND) {
                        Ok(func) => {
                            child_rewind_start = func;
                        }
                        Err(_error) => {
                            return -1;
                        }
                    };

                    // mark the child to rewind state
                    let _ = child_rewind_start.call(&mut store, child_stack_low_usr as i32);

                    // set up asyncify state and thread return value for child
                    store
                        .as_context_mut()
                        .set_asyncify_state(AsyncifyState::Rewind(0));

                    // store stack low and stack high for child
                    store.as_context_mut().set_stack_top(child_stack_low_usr);
                    store.as_context_mut().set_stack_base(stack_addr as u64);

                    // main thread calls fork, then we calls from _start function
                    let child_start_func = instance
                        .get_func(&mut store, "_start")
                        .ok_or_else(|| anyhow!("no func export named `_start` found"))
                        .unwrap();

                    let ty = child_start_func.ty(&store);

                    let values = Vec::new();
                    let mut results = vec![Val::null_func_ref(); ty.results().len()];

                    let invoke_res = child_start_func.call(&mut store, &values, &mut results);

                    // print errors if any when running the thread
                    if let Err(err) = invoke_res {
                        let e = wasi_common::maybe_exit_on_error(err);
                        eprintln!("Error: {:?}", e);
                        return 0;
                    }

                    // get the exit code of the module
                    let exit_code = results
                        .get(0)
                        .expect("_start function does not have a return value");
                    match exit_code {
                        Val::I32(val) => {
                            if !rm_vmctx_thread(child_cageid as u64, next_tid as u64) {
                                panic!(
                                    "[wasmtime|thread] Failed to remove existing VMContext for cage_id {}, tid {}",
                                    child_cageid, next_tid
                                );
                            }
                        }
                        _ => {
                            eprintln!("unexpected _start function return type: {:?}", exit_code);
                        }
                    }

                    return 0;
                })
                .unwrap();

            // loop {}
            // mark the parent to rewind state
            let _ =
                asyncify_start_rewind_func.call(&mut store, parent_unwind_data_start_usr as i32);

            // set up asyncify state and thread return value for parent
            store.set_asyncify_state(AsyncifyState::Rewind(next_tid as i32));

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // set asyncify state to unwind for parent
        store.set_asyncify_state(AsyncifyState::Unwind);

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // execve syscall
    // Function Argument:
    // * path: the address of the path string in wasm memory
    // * argv: the address of the argument list in wasm memory
    // * envs: the address of the environment variable list in wasm memory
    pub fn execve_call(
        &self,
        mut caller: &mut Caller<'_, T>,
        path: String,
        argv: Vec<String>,
        environs: Option<Vec<(String, Option<String>)>>,
        recursion_depth: i32,
    ) -> Result<i32> {
        // linux limits the maximum recursion depth of shebang
        // it's typical value is 4, so let's use the same value
        if recursion_depth > MAX_SHEBANG_DEPTH {
            return Ok(-(Errno::ELOOP as i32));
        }

        // if the file to exec does not exist
        if !std::path::Path::new(&path).exists() {
            // return ENOENT
            return Ok(-(Errno::ENOENT as i32));
        }

        // parse the wasm module as soon as possible to catch the error before unwinding, which is hard to unwind back if exec file has some problems
        let mut main_module = &self.modules.get(0).unwrap().2;
        let engine = main_module.engine().clone();
        let exec_file_path = Path::new(&path);
        let exec_module = match engine.detect_precompiled_file(exec_file_path) {
            Ok(_) => unsafe { Module::deserialize_file(&engine, exec_file_path) },
            Err(_) => Module::from_file(&engine, exec_file_path),
        };
        if exec_module.is_err() {
            let shebang_res = parse_shebang(exec_file_path);

            if shebang_res.is_err() {
                return Ok(-(Errno::ENOEXEC as i32));
            }

            let shebang_opt = shebang_res.unwrap();
            if shebang_opt.is_none() {
                return Ok(-(Errno::ENOEXEC as i32));
            }

            // if shebang is present, we reconstruct the argv and path and call execve again with the interpreter specified by shebang
            let shebang = shebang_opt.unwrap();

            let new_argv = match build_shebang_argv(&shebang, &argv) {
                Ok(args) => args,
                Err(_) => return Ok(-(Errno::ENOEXEC as i32)),
            };
            // it's safe to unwrap here since above build_shebang_argv already checks if the interpreter path is valid
            let new_path = shebang.interpreter.to_str().unwrap().to_string();

            return self.execve_call(caller, new_path, new_argv, environs, recursion_depth + 1);
        }
        let exec_module = exec_module.unwrap();

        // get the base address of the memory
        let address = get_memory_base(&mut caller);

        // get the wasm stack top address
        let parent_stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let parent_unwind_data_start_usr = parent_stack_low_usr;
        let parent_unwind_data_start_sys = address as u64 + parent_unwind_data_start_usr;

        // get the current stack pointer
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // reference comments in fork_call
        unsafe {
            // 16 because it is the size of two u64
            *(parent_unwind_data_start_sys as *mut u64) =
                parent_unwind_data_start_usr + UNWIND_METADATA_SIZE;
            *(parent_unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        // mark the start of unwind
        let _res =
            asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();

        let store = caller.as_context_mut().0;

        let cloned_lindboot_cli = self.lindboot_cli.clone();
        let cloned_lind_manager = self.lind_manager.clone();
        let cloned_cageid = self.cageid;

        let exec_call = self.exec_host.clone();

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // for exec, we do not need to do rewind after unwinding is done
            store.set_asyncify_state(AsyncifyState::Normal);

            if !rm_vmctx(cloned_cageid as u64) {
                panic!(
                    "[wasmtime|run] Failed to remove existing VMContext for cage_id {}",
                    cloned_cageid
                );
            }

            let ret = exec_call(
                &cloned_lindboot_cli,
                &path,
                &argv,
                engine,
                exec_module,
                cloned_cageid,
                &cloned_lind_manager,
                &environs,
            );

            return Ok(OnCalledAction::Finish(ret.expect("exec-ed module error")));
        }));

        // set asyncify state to unwind
        store.set_asyncify_state(AsyncifyState::Unwind);

        // after returning from here, unwind process should start
        // we use 0 to tell upstream (e.g. rawposix) that exec is successful
        // so that they are safe to execute their own cleanup mechanisms
        return Ok(0);
    }

    // exit syscall
    // actual exit syscall that would kill other threads is not supported yet
    // TODO: exit_call should be switched to epoch interrupt method later
    pub fn exit_call(&self, mut caller: &mut Caller<'_, T>, code: i32, _is_last_thread: u64) {
        // Capture values for the deferred OnCalledAction closure.
        // Every thread defers lind_thread_exit to OnCalledAction so that
        // the epoch_handler entry stays alive until the asyncify unwind
        // fully completes.  The actual last thread to finish handles
        // cage_finalize (zombie, SIGCHLD, rm_vmctx, cage removal).
        let deferred_cageid = self.cageid as u64;
        let deferred_tid = self.tid as u64;
        let deferred_lind_manager = self.lind_manager.clone();
        // get the base address of the memory
        let address = get_memory_base(&mut caller) as *mut u8;

        // get the wasm stack top address
        let parent_stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let parent_unwind_data_start_usr = parent_stack_low_usr;
        let parent_unwind_data_start_sys = address as u64 + parent_unwind_data_start_usr;

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // reference comments in fork_call
        unsafe {
            // 16 because it is the size of two u64
            *(parent_unwind_data_start_sys as *mut u64) = parent_unwind_data_start_usr + 16;
            *(parent_unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        // mark the start of unwind
        let _res =
            asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();

        let store = caller.as_context_mut().0;

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // Remove this thread from epoch_handler.  If this was the
            // actual last thread in the cage, lind_thread_exit returns
            // true and we handle full cage teardown.
            let is_last = cage::signal::lind_thread_exit(deferred_cageid, deferred_tid);
            if is_last {
                // cage_finalize waits for grate_inflight to drain,
                // records zombie/SIGCHLD, removes fdtable + cage.
                cage::cage_finalize(deferred_cageid);

                // Remove the VMContext pool (backup instances).
                if !rm_vmctx(deferred_cageid) {
                    eprintln!(
                        "[wasmtime|exit] Failed to remove VMContext for cage_id {}",
                        deferred_cageid
                    );
                }

                // Decrement the global cage count.
                deferred_lind_manager.decrement();
            }

            return Ok(OnCalledAction::Finish(vec![Val::I32(code)]));
        }));

        // set asyncify state to unwind
        store.set_asyncify_state(AsyncifyState::Unwind);
        // after returning from here, unwind process should start
    }

    // setjmp call
    // Basically do an unwind and rewind to the current process, and store the unwind_data into a hashmap
    // with the hash of the unwind_data as the key. The hash of the unwind_data also serves as the jmp_buf data.
    // When longjmp is called, the hash in the jmp_buf is retrieved and the unwind_data is obtained from the hashmap
    // Then perform an unwind on the current process, but then replace the unwind_data with the saved unwind_data
    // retrieved from hashmap, and continue the rewind. This approach allows the wasm process to restore to its
    // previous state
    pub fn setjmp_call(&self, mut caller: &mut Caller<'_, T>, jmp_buf: u32) -> Result<i32> {
        // get the base address of the memory
        let address = get_memory_base(&mut caller);

        // get the wasm stack top address
        let stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let unwind_data_start_usr = stack_low_usr;
        let unwind_data_start_sys = address as u64 + unwind_data_start_usr;

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // reference comments in fork_call
        unsafe {
            // 16 because it is the size of two u64
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + UNWIND_METADATA_SIZE;
            *(unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        // mark the start of unwind
        let _res = asyncify_start_unwind_func.call(&mut caller, unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to the thread
        let cloned_address = address as u64;

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        store.set_on_called(Box::new(move |mut store| {
            // once unwind is finished, the first u64 stored on the unwind_data becomes the actual
            // end address of the unwind_data
            let unwind_data_end_usr = unsafe { *(unwind_data_start_sys as *mut u64) };

            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            let rewind_total_size = (unwind_data_end_usr - unwind_data_start_usr) as usize;

            // store the unwind data
            let hash =
                store.store_unwind_data(unwind_data_start_sys as *const u8, rewind_total_size);
            unsafe {
                std::ptr::write_unaligned((cloned_address + jmp_buf as u64) as *mut u64, hash);
            }

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, unwind_data_start_usr as i32);

            // set up asyncify state and return value
            store.set_asyncify_state(AsyncifyState::Rewind(0));

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // set asyncify state to unwind
        store.set_asyncify_state(AsyncifyState::Unwind);

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // longjmp call
    // See comment above `setjmp_call`
    pub fn longjmp_call(
        &self,
        mut caller: &mut Caller<'_, T>,
        jmp_buf: u32,
        retval: i32,
    ) -> Result<i32> {
        // get the base address of the memory
        let address = get_memory_base(&mut caller);

        // get the wasm stack top address
        let stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let unwind_data_start_usr = stack_low_usr;
        let unwind_data_start_sys = address as u64 + unwind_data_start_usr;

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        let asyncify_start_unwind_func = caller.get_asyncify_start_unwind().unwrap();

        // store the parameter at the top of the stack
        // reference comments in fork_call
        unsafe {
            // 16 because it is the size of two u64
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + UNWIND_METADATA_SIZE;
            *(unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }

        // mark the start of unwind
        let _res = asyncify_start_unwind_func.call(&mut caller, unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to the thread
        let cloned_address = address as u64;

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            let hash =
                unsafe { std::ptr::read_unaligned((cloned_address + jmp_buf as u64) as *mut u64) };
            // retrieve the unwind data
            let data = store.retrieve_unwind_data(hash);

            let result = retval;

            if let Some(unwind_data) = data {
                // replace the unwind data
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        unwind_data.as_ptr(),
                        unwind_data_start_sys as *mut u8,
                        unwind_data.len(),
                    );
                }
            } else {
                // if the hash does not exist
                // according to standard, calling longjmp with invalid jmp_buf would
                // cause undefined behavior and may lead to crash. Since invalid jmp_buf is not able to be detected,
                // it will not return any kind of error.
                // However, our approach of using Asyncify to implement longjmp is able to detect it
                // and return an error if we want. But let's just follow the standard and just crash the program
                panic!("invalid longjmp jmp_buf!");
            }

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, unwind_data_start_usr as i32);

            // set up asyncify state and return value
            store.set_asyncify_state(AsyncifyState::Rewind(result));

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // set asyncify state to unwind
        store.set_asyncify_state(AsyncifyState::Unwind);

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // Get the cageid associated with the context.
    pub fn this_cageid(&self) -> i32 {
        self.cageid
    }

    // get the next thread id
    fn next_thread_id(&self) -> Option<u32> {
        match self
            .next_threadid
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| match v {
                ..=0x1ffffffe => Some(v + 1),
                _ => None,
            }) {
            Ok(v) => Some(v + 1),
            Err(_) => None,
        }
    }

    // fork the state for new process
    pub fn fork_process(&self) -> Self {
        // Child process gets its OWN Arc with a copy of the parent's dlopen list.
        // Processes are independent after fork; fork_call replays all entries from scratch.
        let forked_ctx = Self {
            linker: None,    // Linker is explicitly set up by the caller
            got_table: None, // new process should use a new GOT
            modules: self.modules.clone(),
            dlopen_modules: Arc::new(Mutex::new(self.dlopen_modules.lock().unwrap().clone())),
            dlopen_replay_index: 0, // fork_call replays all entries from scratch
            cageid: 0,              // cageid is managed by lind-common
            tid: 1,                 // thread id starts from 1
            next_threadid: Arc::new(AtomicU32::new(1)), // thread id starts from 1
            lind_manager: self.lind_manager.clone(),
            lindboot_cli: self.lindboot_cli.clone(),
            get_cx: self.get_cx.clone(),
            fork_host: self.fork_host.clone(),
            exec_host: self.exec_host.clone(),
        };

        return forked_ctx;
    }

    // fork the state for new thread
    pub fn fork_thread(&self) -> Self {
        // New thread shares the SAME Arc (not a copy). pthread_create_call replays all
        // current entries during thread creation, so start the index at the current length.
        let replay_start = self.dlopen_modules.lock().unwrap().len();
        let forked_ctx = Self {
            linker: None,    // Linker is explicitly set up by the caller
            got_table: None, // threads within a process should use same GOT
            modules: self.modules.clone(),
            dlopen_modules: Arc::clone(&self.dlopen_modules),
            dlopen_replay_index: replay_start, // already caught up via pthread_create_call replay
            cageid: self.cageid,
            tid: self.tid,
            next_threadid: self.next_threadid.clone(),
            lind_manager: self.lind_manager.clone(),
            lindboot_cli: self.lindboot_cli.clone(),
            get_cx: self.get_cx.clone(),
            fork_host: self.fork_host.clone(),
            exec_host: self.exec_host.clone(),
        };

        return forked_ctx;
    }
}

impl<T, U> Clone for LindCtx<T, U>
where
    T: Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        self.fork_thread()
    }
}

// get the base address of the wasm process
pub fn get_memory_base<T: Clone + Send + 'static + std::marker::Sync>(
    mut caller: &mut Caller<'_, T>,
) -> u64 {
    let mut memory_iter = caller.as_context_mut().0.all_memories();
    let memory = memory_iter.next().expect("no defined memory found").clone();
    drop(memory_iter);

    memory.data_ptr(caller.as_context()) as usize as u64
}

// entry point of fork syscall
pub fn lind_fork<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    child_cageid: u64,
) -> Result<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.fork_call(caller, child_cageid)
}

// entry point of pthread_create syscall
pub fn lind_pthread_create<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    stack_addr: u32,
    stack_size: u32,
    child_tid: u64,
) -> Result<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.pthread_create_call(caller, stack_addr, stack_size, child_tid)
}

// entry point of catch_rewind
pub fn catch_rewind<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
) -> Option<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.catch_rewind(caller)
}

/// Re-entering Wasmtime trampoline for `clone` semantics (fork / pthread_create).
///
/// This function is the **Wasmtime re-entry trampoline** used by the Lind to
/// complete cloning semantics that must be implemented inside the runtime.
///
/// Conceptually, the execution flow is:
///   Wasm
///     -> Wasmtime lind-common trampoline
///     -> 3i dispatch with grateid=RAWPOSIX
///     -> RawPOSIX syscall handling (decides fork vs thread)
///     -> 3i dispatch with grateid=WASMTIME
///     -> **back to Wasmtime (this function)**
///         -> lind_fork / lind_pthread_create
///
/// During `lind-boot` initialization, this function is extracted as a raw `u64`
/// function pointer and registered into the **3i handler table**, so RawPOSIX
/// can dispatch back into Wasmtime when it needs runtime support.
///
/// ## fork vs pthread_create
///
/// The decision is encoded in the `CloneArgStruct.flags`:
/// - If `CLONE_VM` is **not** set： treat as **cage(process) clone** (fork-like)
///     and call `lind_fork`.
/// - If `CLONE_VM` **is** set： treat as **thread clone** (pthread_create-like)
///     and call `lind_pthread_create`.
///
/// ## VMContext resolution
///
/// We must re-enter the correct Wasmtime instance/thread, so we resolve the
/// appropriate `VMContext` using the parent cage and the parent tid from the active
/// per-thread VMContext table. See more details on [lind-3i/src/lib.rs].
pub fn clone_syscall<T, U>(
    cageid: u64,
    clone_arg: u64,
    clone_arg_cageid: u64,
    parent_cageid: u64,
    parent_tid: u64,
    child_cageid: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    // `clone_arg` points to a shared ABI struct carrying clone flags and
    // thread creation parameters (stack, tid pointer, etc.).
    let args = unsafe { &mut *(clone_arg as *mut sys_struct::CloneArgStruct) };
    // Determine whether this clone request represents:
    // if CLONE_VM is set, we are creating a new thread (i.e. pthread_create)
    // otherwise, we are creating a process (i.e. fork)
    let flags = args.flags;
    let isthread = flags & (sys_const::CLONE_VM);

    unsafe {
        // Resolve the correct VMContext wrapper to re-enter Wasmtime.
        //
        // For fork-like clones we always use cage-level VMContext.
        // For thread clones we use the per-thread VMContext of `parent_tid`,
        // except that `parent_tid == 1` uses cage-level VMContext.
        let vmctx_wrapper: VmCtxWrapper = match get_vmctx_thread(parent_cageid, parent_tid) {
            Some(v) => v,
            None => {
                panic!("no VMContext found for cage_id {}", parent_cageid);
            }
        };

        // Convert back to VMContext
        let opaque: *mut VMOpaqueContext = vmctx_wrapper.as_ptr() as *mut VMOpaqueContext;
        let vmctx_raw: *mut VMContext = unsafe { VMContext::from_opaque(opaque) };

        let ret = Caller::with(vmctx_raw, |mut caller: Caller<'_, T>| {
            if isthread == 0 {
                // fork
                match lind_fork(&mut caller, child_cageid) {
                    Ok(res) => res,
                    Err(_e) => -1,
                }
            } else {
                // pthread_create
                match lind_pthread_create(
                    &mut caller,
                    args.stack as u32,
                    args.stack_size as u32,
                    args.child_tid,
                ) {
                    Ok(res) => res,
                    Err(_e) => -1,
                }
            }
        });

        set_vmctx_thread(parent_cageid, parent_tid, vmctx_wrapper);
        return ret;
    }
}

/// Entry point for `execve` after RawPOSIX-side processing.
///
/// This function represents the *return-to-Wasmtime* boundary after RawPOSIX
/// has finished its syscall-level work for `execve`.
///
/// Conceptually, the execution flow is:
///   Wasm
///     -> Wasmtime lind-common trampoline
///     -> 3i dispatch with grateid=RAWPOSIX
///     -> RawPOSIX syscall handling
///     -> 3i dispatch with grateid=WASMTIME
///     -> **back to Wasmtime**
///
/// `exec_syscall` is the point where we re-enter Wasmtime execution and
/// continue with Wasmtime-specific logic (e.g., instantiating the new module,
/// updating the caller context, and transferring control).
///
/// The function pointer of `exec_syscall` is registered into the 3i handler
/// table during lind-boot initialization (see `lind-boot/execute.rs` for the
/// detailed registration logic).
///
/// From that point on, 3i may invoke this function as a callback when an
/// `execve` syscall requires Wasmtime-side continuation.
///
/// ---
///
/// Cage ID semantics:
///
/// Due to the presence of *grates*, the cage that *issues* the syscall is not
/// necessarily the cage whose Wasmtime instance must be resumed.
///
/// In particular, path-related arguments may originate from a different cage
/// than `cageid`. Therefore, we explicitly use `path_cageid` as the *source
/// cage ID* when retrieving the `VMContext`.
///
/// This reflects the logical ownership of the address space that contains
/// the `path` argument. For the conceptual model behind this separation,
/// see the Grate/API documentation.
pub fn exec_syscall<T, U>(
    cageid: u64,
    path: u64,
    path_cageid: u64,
    argv: u64,
    argv_cageid: u64,
    envs: u64,
    envs_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    unsafe {
        let vmctx_wrapper: VmCtxWrapper =
            match get_vmctx_thread(path_cageid, THREAD_START_ID as u64) {
                Some(v) => v,
                None => {
                    panic!("no VMContext found for cage_id {}", path_cageid);
                }
            };
        // Convert back to VMContext
        let opaque: *mut VMOpaqueContext = vmctx_wrapper.as_ptr() as *mut VMOpaqueContext;

        let vmctx_raw: *mut VMContext = unsafe { VMContext::from_opaque(opaque) };

        Caller::with(vmctx_raw, |mut caller: Caller<'_, T>| {
            let host = caller.data().clone();
            let ctx = host.get_ctx();

            // parse the arguments from the caller's memory space
            let path = match parse_path(path) {
                Ok(path) => path,
                Err(_) => return -(Errno::EFAULT as i32),
            };

            let argv = match parse_argv(argv) {
                Ok(argv) => argv,
                Err(_) => return -(Errno::EFAULT as i32),
            };

            let envs = match parse_env(envs) {
                Ok(envs) => envs,
                Err(_) => return -(Errno::EFAULT as i32),
            };

            // exec depth starts from 1
            match ctx.execve_call(&mut caller, path, argv, envs, 1) {
                Ok(ret) => ret,
                Err(e) => {
                    log::error!("failed to exec: {}", e);
                    -1
                }
            }
        })
    }
}

/// Re-entering Wasmtime trampoline for the `exit` syscall.
///
/// This function serves as the **Wasmtime re-entry trampoline** for `exit`,
/// bridging execution back from the Lind / RawPOSIX world into Wasmtime.
///
/// Conceptually, the execution flow is:
///   Wasm
///     -> Wasmtime lind-common trampoline
///     -> 3i dispatch with grateid=RAWPOSIX
///     -> RawPOSIX syscall handling
///     -> 3i dispatch with grateid=WASMTIME
///     -> **back to Wasmtime**
///
/// During `lind-boot` initialization, this function is extracted as a raw
/// `u64` function pointer and registered into the **3i handler table**.
/// All subsequent `exit` syscalls are routed through this trampoline.
///
/// ## Thread exit vs. Cage (process) exit
///
/// The `exit` syscall must distinguish between **Thread exit** (non-last thread)
/// and **Cage / process exit** (or last thread exit) This distinction is resolved
/// in RawPOSIX, which determines whether the exiting thread is the last live
/// thread in the cage. The result is passed back through the `is_last_thread`
/// flag:
///
/// - `0`: not the last thread
/// - `1`: last thread (entire cage must be torn down)
///
/// Resource cleanup for the entire cage is triggered only when
/// `is_last_thread == 1` in exit_call implementation.
///
/// ## VMContext resolution
///
/// Since each thread owns a distinct `VMContext`, we must use `tid` to
/// resolve the correct context:
/// - `tid == 1`: main thread VMContext
/// - otherwise: per-thread VMContext
///
/// This VMContext is then used to re-enter Wasmtime and invoke
/// `ctx.exit_call`, completing the control transfer.
pub fn exit_syscall<T, U>(
    cageid: u64,
    exit_code: u64,
    exit_code_cageid: u64,
    tid: u64,
    is_last_thread: u64,
    _arg3: u64,
    _arg3_cageid: u64,
    _arg4: u64,
    _arg4_cageid: u64,
    _arg5: u64,
    _arg5_cageid: u64,
    _arg6: u64,
    _arg6_cageid: u64,
) -> i32
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    unsafe {
        // Resolve the correct VMContext wrapper based on thread id.
        // Since `exit` is thread-specific, we always use `tid` to resolve the context,
        // even for the main thread (`tid == 1`).
        let vmctx_wrapper: VmCtxWrapper = match get_vmctx_thread(exit_code_cageid, tid) {
            Some(v) => v,
            None => {
                panic!("no VMContext found for cage_id {}", exit_code_cageid);
            }
        };

        // Convert the stored opaque pointer back into a concrete VMContext
        // so that we can safely re-enter Wasmtime execution.
        let opaque: *mut VMOpaqueContext = vmctx_wrapper.as_ptr() as *mut VMOpaqueContext;
        let vmctx_raw: *mut VMContext = VMContext::from_opaque(opaque);

        // Re-enter Wasmtime with the recovered VMContext.
        Caller::with(vmctx_raw, |mut caller: Caller<'_, T>| {
            let host = caller.data().clone();
            let ctx = host.get_ctx();

            // Delegate exit handling back to Wasmtime.
            // `is_last_thread` determines whether this is a thread exit
            // or a full cage (process) exit.
            ctx.exit_call(&mut caller, exit_code as i32, is_last_thread);

            // `exit` syscall is not expected to fail.
            0
        })
    }
}

pub fn setjmp_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    jmp_buf: u32,
) -> i32 {
    // first let's check if the process is currently in rewind state
    let rewind_res = catch_rewind(caller);
    if rewind_res.is_some() {
        return rewind_res.unwrap();
    }

    let host = caller.data().clone();
    let ctx = host.get_ctx();

    ctx.setjmp_call(caller, jmp_buf).unwrap()
}

pub fn longjmp_call<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    jmp_buf: u32,
    retval: i32,
) -> i32 {
    let host = caller.data().clone();
    let ctx = host.get_ctx();

    let _res = ctx.longjmp_call(caller, jmp_buf, retval);

    0
}

pub fn current_cageid<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
) -> i32 {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.this_cageid()
}

// Get thread id of current caller
pub fn current_tid<T, U>(caller: &mut Caller<'_, T>) -> i32
where
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
{
    caller.data().get_ctx().tid as i32
}

// attach a new SharedMemory to the Linker for multi-threading usage
// Warning: only set need_init to true for first cage initialization
pub fn attach_shared_memory<
    T: LindHost<T, U> + Clone + Send + Sync + 'static,
    U: Clone + Send + Sync + 'static,
>(
    store: impl AsContext<Data = T>,
    mut linker: &mut Linker<T>,
    module: &Module,
    need_init: bool,
    cageid: i32,
) -> Result<()> {
    for import in module.imports() {
        if let Some(m) = import.ty().memory() {
            if m.is_shared() {
                let mem = SharedMemory::new(module.engine(), m.clone())?;
                if need_init {
                    // in case of first cage
                    // Initialize vmmap immediately after creating the shared linear memory
                    let memory_base = mem.get_memory_base();
                    cage::init_vmmap(cageid as u64, memory_base as usize, None);
                }
                linker.define(&store, import.module(), import.name(), mem.clone())?;

                return Ok(());
            }
        }
    }

    Err(anyhow!("Main Module does not contain a shared memory"))
}

// check if the module has the necessary exported Asyncify functions
fn support_asyncify(module: &Module) -> bool {
    module.get_export(ASYNCIFY_START_UNWIND).is_some()
        && module.get_export(ASYNCIFY_STOP_UNWIND).is_some()
        && module.get_export(ASYNCIFY_START_REWIND).is_some()
        && module.get_export(ASYNCIFY_STOP_REWIND).is_some()
}

// check if each exported Asyncify function has correct signature
fn has_correct_signature(module: &Module) -> bool {
    if !match module.get_export(ASYNCIFY_START_UNWIND) {
        Some(ExternType::Func(ty)) => {
            ty.params().len() == 1
                && ty.params().nth(0).unwrap().is_i32()
                && ty.results().len() == 0
        }
        _ => false,
    } {
        return false;
    }
    if !match module.get_export(ASYNCIFY_STOP_UNWIND) {
        Some(ExternType::Func(ty)) => ty.params().len() == 0 && ty.results().len() == 0,
        _ => false,
    } {
        return false;
    }
    if !match module.get_export(ASYNCIFY_START_REWIND) {
        Some(ExternType::Func(ty)) => {
            ty.params().len() == 1
                && ty.params().nth(0).unwrap().is_i32()
                && ty.results().len() == 0
        }
        _ => false,
    } {
        return false;
    }
    if !match module.get_export(ASYNCIFY_STOP_REWIND) {
        Some(ExternType::Func(ty)) => ty.params().len() == 0 && ty.results().len() == 0,
        _ => false,
    } {
        return false;
    }

    true
}
