use cage::memory::vmmap::{MemoryBackingType, Vmmap, VmmapOps};
use cage::timer::IntervalTimer;
use cage::{Cage, Condvar, DashMap, Mutex, RwLock};
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;

fn test_cage(cageid: u64, base: usize) -> Cage {
    let mut vmmap = Vmmap::new();
    vmmap.set_base_address(base);
    vmmap
        .add_entry_with_overwrite(
            0,
            1,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            MemoryBackingType::Anonymous,
            0,
            4096,
            cageid,
        )
        .unwrap();
    Cage {
        cageid,
        parent: cageid,
        cwd: RwLock::new(Arc::new(PathBuf::from("/"))),
        rev_shm: Mutex::new(Vec::new()),
        main_threadid: RwLock::new(0),
        interval_timer: IntervalTimer::new(cageid),
        epoch_handler: DashMap::new(),
        os_tid_map: DashMap::new(),
        pending_signals: RwLock::new(Vec::new()),
        signalhandler: DashMap::new(),
        sigset: AtomicU64::new(0),
        zombies: Mutex::new(Vec::new()),
        wait_cond: Condvar::new(),
        child_num: AtomicU64::new(0),
        vmmap: RwLock::new(vmmap),
        final_exit_status: RwLock::new(None),
        exit_group_initiated: AtomicBool::new(false),
        is_dead: AtomicBool::new(false),
        grate_inflight: AtomicU64::new(0),
    }
}

#[test]
fn fork_memory_copy_reports_inaccessible_source_without_crashing() {
    unsafe {
        let parent = libc::mmap(
            std::ptr::null_mut(),
            8192,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        let child = libc::mmap(
            std::ptr::null_mut(),
            8192,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        );
        assert_ne!(parent, libc::MAP_FAILED);
        assert_ne!(child, libc::MAP_FAILED);
        std::ptr::write_bytes(parent.cast::<u8>(), 0x5a, 8192);

        cage::add_cage(61, test_cage(61, parent as usize));
        cage::add_cage(62, test_cage(62, child as usize));
        // The parent can change its mappings after the child record is prepared.
        cage::get_cage(61)
            .unwrap()
            .vmmap
            .write()
            .add_entry_with_overwrite(
                1,
                1,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                MemoryBackingType::Anonymous,
                0,
                4096,
                61,
            )
            .unwrap();
        assert!(cage::fork_vmmap(61, 61).is_err());
        cage::fork_vmmap(61, 62).unwrap();
        assert!(cage::get_cage(62)
            .unwrap()
            .vmmap
            .read()
            .find_page(1)
            .is_some());
        assert!(std::slice::from_raw_parts(child.cast::<u8>(), 8192)
            .iter()
            .all(|byte| *byte == 0x5a));

        cage::get_cage(61)
            .unwrap()
            .vmmap
            .write()
            .change_prot(0, 1, libc::PROT_NONE);
        assert_eq!(libc::mprotect(parent, 4096, libc::PROT_NONE), 0);
        cage::fork_vmmap(61, 62).unwrap();
        assert_eq!(
            cage::get_cage(62)
                .unwrap()
                .vmmap
                .read()
                .find_page(0)
                .unwrap()
                .prot,
            libc::PROT_NONE
        );
        assert_eq!(
            libc::mprotect(child, 4096, libc::PROT_READ | libc::PROT_WRITE),
            0
        );
        assert!(std::slice::from_raw_parts(child.cast::<u8>(), 4096)
            .iter()
            .all(|byte| *byte == 0x5a));

        cage::get_cage(61).unwrap().vmmap.write().change_prot(
            0,
            1,
            libc::PROT_READ | libc::PROT_WRITE,
        );
        assert!(cage::fork_vmmap(61, 62).is_err());

        cage::remove_cage(61);
        cage::remove_cage(62);
        assert_eq!(libc::munmap(parent, 8192), 0);
        assert_eq!(libc::munmap(child, 8192), 0);
    }
}

#[test]
fn activating_part_of_a_reservation_marks_only_that_part_as_data() {
    let mut vmmap = Vmmap::new();
    vmmap
        .add_entry_with_overwrite(
            1,
            3,
            libc::PROT_NONE,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            MemoryBackingType::Anonymous,
            0,
            3 * 4096,
            1,
        )
        .unwrap();
    vmmap.mark_reservation(1, 3).unwrap();
    vmmap.change_prot(2, 1, libc::PROT_READ | libc::PROT_WRITE);

    let regions: Vec<_> = vmmap
        .entries
        .iter()
        .map(|(interval, entry)| (interval.start(), interval.end(), entry.is_reservation))
        .collect();
    assert_eq!(regions, [(1, 1, true), (2, 2, false), (3, 3, true)]);

    vmmap.change_prot(2, 1, libc::PROT_NONE);
    let regions: Vec<_> = vmmap
        .entries
        .iter()
        .map(|(interval, entry)| (interval.start(), interval.end(), entry.is_reservation))
        .collect();
    assert_eq!(regions, [(1, 1, true), (2, 2, false), (3, 3, true)]);
}

#[test]
fn fork_reader_does_not_occupy_closed_stdin() {
    if std::env::var_os("LIND_TEST_CLOSED_STDIN").is_some() {
        assert_eq!(unsafe { libc::close(libc::STDIN_FILENO) }, 0);
        cage::init_fork_memory_reader().unwrap();
        assert_eq!(
            unsafe { libc::fcntl(libc::STDIN_FILENO, libc::F_GETFD) },
            -1
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EBADF)
        );
        return;
    }

    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "fork_reader_does_not_occupy_closed_stdin"])
        .env("LIND_TEST_CLOSED_STDIN", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "subprocess failed: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
