#![allow(dead_code)]

use anyhow::{anyhow, Result};
use rawposix::safeposix::dispatcher::lind_syscall_api;
use wasi_common::WasiCtx;
use wasmtime_lind_utils::{parse_env_var, LindCageManager};

use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use wasmtime::{AsContext, AsContextMut, Caller, ExternType, Linker, Module, SharedMemory, Store, Val, Extern, OnCalledAction, RewindingReturn, StoreOpaque, InstanceId};

use wasmtime_environ::MemoryIndex;

pub mod clone_constants;

const ASYNCIFY_START_UNWIND: &str = "asyncify_start_unwind";
const ASYNCIFY_STOP_UNWIND: &str = "asyncify_stop_unwind";
const ASYNCIFY_START_REWIND: &str = "asyncify_start_rewind";
const ASYNCIFY_STOP_REWIND: &str = "asyncify_stop_rewind";

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
    pub fn catch_rewind(&self, mut caller: &mut Caller<'_, T>) -> Result<i32> {
        if caller.as_context().get_rewinding_state().rewinding {
            // stop the rewind
            if let Some(asyncify_stop_rewind_extern) = caller.get_export(ASYNCIFY_STOP_REWIND) {
                match asyncify_stop_rewind_extern {
                    Extern::Func(asyncify_stop_rewind) => {
                        match asyncify_stop_rewind.typed::<(), ()>(&caller) {
                            Ok(func) => {
                                let _res = func.call(&mut caller, ());
                            }
                            Err(err) => {
                                eprintln!("the signature of asyncify_stop_rewind is not correct: {:?}", err);
                                return Ok(-1);
                            }
                        }
                    },
                    _ => {
                        eprintln!("asyncify_stop_rewind export is not a function");
                        return Ok(-1);
                    }
                }
            }
            else {
                eprintln!("asyncify_stop_rewind export not found");
                return Ok(-1);
            }

            // retrieve the fork return value
            let retval = caller.as_context().get_rewinding_state().retval;

            // set rewinding state to false
            caller.as_context_mut().set_rewinding_state(RewindingReturn {
                rewinding: false,
                retval: 0,
            });

            return Ok(retval);
        }

        Ok(-1)
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

        // start unwind
        if let Some(asyncify_start_unwind_extern) = caller.get_export(ASYNCIFY_START_UNWIND) {
            match asyncify_start_unwind_extern {
                Extern::Func(asyncify_start_unwind) => {
                    match asyncify_start_unwind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            let unwind_pointer: u64 = 0;
                            // 8 because we need to store unwind_data_start and unwind_data_end
                            // at the beginning of the unwind stack as the parameter for asyncify_start_unwind
                            // each of them are u64, so together is 8 bytes
                            let unwind_data_start: u64 = unwind_pointer + 8;
                            let unwind_data_end: u64 = stack_pointer as u64;
    
                            // store the parameter at the top of the stack
                            unsafe {
                                *(address as *mut u64) = unwind_data_start;
                                *(address as *mut u64).add(1) = unwind_data_end;
                            }
                            
                            // mark the start of unwind
                            let _res = func.call(&mut caller, unwind_pointer as i32);
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_start_unwind export not found");
            return Ok(-1);
        }

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func;
        let asyncify_start_rewind_func;

        if let Some(asyncify_stop_unwind_extern) = caller.get_export(ASYNCIFY_STOP_UNWIND) {
            match asyncify_stop_unwind_extern {
                Extern::Func(asyncify_stop_unwind) => {
                    match asyncify_stop_unwind.typed::<(), ()>(&caller) {
                        Ok(func) => {
                            asyncify_stop_unwind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_stop_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_stop_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_stop_unwind export not found");
            return Ok(-1);
        }

        if let Some(asyncify_start_rewind_extern) = caller.get_export(ASYNCIFY_START_REWIND) {
            match asyncify_start_rewind_extern {
                Extern::Func(asyncify_start_rewind) => {
                    match asyncify_start_rewind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            asyncify_start_rewind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_rewind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_rewind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_start_rewind export not found");
            return Ok(-1);
        }

        // we want to send this address to child thread
        let cloned_address = address as u64;

        // retrieve the child host
        let mut child_host = (self.fork_host)(caller.data());
        // get next cage id
        let child_cageid = self.next_cage_id();
        if let None = child_cageid {
            println!("running out of cageid!");
        }
        let child_cageid = child_cageid.unwrap();

        // calling fork in rawposix to fork the cage
        lind_syscall_api(
            self.pid as u64,
            68 as u32, // fork syscall
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
        let parent_pid = self.pid;

        // set up unwind callback function
        let store = caller.as_context_mut().0;
        let is_parent_thread = store.is_thread();
        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            let rewind_pointer: u64 = 0;

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
                let instance = instance_pre.instantiate(&mut store).unwrap();

                // copy the entire memory from parent, note that the unwind data is also copied together
                // with the memory
                let child_address: *mut u8;
                let address_length: usize;

                // get the base address of the memory
                {
                    let handle = store.inner_mut().instance(InstanceId::from_index(0));
                    let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
                    child_address = defined_memory.base;
                    address_length = defined_memory.current_length();
                }

                // copy the entire memory area from parent to child
                // this will be changed after mmap has been integrated into lind-wasm
                unsafe { std::ptr::copy_nonoverlapping(cloned_address as *mut u8, child_address, address_length); }

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
                let _ = child_rewind_start.call(&mut store, rewind_pointer as i32);

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
                                30 as u32,
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
            let _ = asyncify_start_rewind_func.call(&mut store, rewind_pointer as i32);

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
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;
        let parent_addr_len = defined_memory.current_length();

        let parent_stack_base = caller.as_context().get_stack_top();

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        if let Some(asyncify_start_unwind_extern) = caller.get_export(ASYNCIFY_START_UNWIND) {
            match asyncify_start_unwind_extern {
                Extern::Func(asyncify_start_unwind) => {
                    match asyncify_start_unwind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            let unwind_pointer: u64 = parent_stack_base;
                            // 8 because we need to store unwind_data_start and unwind_data_end
                            // at the beginning of the unwind stack as the parameter for asyncify_start_unwind
                            // each of them are u64, so together is 8 bytes
                            let unwind_data_start: u64 = unwind_pointer + 8;
                            let unwind_data_end: u64 = stack_pointer as u64;
    
                            // store the parameter at the top of the stack
                            unsafe {
                                *(address as *mut u64) = unwind_data_start;
                                *(address as *mut u64).add(1) = unwind_data_end;
                            }
                            
                            // mark the start of unwind
                            let _res = func.call(&mut caller, unwind_pointer as i32);
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_start_unwind export not found");
            return Ok(-1);
        }

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func;
        let asyncify_start_rewind_func;

        if let Some(asyncify_stop_unwind_extern) = caller.get_export(ASYNCIFY_STOP_UNWIND) {
            match asyncify_stop_unwind_extern {
                Extern::Func(asyncify_stop_unwind) => {
                    match asyncify_stop_unwind.typed::<(), ()>(&caller) {
                        Ok(func) => {
                            asyncify_stop_unwind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_stop_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_stop_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_stop_unwind export not found");
            return Ok(-1);
        }

        if let Some(asyncify_start_rewind_extern) = caller.get_export(ASYNCIFY_START_REWIND) {
            match asyncify_start_rewind_extern {
                Extern::Func(asyncify_start_rewind) => {
                    match asyncify_start_rewind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            asyncify_start_rewind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_rewind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_rewind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_start_rewind export not found");
            return Ok(-1);
        }

        // we want to send this address to child thread
        let cloned_address = address as u64;
        let parent_stack_bottom = caller.as_context().get_stack_base();

        // retrieve the child host
        let mut child_host = caller.data().clone();
        // get next cage id
        let child_cageid = self.pid;

        // use the same engine for parent and child
        let engine = self.module.engine().clone();

        let get_cx = self.get_cx.clone();
        let parent_pid = self.pid;

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
        let is_parent_thread = store.is_thread();
        store.set_on_called(Box::new(move |mut store| {
            let unwind_stack_finish;

            let address = cloned_address as *mut u64;
            let unwind_start_address = (cloned_address + 8) as *mut u64;

            unsafe {
                unwind_stack_finish = *address;
            }

            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            let rewind_base = parent_stack_base;

            let rewind_pointer: u64 = rewind_base;
            let rewind_pointer_child = stack_addr as u64 - stack_size as u64;

            let rewind_start_parent = (cloned_address + rewind_pointer) as *mut u8;
            let rewind_start_child = (cloned_address + rewind_pointer_child) as *mut u8;
            let rewind_total_size = (unwind_stack_finish - rewind_base) as usize;
            // copy the unwind data to child stack
            unsafe { std::ptr::copy_nonoverlapping(rewind_start_parent, rewind_start_child, rewind_total_size); }
            // manage child's unwind context. The unwind context is consumed when the process uses it to rewind the callstack
            // so a seperate copy is needed for child. The unwind context also contains some absolute address that is relative to parent
            // hence we also need to translate it to be relative to child's stack
            unsafe {
                // value used to restore the stack pointer is stored at offset of 0xc (12) from unwind data start
                // let's retrieve it
                let stack_pointer_address = rewind_start_child.add(12) as *mut u32;
                // offset = parent's stack bottom - stored sp (how far is stored sp from parent's stack bottom)
                let offset = parent_stack_bottom as u32 - *stack_pointer_address;
                // child stored sp = child's stack bottom - offset = child's stack bottom - (parent's stack bottom - stored sp)
                // child stored sp = child's stack bottom - parent's stack bottom + stored sp
                // keep child's stored sp same distance from its stack bottom
                let child_sp_val = stack_addr as u32 - offset;
                // replace the stored stack pointer in child's unwind data
                *stack_pointer_address = child_sp_val;

                // first 4 bytes in unwind data represent the address of the end of the unwind data
                // we also need to change this for child
                let child_rewind_data_start = *(rewind_start_child as *mut u32) + rewind_pointer_child as u32;

                *(rewind_start_child as *mut u32) = child_rewind_data_start;
            }

            let builder = thread::Builder::new().name(format!("lind-thread-{}", next_tid));
            builder.spawn(move || {
                // create a new instance
                let store_inner = Store::<T>::new_inner(&engine);

                // get child context
                let child_ctx = get_cx(&mut child_host);
                child_ctx.pid = child_cageid as i32;

                let instance_pre = Arc::new(child_ctx.linker.instantiate_pre(&child_ctx.module).unwrap());

                let mut store = Store::new_with_inner(&engine, child_host, store_inner);

                // mark as thread
                store.set_is_thread(true);

                // instantiate the module
                let instance = instance_pre.instantiate(&mut store).unwrap();

                // we might also want to perserve the offset of current stack pointer to stack bottom
                // not very sure if this is required, but just keep everything the same from parent seems to be good
                let offset = parent_stack_base as i32 - stack_pointer;
                let stack_pointer_setter = instance
                    .get_typed_func::<(i32), ()>(&mut store, "set_stack_pointer")
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
                let _ = child_rewind_start.call(&mut store, rewind_pointer_child as i32);

                // set up rewind state and fork return value for child
                store.as_context_mut().set_rewinding_state(RewindingReturn {
                    rewinding: true,
                    retval: 0,
                });

                // set stack base for child
                store.as_context_mut().set_stack_top(rewind_pointer_child);

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
                    Val::I32(val) => {
                        // technically we need to do some clean up here like cleaning up signal stuff
                        // but signal is still WIP so this is a placeholder for it in the future
                    },
                    _ => {
                        eprintln!("unexpected _start function return type: {:?}", exit_code);
                    }
                }

                return 0;
            }).unwrap();

            // mark the parent to rewind state
            let _ = asyncify_start_rewind_func.call(&mut store, rewind_pointer as i32);

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

        // if the file to exec does not exist
        if !std::path::Path::new(path_str).exists() {
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

        // get the stack pointer global
        let stack_pointer;
        if let Some(sp_extern) = caller.get_export("__stack_pointer") {
            match sp_extern {
                Extern::Global(sp) => {
                    match sp.get(&mut caller) {
                        Val::I32(val) => {
                            stack_pointer = val;
                        }
                        _ => {
                            println!("__stack_pointer export is not an i32");
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("__stack_pointer export is not a Global");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("__stack_pointer export not found");
            return Ok(-1);
        }

        // start unwind
        if let Some(asyncify_start_unwind_extern) = caller.get_export(ASYNCIFY_START_UNWIND) {
            match asyncify_start_unwind_extern {
                Extern::Func(asyncify_start_unwind) => {
                    match asyncify_start_unwind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            let unwind_pointer: u64 = 0;
                            // 8 because we need to store unwind_data_start and unwind_data_end
                            // at the beginning of the unwind stack as the parameter for asyncify_start_unwind
                            // each of them are u64, so together is 8 bytes
                            let unwind_data_start: u64 = unwind_pointer + 8;
                            let unwind_data_end: u64 = stack_pointer as u64;
    
                            unsafe {
                                *(address as *mut u64) = unwind_data_start;
                                *(address as *mut u64).add(1) = unwind_data_end;
                            }
    
                            // mark the state to unwind
                            let _res = func.call(&mut caller, unwind_pointer as i32);
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_start_unwind export not found");
            return Ok(-1);
        }

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func;

        if let Some(asyncify_stop_unwind_extern) = caller.get_export(ASYNCIFY_STOP_UNWIND) {
            match asyncify_stop_unwind_extern {
                Extern::Func(asyncify_stop_unwind) => {
                    match asyncify_stop_unwind.typed::<(), ()>(&caller) {
                        Ok(func) => {
                            asyncify_stop_unwind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_stop_unwind function is not correct: {:?}", err);
                            return Ok(-1);
                        }
                    }
                },
                _ => {
                    println!("asyncify_stop_unwind export is not a function");
                    return Ok(-1);
                }
            }
        }
        else {
            println!("asyncify_stop_unwind export not found");
            return Ok(-1);
        }

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
            let ret = exec_call(&cloned_run_command, path_str, &args, cloned_pid, &cloned_next_cageid, &cloned_lind_manager, &environs);

            return Ok(OnCalledAction::Finish(ret.expect("exec-ed module error")));
        }));

        // after returning from here, unwind process should start
        return Ok(0);
    }

    // exit syscall
    // technically this is pthread_exit syscall
    // actual exit syscall that would kill other threads is not supported yet
    pub fn exit_call(&self, mut caller: &mut Caller<'_, T>, code: i32) {
        // get the base address of the memory
        let handle = caller.as_context().0.instance(InstanceId::from_index(0));
        let defined_memory = handle.get_memory(MemoryIndex::from_u32(0));
        let address = defined_memory.base;

        // get the stack pointer global
        let stack_pointer = caller.get_stack_pointer().unwrap();

        // start unwind
        if let Some(asyncify_start_unwind_extern) = caller.get_export(ASYNCIFY_START_UNWIND) {
            match asyncify_start_unwind_extern {
                Extern::Func(asyncify_start_unwind) => {
                    match asyncify_start_unwind.typed::<i32, ()>(&caller) {
                        Ok(func) => {
                            let unwind_pointer: u64 = 0;
                            // 8 because we need to store unwind_data_start and unwind_data_end
                            // at the beginning of the unwind stack as the parameter for asyncify_start_unwind
                            // each of them are u64, so together is 8 bytes
                            let unwind_data_start: u64 = unwind_pointer + 8;
                            let unwind_data_end: u64 = stack_pointer as u64;
    
                            unsafe {
                                *(address as *mut u64) = unwind_data_start;
                                *(address as *mut u64).add(1) = unwind_data_end;
                            }
    
                            // mark the state to unwind
                            let _res = func.call(&mut caller, unwind_pointer as i32);
                        }
                        Err(err) => {
                            println!("the signature of asyncify_start_unwind function is not correct: {:?}", err);
                            return;
                        }
                    }
                },
                _ => {
                    println!("asyncify_start_unwind export is not a function");
                    return;
                }
            }
        }
        else {
            println!("asyncify_start_unwind export not found");
            return;
        }

        // get the asyncify_stop_unwind and asyncify_start_rewind, which will later
        // be used when the unwind process finished
        let asyncify_stop_unwind_func;

        if let Some(asyncify_stop_unwind_extern) = caller.get_export(ASYNCIFY_STOP_UNWIND) {
            match asyncify_stop_unwind_extern {
                Extern::Func(asyncify_stop_unwind) => {
                    match asyncify_stop_unwind.typed::<(), ()>(&caller) {
                        Ok(func) => {
                            asyncify_stop_unwind_func = func;
                        }
                        Err(err) => {
                            println!("the signature of asyncify_stop_unwind function is not correct: {:?}", err);
                            return;
                        }
                    }
                },
                _ => {
                    println!("asyncify_stop_unwind export is not a function");
                    return;
                }
            }
        }
        else {
            println!("asyncify_stop_unwind export not found");
            return;
        }

        let store = caller.as_context_mut().0;

        store.set_on_called(Box::new(move |mut store| {
            // unwind finished and we need to stop the unwind
            let _res = asyncify_stop_unwind_func.call(&mut store, ());

            // after unwind, just continue returning

            return Ok(OnCalledAction::Finish(vec![Val::I32(code)]));
        }));
        // after returning from here, unwind process should start
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
                    plan.set_minimum((size as u64).div_ceil(m.page_size()));

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
pub fn catch_rewind<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>(caller: &mut Caller<'_, T>) -> Result<i32> {
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    ctx.catch_rewind(caller)
}

// entry point of clone_syscall, called by lind-common
pub fn clone_syscall<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>, args: &mut clone_constants::CloneArgStruct) -> i32
{
    // first let's check if the process is currently in rewind state
    let rewind_res = match catch_rewind(caller) {
        Ok(val) => val,
        Err(_) => -1
    };

    if rewind_res >= 0 { return rewind_res; }

    // get the flags
    let flags = args.flags;
    // if CLONE_VM is set, we are creating a new thread (i.e. pthread_create)
    // otherwise, we are creating a process (i.e. fork)
    let isthread = flags & (clone_constants::CLONE_VM as u64);

    if isthread == 0 {
        match lind_fork(caller) {
            Ok(res) => res,
            Err(e) => -1
        }
    }
    else {
        // pthread_create
        match lind_pthread_create(caller, args.stack as i32, args.stack_size as i32, args.child_tid) {
            Ok(res) => res,
            Err(e) => -1
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
