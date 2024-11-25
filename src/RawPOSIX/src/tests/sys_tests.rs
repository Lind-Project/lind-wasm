#![allow(dead_code)] //suppress warning for these functions not being used in targets other than the
                     // tests

#[allow(unused_parens)]
#[cfg(test)]
pub mod sys_tests {
    use super::super::*;
    use crate::interface;
    // use crate::safeposix::cage::{FileDescriptor::*, *};
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
        assert_eq!(cage1.exec_syscall(2), 0);
        // Assert that the fork was correct
        let child_cage = interface::cagetable_getref(2);
        assert_eq!(child_cage.getuid_syscall(), -1);
        assert_eq!(child_cage.getuid_syscall(), DEFAULT_UID as i32);
        
        lindrustfinalize();
    }
}
