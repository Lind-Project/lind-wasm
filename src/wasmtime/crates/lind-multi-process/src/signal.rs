use rawposix::{interface::signal_epoch_reset, safeposix::dispatcher::{lind_check_no_pending_signal, lindgetfirstsignal}};
use wasmtime::{raise_trap, AsContext, AsContextMut, AsyncifyState, Caller, Trap};

use crate::LindHost;

enum SignalDefaultHandler {
    Terminate,
    Ignore,
    Stop,
    Continue,
}

pub fn signal_handler<T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync, U: Clone + Send + 'static + std::marker::Sync>
        (caller: &mut Caller<'_, T>) -> i32 {
    let signal_func = caller.get_signal_callback().unwrap();

    if caller.as_context().get_rewinding_state().rewinding == AsyncifyState::Rewind {
        // let manager = get_signal_asyncify_manager().lock().unwrap();
        let data = caller.as_context_mut().get_current_signal_rewind_data().unwrap();
        signal_func.call(caller.as_context_mut(), (data.signal_handler, data.signo));
        return 0;
    }

    let host = caller.data().clone();
    let ctx = host.get_ctx();

    if rawposix::interface::thread_check_killed(ctx.pid as u64, ctx.tid as u64) {
        // we check if the thread is supposed to be killed first
        thread_suicide();
    }
    
    loop {
        let signal = lindgetfirstsignal(ctx.pid as u64);
        if signal.is_none() {
            break;
        }

        // if there is no pending (unblocked) signal in list, we can reset epoch
        // since any new signals (from kill) or switching of blocked signal to unblocked signal (from sigprocmask)
        // should incremenet their epoch
        if lind_check_no_pending_signal(ctx.pid as u64) {
            // println!("reset epoch");
            signal_epoch_reset(ctx.pid as u64);
        }

        let (signo, signal_handler, restorer) = signal.unwrap();
        // let signal_handler = lindgetsighandler(ctx.pid as u64, signo);
        if signal_handler == 0 { // default handler
            match signal_default_handler_dispatcher(signo) {
                SignalDefaultHandler::Terminate => {
                    rawposix::interface::signal_epoch_trigger_all(ctx.pid as u64);
                    thread_suicide();
                },
                SignalDefaultHandler::Ignore => {
                    continue;
                },
                SignalDefaultHandler::Stop => {
                    // TODO: support STOP signals
                    eprintln!("Warning: STOP signal received but currently not supported!");
                    continue;
                },
                SignalDefaultHandler::Continue => {
                    // TODO: support CONTINUE signals
                    eprintln!("Warning: CONTINUE signal received but currently not supported!");
                    continue;
                }
            }
        } else if signal_handler == 1 { // ignore
            println!("------ignore {}------", signo);
            continue;
        } else {
            // let mut manager = get_signal_asyncify_manager().lock().unwrap();
            // manager.set(signal_handler as i32, signo);
            caller.as_context_mut().append_signal_asyncify_data(signal_handler as i32, signo);
            // drop(manager);
            let _res = signal_func.call(caller.as_context_mut(), (signal_handler as i32, signo));
            if caller.as_context().get_rewinding_state().rewinding == AsyncifyState::Unwind {
                return 0;
            } else {
                // restore signal mask
                restorer(ctx.pid as u64);
                caller.as_context_mut().pop_signal_asyncify_data(signal_handler as i32, signo);
            }
        }
    }
    
    0
}

pub fn thread_suicide() -> ! {
    let err = Trap::Interrupt;
    unsafe {
        raise_trap(err.into());
    }
    unreachable!();
}

fn signal_default_handler_dispatcher(signo: i32) -> SignalDefaultHandler {
    match signo {
        rawposix::constants::SIGHUP => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGINT => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGQUIT => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGILL => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGTRAP => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGABRT => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGBUS => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGFPE => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGKILL => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGUSR1 => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGSEGV => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGUSR2 => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGPIPE => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGALRM => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGTERM => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGSTKFLT => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGCHLD => SignalDefaultHandler::Ignore,
        rawposix::constants::SIGCONT => SignalDefaultHandler::Continue,
        rawposix::constants::SIGSTOP => SignalDefaultHandler::Stop,
        rawposix::constants::SIGTSTP => SignalDefaultHandler::Stop,
        rawposix::constants::SIGTTIN => SignalDefaultHandler::Stop,
        rawposix::constants::SIGTTOU => SignalDefaultHandler::Stop,
        rawposix::constants::SIGURG => SignalDefaultHandler::Ignore,
        rawposix::constants::SIGXCPU => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGXFSZ => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGVTALRM => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGPROF => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGWINCH => SignalDefaultHandler::Ignore,
        rawposix::constants::SIGIO => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGPWR => SignalDefaultHandler::Terminate,
        rawposix::constants::SIGSYS => SignalDefaultHandler::Terminate,
        _ => panic!("invalid signal number!")
    }
}
