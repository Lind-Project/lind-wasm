use cage::memory::vmmap::Vmmap;
use cage::timer::IntervalTimer;
use cage::{Cage, Condvar, Mutex, RwLock};
use dashmap::DashMap;
use lind_core::fork::{abort_unstarted_fork, prepare_fork};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::err_const::Errno;

#[test]
fn fork_preparation_commits_or_rolls_back_resources() {
    let parent_id = 1;
    let parent = Cage {
        cageid: parent_id,
        parent: parent_id,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(parent_id),
        epoch_handler: DashMap::new(),
        os_tid_map: DashMap::new(),
        pending_signals: RwLock::new(Vec::new()),
        signalhandler: DashMap::new(),
        sigset: AtomicU64::new(0),
        zombies: Mutex::new(Vec::new()),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(Vmmap::new()),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
    };
    cage::add_cage(parent_id, parent);
    fdtables::init_empty_cage(parent_id);
    threei::handler_table::register_handler_impl(parent_id, 42, 17, 123);
    threei::set_cage_runtime(parent_id, 7);
    let parent = cage::get_cage(parent_id).unwrap();
    parent.sigset.store(0x10, Ordering::Relaxed);

    let pending = prepare_fork(parent_id).unwrap();
    let child_id = pending.cageid();
    assert_eq!(cage::get_cage(child_id).unwrap().parent, parent_id);
    assert_eq!(
        cage::get_cage(child_id)
            .unwrap()
            .sigset
            .load(Ordering::Relaxed),
        0x10
    );
    assert!(fdtables::check_cage_exists(child_id));
    assert_eq!(threei::get_cage_runtime(child_id), Some(7));
    assert_eq!(
        threei::handler_table::_get_handler(child_id, 42, 17),
        Some((17, 123))
    );
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 1);
    drop(pending);

    assert!(cage::get_cage(child_id).is_none());
    assert!(!fdtables::check_cage_exists(child_id));
    assert_eq!(threei::get_cage_runtime(child_id), None);
    assert!(!threei::handler_table::HANDLERTABLE
        .lock()
        .unwrap()
        .contains_key(&child_id));
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 0);
    assert!(parent.zombies.lock().is_empty());

    threei::handler_table::_rm_cage_from_handler(parent_id);
    assert_eq!(prepare_fork(parent_id).err(), Some(-(Errno::EIO as i64)));
    assert!(!fdtables::check_cage_exists(child_id + 1));
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 0);

    threei::handler_table::register_handler_impl(parent_id, 42, 17, 123);
    let committed = prepare_fork(parent_id).unwrap();
    let committed_id = committed.cageid();
    committed.commit();
    assert!(cage::get_cage(committed_id).is_some());
    assert!(fdtables::check_cage_exists(committed_id));
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 1);
    abort_unstarted_fork(parent_id, committed_id).unwrap();
    assert!(cage::get_cage(committed_id).is_none());
    assert!(!fdtables::check_cage_exists(committed_id));
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 0);

    let completed = prepare_fork(parent_id).unwrap();
    let completed_id = completed.cageid();
    completed.commit();
    lind_core::finalize_cage(completed_id);
    assert!(cage::get_cage(completed_id).is_none());
    assert!(!fdtables::check_cage_exists(completed_id));
    assert_eq!(threei::get_cage_runtime(completed_id), None);
    assert!(!threei::handler_table::HANDLERTABLE
        .lock()
        .unwrap()
        .contains_key(&completed_id));
    assert_eq!(parent.child_num.load(Ordering::SeqCst), 0);
    assert_eq!(parent.zombies.lock().pop().unwrap().cageid, completed_id);

    let orphan = prepare_fork(parent_id).unwrap();
    let orphan_id = orphan.cageid();
    orphan.commit();
    cage::remove_cage(parent_id);
    abort_unstarted_fork(parent_id, orphan_id).unwrap();
    assert!(cage::get_cage(orphan_id).is_none());
    assert!(!fdtables::check_cage_exists(orphan_id));
    assert_eq!(threei::get_cage_runtime(orphan_id), None);

    threei::handler_table::_rm_cage_from_handler(parent_id);
    threei::remove_cage_runtime(parent_id);
    fdtables::remove_cage_from_fdtable(parent_id);
}
