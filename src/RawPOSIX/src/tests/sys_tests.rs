#![allow(dead_code)] //suppress warning for these functions not being used in targets other than the
                     // tests

#[allow(unused_parens)]
#[cfg(test)]
pub mod sys_tests {
    use sysdefs::constants::sys_const::{DEFAULT_GID, DEFAULT_UID, EXIT_SUCCESS};

    use super::super::*;
    use crate::interface;
    use crate::safeposix::{cage::*, dispatcher::*, filesystem};

    #[test]
    pub fn ut_lind_getpid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        assert_eq!(cage.getpid_syscall(), 1);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_getppid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        cage.fork_syscall(2);
        let cage2 = interface::cagetable_getref(2);
        assert_eq!(cage2.getppid_syscall(), 1);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_getuid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // The first call to geteuid always returns -1
        assert_eq!(cage.getuid_syscall(), -1);
        // Subsequent calls return the default value
        assert_eq!(cage.getuid_syscall(), DEFAULT_UID as i32);
        lindrustfinalize()
    }

    #[test]
    pub fn ut_lind_geteuid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // The first call to geteuid always returns -1
        assert_eq!(cage.geteuid_syscall(), -1);
        // Subsequent calls return the default value
        assert_eq!(cage.geteuid_syscall(), DEFAULT_UID as i32);
        lindrustfinalize()
    }

    #[test]
    pub fn ut_lind_getgid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // The first call to geteuid always returns -1
        assert_eq!(cage.getgid_syscall(), -1);
        // Subsequent calls return the default value
        assert_eq!(cage.getgid_syscall(), DEFAULT_GID as i32);
        lindrustfinalize()
    }

    #[test]
    pub fn ut_lind_getegid() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // The first call to geteuid always returns -1
        assert_eq!(cage.getegid_syscall(), -1);
        // Subsequent calls return the default value
        assert_eq!(cage.getegid_syscall(), DEFAULT_GID as i32);
        lindrustfinalize()
    }

    #[test]
    pub fn ut_lind_fork() {
        // Since the fork syscall is heavily tested in relation to other syscalls
        // we only perform simple checks for testing the sanity of the fork syscall
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // Spawn a new child object using the fork syscall
        cage.fork_syscall(2);
        // Search for the new cage object with cage_id = 2
        let child_cage = interface::cagetable_getref(2);
        // Assert the parent value is the the id of the first cage object
        assert_eq!(child_cage.getppid_syscall(), 1);
        // Assert that the cage id of the child is the value passed in the original fork
        // syscall
        assert_eq!(child_cage.getuid_syscall(), -1);
        assert_eq!(child_cage.getuid_syscall(), DEFAULT_UID as i32);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_exit() {
        // Since exit function is heavily used and tested in other syscalls and their
        // tests We only perform preliminary checks for checking the sanity of
        // this syscall We don't check for cases such as exiting a cage twice -
        // since the exiting process is handled by the NaCl runtime - and it
        // ensures that a cage does not exit twice acquiring a lock on TESTMUTEX
        // prevents other tests from running concurrently, and also performs
        // clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);
        // Call the exit call
        assert_eq!(cage.exit_syscall(EXIT_SUCCESS), EXIT_SUCCESS);
        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_exec() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage1 = interface::cagetable_getref(1);
        // Exec a new child
        assert_eq!(cage1.exec_syscall(), 0);
        // Assert that the fork was correct
        let child_cage = interface::cagetable_getref(2);
        assert_eq!(child_cage.getuid_syscall(), -1);
        assert_eq!(child_cage.getuid_syscall(), DEFAULT_UID as i32);

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_waitpid() {
        // acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // first let's fork some children
        cage.fork_syscall(2);
        cage.fork_syscall(3);
        cage.fork_syscall(4);
        cage.fork_syscall(5);

        let child_cage2 = interface::cagetable_getref(2);
        let child_cage3 = interface::cagetable_getref(3);
        let child_cage4 = interface::cagetable_getref(4);

        // cage2 exit before parent wait
        child_cage2.exit_syscall(123);

        let mut status = 0;
        let pid = cage.waitpid_syscall(2, &mut status, 0);
        assert_eq!(pid, 2);
        assert_eq!(status, 123);

        // test for WNOHANG option
        let pid = cage.waitpid_syscall(0, &mut status, libc::WNOHANG);
        assert_eq!(pid, 0);

        // Store the cage IDs we want to exit
        let cage3_id = 3;
        let cage4_id = 4;

        // test for waitpid when the cage exits in the middle of waiting
        let thread1 = interface::helper_thread(move || {
            interface::sleep(interface::RustDuration::from_millis(100));
            // Instead of moving cages, we'll get new references inside the thread
            let thread_cage4 = interface::cagetable_getref(cage4_id);
            let thread_cage3 = interface::cagetable_getref(cage3_id);
            thread_cage4.exit_syscall(4);
            thread_cage3.exit_syscall(3);
        });

        let pid = cage.waitpid_syscall(0, &mut status, 0);
        assert_eq!(pid, 4);
        assert_eq!(status, 4);

        let pid = cage.waitpid_syscall(0, &mut status, 0);
        assert_eq!(pid, 3);
        assert_eq!(status, 3);

        let _ = thread1.join().unwrap();

        lindrustfinalize();
    }

    #[test]
    pub fn ut_lind_waitpid_signal_interruption() {
        //acquiring a lock on TESTMUTEX prevents other tests from running concurrently,
        // and also performs clean env setup
        let _thelock = setup::lock_and_init();
        let cage = interface::cagetable_getref(1);

        // waitpid call on non-existent child with WNOHANG
        // Should return immediately without hanging (key requirement of PR #228)
        let mut status = 0;
        let pid = cage.waitpid_syscall(-1, &mut status, libc::WNOHANG);

        // Should return 0 (no children) or negative error, not hang
        assert!(
            pid <= 0,
            "waitpid should return 0 or error for no children, got: {}",
            pid
        );

        // Test: waitpid on specific non-existent PID with WNOHANG
        let pid = cage.waitpid_syscall(999, &mut status, libc::WNOHANG);
        assert!(
            pid < 0,
            "waitpid should return error for non-existent child"
        );

        lindrustfinalize();
    }
}
