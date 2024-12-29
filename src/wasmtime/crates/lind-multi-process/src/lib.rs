#![allow(dead_code)]

use anyhow::{anyhow, Result};
use rawposix::safeposix::dispatcher::lind_syscall_api;
use wasmtime_lind_utils::{parse_env_var, LindCageManager};

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use wasmtime::{AsContext, AsContextMut, Caller, ExternType, InstanceId, InstantiateType, Linker, Module, OnCalledAction, RewindingReturn, SharedMemory, Store, StoreOpaque, Val};

use wasmtime_environ::MemoryIndex;

pub mod clone_constants;

const ASYNCIFY_START_UNWIND: &str = "asyncify_start_unwind";
const ASYNCIFY_STOP_UNWIND: &str = "asyncify_stop_unwind";
const ASYNCIFY_START_REWIND: &str = "asyncify_start_rewind";
const ASYNCIFY_STOP_REWIND: &str = "asyncify_stop_rewind";

const LIND_FS_ROOT: &str = "/home/lind-wasm/src/RawPOSIX/tmp";

// Define the trait with the required method
pub trait LindHost<T, U> {
    fn get_ctx(&self) -> LindCtx<T, U>;
}

// Closures are abused in this file, mainly because the architecture of wasmtime itself does not support
// the sub modules to directly interact with the top level runtime engine. But multi-processing, especially exec syscall,
// would heavily require to do so. So the only convenient way to break the rule and communicate with the
// top level runtime engine is abusing closures.
#[derive(Clone)]
pub struct LindCtx<T, U> {
    // linker used by the module
    linker: Linker<T>,
    // the module associated with the ctx
    module: Module,

    // process id, should be same as cage id
    pid: i32,
    
    // next cage id
    next_cageid: Arc<AtomicU64>,

    // next thread id
    next_threadid: Arc<AtomicU32>,

    // used to keep track of how many active cages are running
    lind_manager: Arc<LindCageManager>,

    // from run.rs, used for exec call
    run_command: U,

    // get LindCtx from host
    get_cx: Arc<dyn Fn(&mut T) -> &mut LindCtx<T, U> + Send + Sync + 'static>,

    // fork the host
    fork_host: Arc<dyn Fn(&T) -> T + Send + Sync + 'static>,

    // exec the host
    exec_host: Arc<dyn Fn(&U, &str, &Vec<String>, i32, &Arc<AtomicU64>, &Arc<LindCageManager>, &Option<Vec<(String, Option<String>)>>) -> Result<Vec<Val>> + Send + Sync + 'static>,
}

impl<T: Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync> LindCtx<T, U> {
    // create a new LindContext
    // Function Argument:
    // * module: wasmtime module object, used to fork a new instance
    // * linker: wasmtime function linker. Used to link the imported functions
    // * lind_manager: global lind cage counter. Used to make sure the wasmtime runtime would only exit after all cages have exited
    // * run_command: used by exec closure below.
    // * next_cageid: a shared cage id counter, managed by lind-common.
    // * get_cx: get lindContext from Host object
    // * fork_host: closure to fork a host
    // * exec: closure for the exec syscall entry
    pub fn new(module: Module, linker: Linker<T>, lind_manager: Arc<LindCageManager>, run_command: U,
               next_cageid: Arc<AtomicU64>,
               get_cx: impl Fn(&mut T) -> &mut LindCtx<T, U> + Send + Sync + 'static,
               fork_host: impl Fn(&T) -> T + Send + Sync + 'static,
               exec: impl Fn(&U, &str, &Vec<String>, i32, &Arc<AtomicU64>, &Arc<LindCageManager>, &Option<Vec<(String, Option<String>)>>) -> Result<Vec<Val>> + Send + Sync + 'static,
            ) -> Result<Self> {
        // this method should only be called once from run.rs, other instances of LindCtx
        // are supposed to be created from fork() method

        let get_cx = Arc::new(get_cx);
        let fork_host = Arc::new(fork_host);
        let exec_host = Arc::new(exec);
        
        // cage id starts from 1
        let pid = 1;
        let next_threadid = Arc::new(AtomicU32::new(1)); // cageid starts from 1
        Ok(Self { linker, module: module.clone(), pid, next_cageid, next_threadid, lind_manager: lind_manager.clone(), run_command, get_cx, fork_host, exec_host })
    }

    // create a new LindContext with provided pid (cageid). This function is used by exec_syscall to create a new lindContext
    // Function Argument:
    // * module: wasmtime module object, used to fork a new instance
    // * linker: wasmtime function linker. Used to link the imported functions
    // * lind_manager: global lind cage counter. Used to make sure the wasmtime runtime would only exit after all cages have exited
    // * run_command: used by exec closure below.
    // * pid: pid(cageid) associated with the context
    // * next_cageid: a shared cage id counter, managed by lind-common.
    // * get_cx: get lindContext from Host object
    // * fork_host: closure to fork a host
    // * exec: closure for the exec syscall entry
    pub fn new_with_pid(module: Module, linker: Linker<T>, lind_manager: Arc<LindCageManager>, run_command: U, pid: i32, next_cageid: Arc<AtomicU64>,
                        get_cx: impl Fn(&mut T) -> &mut LindCtx<T, U> + Send + Sync + 'static,
                        fork_host: impl Fn(&T) -> T + Send + Sync + 'static,
                        exec: impl Fn(&U, &str, &Vec<String>, i32, &Arc<AtomicU64>, &Arc<LindCageManager>, &Option<Vec<(String, Option<String>)>>) -> Result<Vec<Val>> + Send + Sync + 'static,
        ) -> Result<Self> {
        let get_cx = Arc::new(get_cx);
        let fork_host = Arc::new(fork_host);
        let exec_host = Arc::new(exec);

        let next_threadid = Arc::new(AtomicU32::new(1)); // cageid starts from 1

        Ok(Self { linker, module: module.clone(), pid, next_cageid, next_threadid, lind_manager: lind_manager.clone(), run_command, get_cx, fork_host, exec_host })
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
        if caller.as_context().get_rewinding_state().rewinding {
            // stop the rewind
            let asyncify_stop_rewind_func = caller.get_asyncify_stop_rewind().unwrap();
            let _res = asyncify_stop_rewind_func.call(&mut caller, ());

            // retrieve the fork return value
            let retval = caller.as_context().get_rewinding_state().retval;

            // set rewinding state to false
            caller.as_context_mut().set_rewinding_state(RewindingReturn {
                rewinding: false,
                retval: 0,
            });

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
    pub fn fork_call(&self, mut caller: &mut Caller<'_, T>
                ) -> Result<i32> {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;
        let parent_addr_len = defined_memory.current_length();

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
        // -------------------------- <----- _usr (stack low)
        // |   unwind_data_start    | <----- u64
        // --------------------------
        // |   unwind_data_end      | <----- u64
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
            // 16 because it is the size of two u64
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + 16;
            *(unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }
        
        // mark the start of unwind
        let _res = asyncify_start_unwind_func.call(&mut caller, unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to child thread
        let cloned_address = address as u64;

        // retrieve the child host
        let mut child_host = (self.fork_host)(caller.data());
        // get next cage id
        let child_cageid = self.next_cage_id();
        if let None = child_cageid {
            panic!("running out of cageid!");
        }
        let child_cageid = child_cageid.unwrap();
        let parent_pid = self.pid;

        // calling fork in rawposix to fork the cage
        lind_syscall_api(
            self.pid as u64,
            68, // fork syscall
            0,
            0,
            child_cageid,
            0,
            0,
            0,
            0,
            0,
        );

        // use the same engine for parent and child
        let engine = self.module.engine().clone();

        let get_cx = self.get_cx.clone();

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        let is_parent_thread = store.is_thread();
        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // use a barrier to make sure the child has fully copied parent's memory before parent
            // resumes its execution
            let barrier = Arc::new(Barrier::new(2));
            let barrier_clone = Arc::clone(&barrier);

            let builder = thread::Builder::new().name(format!("lind-fork-{}", child_cageid));
            builder.spawn(move || {
                // create a new instance
                let store_inner = Store::<T>::new_inner(&engine);

                // get child context
                let child_ctx = get_cx(&mut child_host);
                child_ctx.pid = child_cageid as i32;

                // create a new memory area for child
                child_ctx.fork_memory(&store_inner, parent_addr_len);
                let instance_pre = Arc::new(child_ctx.linker.instantiate_pre(&child_ctx.module).unwrap());

                let lind_manager = child_ctx.lind_manager.clone();
                let mut store = Store::new_with_inner(&engine, child_host, store_inner);

                // if parent is a thread, so does the child
                if is_parent_thread {
                    store.set_is_thread(true);
                }

                // instantiate the module
                let instance = instance_pre.instantiate_with_lind(&mut store,
                    InstantiateType::InstantiateChild {
                        parent_pid: parent_pid as u64, child_pid: child_cageid
                    }).unwrap();

                // copy the entire memory from parent, note that the unwind data is also copied together
                // with the memory
                // let child_address: *mut u8;

                // // get the base address of the memory
                // {
                //     let handle = store.inner_mut().instance(InstanceId::from_index(0));
                //     let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
                //     child_address = defined_memory.base;
                // }
                
                // rawposix::safeposix::dispatcher::set_base_address(child_cageid, child_address as i64);
                // rawposix::safeposix::dispatcher::fork_vmmap_helper(parent_pid as u64, child_cageid);

                // new cage created, increment the cage counter
                lind_manager.increment();
                // create the cage in rustposix via rustposix fork

                barrier_clone.wait();

                // get the asyncify_rewind_start and module start function
                let child_rewind_start;

                match instance.get_typed_func::<i32, ()>(&mut store, ASYNCIFY_START_REWIND) {
                    Ok(func) => {
                        child_rewind_start = func;
                    },
                    Err(_error) => {
                        return -1;
                    }
                };

                // mark the child to rewind state
                let _ = child_rewind_start.call(&mut store, unwind_data_start_usr as i32);

                // set up rewind state and fork return value for child
                store.as_context_mut().set_rewinding_state(RewindingReturn {
                    rewinding: true,
                    retval: 0,
                });

                if store.is_thread() {
                    // fork inside a thread is currently not supported
                    return -1;
                } else {
                    // main thread calls fork, then we just call _start function
                    let child_start_func = instance
                        .get_func(&mut store, "_start")
                        .ok_or_else(|| anyhow!("no func export named `_start` found")).unwrap();

                    let ty = child_start_func.ty(&store);

                    let values = Vec::new();
                    let mut results = vec![Val::null_func_ref(); ty.results().len()];

                    let invoke_res = child_start_func
                        .call(&mut store, &values, &mut results);

                    // print errors if any when running the child process
                    if let Err(err) = invoke_res {
                        let e = wasi_common::maybe_exit_on_error(err);
                        eprintln!("Error: {:?}", e);
                        return 0;
                    }

                    // get the exit code of the module
                    let exit_code = results.get(0).expect("_start function does not have a return value");
                    match exit_code {
                        Val::I32(val) => {
                            // exit the cage with the exit code
                            lind_syscall_api(
                                child_cageid,
                                30,
                                0,
                                0,
                                *val as u64,
                                0,
                                0,
                                0,
                                0,
                                0,
                            );
                            // let _ = on_child_exit(*val);
                        },
                        _ => {
                            eprintln!("unexpected _start function return type!");
                        }
                    }

                    // the cage just exited, decrement the cage counter
                    lind_manager.decrement();
                }

                return 0;
            }).unwrap();

            // wait until child has fully copied the memory
            barrier.wait();

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, unwind_data_start_usr as i32);

            // set up rewind state and fork return value for parent
            store.set_rewinding_state(RewindingReturn {
                rewinding: true,
                retval: child_cageid as i32,
            });

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // shared-memory version of fork syscall, used to create a new thread
    // This is very similar to normal fork syscall, except the memory is not copied
    // and the saved unwind context need to be carefully copied and managed since parent
    // and child are operating two copies to unwind data in the same memory
    // Function Argument:
    // * stack_addr: child's base stack address
    // * stack_size: child's stack size
    // * child_tid: the address of the child's thread id. This should be set by wasmtime
    pub fn pthread_create_call(&self, mut caller: &mut Caller<'_, T>,
                    stack_addr: i32, stack_size: i32, child_tid: u64
                ) -> Result<i32> {
        println!("-----stack_addr: {}, stack_size: {}", stack_addr, stack_size);
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let parent_address = defined_memory.base;

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
            // 16 because it is the size of two u64
            *(parent_unwind_data_start_sys as *mut u64) = parent_unwind_data_start_usr + 16;
            *(parent_unwind_data_start_sys as *mut u64).add(1) = stack_pointer as u64;
        }
        
        // mark the start of unwind
        let _res = asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();
        let asyncify_start_rewind_func = caller.get_asyncify_start_rewind().unwrap();

        // we want to send this address to child thread
        let parent_address_u64 = parent_address as u64;
        let parent_stack_high_usr = caller.as_context().get_stack_base();

        // retrieve the child host
        let mut child_host = caller.data().clone();
        // get current cageid, child should have the same cageid
        let child_cageid = self.pid;

        // use the same engine for parent and child
        let engine = self.module.engine().clone();

        let get_cx = self.get_cx.clone();

        // set up child_tid
        let next_tid = match self.next_thread_id() {
            Some(val) => val,
            None => {
                println!("running out of thread id!");
                0
            }
        };
        let child_tid = child_tid as *mut u32;
        unsafe { *child_tid = next_tid; }

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        store.set_on_called(Box::new(move |mut store| {
            // once unwind is finished, the first u64 stored on the unwind_data becomes the actual
            // end address of the unwind_data
            let parent_unwind_data_end_usr = unsafe {
                *(parent_unwind_data_start_sys as *mut u64)
            };

            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // child's stack low = stack_high - stack_size
            let child_stack_low_usr = stack_addr as u64 - stack_size as u64;
            let child_unwind_data_start_usr = child_stack_low_usr;

            let child_unwind_data_start_sys = (parent_address_u64 + child_unwind_data_start_usr) as *mut u8;
            let rewind_total_size = (parent_unwind_data_end_usr - parent_unwind_data_start_usr) as usize;

            // copy the unwind data to child stack
            unsafe { std::ptr::copy_nonoverlapping(parent_unwind_data_start_sys as *const u8, child_unwind_data_start_sys, rewind_total_size); }
            // manage child's unwind context. The unwind context is consumed when the process uses it to rewind the callstack
            // so a seperate copy is needed for child. The unwind context also contains some absolute address that is relative to parent
            // hence we also need to translate it to be relative to child's stack
            unsafe {
                // first 4 bytes in unwind data represent the address of the end of the unwind data
                // we also need to change this for child
                *(child_unwind_data_start_sys as *mut u64) = child_unwind_data_start_usr + rewind_total_size as u64;
            }

            let builder = thread::Builder::new().name(format!("lind-thread-{}", next_tid));
            builder.spawn(move || {
                // create a new instance
                let store_inner = Store::<T>::new_inner(&engine);

                // get child context
                let child_ctx = get_cx(&mut child_host);
                // set up child pid
                child_ctx.pid = child_cageid;

                let instance_pre = Arc::new(child_ctx.linker.instantiate_pre(&child_ctx.module).unwrap());

                let mut store = Store::new_with_inner(&engine, child_host, store_inner);

                // mark as thread
                store.set_is_thread(true);

                // instantiate the module
                let instance = instance_pre.instantiate(&mut store).unwrap();

                // we might also want to perserve the offset of current stack pointer to stack bottom
                // not very sure if this is required, but just keep everything the same from parent seems to be good
                let offset = parent_stack_high_usr as i32 - stack_pointer;
                let stack_pointer_setter = instance
                    .get_typed_func::<i32, ()>(&mut store, "set_stack_pointer")
                    .unwrap();
                let _ = stack_pointer_setter.call(&mut store, stack_addr - offset);

                // get the asyncify_rewind_start and module start function
                let child_rewind_start;

                match instance.get_typed_func::<i32, ()>(&mut store, ASYNCIFY_START_REWIND) {
                    Ok(func) => {
                        child_rewind_start = func;
                    },
                    Err(_error) => {
                        return -1;
                    }
                };

                // mark the child to rewind state
                let _ = child_rewind_start.call(&mut store, child_stack_low_usr as i32);

                // set up rewind state and fork return value for child
                store.as_context_mut().set_rewinding_state(RewindingReturn {
                    rewinding: true,
                    retval: 0,
                });

                // store stack low and stack high for child
                store.as_context_mut().set_stack_top(child_stack_low_usr);
                store.as_context_mut().set_stack_base(stack_addr as u64);

                // main thread calls fork, then we calls from _start function
                let child_start_func = instance
                    .get_func(&mut store, "_start")
                    .ok_or_else(|| anyhow!("no func export named `_start` found")).unwrap();

                let ty = child_start_func.ty(&store);

                let values = Vec::new();
                let mut results = vec![Val::null_func_ref(); ty.results().len()];

                let invoke_res = child_start_func
                    .call(&mut store, &values, &mut results);

                // print errors if any when running the thread
                if let Err(err) = invoke_res {
                    let e = wasi_common::maybe_exit_on_error(err);
                    eprintln!("Error: {:?}", e);
                    return 0;
                }

                // get the exit code of the module
                let exit_code = results.get(0).expect("_start function does not have a return value");
                match exit_code {
                    Val::I32(_val) => {
                        // technically we need to do some clean up here like cleaning up signal stuff
                        // but signal is still WIP so this is a placeholder for it in the future
                    },
                    _ => {
                        eprintln!("unexpected _start function return type: {:?}", exit_code);
                    }
                }

                return 0;
            }).unwrap();

            // loop {}
            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, parent_unwind_data_start_usr as i32);

            // set up rewind state and fork return value for parent
            store.set_rewinding_state(RewindingReturn {
                rewinding: true,
                retval: next_tid as i32,
            });

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // execve syscall
    // Function Argument:
    // * path: the address of the path string in wasm memory
    // * argv: the address of the argument list in wasm memory
    // * envs: the address of the environment variable list in wasm memory
    pub fn execve_call(&self, mut caller: &mut Caller<'_, T>,
                             path: i64,
                             argv: i64,
                             envs: Option<i64>
                     ) -> Result<i32> {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;

        // get the wasm stack top address
        let parent_stack_low_usr = caller.as_context().get_stack_top();

        // we store the unwind at the top of the user stack
        let parent_unwind_data_start_usr = parent_stack_low_usr;
        let parent_unwind_data_start_sys = address as u64 + parent_unwind_data_start_usr;

        // parse the path and argv
        let path_ptr = ((address as i64) + path) as *const u8;
        let path_str;

        // NOTE: the address passed from wasm module is 32-bit address
        let argv_ptr = ((address as i64) + argv) as *const *const u8;
        let mut args = Vec::new();
        let mut environs = None;

        // convert the address into a list of argument string
        unsafe {
            // Manually find the null terminator
            let mut len = 0;
            while *path_ptr.add(len) != 0 {
                len += 1;
            }
    
            // Create a byte slice from the pointer
            let byte_slice = std::slice::from_raw_parts(path_ptr, len);
    
            // Convert the byte slice to a Rust string slice
            path_str = std::str::from_utf8(byte_slice).unwrap();

            let mut i = 0;

            // parse the arg pointers
            // Iterate over argv until we encounter a NULL pointer
            loop {
                let c_str = *(argv_ptr as *const i32).add(i) as *const i32;

                if c_str.is_null() {
                    break;  // Stop if we encounter NULL
                }

                let arg_ptr = ((address as i64) + (c_str as i64)) as *const c_char;

                // Convert it to a Rust String
                let arg = CStr::from_ptr(arg_ptr)
                    .to_string_lossy()
                    .into_owned();
                args.push(arg);

                i += 1;  // Move to the next argument
            }
        }

        // if user is passing absolute path, we need to first convert it to a relative path
        // by removing prefix "/" at the beginning, then join with lind filesystem root folder
        let usr_path = Path::new(path_str).strip_prefix("/").unwrap_or(Path::new(path_str));

        // NOTE: join method will replace the original path if joined path is an absolute path
        // so must make sure the usr_path is not absolute otherwise it may escape the lind filesystem
        let real_path = Path::new(LIND_FS_ROOT).join(usr_path);
        let real_path_str = String::from(real_path.to_str().unwrap());

        // if the file to exec does not exist
        if !std::path::Path::new(&real_path_str).exists() {
            // return ENOENT
            return Ok(-2);
        }

        // parse the environment variables
        if let Some(envs_addr) = envs {
            let env_ptr = ((address as i64) + envs_addr) as *const *const u8;
            let mut env_vec = Vec::new();

            unsafe {
                let mut i = 0;
    
                // Iterate over argv until we encounter a NULL pointer
                loop {
                    let c_str = *(env_ptr as *const i32).add(i) as *const i32;
    
                    if c_str.is_null() {
                        break;  // Stop if we encounter NULL
                    }
    
                    let env_ptr = ((address as i64) + (c_str as i64)) as *const c_char;
    
                    // Convert it to a Rust String
                    let env = CStr::from_ptr(env_ptr)
                        .to_string_lossy()
                        .into_owned();
                    let parsed = parse_env_var(&env);
                    env_vec.push(parsed);
    
                    i += 1;  // Move to the next argument
                }
            }
            environs = Some(env_vec);
        }

        // get the current stack pointer
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
        let _res = asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();

        let store = caller.as_context_mut().0;

        let cloned_run_command = self.run_command.clone();
        let cloned_next_cageid = self.next_cageid.clone();
        let cloned_lind_manager = self.lind_manager.clone();
        let cloned_pid = self.pid;

        let exec_call = self.exec_host.clone();

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // to-do: exec should not change the process id/cage id, however, the exec call from rustposix takes an
            // argument to change the process id. If we pass the same cageid, it would cause some error
            // lind_exec(cloned_pid as u64, cloned_pid as u64);
            let ret = exec_call(&cloned_run_command, &real_path_str, &args, cloned_pid, &cloned_next_cageid, &cloned_lind_manager, &environs);

            return Ok(OnCalledAction::Finish(ret.expect("exec-ed module error")));
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // exit syscall
    // technically this is pthread_exit syscall
    // actual exit syscall that would kill other threads is not supported yet
    // TODO: exit_call should be switched to epoch interrupt method later
    pub fn exit_call(&self, mut caller: &mut Caller<'_, T>, code: i32) {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;

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
        let _res = asyncify_start_unwind_func.call(&mut caller, parent_unwind_data_start_usr as i32);

        // get the asyncify_stop_unwind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func = caller.get_asyncify_stop_unwind().unwrap();

        let store = caller.as_context_mut().0;

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // after unwind, just continue returning

            return Ok(OnCalledAction::Finish(vec![Val::I32(code)]));
        }));
        // after returning from here, unwind process should start
    }

    // setjmp call
    // Basically do an unwind and rewind to the current process, and store the unwind_data into a hashmap
    // with the hash of the unwind_data as the key. The hash of the unwind_data also serves as the jmp_buf data.
    // When longjmp is called, the hash in the jmp_buf is retrieved and the unwind_data is obtained from the hashmap
    // Then perform an unwind on the current process, but then replace the unwind_data with the saved unwind_data
    // retrieved from hashmap, and continue the rewind. This approach allows the wasm process to restore to its
    // previous state
    pub fn setjmp_call(&self, mut caller: &mut Caller<'_, T>, jmp_buf: i32) -> Result<i32> {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;

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
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + 16;
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
            let unwind_data_end_usr = unsafe {
                *(unwind_data_start_sys as *mut u64)
            };

            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            let rewind_total_size = (unwind_data_end_usr - unwind_data_start_usr) as usize;

            // store the unwind data
            let hash = store.store_unwind_data(unwind_data_start_sys as *const u8, rewind_total_size);
            unsafe { *((cloned_address + jmp_buf as u64) as *mut u64) = hash; }

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, unwind_data_start_usr as i32);

            // set up rewind state and return value
            store.set_rewinding_state(RewindingReturn {
                rewinding: true,
                retval: 0,
            });

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // longjmp call
    // See comment above `setjmp_call`
    pub fn longjmp_call(&self, mut caller: &mut Caller<'_, T>, jmp_buf: i32, retval: i32) -> Result<i32> {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;

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
            *(unwind_data_start_sys as *mut u64) = unwind_data_start_usr + 16;
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

            let hash = unsafe { *((cloned_address + jmp_buf as u64) as *mut u64) };
            // retrieve the unwind data
            let data = store.retrieve_unwind_data(hash);

            let result = retval;

            if let Some(unwind_data) = data {
                // replace the unwind data
                unsafe { std::ptr::copy_nonoverlapping(unwind_data.as_ptr(), unwind_data_start_sys as *mut u8, unwind_data.len()); }
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

            // set up rewind state and return value
            store.set_rewinding_state(RewindingReturn {
                rewinding: true,
                retval: result,
            });

            // return InvokeAgain here would make parent re-invoke main
            return Ok(OnCalledAction::InvokeAgain);
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // Get the pid associated with the context. Currently unused interface
    pub fn getpid(&self) -> i32 {
        self.pid
    }

    // get the next cage id
    fn next_cage_id(&self) -> Option<u64> {
        // cageid is managed by lind-common
        return Some(self.next_cageid.load(Ordering::SeqCst));
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

    // fork the memory to child
    // Memory is attached to Linker instead of a specific wasm instance since
    // the memory needs to be shared between threads. To achieve this, we have to set the
    // memory to be imported memory, then share the imported memory to all the child thread.
    // Then when we want to fork a thread, we need to clone the Linker, then replace the
    // imported memory that it links to a new memory region.
    fn fork_memory(&mut self, store: &StoreOpaque, size: usize) {
        // allow shadowing means defining a symbol that already exits would replace the old one
        self.linker.allow_shadowing(true);
        for import in self.module.imports() {
            if let Some(m) = import.ty().memory() {
                if m.is_shared() {
                    // define a new shared memory for the child
                    let mut plan = m.clone();
                    // plan.set_minimum((size as u64).div_ceil(m.page_size()));

                    let mem = SharedMemory::new(self.module.engine(), plan.clone()).unwrap();
                    self.linker.define_with_inner(store, import.module(), import.name(), mem.clone()).unwrap();
                }
            }
        }
        // set shadowing state back
        self.linker.allow_shadowing(false);
    }

    // fork the state
    pub fn fork(&self) -> Self {
        let forked_ctx = Self {
            linker: self.linker.clone(),
            module: self.module.clone(),
            pid: 0, // pid is managed by lind-common
            next_cageid: self.next_cageid.clone(),
            next_threadid: Arc::new(AtomicU32::new(1)), // thread id starts from 1
            lind_manager: self.lind_manager.clone(),
            run_command: self.run_command.clone(),
            get_cx: self.get_cx.clone(),
            fork_host: self.fork_host.clone(),
            exec_host: self.exec_host.clone()
        };

        return forked_ctx;
    }
}

// get the base address of the wasm process
pub fn get_memory_base<T: Clone + Send + 'static + std::marker::Sync>(caller: &Caller<'_, T>) -> u64 {
    let handle = caller.as_context().0.instance(InstanceId::from_index(0));
    let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
    defined_memory.base as u64
}

// entry point of fork syscall
pub fn lind_fork<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>) -> Result<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.fork_call(caller)
}

// entry point of pthread_create syscall
pub fn lind_pthread_create<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>,
        stack_addr: i32, stack_size: i32, child_tid: u64) -> Result<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.pthread_create_call(caller, stack_addr, stack_size, child_tid)
}

// entry point of catch_rewind
pub fn catch_rewind<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>(caller: &mut Caller<'_, T>) -> Option<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.catch_rewind(caller)
}

// entry point of clone_syscall, called by lind-common
pub fn clone_syscall<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, args: &mut clone_constants::CloneArgStruct) -> i32
{
    // first let's check if the process is currently in rewind state
    let rewind_res = catch_rewind(caller);
    if rewind_res.is_some() {
        return rewind_res.unwrap();
    }

    // get the flags
    let flags = args.flags;
    // if CLONE_VM is set, we are creating a new thread (i.e. pthread_create)
    // otherwise, we are creating a process (i.e. fork)
    let isthread = flags & (clone_constants::CLONE_VM);

    if isthread == 0 {
        match lind_fork(caller) {
            Ok(res) => res,
            Err(_e) => -1
        }
    }
    else {
        // pthread_create
        match lind_pthread_create(caller, args.stack as i32, args.stack_size as i32, args.child_tid) {
            Ok(res) => res,
            Err(_e) => -1
        }
    }
}

// entry point of exec_syscall, called by lind-common
pub fn exec_syscall<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, path: i64, argv: i64, envs: i64) -> i32 {
    let host = caller.data().clone();
    let ctx = host.get_ctx();

    match ctx.execve_call(caller, path, argv, Some(envs))  {
        Ok(ret) => {
            ret
        }
        Err(e) => {
            log::error!("failed to exec: {}", e);
            -1
        }
    }
}

pub fn exit_syscall<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, exit_code: i32) -> i32 {
    let host = caller.data().clone();
    let ctx = host.get_ctx();

    ctx.exit_call(caller, exit_code);
    
    // exit syscall should not fail
    0
}

pub fn setjmp_call<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, jmp_buf: i32) -> i32 {
    // first let's check if the process is currently in rewind state
    let rewind_res = catch_rewind(caller);
    if rewind_res.is_some() {
        return rewind_res.unwrap();
    }
        
    let host = caller.data().clone();
    let ctx = host.get_ctx();

    ctx.setjmp_call(caller, jmp_buf).unwrap()
}

pub fn longjmp_call<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, jmp_buf: i32, retval: i32) -> i32 {
    let host = caller.data().clone();
    let ctx = host.get_ctx();

    let _res = ctx.longjmp_call(caller, jmp_buf, retval);
    
    0
}

// check if the module has the necessary exported Asyncify functions
fn support_asyncify(module: &Module) -> bool {
    module.get_export(ASYNCIFY_START_UNWIND).is_some() &&
    module.get_export(ASYNCIFY_STOP_UNWIND).is_some() &&
    module.get_export(ASYNCIFY_START_REWIND).is_some() &&
    module.get_export(ASYNCIFY_STOP_REWIND).is_some()
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
        Some(ExternType::Func(ty)) => {
            ty.params().len() == 0
                && ty.results().len() == 0
        }
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
        Some(ExternType::Func(ty)) => {
            ty.params().len() == 0
                && ty.results().len() == 0
        }
        _ => false,
    } {
        return false;
    }

    true
}
