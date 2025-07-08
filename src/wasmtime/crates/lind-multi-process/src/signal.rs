use sysdefs::constants::{SIG_DFL, SIG_IGN};
use wasmtime::{raise_trap, AsContext, AsContextMut, AsyncifyState, Caller, Trap};

use crate::LindHost;
use cage::signal::{thread_check_killed, lind_get_first_signal, lind_check_no_pending_signal, signal_epoch_reset, epoch_kill_all, lind_signal_init, lind_thread_exit};

// default signal handler actions
enum SignalDefaultHandler {
    Terminate, // terminate the process
    Ignore,    // ignore the signal
    Stop,      // stop the current process
    Continue,  // resume the stopped process
}

// handle all the epoch callback
// this is where the wasm instance is directed when epoch is triggered
// this function could possibly be on the callstack of the Asyncify operation
// therefore this function needs to be compatible with Asyncify as well
// If it is not in Asyncify state, then we do the following to handle the epoch callback
// 1. check if epoch is triggered due to `killed` action, if it is, perform a suicide
// 2. otherwise, retrieve the signal one by one and its handler
// 3. if it is a default handler, we looked up the table and execute the default handler
//    a. in case of termination, we signal all other threads in the cage to `killed` state and perform a suicide
//    b. in case of ignore, we simply ignore this signal and do not do anything
//    c. in case of stop/continue, this is currently also ignored but would possibly be a TODO to implement in the future
// 4. otherwise if it is a custom handler, just call into glibc's signal handler directly
pub fn signal_handler<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
) -> i32 {
    // retrieve glibc's signal callback function, see line #87 in glibc/sysdeps/unix/sysv/linux/i386/libc_sigaction.c for more detail
    let signal_func = caller.get_signal_callback().unwrap();

    // if we are reaching here under Asyncify rewinding process, we need to resume its callstack instead of doing the normal execution
    if let AsyncifyState::Rewind(_) = caller.as_context().get_asyncify_state() {
        // retrieve the signal function entered last time with its parameters
        let data = caller
            .as_context_mut()
            .get_current_signal_rewind_data()
            .unwrap();
        let _ = signal_func.call(caller.as_context_mut(), (data.signal_handler, data.signo));
        return 0;
    }
    // otherwise, we are in normal execution and we should handle signals appropriately

    let host = caller.data().clone();
    let ctx = host.get_ctx();
    let cageid = ctx.pid as u64;

    // first let's check if the epoch state is in "killed" state
    if thread_check_killed(cageid, ctx.tid as u64) {
        // if we are already killed, then perform a suicide
        thread_suicide();
    }
    // all non-main thread of the cage should not be able to reach the below routine
    // as only main thread is responsible for handling the signals, and the only situation for
    // other non-main thread entered the epoch callback is that they are killed

    // we loop to retrieve pending signals one by one untill there isn't any unblocked pending signals
    loop {
        let signal = lind_get_first_signal(cageid);
        if signal.is_none() {
            break;
        }

        // if this is the last pending (unblocked) signal in list, we should reset epoch
        if lind_check_no_pending_signal(cageid) {
            signal_epoch_reset(cageid);
        }

        let (signo, signal_handler, restorer) = signal.unwrap();
        if signal_handler == SIG_DFL as u32 {
            // default handler
            // look up the signal's default handler
            match signal_default_handler_dispatcher(signo) {
                SignalDefaultHandler::Terminate => {
                    // if we are supposed to be terminated, switch the epoch state of all other threads
                    // to "killed" state and perform a suicide
                    epoch_kill_all(cageid);
                    thread_suicide();
                }
                SignalDefaultHandler::Ignore => {
                    continue;
                }
                SignalDefaultHandler::Stop => {
                    // TODO: support STOP signals
                    eprintln!("Warning: STOP signal received but currently not supported!");
                    continue;
                }
                SignalDefaultHandler::Continue => {
                    // TODO: support CONTINUE signals
                    eprintln!("Warning: CONTINUE signal received but currently not supported!");
                    continue;
                }
            }
        } else if signal_handler == SIG_IGN as u32 {
            // ignore the signal
            continue;
        } else {
            // we should invoke user's custom signal handler

            // before invoke the function, let's record the signal callstack information in case user performed
            // any Asyncify-related operation in signal handler
            caller
                .as_context_mut()
                .append_signal_asyncify_data(signal_handler as i32, signo);
            // invoke the
            let invoke_res =
                signal_func.call(caller.as_context_mut(), (signal_handler as i32, signo));
            // print errors if any when running the signal handler
            if let Err(err) = invoke_res {
                let e = wasi_common::maybe_exit_on_error(err);
                eprintln!("Error: {:?}", e);
                // if we encountered any error when executing the signal handler, we should terminate the cage
                epoch_kill_all(cageid);
                thread_suicide();
            }

            // first let's check if the signal handler returns due to Asyncify Unwind operation
            if caller.as_context().get_asyncify_state() == AsyncifyState::Unwind {
                // if it is, then return immediately
                return 0;
            } else {
                // otherwise, the signal handler returns normally

                // restore signal mask
                restorer(cageid);
                // clean up the signal callstack information for Asyncify
                caller
                    .as_context_mut()
                    .pop_signal_asyncify_data(signal_handler as i32, signo);
            }
        }
    }

    0
}

// raise a trap to the current thread
// this is paired with catch_traps function in /crates/wasmtime/src/runtime/vm/traphandlers.rs
// which will catch the trap raised here and perform the clean up
pub fn thread_suicide() -> ! {
    // we raise Trap::Interrupt instead of other trap type
    // because this is the trap type used by wasmtime's built-in epoch
    // and epoch is the only possible source of this type of trap
    let err = Trap::Interrupt;
    unsafe {
        raise_trap(err.into());
    }
    unreachable!();
}

// maps each signal to its default handler
// see https://man7.org/linux/man-pages/man7/signal.7.html for more information
fn signal_default_handler_dispatcher(signo: i32) -> SignalDefaultHandler {
    match signo {
        sysdefs::constants::SIGHUP => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGINT => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGQUIT => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGILL => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGTRAP => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGABRT => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGBUS => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGFPE => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGKILL => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGUSR1 => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGSEGV => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGUSR2 => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGPIPE => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGALRM => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGTERM => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGSTKFLT => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGCHLD => SignalDefaultHandler::Ignore,
        sysdefs::constants::SIGCONT => SignalDefaultHandler::Continue,
        sysdefs::constants::SIGSTOP => SignalDefaultHandler::Stop,
        sysdefs::constants::SIGTSTP => SignalDefaultHandler::Stop,
        sysdefs::constants::SIGTTIN => SignalDefaultHandler::Stop,
        sysdefs::constants::SIGTTOU => SignalDefaultHandler::Stop,
        sysdefs::constants::SIGURG => SignalDefaultHandler::Ignore,
        sysdefs::constants::SIGXCPU => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGXFSZ => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGVTALRM => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGPROF => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGWINCH => SignalDefaultHandler::Ignore,
        sysdefs::constants::SIGIO => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGPWR => SignalDefaultHandler::Terminate,
        sysdefs::constants::SIGSYS => SignalDefaultHandler::Terminate,
        _ => panic!("invalid signal number!"),
    }
}
