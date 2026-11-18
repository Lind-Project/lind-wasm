use cage::timer::IntervalTimer;
use cage::{Cage, Condvar, Mutex, RwLock};
use dashmap::DashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{UNUSED_ARG, UNUSED_ID};

/// A child cage prepared for a runtime fork. Dropping it before commit rolls
/// back the Core-owned resources without creating a zombie.
pub struct PendingFork {
    parent: Arc<Cage>,
    child_cageid: u64,
    committed: bool,
}

impl PendingFork {
    pub fn cageid(&self) -> u64 {
        self.child_cageid
    }

    pub fn commit(mut self) {
        self.committed = true;
    }
}

impl Drop for PendingFork {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        cleanup_unstarted_cage(self.child_cageid);

        let _zombies = self.parent.zombies.lock();
        self.parent.child_num.fetch_sub(1, Ordering::SeqCst);
        self.parent.wait_cond.notify_all();
    }
}

fn cleanup_unstarted_cage(cageid: u64) {
    threei::handler_table::_rm_cage_from_handler(cageid);
    threei::remove_cage_runtime(cageid);
    fdtables::remove_cage_from_fdtable(cageid);
    cage::unmap_shm_mappings(cageid);
    cage::remove_cage(cageid);
}

/// Prepare the fork's cage metadata, descriptor table, and 3i routing before
/// entering the runtime. Returns a negative Linux errno on failure.
pub fn prepare_fork(parent_cageid: u64) -> Result<PendingFork, i64> {
    let parent = cage::get_cage(parent_cageid).ok_or(-(Errno::ESRCH as i64))?;
    if parent.is_dead.load(Ordering::Acquire) || !fdtables::check_cage_exists(parent_cageid) {
        return Err(-(Errno::ESRCH as i64));
    }
    let runtime = threei::get_cage_runtime(parent_cageid).ok_or(-(Errno::ESRCH as i64))?;
    let child_cageid = cage::alloc_cage_id().ok_or(-(Errno::EAGAIN as i64))?;

    if let Err(errno) = fdtables::copy_fdtable_for_cage(parent_cageid, child_cageid) {
        fdtables::remove_cage_from_fdtable(child_cageid);
        return Err(-(errno as i64));
    }

    let routing_result = threei::copy_handler_table_to_cage(
        UNUSED_ARG,
        UNUSED_ARG,
        parent_cageid,
        child_cageid,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );
    if routing_result != 0 {
        threei::handler_table::_rm_cage_from_handler(child_cageid);
        fdtables::remove_cage_from_fdtable(child_cageid);
        eprintln!("failed to copy fork routing for cage {parent_cageid}: {routing_result:#x}");
        let errno = if routing_result == threei::ELINDESRCH {
            Errno::ESRCH
        } else {
            Errno::EIO
        };
        return Err(-(errno as i64));
    }

    let child = Cage {
        cageid: child_cageid,
        cwd: RwLock::new(parent.cwd.read().clone()),
        parent: parent_cageid,
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(child_cageid),
        epoch_handler: DashMap::new(),
        os_tid_map: DashMap::new(),
        pending_signals: RwLock::new(vec![]),
        signalhandler: parent.signalhandler.clone(),
        sigset: AtomicU64::new(parent.sigset.load(Ordering::Relaxed)),
        zombies: Mutex::new(vec![]),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(parent.vmmap.read().clone()),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
    };

    threei::set_cage_runtime(child_cageid, runtime);
    {
        let _zombies = parent.zombies.lock();
        parent.child_num.fetch_add(1, Ordering::SeqCst);
        cage::add_cage(child_cageid, child);
    }
    Ok(PendingFork {
        parent,
        child_cageid,
        committed: false,
    })
}

/// Roll back a deferred runtime setup that failed before the child could run.
pub fn abort_unstarted_fork(parent_cageid: u64, child_cageid: u64) -> Result<(), i64> {
    let child = cage::get_cage(child_cageid).ok_or(-(Errno::ESRCH as i64))?;
    if child.parent != parent_cageid {
        return Err(-(Errno::EINVAL as i64));
    }

    if let Some(parent) = cage::get_cage(parent_cageid) {
        drop(PendingFork {
            parent,
            child_cageid,
            committed: false,
        });
    } else {
        cleanup_unstarted_cage(child_cageid);
    }
    Ok(())
}
