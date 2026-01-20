//! This module provides an fdtable, an abstraction which makes it easy
//! to separate out file descriptors for different cages.  There are several
//! reasons why this is needed.  First, different cages are not permitted to
//! see or access each others' descriptors, hence one needs a means to track
//! this somehow.  Second, different cages may each want to have something
//! like their STDERR or STDOUT directed to different locations.  Third,
//! when a cage forks, its fds are inherited by the child, but operations on
//! those file descriptors (e.g., close) may happen independenty and must not
//! impact the other cage.
//!
//! As such, this is a general library meant to handle those issues.  It has
//! the primary function of letting set up virtual (child cage) to
//! mappings.
//!
//! Note that the code re-exports an implementation from a specific submodule.
//! This was done to make the algorithmic options easier to benchmark and
//! compare.  You, the caller, should only use the base `fdtables` API and
//! not `fdtables::algorithmname` directly, as the latter will not be stable
//! over time.

// ********************** CLIPPY DISCUSSION **************************** //
// Copied from Tom Buckley-Houston
// =========================================================================
//                  Canonical lints for whole crate
// =========================================================================
// Official docs:
//   https://doc.rust-lang.org/nightly/clippy/lints.html
// Useful app to lookup full details of individual lints:
//   https://rust-lang.github.io/rust-clippy/master/index.html
//
// We set base lints to give the fullest, most pedantic feedback possible.
// Though we prefer that they are just warnings during development so that build-denial
// is only enforced in CI.
//
#![warn(
    // `clippy::all` is already on by default. It implies the following:
    //   clippy::correctness code that is outright wrong or useless
    //   clippy::suspicious code that is most likely wrong or useless
    //   clippy::complexity code that does something simple but in a complex way
    //   clippy::perf code that can be written to run faster
    //   clippy::style code that should be written in a more idiomatic way
    clippy::all,

    // It's always good to write as much documentation as possible
    missing_docs,

    // > clippy::pedantic lints which are rather strict or might have false positives
    clippy::pedantic,

    // > new lints that are still under development"
    // (so "nursery" doesn't mean "Rust newbies")
//    clippy::nursery,

    // > The clippy::cargo group gives you suggestions on how to improve your Cargo.toml file.
    // > This might be especially interesting if you want to publish your crate and are not sure
    // > if you have all useful information in your Cargo.toml.
    clippy::cargo
)]
// > The clippy::restriction group will restrict you in some way.
// > If you enable a restriction lint for your crate it is recommended to also fix code that
// > this lint triggers on. However, those lints are really strict by design and you might want
// > to #[allow] them in some special cases, with a comment justifying that.
#![allow(clippy::blanket_clippy_restriction_lints)]
// JAC: I took a look at these and it seems like these are mostly uninteresting
// false positives.
//#![warn(clippy::restriction)]

// I do a fair amount of casting to usize so that I can index values in arrays.
// I can't annotate them all separately because I can't assign attributes to
// expressions.  So I'll turn this off.
#![allow(clippy::cast_possible_truncation)]
// TODO: This is to disable a warning in threei's reversible enum definition.
// I'd like to revisit that clippy warning later and see if we want to handle
// it differently
#![allow(clippy::result_unit_err)]

// ********************* END CLIPPY DISCUSSION ************************* //

// NOTE: This setup is a bit odd, I know.  I'm creating different
// implementations of the same algorithm and I'd like to test them.  Originally
// I was going to have a struct interface where I switched between them by
// swapping out structs with the same trait.  This was a pain-in-the-butt, but
// it worked for single threaded things or multi-threaded readable things.
// However, I couldn't figure out how to make this work with having threads
// share a struct where the underlying things which were mutable (even though
// the underlying items were locked appropriately in a generic way).
//
// This makes things like the doc strings very odd as well.  I am extracting
// these out to separate files instead of having them in-line, since the
// different implementations will have the same doc strings.
//
// How this works is that I will import a single implementation as a mod here
// and this is what the benchmarker will use.  If you want to change the
// implementation you benchmark / test / use, you need to change the lines
// below...
//
// I've looked at traits and patterns.  It's possible there is a better way to
// do this which I'm currently unable to devise given my unfamliarity with
// Rust...

// Please see the doc strings for more information about the implementations.

// This library is likely the place in the system where we should consider
// putting in place limits on file descriptors.  Linux does this through two
// error codes, one for a per-process limit and the other for an overall system
// limit.  My thinking currently is that both will be configurable values in
// the library.
//
//       EMFILE The per-process limit on the number of open file
//              descriptors has been reached.
//
//       ENFILE The system-wide limit on the total number of open files
//              has been reached. (mostly unimplemented)

// This includes the specific implementation of the algorithm chosen.
include!("current_impl");

// This includes general constants and definitions for things that are
// needed everywhere, like FDTableEntry.  I use the * import here to flatten
// the namespace so folks importing this have the symbols directly imported.
mod commonconstants;
pub use commonconstants::*;

// This is used everywhere...  Should I re-export more of these symbols?
pub mod threei;
/// Error values (matching errno in Linux) for the various call Results
pub use threei::Errno;

/***************************** TESTS FOLLOW ******************************/

// I'm including my unit tests in-line, in this code.  Integration tests will
// exist in the tests/ directory.
#[cfg(test)]
mod tests {

    use lazy_static::lazy_static;

    use std::sync::{Mutex, MutexGuard};

    use std::thread;

    use std::collections::HashSet;

    // I'm having a global testing mutex because otherwise the tests will
    // run concurrently.  This messes up some tests, especially testing
    // that tries to get all FDs, etc.
    lazy_static! {
        // This has a junk value (a bool).  Could be anything...
        #[derive(Debug)]
        static ref TESTMUTEX: Mutex<bool> = {
            Mutex::new(true)
        };
    }

    // Import the symbols, etc. in this file...
    use super::*;

    fn do_panic(_: FDTableEntry, _: u64) {
        panic!("do_panic!");
    }

    #[test]
    // Basic test to ensure that I can get a virtual fd and the info back
    // find the value in the table afterwards...
    fn get_and_translate_work() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FDKIND: u32 = 0;
        const UNDERFD: u64 = 10;
        // Acquire a virtual fd...
        let my_virt_fd =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, UNDERFD, false, 100).unwrap();
        let _ = get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, UNDERFD, false, 100).unwrap();
        let _ = get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, UNDERFD, false, 100).unwrap();
        let _ = get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, UNDERFD, false, 100).unwrap();
        assert_eq!(
            UNDERFD,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd)
                .unwrap()
                .underfd
        );
        assert_eq!(
            FDKIND,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd)
                .unwrap()
                .fdkind
        );
    }

    #[test]
    // Do more complex things work with get and translate?
    fn more_complex_get_and_translate() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire a virtual fd...
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 2, false, 3).unwrap();
        let my_virt_fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 7, 8, true, 9).unwrap();
        assert_eq!(
            FDTableEntry {
                fdkind: 1,
                underfd: 2,
                should_cloexec: false,
                perfdinfo: 3
            },
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd).unwrap()
        );
        assert_eq!(
            FDTableEntry {
                fdkind: 7,
                underfd: 8,
                should_cloexec: true,
                perfdinfo: 9
            },
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2).unwrap()
        );
    }

    #[test]
    // Let's see if I can change the cloexec flag...
    fn try_set_cloexec() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire a virtual fd...
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 2, false, 3).unwrap();
        set_cloexec(threei::TESTING_CAGEID, my_virt_fd, true).unwrap();

        assert_eq!(
            FDTableEntry {
                fdkind: 1,
                underfd: 2,
                should_cloexec: true, // Should be set now...
                perfdinfo: 3
            },
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd).unwrap()
        );
    }

    #[test]
    // Set perfdinfo
    fn try_set_perfdinfo() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire two virtual fds with the same fdkind and underfd...
        let my_virt_fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 3, 4, false, 150).unwrap();
        let my_virt_fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 3, 4, true, 250).unwrap();
        set_perfdinfo(threei::TESTING_CAGEID, my_virt_fd1, 500).unwrap();
        assert_eq!(
            500,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        // Changing one should not have changed the other...
        assert_eq!(
            250,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2)
                .unwrap()
                .perfdinfo
        );
    }

    #[test]
    fn test_remove_cage_from_fdtable() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire two virtual fds...
        let _my_virt_fd1 =
            get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 150).unwrap();
        let _my_virt_fd2 =
            get_unused_virtual_fd(threei::TESTING_CAGEID, 4, 13, false, 150).unwrap();

        // let's drop this fdtable...
        remove_cage_from_fdtable(threei::TESTING_CAGEID);
        // Likely should have a better test, but everything will panic...
    }

    #[test]
    fn test_empty_fds_for_exec() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire two virtual fds...
        let my_virt_fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 150).unwrap();
        let my_virt_fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 4, true, 250).unwrap();

        empty_fds_for_exec(threei::TESTING_CAGEID);

        assert_eq!(
            150,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        // Should be missing...
        assert!(translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2).is_err());
    }

    #[test]
    fn return_fdtable_copy_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();
        // Acquire two virtual fds...
        let my_virt_fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 150).unwrap();
        let my_virt_fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 4, true, 250).unwrap();

        // Copy the fdtable over to a new cage...
        let mut myhm = return_fdtable_copy(threei::TESTING_CAGEID);

        // Check we got what we expected...
        assert_eq!(
            *(myhm.get(&my_virt_fd1).unwrap()),
            FDTableEntry {
                fdkind: 0,
                underfd: 10,
                should_cloexec: false,
                perfdinfo: 150
            }
        );
        assert_eq!(
            *(myhm.get(&my_virt_fd2).unwrap()),
            FDTableEntry {
                fdkind: 1,
                underfd: 4,
                should_cloexec: true,
                perfdinfo: 250
            }
        );

        myhm.insert(
            my_virt_fd1,
            FDTableEntry {
                fdkind: 2,
                underfd: 100,
                should_cloexec: false,
                perfdinfo: 15,
            },
        )
        .unwrap();

        // has my hashmap been updated?
        assert_eq!(
            *(myhm.get(&my_virt_fd1).unwrap()),
            FDTableEntry {
                fdkind: 2,
                underfd: 100,
                should_cloexec: false,
                perfdinfo: 15,
            }
        );

        // Check to make sure the actual table is still intact...
        assert_eq!(
            150,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        assert_eq!(
            250,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2)
                .unwrap()
                .perfdinfo
        );
    }

    #[test]
    fn test_copy_fdtable_for_cage() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Acquire two virtual fds...
        let my_virt_fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 150).unwrap();
        let my_virt_fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 4, true, 250).unwrap();

        assert_eq!(
            150,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        assert_eq!(
            250,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2)
                .unwrap()
                .perfdinfo
        );

        // Copy the fdtable over to a new cage...
        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID1).unwrap();

        // Check the elements exist...
        assert_eq!(
            150,
            translate_virtual_fd(threei::TESTING_CAGEID1, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        assert_eq!(
            250,
            translate_virtual_fd(threei::TESTING_CAGEID1, my_virt_fd2)
                .unwrap()
                .perfdinfo
        );
        // ... and are independent...
        set_perfdinfo(threei::TESTING_CAGEID, my_virt_fd1, 500).unwrap();
        assert_eq!(
            150,
            translate_virtual_fd(threei::TESTING_CAGEID1, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
        assert_eq!(
            500,
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd1)
                .unwrap()
                .perfdinfo
        );
    }

    #[test]
    // Do close_virtualfd(...) testing...
    fn test_close_virtualfd_with_fdkind_0() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FD1: u64 = 57;

        const FD2: u64 = 101;

        const SPECIFICVIRTUALFD: u64 = 15;

        // None of my closes (until the end) will be the last...
        register_close_handlers(0, NULL_FUNC, do_panic);

        // use the same fd a few times in different ways...
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD1, false, 10).unwrap();
        get_specific_virtual_fd(threei::TESTING_CAGEID, SPECIFICVIRTUALFD, 0, FD1, false, 10)
            .unwrap();
        let cloexecfd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD1, true, 10).unwrap();
        // and a different fd
        let _my_virt_fd3 =
            get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD2, false, 10).unwrap();

        // let's close one (should have two left...)
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();

        // Let's fork (to double the count)!
        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID7).unwrap();

        // let's simulate exec, which should close one of these...
        empty_fds_for_exec(threei::TESTING_CAGEID7);

        // but the copy in the original cage table should remain, so this
        // shouldn't error...
        translate_virtual_fd(threei::TESTING_CAGEID, cloexecfd).unwrap();

        // However, the other should be gone and should error...
        assert!(translate_virtual_fd(threei::TESTING_CAGEID7, cloexecfd).is_err());

        // Let's simulate exit on the initial cage, to close two of them...
        remove_cage_from_fdtable(threei::TESTING_CAGEID);

        // panic if this isn't the last one (from now on)
        register_close_handlers(0, do_panic, NULL_FUNC);

        // Now this is the last one!
        close_virtualfd(threei::TESTING_CAGEID7, SPECIFICVIRTUALFD).unwrap();
    }

    #[test]
    // Do close_virtualfd(...) testing on different fdkinds...
    fn test_close_virtualfd_with_varied_fdkinds() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FDKIND1: u32 = 57;
        const FD1: u64 = 57;

        const FDKIND2: u32 = 57;
        const FD2: u64 = 101;

        const SPECIFICVIRTUALFD: u64 = 15;

        // Should not be called because I'm doing different fds...
        register_close_handlers(0, do_panic, do_panic);

        // use the same fd a few times in different ways...
        let my_virt_fd =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND1, FD1, false, 10).unwrap();
        get_specific_virtual_fd(
            threei::TESTING_CAGEID,
            SPECIFICVIRTUALFD,
            FDKIND1,
            FD1,
            false,
            10,
        )
        .unwrap();
        let cloexecfd =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND1, FD1, true, 10).unwrap();
        // and a different fd
        let _my_virt_fd3 =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND2, FD2, false, 10).unwrap();

        // let's close one (should have two left...)
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();

        // Let's fork (to double the count)!
        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID7).unwrap();

        // let's simulate exec, which should close one of these...
        empty_fds_for_exec(threei::TESTING_CAGEID7);

        // but the copy in the original cage table should remain, so this
        // shouldn't error...
        translate_virtual_fd(threei::TESTING_CAGEID, cloexecfd).unwrap();

        // However, the other should be gone and should error...
        assert!(translate_virtual_fd(threei::TESTING_CAGEID7, cloexecfd).is_err());

        // Let's simulate exit on the initial cage, to close two of them...
        remove_cage_from_fdtable(threei::TESTING_CAGEID);

        // Now this is the last one!
        close_virtualfd(threei::TESTING_CAGEID7, SPECIFICVIRTUALFD).unwrap();
    }

    #[test]
    #[should_panic]
    // Check for duplicate uses of the same fd...
    fn test_dup_close() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // get the fd...  I tested this in the test above, so should not
        // panic...
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 10).unwrap();
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();

        // Panic on this one...
        register_close_handlers(0, do_panic, NULL_FUNC);

        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 10).unwrap();
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();
    }

    // Helper for the close handler recursion tests...
    fn _test_close_handler_recursion_helper(_: FDTableEntry, _: u64) {
        // reset helpers
        register_close_handlers(0, NULL_FUNC, NULL_FUNC);

        const FD: u64 = 57;
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 10).unwrap();
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();
    }

    #[test]
    // check to see what happens if close handlers call other operations...
    fn test_close_handler_recursion() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FD: u64 = 57;

        // Register my helper to be called when I call close...
        register_close_handlers(0, NULL_FUNC, _test_close_handler_recursion_helper);

        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 10).unwrap();
        // Call this which calls the close handler
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();
    }

    #[test]
    // get_specific_virtual_fd closehandler recursion... likely deadlock on
    // fail.
    fn test_gsvfd_handler_recursion() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FD: u64 = 57;

        // Register my helper to be called when I call close...
        register_close_handlers(0, NULL_FUNC, _test_close_handler_recursion_helper);

        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 10).unwrap();
        // Call this which calls the close handler
        get_specific_virtual_fd(threei::TESTING_CAGEID, my_virt_fd, 0, 123, true, 0).unwrap();
    }

    #[test]
    // remove_cage_from_fdtable closehandler recursion... likely deadlock on
    // fail.
    fn test_rcffdt_handler_recursion() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FD: u64 = 57;
        // Since I'm removing a cage here, yet doing operations afterwards,
        // I need to have an empty cage first.
        init_empty_cage(threei::TESTING_CAGEID5);

        // Register my helper to be called when I call close...
        register_close_handlers(0, NULL_FUNC, _test_close_handler_recursion_helper);

        let _my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID5, 0, FD, false, 10).unwrap();
        // Call this which calls the close handler
        remove_cage_from_fdtable(threei::TESTING_CAGEID5);
    }

    #[test]
    // empty_fds_for_exec closehandler recursion...  likely deadlock on fail.
    fn test_effe_handler_recursion() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // Use a different fdkind...
        const FDKIND: u32 = 1000;
        const FD: u64 = 12;

        // Register my helper to be called when I call close on only FDKIND
        // 0.  This should not be called because FDKIND is different...
        register_close_handlers(0, NULL_FUNC, _test_close_handler_recursion_helper);

        let _my_virt_fd =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, FD, true, 10).unwrap();
        empty_fds_for_exec(threei::TESTING_CAGEID);
    }

    #[test]
    // check some common poll cases...
    fn check_poll_helpers() {
        let mut _thelock: MutexGuard<bool>;
        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        let cage_id = threei::TESTING_CAGEID;

        get_specific_virtual_fd(cage_id, 3, 0, 7, false, 10).unwrap();
        get_specific_virtual_fd(cage_id, 5, 100, 32, false, 123).unwrap();
        get_specific_virtual_fd(cage_id, 9, 0, 20, true, 0).unwrap();

        let (pollhashmap, mappingtable) =
            convert_virtualfds_for_poll(cage_id, HashSet::from([1, 3, 5, 9]));

        assert_eq!(pollhashmap.len(), 3); // 3 different keys for fdkinds
        assert_eq!(pollhashmap.get(&0).unwrap().len(), 2);
        assert_eq!(pollhashmap.get(&100).unwrap().len(), 1);
        assert_eq!(pollhashmap.get(&FDT_INVALID_FD).unwrap().len(), 1);

        // poll(...)  // let's pretend that fd 7 had its event triggered...
        let newfds = convert_poll_result_back_to_virtual(0, 7, &mappingtable);
        // virtfd 3 should be returned
        assert_eq!(newfds, Some(3));
    }

    #[test]
    // check some common epoll cases...
    fn check_epoll_helpers() {
        let mut _thelock: MutexGuard<bool>;
        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        let cage_id = threei::TESTING_CAGEID;

        const EMULFDKIND: u32 = 2;
        const FDKIND: u32 = 1;
        let virtfd1 = 5;
        let virtfd2 = 6;
        let virtfd3 = 10;
        let epollunderfd = 100;
        // get_specific_virtual_fd(cage_id, VIRTFD, REALFD, CLOEXEC, OPTINFO)
        get_specific_virtual_fd(cage_id, virtfd1, EMULFDKIND, 10, false, 123).unwrap();
        get_specific_virtual_fd(cage_id, virtfd2, EMULFDKIND, 11, false, 456).unwrap();
        get_specific_virtual_fd(cage_id, virtfd3, FDKIND, 20, true, 0).unwrap();

        // get an epollfd...
        let epollfd = epoll_create_empty(cage_id, false).unwrap();
        // ... set the underfd ...
        epoll_add_underfd(cage_id, epollfd, FDKIND, epollunderfd).unwrap();

        let myevent1 = epoll_event {
            events: (EPOLLIN + EPOLLOUT) as u32,
            u64: 0,
        };
        let myevent2 = epoll_event {
            events: (EPOLLIN) as u32,
            u64: 0,
        };

        // try to add the epollfd, which should fail
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_ADD, virtfd3, myevent1.clone())
                .unwrap(),
            ()
        );

        // Only one key,
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd).unwrap().len(),
            1
        );
        // ...with a value
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&FDKIND)
                .unwrap()
                .len(),
            1
        );

        // Add in one fd...
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_ADD, virtfd1, myevent1.clone())
                .unwrap(),
            ()
        );

        // Should have two keys now
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd).unwrap().len(),
            2
        );

        // Delete an item...
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_DEL, virtfd1, myevent1.clone())
                .unwrap(),
            ()
        );

        // Only one key,
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd).unwrap().len(),
            1
        );

        // Add in two EMULFDKINDS
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_ADD, virtfd1, myevent1.clone())
                .unwrap(),
            ()
        );
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_ADD, virtfd2, myevent2.clone())
                .unwrap(),
            ()
        );
        // Should have two kinds...
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd).unwrap().len(),
            2
        );
        // ...and two values of kind EMULFDKIND

        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd).unwrap().len(),
            2
        );
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&EMULFDKIND)
                .unwrap()
                .len(),
            2
        );

        // Check their event types are correct...
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&EMULFDKIND)
                .unwrap()
                .get(&virtfd1)
                .unwrap()
                .events,
            myevent1.events
        );
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&EMULFDKIND)
                .unwrap()
                .get(&virtfd2)
                .unwrap()
                .events,
            myevent2.events
        );

        // Let's switch one of them...
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd, EPOLL_CTL_MOD, virtfd1, myevent2.clone())
                .unwrap(),
            ()
        );

        // Check their event types are correct...
        // not anymore!
        assert_ne!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&EMULFDKIND)
                .unwrap()
                .get(&virtfd1)
                .unwrap()
                .events,
            myevent1.events
        );
        // still the same...
        assert_eq!(
            get_virtual_epoll_wait_data(cage_id, epollfd)
                .unwrap()
                .get(&EMULFDKIND)
                .unwrap()
                .get(&virtfd2)
                .unwrap()
                .events,
            myevent2.events
        );
    }

    #[test]
    #[ignore]
    // Add these if I do the complete epoll later.  These tests are amazing!
    // https://github.com/heiher/epoll-wakeup
    // Right now, just check, did I implement epoll of epoll fds?
    #[allow(non_snake_case)]
    fn check_SHOULD_FAIL_FOR_NOW_if_we_support_epoll_of_epoll() {
        let mut _thelock: MutexGuard<bool>;
        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        let cage_id = threei::TESTING_CAGEID;

        // get two epollfds...
        let epollfd1 = epoll_create_empty(cage_id, false).unwrap();
        let epollfd2 = epoll_create_empty(cage_id, false).unwrap();

        let myevent1 = epoll_event {
            events: (EPOLLIN + EPOLLOUT) as u32,
            u64: 0,
        };

        // try to add an epollfd to an epollfd
        assert_eq!(
            virtualize_epoll_ctl(cage_id, epollfd1, EPOLL_CTL_ADD, epollfd2, myevent1.clone())
                .unwrap(),
            ()
        );
    }

    #[test]
    // check some common select cases...
    fn check_basic_select() {
        let mut _thelock: MutexGuard<bool>;
        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        let cage_id = threei::TESTING_CAGEID;

        get_specific_virtual_fd(cage_id, 3, 0, 7, false, 10).unwrap();
        get_specific_virtual_fd(cage_id, 5, 1, 123, false, 123).unwrap();

        let mut bad_fds_to_check = _init_fd_set();

        // check all "None" is okay...
        assert!(
            prepare_bitmasks_for_select(cage_id, 6, None, None, None, &HashSet::from([0])).is_ok()
        );

        // check a few different "empty" bitmask cases too...
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            Some(bad_fds_to_check),
            None,
            None,
            &HashSet::from([0])
        )
        .is_ok());
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            None,
            None,
            Some(bad_fds_to_check),
            &HashSet::from([0])
        )
        .is_ok());
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            Some(bad_fds_to_check),
            Some(bad_fds_to_check),
            Some(bad_fds_to_check),
            &HashSet::from([0])
        )
        .is_ok());

        // Okay!   Now, set a fd...
        _fd_set(2, &mut bad_fds_to_check);

        // check all of the positions!
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            Some(bad_fds_to_check),
            None,
            None,
            &HashSet::from([0])
        )
        .is_err());
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            None,
            Some(bad_fds_to_check),
            None,
            &HashSet::from([0])
        )
        .is_err());
        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            None,
            None,
            Some(bad_fds_to_check),
            &HashSet::from([0])
        )
        .is_err());

        // but if I drop the nfds too low, it is okay...
        assert!(prepare_bitmasks_for_select(
            cage_id,
            2,
            None,
            None,
            Some(bad_fds_to_check),
            &HashSet::from([0])
        )
        .is_ok());

        // too high also errors...
        assert!(prepare_bitmasks_for_select(
            cage_id,
            1024,
            None,
            None,
            Some(bad_fds_to_check),
            &HashSet::from([0])
        )
        .is_err());

        // recall, we set up some actual virtualfds above...
        let mut actual_fds_to_check = _init_fd_set();
        _fd_set(3, &mut actual_fds_to_check);
        _fd_set(5, &mut actual_fds_to_check);

        assert!(prepare_bitmasks_for_select(
            cage_id,
            6,
            Some(actual_fds_to_check),
            Some(actual_fds_to_check),
            None,
            &HashSet::from([0])
        )
        .is_ok());

        // let's peek closer at an actual call...
        let (selectbittables, unparsedtables, mappingtable) = prepare_bitmasks_for_select(
            cage_id,
            6,
            Some(actual_fds_to_check),
            None,
            None,
            &HashSet::from([0]),
        )
        .unwrap();
        // The first bitmask should be filled out...
        assert!(selectbittables[0].get(&0).is_some());
        assert!(selectbittables[1].get(&0).is_none());
        assert!(selectbittables[2].get(&0).is_none());
        // Only the first one should be non-empty...
        assert_eq!(unparsedtables[0].len(), 1);
        assert_eq!(unparsedtables[1].len(), 0);
        assert_eq!(unparsedtables[2].len(), 0);
        // Both fdkinds end up in the mapping table...
        assert_eq!(mappingtable.len(), 2);
    }

    #[test]
    // Let's test to see our functions error gracefully with badfds...
    fn get_specific_virtual_fd_tests() {
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, false, 150).unwrap();

        // Choose an unused new_fd
        let my_new_fd: u64;
        if my_virt_fd == 0 {
            my_new_fd = 100;
        } else {
            my_new_fd = 0;
        }
        get_specific_virtual_fd(threei::TESTING_CAGEID, my_new_fd, 0, 1, true, 5).unwrap();
        assert_eq!(
            translate_virtual_fd(threei::TESTING_CAGEID, my_new_fd)
                .unwrap()
                .perfdinfo,
            5
        );
        assert_eq!(
            translate_virtual_fd(threei::TESTING_CAGEID, my_new_fd)
                .unwrap()
                .underfd,
            1
        );

        // Check if I get an error going out of range...
        assert!(get_specific_virtual_fd(
            threei::TESTING_CAGEID,
            FD_PER_PROCESS_MAX + 1,
            0,
            1,
            true,
            5
        )
        .is_err());
    }

    #[test]
    // Let's test to see our functions error gracefully with badfds...
    fn badfd_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        // some made up number...
        let my_virt_fd = 135;
        assert!(translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd).is_err());
        assert!(set_cloexec(threei::TESTING_CAGEID, my_virt_fd, true).is_err());
        assert!(set_perfdinfo(threei::TESTING_CAGEID, my_virt_fd, 37).is_err());
    }

    #[test]
    // Let's do a multithreaded test...
    fn multithreaded_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });

        refresh();
        let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, true, 100).unwrap();
        let fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 20, true, 200).unwrap();
        let fd3 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 30, true, 300).unwrap();
        for threadcount in [1, 2, 4, 8, 16].iter() {
            let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
            for _numthreads in 0..*threadcount {
                let thisthreadcount = *threadcount;

                thread_handle_vec.push(thread::spawn(move || {
                    // Do 10K / threadcount of 10 requests each.  100K total
                    for _ in 0..10000 / thisthreadcount {
                        translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd2).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd3).unwrap();
                    }
                }));
            }
            for handle in thread_handle_vec {
                handle.join().unwrap();
            }
        }
    }

    #[test]
    // Let's do a multithreaded test...
    fn multithreaded_write_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });

        refresh();
        for threadcount in [1, 2, 4, 8, 16].iter() {
            let mut thread_handle_vec: Vec<thread::JoinHandle<()>> = Vec::new();
            for _numthreads in 0..*threadcount {
                let thisthreadcount = *threadcount;

                thread_handle_vec.push(thread::spawn(move || {
                    // Do 1000 writes, then flush it out...
                    for _ in 0..1000 / thisthreadcount {
                        let fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, 10, true, 100)
                            .unwrap();
                        translate_virtual_fd(threei::TESTING_CAGEID, fd).unwrap();
                    }
                }));
            }
            for handle in thread_handle_vec {
                handle.join().unwrap();
            }
            refresh();
        }
    }

    // Let's use up all the fds and verify we get an error...
    #[test]
    fn use_all_fds_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FD: u64 = 10;
        for _current in 0..FD_PER_PROCESS_MAX {
            // check to make sure that the number of items is equal to the
            // number of times through this loop...
            //
            // Note: if this test is failing on the next line, it is likely
            // because some extra fds are allocated for the cage (like stdin,
            // stdout, and stderr).
            //
            // I removed this because it lifts the veil of the interface by
            // peeking into the GLOBALFDTABLE
            /*            assert_eq!(
                GLOBALFDTABLE
                    .lock()
                    .unwrap()
                    .get(&threei::TESTING_CAGEID)
                    .unwrap()
                    .len(),
                current as usize
            ); */

            let _ = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 100).unwrap();
        }
        // If the test is failing by not triggering here, we're not stopping
        // at the limit...
        if get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 100).is_err() {
            refresh();
        } else {
            panic!("Should have raised an error...");
        }
    }

    #[test]
    // Do we close a virtualfd when we select it?  (Do nothing, but see the
    // next test.)
    fn check_get_specific_virtual_fd_close_ok_test() {
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID10).unwrap();

        let virtfd = get_unused_virtual_fd(threei::TESTING_CAGEID10, 0, 10, false, 100).unwrap();
        // Do nothing.  See next test...
        get_specific_virtual_fd(threei::TESTING_CAGEID10, virtfd, 0, 10, false, 100).unwrap();
    }

    #[test]
    #[should_panic]
    // checks that init correctly panics
    fn check_init_panics() {
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID11).unwrap();
        // panic!
        init_empty_cage(threei::TESTING_CAGEID11);
    }

    #[test]
    #[should_panic]
    // Do we close a virtualfd when we call get_specific on it?
    fn check_get_specific_virtual_fd_close_panic_test() {
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        copy_fdtable_for_cage(threei::TESTING_CAGEID, threei::TESTING_CAGEID11).unwrap();
        // panic in a moment!
        register_close_handlers(0, do_panic, do_panic);
        let virtfd = get_unused_virtual_fd(threei::TESTING_CAGEID11, 0, 234, false, 100).unwrap();
        // panic!!!
        get_specific_virtual_fd(threei::TESTING_CAGEID11, virtfd, 0, 10, false, 100).unwrap();
    }

    #[test]
    #[should_panic]
    // Let's check to make sure we panic with an invalid cageid
    fn translate_panics_on_bad_cageid_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });

        let _ = translate_virtual_fd(threei::INVALID_CAGEID, 10);
    }

    #[test]
    #[should_panic]
    // Let's check to make sure we panic with an invalid cageid
    fn get_unused_virtual_fd_panics_on_bad_cageid_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });

        let _ = get_unused_virtual_fd(threei::INVALID_CAGEID, 0, 10, false, 100);
    }

    #[test]
    #[should_panic]
    // Let's check to make sure we panic with an invalid cageid
    fn set_cloexec_panics_on_bad_cageid_test() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });

        let _ = set_cloexec(threei::INVALID_CAGEID, 10, true);
    }

    #[test]
    #[should_panic]
    // Let's check that our callback for close is working correctly by having
    // it panic
    fn test_intermediate_handler() {
        // Get the guard in a way that if we unpoison it, we don't end up
        // with multiple runners...
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }

        refresh();

        const FD: u64 = 132;
        // I'm using unwrap_or because I don't want a panic here to be
        // considered passing the test
        let fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 100).unwrap_or(1);
        let _fd2 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 100).unwrap_or(1);

        register_close_handlers(0, do_panic, NULL_FUNC);

        // should panic here...
        close_virtualfd(threei::TESTING_CAGEID, fd1).unwrap();
    }

    #[test]
    #[should_panic]
    // Check final_handler
    fn test_final_handler() {
        // Get the guard in a way that if we unpoison it, we don't end up
        // with multiple runners...
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        const FD: u64 = 109;
        // I'm using unwrap_or because I don't want a panic here to be
        // considered passing the test
        let fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 100).unwrap_or(1);

        register_close_handlers(0, NULL_FUNC, do_panic);

        // should panic here...
        close_virtualfd(threei::TESTING_CAGEID, fd1).unwrap();
    }

    #[test]
    // No panics.  Just call a function...
    fn test_close_handlers() {
        let mut _thelock: MutexGuard<bool>;

        loop {
            match TESTMUTEX.lock() {
                Err(_) => {
                    TESTMUTEX.clear_poison();
                }
                Ok(val) => {
                    _thelock = val;
                    break;
                }
            }
        }
        refresh();

        // I'm using unwrap_or because I don't want a panic here to be
        // considered passing the test
        let fd1 = get_unused_virtual_fd(threei::TESTING_CAGEID, 1, 123, false, 100).unwrap_or(1);

        fn myfunc(_: FDTableEntry, _: u64) {}

        register_close_handlers(0, myfunc, myfunc);

        // should panic here...
        close_virtualfd(threei::TESTING_CAGEID, fd1).unwrap();
    }

    #[test]
    // To check if item has been removed successfully after close
    fn test_close_fdtable_update() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        const FDKIND: u32 = 0;
        const UNDERFD: u64 = 10;
        // Acquire a virtual fd...
        let my_virt_fd =
            get_unused_virtual_fd(threei::TESTING_CAGEID, FDKIND, UNDERFD, false, 100).unwrap();

        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();

        // translate_virtual_fd should return error, because there should have
        // no requested my_virt_fd after close
        match translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd) {
            Ok(_) => panic!("translate_virtual_fd should return error!!"),
            Err(_e) => {
                TESTMUTEX.clear_poison();
            }
        }
    }

    #[test]
    // Do more complex things work with get and translate?
    fn more_complex_get_and_translate_from_arg() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        let arg_fd1 = 5; // fd1 should look for free fd starting from this arguments
        let arg_fd2 = 10; // fd2 should look for free fd starting from this arguments

        // Acquire a virtual fd...
        // NEW API CALLS (6 arguments now)
        let my_virt_fd = get_unused_virtual_fd_from_startfd(
            threei::TESTING_CAGEID,
            1,       // fdkind
            2,       // underfd
            false,   // should_cloexec
            3,       // perfdinfo
            arg_fd1, // startfd
        )
        .unwrap();

        let my_virt_fd2 = get_unused_virtual_fd_from_startfd(
            threei::TESTING_CAGEID,
            7,       // fdkind
            8,       // underfd
            true,    // should_cloexec
            9,       // perfdinfo
            arg_fd2, // startfd
        )
        .unwrap();

        // Check if fd and fd2 is starting from corresponding args
        assert_eq!(my_virt_fd, 5);
        assert_eq!(my_virt_fd2, 10);

        assert_eq!(
            FDTableEntry {
                fdkind: 1,
                underfd: 2,
                should_cloexec: false,
                perfdinfo: 3
            },
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd).unwrap()
        );

        assert_eq!(
            FDTableEntry {
                fdkind: 7,
                underfd: 8,
                should_cloexec: true,
                perfdinfo: 9
            },
            translate_virtual_fd(threei::TESTING_CAGEID, my_virt_fd2).unwrap()
        );
    }
}
