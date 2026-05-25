use sysdefs::constants::lind_platform_const::MAIN_THREADID;
use sysdefs::constants::{SIG_DFL, SIG_IGN};
use wasmtime::{AsContext, AsContextMut, AsyncifyState, Caller, ChildLibraryType, Ref};

use crate::LindHost;

// Replay any dlopen'd modules that the current thread has not yet instantiated.
//
// When thread A calls dlopen() while threads B/C are running, A appends the
// library to the shared dlopen_modules list and fires EPOCH_DLOPEN on B and C.
// The next time B/C enter signal_handler (either via EPOCH_DLOPEN or an
// already-pending EPOCH_SIGNAL), this function catches them up.
//
// Design constraints:
// - Snapshot the list and release the Mutex before calling into Wasm.
// - ChildLibraryType::Process is used (no TLS allocation — known limitation).
// - The cloned linker's bookkeeping changes are discarded after replay; actual
//   store mutations (GOT patches, new instances) are persistent.
fn handle_dlopen_replay<
    T: LindHost<T, U> + Clone + Send + 'static + std::marker::Sync,
    U: Clone + Send + 'static + std::marker::Sync,
>(
    caller: &mut Caller<'_, T>,
    cageid: u64,
) {
    let entries = caller.data().get_ctx().pending_dlopen_entries();
    if entries.is_empty() {
        return;
    }

    let mut table = match caller.get_function_table() {
        Some(t) => t,
        None => return,
    };

    let got_arc = caller.data().get_ctx().got_table.clone();

    for (name, _path, module, memory_base, symbol_map) in &entries {
        let dylink_info = match module.dylink_meminfo() {
            Some(d) => d,
            None => continue,
        };

        // Mirror the main-thread dlopen path (load_library_module): record the
        // table size before the pre-grow, then grow by dylink_info.table_size.
        // module_with_child → apply_GOT_relocs then appends one slot per
        // exported function, producing the same absolute indices that the
        // main-thread's grow_table_lib calls used.  The GOT globals in this
        // thread are pre-filled by LindGOT::new_entry from the symbol_cache,
        // so they point to the correct indices without needing a GOT update here.
        let table_start = table.size(caller.as_context_mut()) as i32;
        let _ = table.grow(
            caller.as_context_mut(),
            dylink_info.table_size as u64,
            Ref::Func(None),
        );

        let mut linker = match caller.data().get_ctx().linker.clone() {
            Some(l) => l,
            None => continue,
        };
        linker.allow_shadowing(true);

        if let Some(ref got_arc) = got_arc {
            let mut got_guard = got_arc.lock().unwrap();
            let _ = linker.define_GOT_dispatcher(&mut *caller, module, &mut *got_guard);
        }

        let _ = linker.module_with_child(
            &mut *caller,
            cageid,
            name,
            module,
            &mut table,
            table_start,
            *memory_base,
            ChildLibraryType::Process,
            &[],
        );
        linker.allow_shadowing(false);

        // Register the library's symbols so dlsym works in this thread.
        let _ = caller.push_library_symbols(symbol_map.clone());
    }

    caller
        .data_mut()
        .get_ctx_mut()
        .advance_dlopen_replay(entries.len());
}

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
        //
        // Use the cage's recorded exit status so that exit_group(N) from any
        // thread (e.g. faulthandler calling _exit(1)) propagates the right
        // code to the OS-level process exit.  Default to 0 when the cage is
        // already gone (late wakeup after cage_finalize).
        let cage_opt = cage::get_cage(cageid);
        let status_opt = cage_opt.as_ref().and_then(|c| *c.final_exit_status.read());
        let exit_code = status_opt
            .map(|st| match st {
                cage::ExitStatus::Exited(code) => code,
                cage::ExitStatus::Signaled(_, _) => 1,
            })
            .unwrap_or(0);
        ctx.exit_call(caller, exit_code, 0);
        return 0;
    }

    // Priority: dlopen replay.
    // Fires regardless of whether EPOCH_DLOPEN or EPOCH_SIGNAL triggered this
    // callback.  If a thread was already handling a signal when dlopen fired,
    // both the replay and the signal are handled in this single callback.
    if caller.data().get_ctx().has_pending_dlopen_replay() {
        handle_dlopen_replay(caller, cageid);
    }

    // Non-main threads only handle killed (above) and dlopen replay.
    // After replay, reset the thread's epoch and return.
    if tid != MAIN_THREADID as i32 {
        if !caller.data().get_ctx().has_pending_dlopen_replay() {
            cage::signal::epoch_thread_reset(cageid, tid);
        }
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
    // all non-main thread of the cage should not be able to reach the below routine
    // as only main thread is responsible for handling the signals, and the only situation for
    // other non-main thread entered the epoch callback is that they are killed

    // we loop to retrieve pending signals one by one untill there isn't any unblocked pending signals
    loop {
        let signal = cage::signal::lind_get_first_signal(cageid);
        if signal.is_none() {
            break;
        }

        // Reset epoch when this is the last pending signal AND no dlopen replay is outstanding.
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
                eprintln!("Error: {:?}", err);
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

    // If the main thread had only a dlopen replay (no signals), reset the epoch here.
    if cage::signal::lind_check_no_pending_signal(cageid)
        && !caller.data().get_ctx().has_pending_dlopen_replay()
    {
        cage::signal::epoch_thread_reset(cageid, tid);
    }

    0
}
