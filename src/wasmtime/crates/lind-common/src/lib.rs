#![allow(dead_code)]

use anyhow::Result;
use rawposix::safeposix::dispatcher::lind_syscall_api;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasmtime::{AsContext, AsContextMut, AsyncifyState, Caller};
use wasmtime_lind_multi_process::{clone_constants::CloneArgStruct, get_memory_base, LindHost};

// lind-common serves as the main entry point when lind_syscall. Any syscalls made in glibc would reach here first,
// then the syscall would be dispatched into rawposix, or other crates under wasmtime, depending on the syscall, to perform its job

#[derive(Clone)]
// stores some attributes associated with current runnning wasm instance (i.e. cage)
// each cage has its own lind-common context
pub struct LindCommonCtx {
    // process id attached to the lind-common context, should be same as cage id
    pid: i32,

    // next cage id, shared between all lind-common context instance (i.e. all cages)
    next_cageid: Arc<AtomicU64>,
}

impl LindCommonCtx {
    // create a new lind-common context, should only be called once for then entire runtime
    pub fn new(next_cageid: Arc<AtomicU64>) -> Result<Self> {
        // cage id starts from 1
        let pid = 1;
        Ok(Self { pid, next_cageid })
    }

    // create a new lind-common context with pid provided, used by exec syscall
    pub fn new_with_pid(pid: i32, next_cageid: Arc<AtomicU64>) -> Result<Self> {
        Ok(Self { pid, next_cageid })
    }

    // entry point for lind_syscall in glibc, dispatching syscalls to rawposix or wasmtime
    pub fn lind_syscall<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        call_number: u32,
        call_name: u64,
        mut caller: &mut Caller<'_, T>,
        arg1: u64,
        arg2: u64,
        arg3: u64,
        arg4: u64,
        arg5: u64,
        arg6: u64,
    ) -> i32 {
        let start_address = get_memory_base(&caller);
        match call_number as i32 {
            // clone syscall
            171 => {
                let clone_args = unsafe { &mut *((arg1 + start_address) as *mut CloneArgStruct) };
                clone_args.child_tid += start_address;
                wasmtime_lind_multi_process::clone_syscall(caller, clone_args)
            }
            // exec syscall
            69 => wasmtime_lind_multi_process::exec_syscall(
                caller,
                arg1 as i64,
                arg2 as i64,
                arg3 as i64,
            ),
            // exit syscall
            30 => wasmtime_lind_multi_process::exit_syscall(caller, arg1 as i32),
            // other syscalls goes into rawposix
            _ => {
                // if we are reaching here at rewind state, that means fork is called within
                // syscall interrupted signals. We should restore the return value of syscall
                if let AsyncifyState::Rewind(_) = caller.as_context().get_asyncify_state() {
                    // retrieve the return value of last syscall
                    let retval = caller
                        .as_context_mut()
                        .get_current_syscall_rewind_data()
                        .unwrap();
                    // let signal handler finish rest of the rewinding process
                    wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
                    // return the return value of last syscall
                    return retval;
                }

                let retval = lind_syscall_api(
                    self.pid as u64,
                    call_number,
                    call_name,
                    arg1,
                    arg2,
                    arg3,
                    arg4,
                    arg5,
                    arg6,
                );

                // Assumption: lind_syscall_api will not switch asyncify state, which holds true for now
                
                // if the syscall is interrupted by signal
                if -retval == sysdefs::constants::Errno::EINTR as i32 {
                    // store the return value of the syscall
                    caller.as_context_mut().append_syscall_asyncify_data(retval);
                    // run the signal handler
                    wasmtime_lind_multi_process::signal::signal_handler(&mut caller);

                    // if fork is invoked within signal handler and switched asyncify state to unwind
                    if caller.as_context().get_asyncify_state() == AsyncifyState::Unwind {
                        // return immediately
                        return 0;
                    } else {
                        // otherwise, pop the retval of the syscall
                        caller.as_context_mut().pop_syscall_asyncify_data();
                    }
                }

                retval
            }
        }
    }

    // setjmp call. This function needs to be handled within wasmtime, but it is not an actual syscall so we use a different routine from lind_syscall
    pub fn lind_setjmp<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        caller: &mut Caller<'_, T>,
        jmp_buf: u32,
    ) -> i32 {
        wasmtime_lind_multi_process::setjmp_call(caller, jmp_buf)
    }

    // longjmp call. This function needs to be handled within wasmtime, but it is not an actual syscall so we use a different routine from lind_syscall
    pub fn lind_longjmp<
        T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
        U: Clone + Send + 'static + std::marker::Sync,
    >(
        &self,
        caller: &mut Caller<'_, T>,
        jmp_buf: u32,
        retval: i32,
    ) -> i32 {
        wasmtime_lind_multi_process::longjmp_call(caller, jmp_buf, retval)
    }

    // get current process id/cageid
    // currently unused interface but may be useful in the future
    pub fn getpid(&self) -> i32 {
        self.pid
    }

    // return the next avaliable cageid (cageid increment sequentially)
    fn next_cage_id(&self) -> Option<u64> {
        match self
            .next_cageid
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| match v {
                ..=0x1ffffffe => Some(v + 1),
                _ => None,
            }) {
            Ok(v) => Some(v + 1),
            Err(_) => None,
        }
    }

    // fork a new lind-common context, used by clone syscall
    pub fn fork(&self) -> Self {
        // cageid is automatically incremented here
        let next_pid = self.next_cage_id().unwrap();

        let forked_ctx = Self {
            pid: next_pid as i32,
            next_cageid: self.next_cageid.clone(),
        };

        return forked_ctx;
    }
}

// function to expose the handler to wasm module
// linker: wasmtime's linker to link the imported function to the actual function definition
// get_cx: function to retrieve LindCommonCtx from caller
pub fn add_to_linker<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    linker: &mut wasmtime::Linker<T>,
    get_cx: impl Fn(&T) -> &LindCommonCtx + Send + Sync + Copy + 'static,
) -> anyhow::Result<()> {
    // attach lind_syscall to wasmtime
    linker.func_wrap(
        "lind",
        "lind-syscall",
        move |mut caller: Caller<'_, T>,
              call_number: u32,
              call_name: u64,
              arg1: u64,
              arg2: u64,
              arg3: u64,
              arg4: u64,
              arg5: u64,
              arg6: u64|
              -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            let retval = ctx.lind_syscall(
                call_number,
                call_name,
                &mut caller,
                arg1,
                arg2,
                arg3,
                arg4,
                arg5,
                arg6,
            );

            retval
        },
    )?;

    // attach setjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-setjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32| -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            ctx.lind_setjmp(&mut caller, jmp_buf as u32)
        },
    )?;

    // attach longjmp to wasmtime
    linker.func_wrap(
        "lind",
        "lind-longjmp",
        move |mut caller: Caller<'_, T>, jmp_buf: i32, retval: i32| -> i32 {
            let host = caller.data().clone();
            let ctx = get_cx(&host);

            ctx.lind_longjmp(&mut caller, jmp_buf as u32, retval)
        },
    )?;

    // epoch callback function
    linker.func_wrap(
        "wasi_snapshot_preview1",
        "epoch_callback",
        move |mut caller: Caller<'_, T>| {
            wasmtime_lind_multi_process::signal::signal_handler(&mut caller);
        },
    )?;

    // a temporary solution to have libc_assert_fail correctly working
    linker.func_wrap(
        "debug",
        "libc_assert_fail",
        move |mut caller: Caller<'_, T>, assertion: i32, file: i32, line: i32, function: i32| {
            let mem_base = get_memory_base(&caller);
            let assertion = rawposix::interface::get_cstr(mem_base + assertion as u64).unwrap();
            let file = rawposix::interface::get_cstr(mem_base + file as u64).unwrap();
            let function = rawposix::interface::get_cstr(mem_base + function as u64).unwrap();
            eprintln!(
                "Fatal glibc error: {}:{} ({}): assertion failed: {}\n",
                assertion, file, line, function
            );
        },
    )?;

    // a temporary solution to have malloc_printerr correctly working
    linker.func_wrap(
        "debug",
        "malloc_printerr",
        move |mut caller: Caller<'_, T>, msg: i32| {
            let mem_base = get_memory_base(&caller);
            let msg = rawposix::interface::get_cstr(mem_base + msg as u64).unwrap();
            eprintln!("malloc_printerr: {}", msg);
        },
    )?;

    Ok(())
}
