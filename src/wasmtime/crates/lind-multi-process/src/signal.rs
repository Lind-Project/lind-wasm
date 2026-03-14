use sysdefs::constants::{SIG_DFL, SIG_IGN};
use wasmtime::{AsContext, AsContextMut, AsyncifyState, Caller};

use crate::LindHost;

// handle all the epoch callback
// this is where the wasm instance is directed when epoch is triggered
// this function could possibly be on the callstack of the Asyncify operation
// therefore this function needs to be compatible with Asyncify as well
// If it is not in Asyncify state, then we do the following to handle the epoch callback
// 1. check if epoch is triggered due to `killed` action, if so, clean up and exit via asyncify
// 2. otherwise, retrieve the signal one by one and its handler
// 3. if it is a default handler, we looked up the table and execute the default handler
//    a. in case of termination, we signal all other threads in the cage to `killed` state and exit via asyncify
//    b. in case of ignore, we simply ignore this signal and do not do anything
//    c. in case of stop/continue, this is currently also ignored but would possibly be a TODO to implement in the future
// 4. otherwise if it is a custom handler, just call into glibc's signal handler directly
pub fn signal_handler<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
) -> i32 {
    // Check the killed state FIRST, before looking up signal_callback.
    // When exit_group or a fatal signal calls epoch_kill_all, it writes
    // EPOCH_KILLED to each thread's epoch pointer.  The epoch interrupt
    // fires and brings us here (signal_handler).  We must check
    // thread_check_killed before attempting to deliver a normal signal,
    // because the thread has no signal to deliver — it just needs to
    // exit cleanly via asyncify unwind (exit_call).
    let host = caller.data().clone();
    let ctx = host.get_ctx();
    let cageid = ctx.cageid as u64;

    if cage::signal::thread_check_killed(cageid, ctx.tid as u64) {
        // If asyncify is already unwinding (e.g. exit_call was already
        // triggered by a prior thread-only exit via syscall 60), don't
        // call exit_call again — a double asyncify_start_unwind corrupts
        // the unwind state and causes OOB memory faults.
        if caller.as_context().get_asyncify_state() == AsyncifyState::Unwind {
            return 0;
        }
        // Don't call lind_thread_exit here — it's deferred to exit_call's
        // OnCalledAction so the epoch_handler entry stays until asyncify
        // unwind completes and any grate dispatch has fully returned.
        ctx.exit_call(caller, 0, 0);
        return 0;
    }

    // retrieve glibc's signal callback function, see line #87 in glibc/sysdeps/unix/sysv/linux/i386/libc_sigaction.c for more detail
    let signal_func = caller.get_signal_callback().unwrap();

    // if we are reaching here under Asyncify rewinding process, we need to resume its callstack instead of doing the normal execution
    if let AsyncifyState::Rewind(_) = caller.as_context().get_asyncify_state() {
        // retrieve the signal function entered last time with its parameters.
        // If there's no signal rewind data, we're rewinding from an exit_call
        // (not a signal handler) — just return and let the rewind complete.
        let data = match caller.as_context_mut().get_current_signal_rewind_data() {
            Some(d) => d,
            None => return 0,
        };
        let _ = signal_func.call(caller.as_context_mut(), (data.signal_handler, data.signo));
        return 0;
    }
    // all non-main thread of the cage should not be able to reach the below routine
    // as only main thread is responsible for handling the signals, and the only situation for
    // other non-main thread entered the epoch callback is that they are killed

    // we loop to retrieve pending signals one by one untill there isn't any unblocked pending signals
    loop {
        let signal = cage::signal::lind_get_first_signal(cageid);
        if signal.is_none() {
            break;
        }

        // if this is the last pending (unblocked) signal in list, we should reset epoch
        if cage::signal::lind_check_no_pending_signal(cageid) {
            cage::signal::signal_epoch_reset(cageid);
        }

        let (signo, signal_handler, restorer) = signal.unwrap();
        if signal_handler == SIG_DFL as u32 {
            // default handler
            // look up the signal's default handler
            match sysdefs::constants::signal_default_handler_dispatcher(signo) {
                sysdefs::constants::SignalDefaultHandler::Terminate => {
                    // Set the exit status of the cage to signaled with the signal number and core dump flag
                    // (currently set to false)
                    cage::cage_record_exit_status(cageid, cage::ExitStatus::Signaled(signo, false));
                    // Mark cage as dead so grate-forwarded calls return -ESRCH.
                    if let Some(c) = cage::get_cage(cageid) {
                        c.is_dead.store(true, std::sync::atomic::Ordering::Release);
                    }
                    threei::EXITING_TABLE.insert(cageid);
                    // Mark all other threads for death
                    cage::signal::epoch_kill_all(cageid, ctx.tid as i32);
                    // Prevent new grate dispatches to this cage.
                    threei::handler_table::_rm_grate_from_handler(cageid);
                    // Asyncify unwind; OnCalledAction handles cage_finalize
                    // when the actual last thread finishes.
                    ctx.exit_call(caller, 128 + signo, 0);
                    return 0;
                }
                sysdefs::constants::SignalDefaultHandler::Ignore => {
                    // NOTE: normally this should not be reached since
                    //       ignored signal would not be queued when sending
                    //       let's be aggressive and panic if it reaches
                    unreachable!();
                }
                sysdefs::constants::SignalDefaultHandler::Stop => {
                    // TODO: support STOP signals
                    eprintln!("Warning: STOP signal received but currently not supported!");
                    continue;
                }
                sysdefs::constants::SignalDefaultHandler::Continue => {
                    // TODO: support CONTINUE signals
                    eprintln!("Warning: CONTINUE signal received but currently not supported!");
                    continue;
                }
                sysdefs::constants::SignalDefaultHandler::NONEXIST => {
                    panic!("signal_handler: NONEXIST signal received!");
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
                cage::cage_record_exit_status(cageid, cage::ExitStatus::Exited(1));
                if let Some(c) = cage::get_cage(cageid) {
                    c.is_dead.store(true, std::sync::atomic::Ordering::Release);
                }
                threei::EXITING_TABLE.insert(cageid);
                cage::signal::epoch_kill_all(cageid, ctx.tid as i32);
                threei::handler_table::_rm_grate_from_handler(cageid);
                ctx.exit_call(caller, 1, 0);
                return 0;
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
