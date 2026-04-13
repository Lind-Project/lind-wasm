use sysdefs::constants::lind_platform_const::MAIN_THREADID;
use sysdefs::constants::{SIG_DFL, SIG_IGN};
use wasmtime::{AsContext, AsContextMut, AsyncifyState, Caller, Ref};

use crate::{ChildLibraryType, LindHost};

// Replay dlopen'd modules into the current thread's Wasm store.
//
// When thread A calls dlopen() while threads B/C/D are running, thread A
// appends the library to the shared dlopen list and triggers EPOCH_DLOPEN on
// the other threads.  The next time each other thread enters signal_handler
// (either because EPOCH_DLOPEN was set, or because it was already handling a
// signal or kill), this function catches them up.
//
// Design constraints:
// - We snapshot the list and release the Mutex before calling into Wasm.
// - We use ChildLibraryType::Process (no TLS).  Allocating TLS mid-execution
//   is unsafe; this is a known limitation (see task3_plan.md).
// - The linker clone is discarded after replay.  Linker bookkeeping changes
//   (instance_dylink) are lost, which only affects grandchild creation from
//   this thread — an acceptable pre-existing limitation.
fn handle_dlopen_replay<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    cageid: u64,
    #[allow(unused_variables)] tid: i32,
) {
    // Snapshot the entries to replay and release the lock.
    let entries = caller.data().get_ctx().pending_dlopen_entries();
    if entries.is_empty() {
        return;
    }

    let mut table = match caller.get_function_table() {
        Some(t) => t,
        None => return,
    };

    for (name, _path, module, memory_base) in &entries {
        let dylink_info = match module.dylink_meminfo() {
            Some(d) => d,
            None => continue,
        };

        let table_start = table.size(caller.as_context_mut()) as i32;
        let _ = table.grow(
            caller.as_context_mut(),
            dylink_info.table_size,
            Ref::Func(None),
        );

        // Clone linker — store mutations (new instance, GOT patches) ARE
        // persistent because they go through Caller's AsContextMut.
        // Linker bookkeeping (instance_dylink) is discarded with the clone.
        let mut linker = match caller.data().get_ctx().linker.clone() {
            Some(l) => l,
            None => continue,
        };
        linker.allow_shadowing(true);
        let _ = linker.module_with_child(
            &mut *caller, // reborrow to avoid moving caller in the loop
            cageid,
            name,
            module,
            &mut table,
            table_start,
            *memory_base,
            ChildLibraryType::Process, // no TLS: known limitation for existing threads
            &[],                       // no global snapshots needed for replay
        );
        linker.allow_shadowing(false);
    }

    // Advance this thread's replay cursor.
    caller
        .data_mut()
        .get_ctx_mut()
        .advance_dlopen_replay(entries.len());

    #[cfg(feature = "debug-dylink")]
    println!(
        "[debug] dlopen replay: cage={} tid={} replayed {} entr{}",
        cageid,
        tid,
        entries.len(),
        if entries.len() == 1 { "y" } else { "ies" }
    );
}

// handle all the epoch callback
// this is where the wasm instance is directed when epoch is triggered
// this function could possibly be on the callstack of the Asyncify operation
// therefore this function needs to be compatible with Asyncify as well
// If it is not in Asyncify state, then we do the following to handle the epoch callback
// 1. check if epoch is triggered due to `killed` action, if so, clean up and exit via asyncify
// 2. check if there are pending dlopen modules to replay into this thread's store
// 3. otherwise, retrieve the signal one by one and its handler (main thread only)
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
    let tid = ctx.tid;

    if cage::signal::thread_check_killed(cageid, tid as u64) {
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
        // Retrieve the signal function entered last time with its parameters.
        // None is expected here: exit_call and syscall-level asyncify also
        // trigger epoch rewind, but they don't push signal rewind data.
        // In that case we just return 0 and let the non-signal rewind complete.
        let data = match caller.as_context_mut().get_current_signal_rewind_data() {
            Some(d) => d,
            None => return 0,
        };
        let _ = signal_func.call(caller.as_context_mut(), (data.signal_handler, data.signo));
        return 0;
    }

    // Priority 3: dlopen replay.
    // This is condition-based — fires regardless of whether EPOCH_DLOPEN or
    // EPOCH_SIGNAL triggered this callback.  If a thread was handling a signal
    // when dlopen fired, it will also replay dlopen here.
    if caller.data().get_ctx().has_pending_dlopen_replay() {
        handle_dlopen_replay(caller, cageid, tid);
    }

    // Non-main threads only handle killed and dlopen replay.
    // After replay is done (or if there was nothing to replay), reset the
    // thread's epoch and return.  Only the main thread delivers signals.
    if tid != MAIN_THREADID as i32 {
        if !caller.data().get_ctx().has_pending_dlopen_replay() {
            cage::signal::epoch_thread_reset(cageid, tid);
        }
        return 0;
    }

    // we loop to retrieve pending signals one by one untill there isn't any unblocked pending signals
    loop {
        let signal = cage::signal::lind_get_first_signal(cageid);
        if signal.is_none() {
            break;
        }

        // Reset epoch when this is the last pending signal AND no dlopen replay
        // is outstanding.  Using epoch_thread_reset (not signal_epoch_reset)
        // so each thread manages its own epoch pointer.
        if cage::signal::lind_check_no_pending_signal(cageid)
            && !caller.data().get_ctx().has_pending_dlopen_replay()
        {
            cage::signal::epoch_thread_reset(cageid, tid);
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

    // If we exited the signal loop without triggering an epoch reset inside it
    // (e.g. EPOCH_DLOPEN triggered this callback but there were no signals),
    // reset now if all pending work is done.
    if cage::signal::lind_check_no_pending_signal(cageid)
        && !caller.data().get_ctx().has_pending_dlopen_replay()
    {
        cage::signal::epoch_thread_reset(cageid, tid);
    }

    0
}
