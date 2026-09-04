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

// The specific implementation of the algorithm is selected via a Cargo feature
// (see Cargo.toml `[features]`). Exactly one impl feature must be enabled; the
// default is `dashmaparray`. To switch, build with e.g.
//   cargo build --no-default-features --features muthashmax

#[cfg(not(any(
    feature = "dashmaparray",
    feature = "dashmapvec",
    feature = "muthashmax",
    feature = "vanilla",
)))]
compile_error!(
    "fdtables: no impl feature enabled; pick exactly one of \
     `dashmaparray`, `dashmapvec`, `muthashmax`, or `vanilla`"
);

#[cfg(any(
    all(feature = "dashmaparray", feature = "dashmapvec"),
    all(feature = "dashmaparray", feature = "muthashmax"),
    all(feature = "dashmaparray", feature = "vanilla"),
    all(feature = "dashmapvec", feature = "muthashmax"),
    all(feature = "dashmapvec", feature = "vanilla"),
    all(feature = "muthashmax", feature = "vanilla"),
))]
compile_error!(
    "fdtables: more than one impl feature enabled; pick exactly one of \
     `dashmaparray`, `dashmapvec`, `muthashmax`, or `vanilla` \
     (use --no-default-features when overriding the default)"
);

#[cfg(feature = "dashmaparray")]
mod dashmaparrayglobal;
#[cfg(feature = "dashmaparray")]
pub use dashmaparrayglobal::*;

#[cfg(feature = "dashmapvec")]
mod dashmapvecglobal;
#[cfg(feature = "dashmapvec")]
pub use dashmapvecglobal::*;

#[cfg(feature = "muthashmax")]
mod muthashmaxglobal;
#[cfg(feature = "muthashmax")]
pub use muthashmaxglobal::*;

#[cfg(feature = "vanilla")]
mod vanillaglobal;
#[cfg(feature = "vanilla")]
pub use vanillaglobal::*;

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

    use std::sync::atomic::{AtomicU64, Ordering};

    use std::sync::{Arc, Barrier};

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

    fn do_panic(_: FDTableEntry, _: u64) -> Result<(), i32> {
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
    // Pre-existing failure on main, not introduced here: this asserts EBADF but
    // every backend's translate_virtual_fd returns EBADFD, while
    // get_specific_virtual_fd returns EBADF for the same class of error. Which
    // side is wrong is a team decision, not a fix.
    #[ignore = "pre-existing: translate_virtual_fd returns EBADFD where this expects EBADF; which side is wrong needs a team decision"]
    // ISO-002 fd-lib inner test: two cages have independent fd tables, so one
    // cage cannot reach another cage's descriptors
    fn cross_cage_fd_isolation() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        let cage_a = threei::TESTING_CAGEID;
        let cage_b = threei::TESTING_CAGEID1;
        init_empty_cage(cage_b);

        let fd_a = get_unused_virtual_fd(cage_a, 0, 111, false, 0).unwrap();
        let fd_b = get_unused_virtual_fd(cage_b, 0, 222, false, 0).unwrap();

        assert_eq!(fd_a, fd_b);

        let entry_a = translate_virtual_fd(cage_a, fd_a).unwrap();
        let entry_b = translate_virtual_fd(cage_b, fd_b).unwrap();

        assert_eq!(entry_a.underfd, 111);
        assert_eq!(entry_b.underfd, 222);
        assert_ne!(entry_a.underfd, entry_b.underfd);

        let fd_a2 = get_unused_virtual_fd(cage_a, 0, 333, false, 0).unwrap();
        assert_eq!(translate_virtual_fd(cage_a, fd_a2).unwrap().underfd, 333);
        assert!(translate_virtual_fd(cage_b, fd_a2).is_err());

        const B_ONLY_FD: u64 = 900;
        get_specific_virtual_fd(cage_b, B_ONLY_FD, 0, 555, false, 0).unwrap();

        assert_eq!(
            translate_virtual_fd(cage_a, B_ONLY_FD).unwrap_err(),
            threei::Errno::EBADF as u64
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
    fn _test_close_handler_recursion_helper(_: FDTableEntry, _: u64) -> Result<(), i32> {
        // reset helpers
        register_close_handlers(0, NULL_FUNC, NULL_FUNC);

        const FD: u64 = 57;
        let my_virt_fd = get_unused_virtual_fd(threei::TESTING_CAGEID, 0, FD, false, 10).unwrap();
        close_virtualfd(threei::TESTING_CAGEID, my_virt_fd).unwrap();

        Ok(())
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
    // Regression: after reserving fds via get_specific_virtual_fd, a
    // subsequent get_unused_virtual_fd must not hand back one of the
    // reserved fds.
    //
    // In the muthashmax backend, get_specific_virtual_fd previously
    // didn't bump `highestneverusedfd`, so the get_unused_virtual_fd
    // fast path would re-issue fd 0, 1, 2... and silently clobber the
    // reservations. This surfaced in the IPC grate's preexec reserving
    // stdio: the user binary's first pipe() call would overwrite stdin
    // and stdout, sending printf into the pipe. The other backends scan
    // for a vacant slot so this regression is impossible there, but the
    // test exercises the contract for all of them.
    fn get_specific_then_get_unused_does_not_reuse_fd() {
        let mut _thelock = TESTMUTEX.lock().unwrap_or_else(|e| {
            refresh();
            TESTMUTEX.clear_poison();
            e.into_inner()
        });
        refresh();

        let cage_id = threei::TESTING_CAGEID;

        // Reserve fds 0, 1, 2 (e.g. stdio) via get_specific_virtual_fd.
        get_specific_virtual_fd(cage_id, 0, 0, 100, false, 0).unwrap();
        get_specific_virtual_fd(cage_id, 1, 0, 101, false, 0).unwrap();
        get_specific_virtual_fd(cage_id, 2, 0, 102, false, 0).unwrap();

        // The next three get_unused_virtual_fd calls must skip the
        // reserved slots.
        for _ in 0..3 {
            let fd = get_unused_virtual_fd(cage_id, 0, 200, false, 0).unwrap();
            assert!(
                fd >= 3,
                "get_unused_virtual_fd returned reserved fd {} after get_specific_virtual_fd",
                fd
            );
        }

        // Reservations survive untouched.
        assert_eq!(translate_virtual_fd(cage_id, 0).unwrap().underfd, 100);
        assert_eq!(translate_virtual_fd(cage_id, 1).unwrap().underfd, 101);
        assert_eq!(translate_virtual_fd(cage_id, 2).unwrap().underfd, 102);
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

        fn myfunc(_: FDTableEntry, _: u64) -> Result<(), i32> {
            Ok(())
        }

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

    // =====================================================================
    // CONC-002: fd-table concurrency stress.
    //
    // Regression coverage for the concurrency fixes in dashmaparrayglobal.rs
    // and its siblings (_increment_fdcount, get_specific_virtual_fd,
    // copy_fdtable_for_cage).
    // =====================================================================

    /// Reserved cage-id block for CONC-002. Disjoint from
    /// threei::TESTING_CAGEID0..15 (0xffff_ffff_ffff_ffe0..=...ffef) and
    /// from any id another test in this module uses. 0xC0C2 == "CONC-002".
    const C2_BASE: u64 = 0x0000_0000_C0C2_0000; // workers, +0..C2_WORKERS
    const C2_CHILD: u64 = 0x0000_0000_C0C2_0020; // fork copies, +0..C2_WORKERS
    const C2_STABLE: u64 = 0x0000_0000_C0C2_0040;
    const C2_VICTIM: u64 = 0x0000_0000_C0C2_0050; // +0..4, cycled by Test 2
                                                  // 0xC0C2_0060..0xC0C2_00FF left free; later CONC test rows use
                                                  // their own 0xC0Cn_0000 blocks instead (see CONC-003 below).

    const C2_WORKERS: usize = 8;

    /// A dedicated fdkind plus a disjoint underfd window guarantees no
    /// FDCOUNT key ever aliases with another test's leftovers. This matters
    /// more here than it would elsewhere: refresh() clears FDTABLE and
    /// CLOSEHANDLERTABLE but never clears FDCOUNT, so refcount state leaks
    /// across every test in this binary. These tests prove their own
    /// accounting with close handlers rather than assuming a clean FDCOUNT.
    const C2_FDKIND: u32 = 0x7E57_0002;
    const C2_UNDERFD_BASE: u64 = 0x2000_0000;

    /// Hands out a globally unique underfd, so that (except where a test
    /// deliberately wants a shared key, e.g. Test 3) no two allocations
    /// anywhere in this test file ever share an FDCOUNT key.
    static C2_UNDERFD_SEQ: AtomicU64 = AtomicU64::new(0);
    fn c2_next_underfd() -> u64 {
        C2_UNDERFD_BASE + C2_UNDERFD_SEQ.fetch_add(1, Ordering::SeqCst)
    }

    static C2_LAST: AtomicU64 = AtomicU64::new(0);
    static C2_MID: AtomicU64 = AtomicU64::new(0);
    static C2_HANDLER_ERRS: AtomicU64 = AtomicU64::new(0);
    lazy_static! {
        /// underfd -> number of *last* closes seen. Every value must end at 1.
        static ref C2_RELEASED: Mutex<std::collections::HashMap<u64, u64>> =
            Mutex::new(std::collections::HashMap::new());
    }

    // Close handlers are plain `fn` pointers and cannot capture, so all
    // bookkeeping lives in the statics above. They never panic: a panic
    // inside fdtables' own teardown path is nearly impossible to attribute,
    // so a violated expectation is recorded for the main thread to assert
    // on instead.
    fn c2_last_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C2_FDKIND || remaining != 0 {
            C2_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        *C2_RELEASED
            .lock()
            .unwrap()
            .entry(entry.underfd)
            .or_insert(0) += 1;
        C2_LAST.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn c2_intermediate_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C2_FDKIND || remaining == 0 {
            C2_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        C2_MID.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Factors out the two copy-pasted TESTMUTEX idioms used elsewhere in
    /// this module. A mutex poisoned by one of the #[should_panic] tests is
    /// recovered rather than cascading a PoisonError into every later test.
    fn c2_test_guard() -> MutexGuard<'static, bool> {
        loop {
            match TESTMUTEX.lock() {
                Ok(g) => return g,
                Err(_) => TESTMUTEX.clear_poison(),
            }
        }
    }

    /// Must be called *after* refresh(): refresh() clears
    /// CLOSEHANDLERTABLE, so a one-time (e.g. std::sync::Once) handler
    /// registration would silently lose the handlers on the next test that
    /// calls refresh().
    fn c2_setup() {
        register_close_handlers(C2_FDKIND, c2_intermediate_close, c2_last_close);
        C2_LAST.store(0, Ordering::SeqCst);
        C2_MID.store(0, Ordering::SeqCst);
        C2_HANDLER_ERRS.store(0, Ordering::SeqCst);
        C2_RELEASED.lock().unwrap().clear();
    }

    fn c2_iters() -> usize {
        std::env::var("LIND_CONC002_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(200)
    }

    #[derive(Default)]
    struct C2Errs {
        alloc: AtomicU64,
        xlate: AtomicU64,
        dup_vfd: AtomicU64,
        close: AtomicU64,
        copy: AtomicU64,
        copied_entries: AtomicU64,
        allocs: AtomicU64,
    }

    #[test]
    /// Mixes allocation, translation, fd-table copying, close, and cage
    /// removal. Each of the C2_WORKERS threads owns one cage end-to-end
    /// (Tests 3 and 4 below cover two cages racing on the same underfd or
    /// fd-table copy); this is the "does the whole lifecycle hold together
    /// under concurrency" smoke test.
    fn conc_002_fd_lifecycle_multicage_stress() {
        let _lock = c2_test_guard();
        refresh();
        c2_setup();

        let iters = c2_iters();
        // Each worker retains 1 of every 4 allocated fds; FD_PER_PROCESS_MAX
        // is 1024, so keep the retained set well under that.
        assert!(
            iters <= 900,
            "LIND_CONC002_ITERS too large for FD_PER_PROCESS_MAX"
        );

        for w in 0..C2_WORKERS as u64 {
            init_empty_cage(C2_BASE + w);
        }

        let errs: Arc<Vec<C2Errs>> = Arc::new((0..C2_WORKERS).map(|_| C2Errs::default()).collect());
        let start = Arc::new(Barrier::new(C2_WORKERS));

        let handles: Vec<_> = (0..C2_WORKERS)
            .map(|w| {
                let errs = Arc::clone(&errs);
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    let cage = C2_BASE + w as u64;
                    let child = C2_CHILD + w as u64;
                    let e = &errs[w];
                    let mut live: Vec<(u64, u64)> = Vec::new();

                    start.wait();
                    for i in 0..iters {
                        // --- allocation: 4 fds, all with globally unique underfds
                        let mut batch: Vec<(u64, u64)> = Vec::with_capacity(4);
                        let mut seen: HashSet<u64> = HashSet::new();
                        for _ in 0..4 {
                            let u = c2_next_underfd();
                            match get_unused_virtual_fd(cage, C2_FDKIND, u, i % 2 == 0, u) {
                                Ok(vfd) => {
                                    if !seen.insert(vfd) || vfd >= FD_PER_PROCESS_MAX {
                                        e.dup_vfd.fetch_add(1, Ordering::SeqCst);
                                    }
                                    e.allocs.fetch_add(1, Ordering::SeqCst);
                                    batch.push((vfd, u));
                                }
                                Err(_) => {
                                    e.alloc.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }

                        // --- translation: exact round-trip of everything installed
                        for &(vfd, u) in &batch {
                            match translate_virtual_fd(cage, vfd) {
                                Ok(ent) => {
                                    if ent.fdkind != C2_FDKIND
                                        || ent.underfd != u
                                        || ent.perfdinfo != u
                                        || ent.should_cloexec != (i % 2 == 0)
                                    {
                                        e.xlate.fetch_add(1, Ordering::SeqCst);
                                    }
                                }
                                Err(_) => {
                                    e.xlate.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }

                        // --- attribute mutation, on an fd no other thread can see
                        if let Some(&(vfd, u)) = batch.first() {
                            let _ = set_cloexec(cage, vfd, true);
                            let _ = set_perfdinfo(cage, vfd, u ^ 0xff);
                            let ok = translate_virtual_fd(cage, vfd)
                                .map(|x| x.should_cloexec && x.perfdinfo == (u ^ 0xff))
                                .unwrap_or(false);
                            if !ok {
                                e.xlate.fetch_add(1, Ordering::SeqCst);
                            }
                        }

                        // --- close 3 of 4, retain 1
                        for &(vfd, _) in batch.iter().skip(1) {
                            if close_virtualfd(cage, vfd).is_err() {
                                e.close.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        if let Some(&first) = batch.first() {
                            live.push(first);
                        }

                        // --- fd-table copy + cage removal, every 16th iteration
                        if i % 16 == 0 {
                            let src = return_fdtable_copy(cage);
                            if copy_fdtable_for_cage(cage, child).is_err() {
                                e.copy.fetch_add(1, Ordering::SeqCst);
                            } else {
                                let dst = return_fdtable_copy(child);
                                if dst != src {
                                    e.copy.fetch_add(1, Ordering::SeqCst);
                                }
                                for (&vfd, ent) in &src {
                                    if translate_virtual_fd(child, vfd).as_ref() != Ok(ent) {
                                        e.copy.fetch_add(1, Ordering::SeqCst);
                                    }
                                }
                                e.copied_entries
                                    .fetch_add(src.len() as u64, Ordering::SeqCst);
                                remove_cage_from_fdtable(child);
                                if check_cage_exists(child) {
                                    e.copy.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }
                    }

                    // Teardown: half the workers drain explicitly, half leave
                    // their fds for remove_cage_from_fdtable. Both paths must
                    // release every underfd exactly once.
                    if w % 2 == 0 {
                        for &(vfd, _) in &live {
                            if close_virtualfd(cage, vfd).is_err() {
                                e.close.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                    remove_cage_from_fdtable(cage);
                })
            })
            .collect();

        for h in handles {
            h.join().expect("a CONC-002 worker panicked");
        }

        for (w, e) in errs.iter().enumerate() {
            assert_eq!(
                e.alloc.load(Ordering::SeqCst),
                0,
                "worker {w}: get_unused_virtual_fd failed"
            );
            assert_eq!(
                e.xlate.load(Ordering::SeqCst),
                0,
                "worker {w}: translate returned the wrong entry"
            );
            assert_eq!(
                e.dup_vfd.load(Ordering::SeqCst),
                0,
                "worker {w}: concurrent allocation handed out a duplicate/OOB vfd"
            );
            assert_eq!(
                e.close.load(Ordering::SeqCst),
                0,
                "worker {w}: close_virtualfd failed"
            );
            assert_eq!(
                e.copy.load(Ordering::SeqCst),
                0,
                "worker {w}: fd-table copy was not an exact snapshot"
            );
        }

        let total_allocs: u64 = errs.iter().map(|e| e.allocs.load(Ordering::SeqCst)).sum();
        let total_copied: u64 = errs
            .iter()
            .map(|e| e.copied_entries.load(Ordering::SeqCst))
            .sum();

        {
            let released = C2_RELEASED.lock().unwrap();
            assert_eq!(
                released.len() as u64,
                total_allocs,
                "not every allocated underfd fired exactly one last-close"
            );
            for (u, n) in released.iter() {
                assert_eq!(
                    *n, 1,
                    "underfd {u:#x} was released {n} times (expected exactly 1)"
                );
            }
        }
        assert_eq!(C2_LAST.load(Ordering::SeqCst), total_allocs);
        // Every entry present at copy time is released twice: once when the
        // child cage is removed (intermediate, refcount 2->1) and once at
        // final teardown (last, 1->0).
        assert_eq!(
            C2_MID.load(Ordering::SeqCst),
            total_copied,
            "intermediate-close count does not match the number of copied entries"
        );
        assert_eq!(
            C2_HANDLER_ERRS.load(Ordering::SeqCst),
            0,
            "a close handler saw the wrong fdkind or refcount polarity"
        );

        for w in 0..C2_WORKERS as u64 {
            assert!(!check_cage_exists(C2_BASE + w));
            assert!(!check_cage_exists(C2_CHILD + w));
        }
        refresh();
    }

    #[test]
    /// A stable cage with a known fd-table, plus victim cages the main
    /// thread creates and destroys underneath 4 reader threads. Readers
    /// translate only in the stable cage and observe victims exclusively
    /// through check_cage_exists(), which does not separately assert-then-
    /// unwrap a FDTABLE.get() (Test 4 drives the analogous race in
    /// copy_fdtable_for_cage specifically).
    fn conc_002_translate_isolation_under_cage_removal() {
        let _lock = c2_test_guard();
        refresh();
        c2_setup();

        const C2_READERS: usize = 4;
        const VICTIM_ITERS: usize = 400;
        const SPIN_BUDGET: u64 = 2_000_000;

        init_empty_cage(C2_STABLE);
        let stable: Vec<(u64, u64)> = (0..16)
            .map(|_| {
                let u = c2_next_underfd();
                (
                    get_unused_virtual_fd(C2_STABLE, C2_FDKIND, u, false, u).unwrap(),
                    u,
                )
            })
            .collect();
        let snapshot = return_fdtable_copy(C2_STABLE);

        #[derive(Default)]
        struct ReaderErrs {
            bad_entry: AtomicU64,
            spin_exhausted: AtomicU64,
            saw_live: AtomicU64,
            saw_gone: AtomicU64,
        }

        let errs: Arc<Vec<ReaderErrs>> =
            Arc::new((0..C2_READERS).map(|_| ReaderErrs::default()).collect());
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let victim_cycle = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..C2_READERS)
            .map(|r| {
                let errs = Arc::clone(&errs);
                let running = Arc::clone(&running);
                let victim_cycle = Arc::clone(&victim_cycle);
                let stable = stable.clone();
                thread::spawn(move || {
                    let e = &errs[r];
                    let mut last_cycle = u64::MAX;
                    let mut budget = SPIN_BUDGET;
                    while running.load(Ordering::SeqCst) {
                        for &(vfd, u) in &stable {
                            match translate_virtual_fd(C2_STABLE, vfd) {
                                Ok(ent)
                                    if ent.fdkind == C2_FDKIND
                                        && ent.underfd == u
                                        && ent.perfdinfo == u => {}
                                _ => {
                                    e.bad_entry.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                        }

                        let victim = C2_VICTIM + (r as u64 % 4);
                        if check_cage_exists(victim) {
                            e.saw_live.fetch_add(1, Ordering::SeqCst);
                        } else {
                            e.saw_gone.fetch_add(1, Ordering::SeqCst);
                        }

                        let cur = victim_cycle.load(Ordering::SeqCst);
                        if cur != last_cycle {
                            last_cycle = cur;
                            budget = SPIN_BUDGET;
                        } else if budget == 0 {
                            e.spin_exhausted.fetch_add(1, Ordering::SeqCst);
                            budget = SPIN_BUDGET; // don't spam if genuinely stalled
                        } else {
                            budget -= 1;
                        }
                        thread::yield_now();
                    }
                })
            })
            .collect();

        for i in 0..VICTIM_ITERS {
            let victim = C2_VICTIM + (i as u64 % 4);
            init_empty_cage(victim);
            for _ in 0..3 {
                let u = c2_next_underfd();
                get_unused_virtual_fd(victim, C2_FDKIND, u, false, u).unwrap();
            }
            remove_cage_from_fdtable(victim);
            victim_cycle.fetch_add(1, Ordering::SeqCst);
        }

        running.store(false, Ordering::SeqCst);
        for h in handles {
            h.join().expect("a CONC-002 reader panicked");
        }

        let mut total_live = 0u64;
        let mut total_gone = 0u64;
        for (r, e) in errs.iter().enumerate() {
            assert_eq!(
                e.bad_entry.load(Ordering::SeqCst),
                0,
                "reader {r}: saw a wrong entry in the stable cage"
            );
            assert_eq!(
                e.spin_exhausted.load(Ordering::SeqCst),
                0,
                "reader {r}: exhausted its spin budget waiting for a victim-cycle change"
            );
            total_live += e.saw_live.load(Ordering::SeqCst);
            total_gone += e.saw_gone.load(Ordering::SeqCst);
        }
        assert!(
            total_live > 0,
            "no reader ever observed a victim cage while it existed"
        );
        assert!(
            total_gone > 0,
            "no reader ever observed a victim cage after removal"
        );

        assert_eq!(return_fdtable_copy(C2_STABLE), snapshot);
        remove_cage_from_fdtable(C2_STABLE);

        {
            let released = C2_RELEASED.lock().unwrap();
            for (u, n) in released.iter() {
                assert_eq!(
                    *n, 1,
                    "underfd {u:#x} was released {n} times (expected exactly 1)"
                );
            }
        }
        assert_eq!(C2_HANDLER_ERRS.load(Ordering::SeqCst), 0);
        refresh();
    }

    #[test]
    /// Pins _increment_fdcount's non-atomic read-modify-write:
    /// it is a get_mut()/else-insert() pair that releases the shard lock
    /// between the two, so two cages first-referencing the same
    /// (fdkind,underfd) concurrently can both insert(1) and undercount.
    ///
    /// Not reachable between two threads in the *same* cage --
    /// get_unused_virtual_fd holds the row guard across the scan and the
    /// increment, so every worker below lives in its own cage, and all
    /// workers race on ONE shared underfd per round, deliberately unlike
    /// every other test in this file.
    ///
    /// Ignored rather than merely failing: the undercount makes a worker
    /// panic inside close_virtualfd, and the surviving workers then block
    /// forever on the round barrier, so on an unfixed tree this DEADLOCKS
    /// the test binary rather than reporting a failure.
    #[ignore = "deadlocks until _increment_fdcount's read-modify-write is made atomic"]
    fn conc_002_shared_underfd_refcount_race() {
        let _lock = c2_test_guard();
        refresh();
        c2_setup();

        const ROUNDS: usize = 500;

        for w in 0..C2_WORKERS as u64 {
            init_empty_cage(C2_BASE + w);
        }

        let alloc_errs = Arc::new(AtomicU64::new(0));
        let close_errs = Arc::new(AtomicU64::new(0));
        let start = Arc::new(Barrier::new(C2_WORKERS));
        let allocated = Arc::new(Barrier::new(C2_WORKERS));

        let handles: Vec<_> = (0..C2_WORKERS)
            .map(|w| {
                let alloc_errs = Arc::clone(&alloc_errs);
                let close_errs = Arc::clone(&close_errs);
                let start = Arc::clone(&start);
                let allocated = Arc::clone(&allocated);
                thread::spawn(move || {
                    let cage = C2_BASE + w as u64;
                    for round in 0..ROUNDS {
                        // Deliberately the SAME underfd across all
                        // C2_WORKERS threads this round: the exact race
                        // window _increment_fdcount's fix closes. Distinct
                        // across rounds so C2_RELEASED can count "one
                        // last-close per round".
                        let shared_underfd = 0x1000_0000u64 + round as u64;
                        start.wait();
                        let vfd = get_unused_virtual_fd(
                            cage,
                            C2_FDKIND,
                            shared_underfd,
                            false,
                            shared_underfd,
                        );
                        allocated.wait();
                        match vfd {
                            Ok(vfd) => {
                                if close_virtualfd(cage, vfd).is_err() {
                                    close_errs.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                            Err(_) => {
                                alloc_errs.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("a CONC-002 worker panicked");
        }

        assert_eq!(alloc_errs.load(Ordering::SeqCst), 0);
        assert_eq!(close_errs.load(Ordering::SeqCst), 0);

        {
            let released = C2_RELEASED.lock().unwrap();
            assert_eq!(
                released.len(),
                ROUNDS,
                "expected exactly one distinct shared underfd per round to be released"
            );
            for (u, n) in released.iter() {
                assert_eq!(
                    *n, 1,
                    "underfd {u:#x} was released {n} times (expected exactly 1; \
                     a refcount race would show 0 or >1)"
                );
            }
        }
        assert_eq!(C2_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        for w in 0..C2_WORKERS as u64 {
            remove_cage_from_fdtable(C2_BASE + w);
        }
        for w in 0..C2_WORKERS as u64 {
            assert!(!check_cage_exists(C2_BASE + w));
        }
        refresh();
    }

    #[test]
    /// Pins copy_fdtable_for_cage reading the source row twice,
    /// once for the snapshot and once for the refcount increments: a
    /// concurrent close/allocate landing between the two desyncs the child's
    /// refcounts from its fd-table contents.
    ///
    /// In-crate mirror of what
    /// tests/unit-tests/process_tests/deterministic/conc_002_cage_fd_fs_stress.c
    /// drives from the C side: fork() interleaved with concurrent fd-table
    /// churn in the forking cage.
    ///
    /// Ignored rather than merely failing: the desync panics a worker and the
    /// rest block forever on the round barrier, so on an unfixed tree this
    /// DEADLOCKS the test binary rather than reporting a failure.
    #[ignore = "deadlocks until copy_fdtable_for_cage snapshots and increments under one guard"]
    fn conc_002_copy_fdtable_vs_concurrent_churn() {
        let _lock = c2_test_guard();
        refresh();
        c2_setup();

        const CHURNERS: usize = 4;
        const COPIES: usize = 300;
        // Bounded independently of `running`: if copy_fdtable_for_cage ever
        // desyncs a child's refcounts from its contents (the bug this test
        // pins), later bookkeeping calls degrade toward a full linear scan
        // of FDCOUNT (see _decrement_fdcount's panic-message path), and 4
        // churner threads hammering the table in a tight, unyielding loop
        // can starve that scan indefinitely under parking_lot's fair-ish
        // shard locks. A hard cap plus a periodic yield keeps this test's
        // own failure mode a fast, clean assertion instead of a livelock.
        const CHURN_MAX: u64 = 200_000;

        let src_cage = C2_BASE;
        let child_cage = C2_CHILD;
        init_empty_cage(src_cage);

        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let churn_errs = Arc::new(AtomicU64::new(0));
        let churn_allocs = Arc::new(AtomicU64::new(0));

        let handles: Vec<_> = (0..CHURNERS)
            .map(|_| {
                let running = Arc::clone(&running);
                let churn_errs = Arc::clone(&churn_errs);
                let churn_allocs = Arc::clone(&churn_allocs);
                thread::spawn(move || {
                    let mut n = 0u64;
                    while running.load(Ordering::SeqCst) && n < CHURN_MAX {
                        let u = c2_next_underfd();
                        // A short-lived fd: opened and immediately closed,
                        // so it is likely to straddle a concurrent copy's
                        // two source reads.
                        match get_unused_virtual_fd(src_cage, C2_FDKIND, u, false, u) {
                            Ok(vfd) => {
                                churn_allocs.fetch_add(1, Ordering::SeqCst);
                                if close_virtualfd(src_cage, vfd).is_err() {
                                    churn_errs.fetch_add(1, Ordering::SeqCst);
                                }
                            }
                            Err(_) => {
                                churn_errs.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                        n += 1;
                        if n % 64 == 0 {
                            thread::yield_now();
                        }
                    }
                })
            })
            .collect();

        let mut copy_errs = 0u64;
        for _ in 0..COPIES {
            if copy_fdtable_for_cage(src_cage, child_cage).is_err() {
                copy_errs += 1;
                continue;
            }
            let child_snapshot = return_fdtable_copy(child_cage);
            // Self-consistency only, not compared against a pre-copy
            // snapshot of src_cage: the churner threads keep mutating it
            // throughout. The real oracle is the exactly-once release
            // accounting below: an under-incremented entry either panics in
            // _decrement_fdcount or shows up as a leaked/double-released fd.
            for (&vfd, ent) in &child_snapshot {
                if translate_virtual_fd(child_cage, vfd).as_ref() != Ok(ent) {
                    copy_errs += 1;
                }
            }
            remove_cage_from_fdtable(child_cage);
        }

        running.store(false, Ordering::SeqCst);
        for h in handles {
            h.join().expect("a CONC-002 churner panicked");
        }

        assert_eq!(churn_errs.load(Ordering::SeqCst), 0);
        assert_eq!(
            copy_errs, 0,
            "copy_fdtable_for_cage produced a child with an internally \
             inconsistent entry (translate_virtual_fd disagreed with \
             return_fdtable_copy)"
        );

        remove_cage_from_fdtable(src_cage);

        {
            let released = C2_RELEASED.lock().unwrap();
            assert_eq!(
                released.len() as u64,
                churn_allocs.load(Ordering::SeqCst),
                "not every underfd the churners allocated fired exactly one \
                 last-close; a leak or a double-release would show up here"
            );
            for (u, n) in released.iter() {
                assert_eq!(
                    *n, 1,
                    "underfd {u:#x} was released {n} times (expected exactly 1)"
                );
            }
        }
        assert_eq!(C2_HANDLER_ERRS.load(Ordering::SeqCst), 0);
        refresh();
    }

    #[test]
    /// Pins an off-by-one in get_specific_virtual_fd: it bounds
    /// with `> FD_PER_PROCESS_MAX` instead of `>=`, so requested_virtualfd ==
    /// FD_PER_PROCESS_MAX passes the check and indexes the backing table one
    /// past its end. That is a host panic a guest can trigger directly with
    /// dup2(x, FD_PER_PROCESS_MAX), where EBADF is the correct answer.
    ///
    /// On an unfixed tree this test panics on the out-of-bounds index rather
    /// than failing its assertion, which under --test-threads=1 can leave the
    /// global tables dirty for whatever runs next.
    #[ignore = "panics on an out-of-bounds index until the FD_PER_PROCESS_MAX bound is >= rather than >"]
    fn get_specific_virtual_fd_rejects_fd_at_max() {
        let _lock = c2_test_guard();
        refresh();

        let cage = C2_STABLE;
        init_empty_cage(cage);
        let result = get_specific_virtual_fd(cage, FD_PER_PROCESS_MAX, C2_FDKIND, 0, false, 0);
        assert_eq!(result, Err(threei::Errno::EBADF as u64));
        remove_cage_from_fdtable(cage);
        refresh();
    }

    // =====================================================================
    // CONC-003: cage-table and fd-refcount operations.
    //
    // Oracle: all interleavings preserve the fdtables refcount invariants:
    //   (i)   sum of live references to a (fdkind, underfd) == FDCOUNT[key]
    //   (ii)  the `last` close handler fires exactly once per key, and only
    //         once no cage holds a reference to it any longer
    //   (iii) the `intermediate` close handler fires once per non-final
    //         release
    // A decrement fires `last` iff it is the one that takes the count to 0,
    // and the count is a pure function of how many increments/decrements
    // have happened so far, not their order, so the totals asserted below
    // are exact equalities, not bounds, regardless of thread interleaving.
    //
    // In-crate mirror of
    // tests/unit-tests/process_tests/deterministic/conc_003_cage_fd_refcounts.c,
    // which drives the same invariant from the C/POSIX side via pipe EOF.
    //
    // conc_003_dup2_overwrite_refcount_conservation below also regression-
    // tests get_specific_virtual_fd's read/write-under-one-guard fix (see
    // dashmaparrayglobal.rs).
    // =====================================================================

    /// Reserved cage-id block for CONC-003. 0xC0C3 == "CONC-003". Disjoint
    /// from threei::TESTING_CAGEID0..15 and from every other block used in
    /// this module.
    const C3_A: u64 = 0x0000_0000_C0C3_0000; // primary cage
    const C3_CHILD: u64 = 0x0000_0000_C0C3_0010; // fork copies, +0..C3_CHILDREN
                                                 // (+8..8+C3_WORKERS reserved as scratch copy
                                                 // targets by Test 4)
    const C3_HOLDER: u64 = 0x0000_0000_C0C3_0030; // permanent reference holder (Test 4)
    const C3_WORKER: u64 = 0x0000_0000_C0C3_0040; // +0..C3_WORKERS

    const C3_CHILDREN: usize = 3;
    const C3_WORKERS: usize = 8;

    /// A dedicated fdkind plus a disjoint underfd window guarantees no
    /// FDCOUNT key ever aliases with another test's leftovers: refresh()
    /// clears FDTABLE and CLOSEHANDLERTABLE but never FDCOUNT, so refcount
    /// state leaks across every test in this binary.
    const C3_FDKIND: u32 = 0x7E57_0003;
    const C3_UNDERFD_BASE: u64 = 0x3000_0000;

    /// Hands out a globally unique underfd, so that no two allocations
    /// anywhere in this block ever share an FDCOUNT key unless a test
    /// deliberately wants that (Test 2 phase 2, Test 4).
    static C3_UNDERFD_SEQ: AtomicU64 = AtomicU64::new(0);
    fn c3_next_underfd() -> u64 {
        C3_UNDERFD_BASE + C3_UNDERFD_SEQ.fetch_add(1, Ordering::SeqCst)
    }

    static C3_LAST: AtomicU64 = AtomicU64::new(0);
    static C3_MID: AtomicU64 = AtomicU64::new(0);
    static C3_HANDLER_ERRS: AtomicU64 = AtomicU64::new(0);
    /// Set while a cage is known to hold a reference on the key(s) under
    /// test; a `last` close observed while this is true is an invariant
    /// violation recorded at the instant it happens, rather than inferred
    /// after the fact from a final tally (used by Test 4).
    static C3_HOLDER_ACTIVE: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    lazy_static! {
        /// underfd -> number of `last` closes seen. Every value must end at 1.
        static ref C3_RELEASED: Mutex<std::collections::HashMap<u64, u64>> =
            Mutex::new(std::collections::HashMap::new());
        /// underfd -> each `remaining` value reported by an `intermediate`
        /// close, in arrival order. Lets a test assert the exact multiset
        /// of remaining-counts observed, not just how many fired.
        static ref C3_INTERMEDIATE_REMAINING: Mutex<std::collections::HashMap<u64, Vec<u64>>> =
            Mutex::new(std::collections::HashMap::new());
    }

    // Close handlers are plain `fn` pointers and cannot capture, so all
    // bookkeeping lives in the statics above. They never panic: a panic
    // inside fdtables' own teardown path is nearly impossible to attribute,
    // so a violated expectation is recorded for the main thread to assert
    // on instead.
    fn c3_last_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C3_FDKIND || remaining != 0 || C3_HOLDER_ACTIVE.load(Ordering::SeqCst) {
            C3_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        *C3_RELEASED
            .lock()
            .unwrap()
            .entry(entry.underfd)
            .or_insert(0) += 1;
        C3_LAST.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn c3_intermediate_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C3_FDKIND || remaining == 0 {
            C3_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        C3_INTERMEDIATE_REMAINING
            .lock()
            .unwrap()
            .entry(entry.underfd)
            .or_default()
            .push(remaining);
        C3_MID.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Must be called *after* refresh(): refresh() clears
    /// CLOSEHANDLERTABLE, so a registration that ran only once would
    /// silently lose the handlers on the next test that calls refresh().
    fn c3_setup() {
        register_close_handlers(C3_FDKIND, c3_intermediate_close, c3_last_close);
        C3_LAST.store(0, Ordering::SeqCst);
        C3_MID.store(0, Ordering::SeqCst);
        C3_HANDLER_ERRS.store(0, Ordering::SeqCst);
        C3_HOLDER_ACTIVE.store(false, Ordering::SeqCst);
        C3_RELEASED.lock().unwrap().clear();
        C3_INTERMEDIATE_REMAINING.lock().unwrap().clear();
    }

    fn c3_iters() -> usize {
        std::env::var("LIND_CONC003_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50)
    }

    #[test]
    /// The roadmap's CONC-003 test. Per iteration: a cage with 4 virtual
    /// fds aliasing one underfd is copied into 3 child cages (count 16),
    /// then 15 of those 16 references are dropped through three different
    /// decrement paths (explicit close, cage removal, and explicit-close-
    /// then-removal) running concurrently, while the primary cage's one
    /// retained reference must survive untouched: no `last` close, and
    /// translate_virtual_fd must keep working.
    fn conc_003_refcount_conservation_across_cages() {
        let _lock = c2_test_guard();
        refresh();
        c3_setup();

        for _ in 0..c3_iters() {
            let u = c3_next_underfd();

            init_empty_cage(C3_A);
            let mut v = [0u64; 4];
            for slot in &mut v {
                *slot = get_unused_virtual_fd(C3_A, C3_FDKIND, u, false, u).unwrap();
            }
            // Count 4.

            for c in 0..C3_CHILDREN as u64 {
                copy_fdtable_for_cage(C3_A, C3_CHILD + c).unwrap();
            }
            // Count 16.

            let start = Arc::new(Barrier::new(4));
            let handles: Vec<_> = (0..4u64)
                .map(|t| {
                    let start = Arc::clone(&start);
                    thread::spawn(move || {
                        start.wait();
                        match t {
                            0 => {
                                // C3_A keeps v[0]; drop its other 3 references.
                                for &vfd in &v[1..] {
                                    close_virtualfd(C3_A, vfd).unwrap();
                                }
                            }
                            1 => remove_cage_from_fdtable(C3_CHILD),
                            2 => remove_cage_from_fdtable(C3_CHILD + 1),
                            _ => {
                                // Explicit-close path, rather than cage
                                // teardown, for the third child: races
                                // the other two decrement paths against
                                // this one.
                                for &vfd in &v {
                                    close_virtualfd(C3_CHILD + 2, vfd).unwrap();
                                }
                                remove_cage_from_fdtable(C3_CHILD + 2);
                            }
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }

            // 15 of the 16 references are gone; C3_A's v[0] is the last one.
            assert_eq!(
                C3_LAST.load(Ordering::SeqCst),
                0,
                "last-close fired while C3_A still holds a reference"
            );
            assert_eq!(C3_MID.load(Ordering::SeqCst), 15);
            assert!(!C3_RELEASED.lock().unwrap().contains_key(&u));
            let entry = translate_virtual_fd(C3_A, v[0]).unwrap();
            assert_eq!((entry.fdkind, entry.underfd), (C3_FDKIND, u));
            for c in 0..C3_CHILDREN as u64 {
                assert!(!check_cage_exists(C3_CHILD + c));
            }
            {
                let mut remaining: Vec<u64> = C3_INTERMEDIATE_REMAINING
                    .lock()
                    .unwrap()
                    .get(&u)
                    .cloned()
                    .unwrap_or_default();
                remaining.sort_unstable_by(|a, b| b.cmp(a));
                assert_eq!(remaining, (1..=15).rev().collect::<Vec<u64>>());
            }

            close_virtualfd(C3_A, v[0]).unwrap();
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 1);
            assert_eq!(C3_MID.load(Ordering::SeqCst), 15);
            assert_eq!(*C3_RELEASED.lock().unwrap().get(&u).unwrap(), 1);

            // Key-removal oracle, portable across all four backends since
            // it never touches a private map directly: _increment_fdcount
            // is `or_insert(0) += 1` and _decrement_fdcount removes the key
            // on reaching 0, so a 0-valued FDCOUNT entry is unrepresentable.
            // If the (C3_FDKIND, u) entry had NOT actually been removed
            // above, it would have to be sitting at some count >= 1, and a
            // fresh allocation on the same key followed by a close would
            // drive it from that count down by one and fire `intermediate`,
            // not `last`. So re-allocating and closing here must produce
            // exactly one more `last` and leave C3_MID unchanged.
            let v2 = get_unused_virtual_fd(C3_A, C3_FDKIND, u, false, u).unwrap();
            close_virtualfd(C3_A, v2).unwrap();
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 2);
            assert_eq!(
                C3_MID.load(Ordering::SeqCst),
                15,
                "a stale FDCOUNT entry for {u:#x} survived the previous last-close"
            );
            assert_eq!(*C3_RELEASED.lock().unwrap().get(&u).unwrap(), 2);

            remove_cage_from_fdtable(C3_A);
            assert!(!check_cage_exists(C3_A));

            c3_setup();
        }

        refresh();
    }

    #[test]
    /// Pins get_specific_virtual_fd's read/write TOCTOU: the old
    /// slot value is read under one DashMap guard and the new one written
    /// under a second, so two dup2()s racing onto the same target can both
    /// decrement the same old entry, or one call's write can clobber a
    /// concurrent get_unused_virtual_fd()'s insert.
    ///
    /// Ignored rather than merely failing: the double-decrement panics a
    /// worker and the rest block forever on the round barrier, so on an
    /// unfixed tree this DEADLOCKS the test binary.
    #[ignore = "deadlocks until get_specific_virtual_fd reads and writes the target slot under one guard"]
    fn conc_003_dup2_overwrite_refcount_conservation() {
        let _lock = c2_test_guard();
        refresh();
        c3_setup();

        // --- Phase 1: self-dup2 (deterministic, single-threaded). POSIX's
        // dup2(fd, fd) is a no-op at the syscall layer, but the underlying
        // fdtables primitive still runs a full get_specific_virtual_fd onto
        // its own slot: this pins that primitive's self-overwrite path,
        // which must fire `intermediate`, never `last`.
        {
            init_empty_cage(C3_A);
            let u = c3_next_underfd();
            let v = get_unused_virtual_fd(C3_A, C3_FDKIND, u, false, u).unwrap();

            get_specific_virtual_fd(C3_A, v, C3_FDKIND, u, true, u ^ 0xff).unwrap();

            assert_eq!(C3_MID.load(Ordering::SeqCst), 1);
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 0);
            assert_eq!(
                C3_INTERMEDIATE_REMAINING.lock().unwrap().get(&u).cloned(),
                Some(vec![1])
            );
            let entry = translate_virtual_fd(C3_A, v).unwrap();
            assert!(entry.should_cloexec);
            assert_eq!(entry.perfdinfo, u ^ 0xff);

            close_virtualfd(C3_A, v).unwrap();
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 1);
            assert_eq!(*C3_RELEASED.lock().unwrap().get(&u).unwrap(), 1);

            remove_cage_from_fdtable(C3_A);
            c3_setup();
        }

        // --- Phase 2: concurrent overwrite conservation. C3_WORKERS
        // threads race to overwrite the SAME target virtual-fd slot with
        // fresh underfds, one after another, while a separate churner
        // thread hammers unrelated slots in the same cage. Every underfd
        // that is ever installed at the target slot has exactly one live
        // reference to it at any moment (nothing else ever aliases a fresh
        // underfd), so its eventual overwrite/close is always a `last`
        // close: the oracle is simply "every underfd installed is released
        // exactly once, and nothing panics"; a panic here is the loud
        // failure mode of the get_specific_virtual_fd fix (FDCOUNT
        // underflow).
        {
            const DUP_ROUNDS: usize = 300;

            init_empty_cage(C3_A);
            let u0 = c3_next_underfd();
            let target = get_unused_virtual_fd(C3_A, C3_FDKIND, u0, false, u0).unwrap();

            let start = Arc::new(Barrier::new(C3_WORKERS + 2));

            let handles: Vec<_> = (0..C3_WORKERS)
                .map(|_| {
                    let start = Arc::clone(&start);
                    thread::spawn(move || {
                        start.wait();
                        let mut installed = Vec::with_capacity(DUP_ROUNDS);
                        for _ in 0..DUP_ROUNDS {
                            let nu = c3_next_underfd();
                            get_specific_virtual_fd(C3_A, target, C3_FDKIND, nu, false, nu)
                                .unwrap();
                            installed.push(nu);
                        }
                        installed
                    })
                })
                .collect();

            let churner = {
                let start = Arc::clone(&start);
                thread::spawn(move || {
                    start.wait();
                    let mut installed = Vec::with_capacity(DUP_ROUNDS);
                    for _ in 0..DUP_ROUNDS {
                        let u = c3_next_underfd();
                        let vfd = get_unused_virtual_fd(C3_A, C3_FDKIND, u, false, u).unwrap();
                        close_virtualfd(C3_A, vfd).unwrap();
                        installed.push(u);
                    }
                    installed
                })
            };

            start.wait();

            let mut expected: std::collections::HashSet<u64> = std::collections::HashSet::new();
            expected.insert(u0);
            for h in handles {
                let installed = h.join().expect("a CONC-003 dup2-overwrite worker panicked");
                expected.extend(installed);
            }
            let churn_installed = churner
                .join()
                .expect("the CONC-003 dup2-overwrite churner panicked");
            expected.extend(churn_installed);

            // Final occupant of `target` still needs its own explicit close.
            let final_entry = translate_virtual_fd(C3_A, target).unwrap();
            assert_eq!(final_entry.fdkind, C3_FDKIND);
            close_virtualfd(C3_A, target).unwrap();

            assert_eq!(C3_HANDLER_ERRS.load(Ordering::SeqCst), 0);
            {
                let released = C3_RELEASED.lock().unwrap();
                assert_eq!(
                    released.len(),
                    expected.len(),
                    "not every underfd installed during the race was released exactly once"
                );
                for u in &expected {
                    assert_eq!(
                        released.get(u).copied(),
                        Some(1),
                        "underfd {u:#x} was released a number of times other than 1"
                    );
                }
            }

            remove_cage_from_fdtable(C3_A);
            c3_setup();
        }

        refresh();
    }

    #[test]
    /// Refcount conservation for the exec (empty_fds_for_exec, drops only
    /// cloexec entries) and cage-exit (remove_cage_from_fdtable, drops
    /// everything) decrement paths, racing against each other and against
    /// copy_fdtable_for_cage.
    fn conc_003_exec_and_exit_refcount_conservation() {
        let _lock = c2_test_guard();
        refresh();
        c3_setup();

        for _ in 0..c3_iters() {
            // --- Sub-phase 1: does the COUNT conserve across a 3-way race
            // between two execs and one cage removal, all decrementing the
            // SAME shared underfd?
            let u = c3_next_underfd();
            init_empty_cage(C3_A);
            let mut v = [0u64; 6];
            for (i, slot) in v.iter_mut().enumerate() {
                *slot = get_unused_virtual_fd(C3_A, C3_FDKIND, u, i % 2 == 0, u).unwrap();
            }
            // Count 6 (3 cloexec, 3 not).

            copy_fdtable_for_cage(C3_A, C3_CHILD).unwrap();
            copy_fdtable_for_cage(C3_A, C3_CHILD + 1).unwrap();
            // Count 18.

            let start = Arc::new(Barrier::new(3));
            let handles: Vec<_> = (0..3u64)
                .map(|t| {
                    let start = Arc::clone(&start);
                    thread::spawn(move || {
                        start.wait();
                        match t {
                            0 => empty_fds_for_exec(C3_A),               // drops 3 cloexec
                            1 => empty_fds_for_exec(C3_CHILD),           // drops 3 cloexec
                            _ => remove_cage_from_fdtable(C3_CHILD + 1), // drops all 6
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().unwrap();
            }
            // 18 - 3 - 3 - 6 = 6 left: the 3 non-cloexec survivors in each
            // of C3_A and C3_CHILD. None of these 12 decrements reached 0.
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 0);
            assert_eq!(C3_MID.load(Ordering::SeqCst), 12);
            assert_eq!(C3_HANDLER_ERRS.load(Ordering::SeqCst), 0);
            for cage in [C3_A, C3_CHILD] {
                let survivors = return_fdtable_copy(cage);
                assert_eq!(survivors.len(), 3);
                for ent in survivors.values() {
                    assert!(!ent.should_cloexec);
                    assert_eq!(ent.underfd, u);
                }
            }
            assert!(!check_cage_exists(C3_CHILD + 1));

            // Remove the two survivors concurrently: the 6th and final
            // decrement, whichever thread performs it, is the sole `last`.
            let start2 = Arc::new(Barrier::new(2));
            let h_a = {
                let start2 = Arc::clone(&start2);
                thread::spawn(move || {
                    start2.wait();
                    remove_cage_from_fdtable(C3_A);
                })
            };
            let h_c = {
                let start2 = Arc::clone(&start2);
                thread::spawn(move || {
                    start2.wait();
                    remove_cage_from_fdtable(C3_CHILD);
                })
            };
            h_a.join().unwrap();
            h_c.join().unwrap();

            assert_eq!(C3_MID.load(Ordering::SeqCst), 17);
            assert_eq!(C3_LAST.load(Ordering::SeqCst), 1);
            assert_eq!(*C3_RELEASED.lock().unwrap().get(&u).unwrap(), 1);
            {
                let mut remaining: Vec<u64> = C3_INTERMEDIATE_REMAINING
                    .lock()
                    .unwrap()
                    .get(&u)
                    .cloned()
                    .unwrap_or_default();
                remaining.sort_unstable_by(|a, b| b.cmp(a));
                assert_eq!(remaining, (1..=17).rev().collect::<Vec<u64>>());
            }
            assert!(!check_cage_exists(C3_A));
            assert!(!check_cage_exists(C3_CHILD));

            c3_setup();

            // --- Sub-phase 2: does the RIGHT set of entries get dropped,
            // not just the right count? Two disjoint underfds: cloexec
            // entries all on u_cx, non-cloexec all on u_keep, so a bug
            // that drops (or spares) the wrong kind shows up as an early or
            // missing release on the wrong key, not just a miscount.
            let u_cx = c3_next_underfd();
            let u_keep = c3_next_underfd();
            init_empty_cage(C3_A);
            for _ in 0..3 {
                get_unused_virtual_fd(C3_A, C3_FDKIND, u_cx, true, u_cx).unwrap();
                get_unused_virtual_fd(C3_A, C3_FDKIND, u_keep, false, u_keep).unwrap();
            }
            copy_fdtable_for_cage(C3_A, C3_CHILD).unwrap();
            // count(u_cx) = 6, count(u_keep) = 6.

            empty_fds_for_exec(C3_CHILD); // drops C3_CHILD's 3 cloexec (u_cx) entries
            assert!(!C3_RELEASED.lock().unwrap().contains_key(&u_cx));
            assert!(!C3_RELEASED.lock().unwrap().contains_key(&u_keep));
            for ent in return_fdtable_copy(C3_CHILD).values() {
                assert!(!ent.should_cloexec);
                assert_eq!(ent.underfd, u_keep);
            }

            let start3 = Arc::new(Barrier::new(2));
            let h_a = {
                let start3 = Arc::clone(&start3);
                thread::spawn(move || {
                    start3.wait();
                    remove_cage_from_fdtable(C3_A); // drops 3x u_cx + 3x u_keep
                })
            };
            let h_c = {
                let start3 = Arc::clone(&start3);
                thread::spawn(move || {
                    start3.wait();
                    remove_cage_from_fdtable(C3_CHILD); // drops remaining 3x u_keep
                })
            };
            h_a.join().unwrap();
            h_c.join().unwrap();

            assert_eq!(C3_HANDLER_ERRS.load(Ordering::SeqCst), 0);
            {
                let released = C3_RELEASED.lock().unwrap();
                assert_eq!(released.get(&u_cx).copied(), Some(1));
                assert_eq!(released.get(&u_keep).copied(), Some(1));
            }
            assert!(!check_cage_exists(C3_A));
            assert!(!check_cage_exists(C3_CHILD));

            c3_setup();
        }

        refresh();
    }

    #[test]
    /// Mirrors the C test's pipe-EOF oracle inside the crate: the `last`
    /// close handler must not fire for a key while ANY cage still holds a
    /// reference to it, regardless of how many other cages concurrently
    /// allocate and release references to that very same key. Distinct
    /// from conc_002_shared_underfd_refcount_race, which has every worker
    /// release its reference by the end of each round and so cannot
    /// express "no last close while a reference exists"; there is no
    /// permanent holder there to violate.
    fn conc_003_no_last_close_while_referenced() {
        let _lock = c2_test_guard();
        refresh();
        c3_setup();

        const ROUNDS: usize = 4000;

        let u = c3_next_underfd();
        init_empty_cage(C3_HOLDER);
        let holder_vfd = get_unused_virtual_fd(C3_HOLDER, C3_FDKIND, u, false, u).unwrap();
        C3_HOLDER_ACTIVE.store(true, Ordering::SeqCst);

        let mut worker_vfd = vec![0u64; C3_WORKERS];
        for w in 0..C3_WORKERS as u64 {
            init_empty_cage(C3_WORKER + w);
            worker_vfd[w as usize] =
                get_unused_virtual_fd(C3_WORKER + w, C3_FDKIND, u, false, u).unwrap();
        }
        // count = 1 (holder) + C3_WORKERS, and never drops below 1 (the
        // holder's own reference) for the rest of this test.

        let start = Arc::new(Barrier::new(C3_WORKERS));
        let handles: Vec<_> = (0..C3_WORKERS as u64)
            .map(|w| {
                let start = Arc::clone(&start);
                let mut vfd = worker_vfd[w as usize];
                thread::spawn(move || {
                    let cage = C3_WORKER + w;
                    start.wait();
                    for r in 0..ROUNDS {
                        if r % 32 == 0 {
                            // Fold copy_fdtable_for_cage/remove_cage_from_fdtable
                            // into the mix without touching this worker's own
                            // reference. C3_CHILD + 8 + w is disjoint from
                            // every id Tests 1-3 use.
                            let tmp = C3_CHILD + 8 + w;
                            copy_fdtable_for_cage(cage, tmp).unwrap();
                            remove_cage_from_fdtable(tmp);
                        } else {
                            close_virtualfd(cage, vfd).unwrap();
                            vfd = get_unused_virtual_fd(cage, C3_FDKIND, u, false, u).unwrap();
                        }
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(
            C3_LAST.load(Ordering::SeqCst),
            0,
            "last-close fired while C3_HOLDER still held a reference"
        );
        assert!(C3_RELEASED.lock().unwrap().is_empty());
        assert_eq!(C3_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        for w in 0..C3_WORKERS as u64 {
            remove_cage_from_fdtable(C3_WORKER + w);
        }
        C3_HOLDER_ACTIVE.store(false, Ordering::SeqCst);
        close_virtualfd(C3_HOLDER, holder_vfd).unwrap();

        assert_eq!(C3_LAST.load(Ordering::SeqCst), 1);
        assert_eq!(*C3_RELEASED.lock().unwrap().get(&u).unwrap(), 1);
        assert_eq!(C3_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        remove_cage_from_fdtable(C3_HOLDER);
        refresh();
    }

    // =====================================================================
    // CONC-004: refcount conservation under dup/close/fork.
    //
    // The narrowly-controlled counterpart to CONC-003: pins down ONE
    // lifecycle: allocate -> duplicate -> fork -> concurrent close ->
    // final close, swept across the full matrix of shapes, under a strict
    // ownership model where every reference is released exactly once by
    // exactly one owner. So every quantity asserted below is an exact
    // equality, not a bound.
    //
    // The primitives model rawposix's three POSIX duplication calls (see
    // src/rawposix/src/fs_calls.rs), which take different paths here:
    //
    //   DupKind::Dup     dup(): FRESH key at count 1 (host-delegated).
    //   DupKind::Dup2    dup2(): SOURCE's underfd, shared key, count += 1.
    //   DupKind::Fdupfd  fcntl(F_DUPFD): SOURCE's underfd, shared key like
    //                    Dup2, unlike Dup.
    //
    // Sweeping all three matters because an implementation that mixed up
    // fresh-vs-shared keys for one of them would still pass a test that
    // only exercised the others.
    //
    // In-crate mirror of
    // tests/unit-tests/process_tests/deterministic/conc_004_dup_close_fork_refcounts.c,
    // which drives the same lifecycle from the C/POSIX side via pipe EOF.
    // =====================================================================

    /// Reserved cage-id block for CONC-004. 0xC0C4 == "CONC-004". Disjoint
    /// from threei::TESTING_CAGEID0..15 and from every other block used in
    /// this module.
    const C4_A: u64 = 0x0000_0000_C0C4_0000; // primary (sentinel-holding) cage
    const C4_CHILD: u64 = 0x0000_0000_C0C4_0010; // fork copies, +0..C4_MAX_CHILD

    const C4_MAX_DUP: usize = 4;
    const C4_MAX_CHILD: usize = 3;
    const C4_MAX_THREAD: usize = 2;

    /// Virtual-fd slot base for the Dup2 path. get_specific_virtual_fd
    /// installs at a caller-chosen slot, so these must be slots nothing
    /// else in a round allocates: the sentinel and the Dup/Fdupfd
    /// duplicates all come from the low end of the table, and
    /// FD_PER_PROCESS_MAX is 1024.
    const C4_DUP2_SLOT: u64 = 100;
    /// Start-fd handed to get_unused_virtual_fd_from_startfd for the
    /// Fdupfd path; a nonzero start exercises the argument that
    /// distinguishes it from plain get_unused_virtual_fd.
    const C4_FDUPFD_START: u64 = 50;

    /// A dedicated fdkind plus a disjoint underfd window guarantees no
    /// FDCOUNT key ever aliases with another test's leftovers: refresh()
    /// clears FDTABLE and CLOSEHANDLERTABLE but never FDCOUNT, so refcount
    /// state leaks across every test in this binary.
    const C4_FDKIND: u32 = 0x7E57_0004;
    const C4_UNDERFD_BASE: u64 = 0x4000_0000;

    static C4_UNDERFD_SEQ: AtomicU64 = AtomicU64::new(0);
    fn c4_next_underfd() -> u64 {
        C4_UNDERFD_BASE + C4_UNDERFD_SEQ.fetch_add(1, Ordering::SeqCst)
    }

    static C4_LAST: AtomicU64 = AtomicU64::new(0);
    static C4_MID: AtomicU64 = AtomicU64::new(0);
    static C4_HANDLER_ERRS: AtomicU64 = AtomicU64::new(0);
    /// The underfd currently acting as the sentinel, and whether it is
    /// still held. A `last` close on THAT key while the flag is set is an
    /// invariant violation recorded at the instant it happens, rather than
    /// inferred after the fact from a final tally. It is keyed on the
    /// underfd because in the Dup rows a `last` close on a *fresh* key
    /// during the same window is expected and correct.
    static C4_SENTINEL_UNDERFD: AtomicU64 = AtomicU64::new(u64::MAX);
    static C4_SENTINEL_ACTIVE: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);
    lazy_static! {
        /// underfd -> number of `last` closes seen. Every key must end at 1.
        static ref C4_RELEASED: Mutex<std::collections::HashMap<u64, u64>> =
            Mutex::new(std::collections::HashMap::new());
        /// underfd -> each `remaining` value reported by an `intermediate`
        /// close, in arrival order.
        static ref C4_INTERMEDIATE_REMAINING: Mutex<std::collections::HashMap<u64, Vec<u64>>> =
            Mutex::new(std::collections::HashMap::new());
    }

    // Close handlers are plain `fn` pointers and cannot capture, so all
    // bookkeeping lives in the statics above. They never panic: a panic
    // inside fdtables' own teardown path is nearly impossible to attribute,
    // so a violated expectation is recorded for the main thread to assert
    // on instead.
    fn c4_last_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        let sentinel_violation = C4_SENTINEL_ACTIVE.load(Ordering::SeqCst)
            && entry.underfd == C4_SENTINEL_UNDERFD.load(Ordering::SeqCst);
        if entry.fdkind != C4_FDKIND || remaining != 0 || sentinel_violation {
            C4_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        *C4_RELEASED
            .lock()
            .unwrap()
            .entry(entry.underfd)
            .or_insert(0) += 1;
        C4_LAST.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn c4_intermediate_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C4_FDKIND || remaining == 0 {
            C4_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        C4_INTERMEDIATE_REMAINING
            .lock()
            .unwrap()
            .entry(entry.underfd)
            .or_default()
            .push(remaining);
        C4_MID.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Must be called *after* refresh(): refresh() clears
    /// CLOSEHANDLERTABLE, so a registration that ran only once would
    /// silently lose the handlers on the next test that calls refresh().
    fn c4_setup() {
        register_close_handlers(C4_FDKIND, c4_intermediate_close, c4_last_close);
        C4_LAST.store(0, Ordering::SeqCst);
        C4_MID.store(0, Ordering::SeqCst);
        C4_HANDLER_ERRS.store(0, Ordering::SeqCst);
        C4_SENTINEL_ACTIVE.store(false, Ordering::SeqCst);
        C4_SENTINEL_UNDERFD.store(u64::MAX, Ordering::SeqCst);
        C4_RELEASED.lock().unwrap().clear();
        C4_INTERMEDIATE_REMAINING.lock().unwrap().clear();
    }

    /// The whole config matrix below is 432 shapes, so a single iteration
    /// already covers far more ground than CONC-003's one fixed shape --
    /// hence a smaller default than c3_iters()'s 50. Raise it with
    /// LIND_CONC004_ITERS for a soak run.
    fn c4_iters() -> usize {
        std::env::var("LIND_CONC004_ITERS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4)
    }

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum DupKind {
        /// dup(): fresh underfd, brand-new key at count 1.
        Dup,
        /// dup2(): shared underfd installed at a caller-chosen slot.
        Dup2,
        /// fcntl(F_DUPFD): shared underfd at the lowest free slot >= start.
        Fdupfd,
    }

    #[derive(Clone, Copy, Debug)]
    struct C4Cfg {
        ndup: usize,
        nchild: usize,
        nthread: usize,
        kind: DupKind,
        /// Duplicates created after the forks, so the children inherit only
        /// the sentinel (pure fork-then-teardown pressure on its key).
        dup_after: bool,
        /// Children explicitly close their assigned inherited duplicates
        /// before cage teardown, instead of leaving everything to teardown.
        child_close: bool,
    }

    /// Creates `n` duplicates of `src_underfd` in `cage`, per `kind`, and
    /// returns (virtual fds, the underfd behind each). For Dup the underfds
    /// are all distinct and fresh; for Dup2/Fdupfd they all equal
    /// `src_underfd`.
    fn c4_make_dups(
        cage: u64,
        src_underfd: u64,
        kind: DupKind,
        from: usize,
        to: usize,
        vfds: &mut Vec<u64>,
        underfds: &mut Vec<u64>,
    ) {
        for i in from..to {
            match kind {
                DupKind::Dup => {
                    let nu = c4_next_underfd();
                    let v = get_unused_virtual_fd(cage, C4_FDKIND, nu, false, nu).unwrap();
                    vfds.push(v);
                    underfds.push(nu);
                }
                DupKind::Dup2 => {
                    let slot = C4_DUP2_SLOT + i as u64;
                    get_specific_virtual_fd(cage, slot, C4_FDKIND, src_underfd, false, src_underfd)
                        .unwrap();
                    vfds.push(slot);
                    underfds.push(src_underfd);
                }
                DupKind::Fdupfd => {
                    let v = get_unused_virtual_fd_from_startfd(
                        cage,
                        C4_FDKIND,
                        src_underfd,
                        false,
                        src_underfd,
                        C4_FDUPFD_START,
                    )
                    .unwrap();
                    vfds.push(v);
                    underfds.push(src_underfd);
                }
            }
        }
    }

    /// One round of the swept lifecycle. Returns nothing; every check is an
    /// assertion. Callers must have just run c4_setup().
    fn c4_run_round(cfg: C4Cfg) {
        let u_sent = c4_next_underfd();
        C4_SENTINEL_UNDERFD.store(u_sent, Ordering::SeqCst);
        C4_SENTINEL_ACTIVE.store(true, Ordering::SeqCst);

        init_empty_cage(C4_A);
        let v_sent = get_unused_virtual_fd(C4_A, C4_FDKIND, u_sent, false, u_sent).unwrap();

        // --- Duplicate (before the forks, unless dup_after).
        let mut dup_vfds: Vec<u64> = Vec::with_capacity(cfg.ndup);
        let mut dup_underfds: Vec<u64> = Vec::with_capacity(cfg.ndup);
        let ninherited = if cfg.dup_after { 0 } else { cfg.ndup };
        c4_make_dups(
            C4_A,
            u_sent,
            cfg.kind,
            0,
            ninherited,
            &mut dup_vfds,
            &mut dup_underfds,
        );

        // --- Fork. Every child is created BEFORE any release, so each one
        // deterministically inherits exactly the `ninherited` duplicates
        // plus the sentinel; there is no "did this child get it or not?"
        // ambiguity for an owner to trip over.
        for c in 0..cfg.nchild as u64 {
            copy_fdtable_for_cage(C4_A, C4_CHILD + c).unwrap();
        }

        // --- Duplicates created after the forks live only in C4_A.
        if cfg.dup_after {
            c4_make_dups(
                C4_A,
                u_sent,
                cfg.kind,
                0,
                cfg.ndup,
                &mut dup_vfds,
                &mut dup_underfds,
            );
        }

        // --- Reference accounting, derived from the config alone.
        let shared = cfg.kind != DupKind::Dup;
        // References on the sentinel key, counting every cage.
        let refs_sent: u64 = if shared {
            if cfg.dup_after {
                (1 + cfg.nchild as u64) + cfg.ndup as u64
            } else {
                (1 + cfg.ndup as u64) * (1 + cfg.nchild as u64)
            }
        } else {
            1 + cfg.nchild as u64
        };
        // References on each fresh key (Dup only).
        let refs_fresh: u64 = if cfg.dup_after {
            1
        } else {
            1 + cfg.nchild as u64
        };
        let nfresh: u64 = if shared { 0 } else { cfg.ndup as u64 };

        // --- Release every owner at once.
        //
        // Owners partition the work: parent thread t releases C4_A's
        // duplicates at indices t, t+nthread, ...; child c releases its own
        // inherited copies at indices c, c+nchild, ... and then drops its
        // cage. The parent's and the children's copies are distinct entries
        // in distinct cages, so the only thing they contend on is the
        // shared FDCOUNT key, which is exactly the contention under test.
        let parties = cfg.nchild + cfg.nthread + 1;
        let start = Arc::new(Barrier::new(parties));

        let mut handles = Vec::with_capacity(cfg.nchild + cfg.nthread);

        for c in 0..cfg.nchild {
            let start = Arc::clone(&start);
            let inherited: Vec<u64> = dup_vfds[..ninherited]
                .iter()
                .skip(c)
                .step_by(cfg.nchild.max(1))
                .copied()
                .collect();
            let child_close = cfg.child_close;
            handles.push(thread::spawn(move || {
                let cage = C4_CHILD + c as u64;
                start.wait();
                if child_close {
                    for vfd in inherited {
                        close_virtualfd(cage, vfd).unwrap();
                    }
                }
                // Everything still open in this cage (the sentinel copy,
                // the duplicates this child does not own, and, when
                // !child_close, all of them) must be released here.
                remove_cage_from_fdtable(cage);
            }));
        }

        for t in 0..cfg.nthread {
            let start = Arc::clone(&start);
            let owned: Vec<u64> = dup_vfds
                .iter()
                .skip(t)
                .step_by(cfg.nthread.max(1))
                .copied()
                .collect();
            handles.push(thread::spawn(move || {
                start.wait();
                for vfd in owned {
                    close_virtualfd(C4_A, vfd).unwrap();
                }
            }));
        }

        start.wait();
        if cfg.nthread == 0 {
            // No closer threads: the main thread is the sole owner of every
            // duplicate, still exactly-once and still concurrent with the
            // children's closes and cage teardowns.
            for &vfd in &dup_vfds {
                close_virtualfd(C4_A, vfd).unwrap();
            }
        }
        for h in handles {
            h.join().expect("a CONC-004 owner thread panicked");
        }

        // --- Everything is gone except C4_A's sentinel reference.
        assert!(
            !C4_RELEASED.lock().unwrap().contains_key(&u_sent),
            "the sentinel key was released while C4_A still held it ({cfg:?})"
        );
        assert_eq!(
            C4_LAST.load(Ordering::SeqCst),
            nfresh,
            "wrong number of last-closes before the final close ({cfg:?})"
        );
        assert_eq!(
            C4_MID.load(Ordering::SeqCst),
            (refs_sent - 1) + nfresh * (refs_fresh - 1),
            "wrong number of intermediate closes ({cfg:?})"
        );

        // The sentinel key was decremented refs_sent-1 times, from
        // refs_sent down to 1, so the `remaining` values reported must be
        // exactly {refs_sent-1, ..., 1}. Sorting first makes this an
        // equality that holds under any interleaving.
        {
            let mut remaining: Vec<u64> = C4_INTERMEDIATE_REMAINING
                .lock()
                .unwrap()
                .get(&u_sent)
                .cloned()
                .unwrap_or_default();
            remaining.sort_unstable_by(|a, b| b.cmp(a));
            assert_eq!(
                remaining,
                (1..refs_sent).rev().collect::<Vec<u64>>(),
                "sentinel key decrement sequence was not conserved ({cfg:?})"
            );
        }

        // Every fresh (dup()-style) key must be fully released ALREADY --
        // independently of the still-held sentinel key.
        {
            let released = C4_RELEASED.lock().unwrap();
            for u in &dup_underfds {
                if *u == u_sent {
                    continue; // shared-key mode
                }
                assert_eq!(
                    released.get(u).copied(),
                    Some(1),
                    "fresh key {u:#x} was not released exactly once ({cfg:?})"
                );
            }
        }

        // The retained reference is not merely counted; it still resolves.
        let entry = translate_virtual_fd(C4_A, v_sent).unwrap();
        assert_eq!((entry.fdkind, entry.underfd), (C4_FDKIND, u_sent));
        for c in 0..cfg.nchild as u64 {
            assert!(!check_cage_exists(C4_CHILD + c));
        }
        assert_eq!(C4_HANDLER_ERRS.load(Ordering::SeqCst), 0, "{cfg:?}");

        // --- The final close: the roadmap's headline invariant.
        C4_SENTINEL_ACTIVE.store(false, Ordering::SeqCst);
        close_virtualfd(C4_A, v_sent).unwrap();
        assert_eq!(
            *C4_RELEASED.lock().unwrap().get(&u_sent).unwrap(),
            1,
            "last_close_count != 1 for the sentinel key ({cfg:?})"
        );
        assert_eq!(C4_LAST.load(Ordering::SeqCst), nfresh + 1, "{cfg:?}");
        assert_eq!(
            C4_MID.load(Ordering::SeqCst),
            (refs_sent - 1) + nfresh * (refs_fresh - 1),
            "the final close was counted as intermediate ({cfg:?})"
        );

        // --- Key-removal oracle, portable across all four backends since
        // it never touches a private map directly: _increment_fdcount is
        // `or_insert(0) += 1` and _decrement_fdcount removes the key on
        // reaching 0, so a 0-valued FDCOUNT entry is unrepresentable. If
        // the (C4_FDKIND, u_sent) entry had NOT actually been removed
        // above, it would be sitting at some count >= 1, and a fresh
        // allocation on the same key followed by a close would drive it
        // down by one and fire `intermediate`, not `last`.
        let mid_before = C4_MID.load(Ordering::SeqCst);
        let v2 = get_unused_virtual_fd(C4_A, C4_FDKIND, u_sent, false, u_sent).unwrap();
        close_virtualfd(C4_A, v2).unwrap();
        assert_eq!(
            *C4_RELEASED.lock().unwrap().get(&u_sent).unwrap(),
            2,
            "{cfg:?}"
        );
        assert_eq!(
            C4_MID.load(Ordering::SeqCst),
            mid_before,
            "a stale FDCOUNT entry for {u_sent:#x} survived the last-close ({cfg:?})"
        );

        remove_cage_from_fdtable(C4_A);
        assert!(!check_cage_exists(C4_A));
        assert_eq!(C4_HANDLER_ERRS.load(Ordering::SeqCst), 0, "{cfg:?}");
    }

    #[test]
    /// The roadmap's CONC-004 test: open -> dup/dup2 -> fork -> concurrent
    /// close -> final close, swept over the full matrix of duplicate
    /// counts, child counts, parent-thread counts, duplication calls,
    /// before/after-fork duplication, and explicit-close vs cage-teardown
    /// child exits. Every reference has exactly one owner, so each round's
    /// expected close tallies are a pure function of the config.
    fn conc_004_owned_close_refcount_conservation() {
        let _lock = c2_test_guard();

        for _ in 0..c4_iters() {
            for &ndup in &[1usize, 2, C4_MAX_DUP] {
                for nchild in 0..=C4_MAX_CHILD {
                    for nthread in 0..=C4_MAX_THREAD {
                        for &kind in &[DupKind::Dup, DupKind::Dup2, DupKind::Fdupfd] {
                            for &dup_after in &[false, true] {
                                for &child_close in &[false, true] {
                                    refresh();
                                    c4_setup();
                                    c4_run_round(C4Cfg {
                                        ndup,
                                        nchild,
                                        nthread,
                                        kind,
                                        dup_after,
                                        child_close,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        refresh();
    }

    #[test]
    /// The case CONC-003 has no analogue for: ONE cage holding shared-key
    /// duplicates (dup2/F_DUPFD) and fresh-key duplicates (dup) at the same
    /// time, forked and torn down concurrently.
    ///
    /// The point is independence. Releasing every dup()-style reference
    /// must drive each of those keys to zero and fire `last` for each --
    /// while the sentinel key, aliased by the dup2/F_DUPFD duplicates, must
    /// not fire `last` at all. A bug that conflated the two (e.g. dup()
    /// sharing the source's key, or dup2() minting a fresh one) shows up
    /// here as a missing or premature release on a specific key, not as a
    /// mere miscount that a single-mode test could absorb.
    fn conc_004_mixed_shared_and_fresh_underfds() {
        let _lock = c2_test_guard();

        const NSHARED: usize = 3;
        const NFRESH: usize = 3;
        const NCHILD: usize = 3;

        for _ in 0..c4_iters() {
            refresh();
            c4_setup();

            let u_sent = c4_next_underfd();
            C4_SENTINEL_UNDERFD.store(u_sent, Ordering::SeqCst);
            C4_SENTINEL_ACTIVE.store(true, Ordering::SeqCst);

            init_empty_cage(C4_A);
            let v_sent = get_unused_virtual_fd(C4_A, C4_FDKIND, u_sent, false, u_sent).unwrap();

            // Shared-key duplicates: alternate dup2 and F_DUPFD so both
            // shared-key primitives are live on the same key at once.
            let mut shared_vfds = Vec::new();
            let mut ignored = Vec::new();
            for i in 0..NSHARED {
                let kind = if i % 2 == 0 {
                    DupKind::Dup2
                } else {
                    DupKind::Fdupfd
                };
                c4_make_dups(C4_A, u_sent, kind, i, i + 1, &mut shared_vfds, &mut ignored);
            }

            // Fresh-key duplicates, interleaved into the same cage.
            let mut fresh_vfds = Vec::new();
            let mut fresh_underfds = Vec::new();
            c4_make_dups(
                C4_A,
                u_sent,
                DupKind::Dup,
                0,
                NFRESH,
                &mut fresh_vfds,
                &mut fresh_underfds,
            );

            for c in 0..NCHILD as u64 {
                copy_fdtable_for_cage(C4_A, C4_CHILD + c).unwrap();
            }

            // count(u_sent)  = (1 + NSHARED) * (1 + NCHILD)
            // count(fresh_i) = 1 * (1 + NCHILD), for each of NFRESH keys
            let refs_sent = (1 + NSHARED as u64) * (1 + NCHILD as u64);
            let refs_fresh = 1 + NCHILD as u64;

            // Owners: one thread per child cage (teardown), one thread for
            // the parent's shared duplicates, one for the parent's fresh
            // duplicates. Disjoint sets, released together.
            let start = Arc::new(Barrier::new(NCHILD + 2));
            let mut handles = Vec::new();

            for c in 0..NCHILD as u64 {
                let start = Arc::clone(&start);
                handles.push(thread::spawn(move || {
                    start.wait();
                    remove_cage_from_fdtable(C4_CHILD + c);
                }));
            }
            {
                let start = Arc::clone(&start);
                let vfds = shared_vfds.clone();
                handles.push(thread::spawn(move || {
                    start.wait();
                    for vfd in vfds {
                        close_virtualfd(C4_A, vfd).unwrap();
                    }
                }));
            }
            {
                let start = Arc::clone(&start);
                let vfds = fresh_vfds.clone();
                handles.push(thread::spawn(move || {
                    start.wait();
                    for vfd in vfds {
                        close_virtualfd(C4_A, vfd).unwrap();
                    }
                }));
            }
            for h in handles {
                h.join()
                    .expect("a CONC-004 mixed-mode owner thread panicked");
            }

            // Independence, in both directions:
            {
                let released = C4_RELEASED.lock().unwrap();
                assert!(
                    !released.contains_key(&u_sent),
                    "the shared sentinel key fired `last` while C4_A still held it"
                );
                for u in &fresh_underfds {
                    assert_eq!(
                        released.get(u).copied(),
                        Some(1),
                        "fresh key {u:#x} was not released exactly once, \
                         even though every reference to it is gone"
                    );
                }
                assert_eq!(released.len(), NFRESH);
            }
            assert_eq!(C4_LAST.load(Ordering::SeqCst), NFRESH as u64);
            assert_eq!(
                C4_MID.load(Ordering::SeqCst),
                (refs_sent - 1) + NFRESH as u64 * (refs_fresh - 1)
            );
            assert_eq!(C4_HANDLER_ERRS.load(Ordering::SeqCst), 0);

            let entry = translate_virtual_fd(C4_A, v_sent).unwrap();
            assert_eq!((entry.fdkind, entry.underfd), (C4_FDKIND, u_sent));

            C4_SENTINEL_ACTIVE.store(false, Ordering::SeqCst);
            close_virtualfd(C4_A, v_sent).unwrap();
            assert_eq!(*C4_RELEASED.lock().unwrap().get(&u_sent).unwrap(), 1);
            assert_eq!(C4_LAST.load(Ordering::SeqCst), NFRESH as u64 + 1);
            assert_eq!(C4_HANDLER_ERRS.load(Ordering::SeqCst), 0);

            remove_cage_from_fdtable(C4_A);
        }

        refresh();
    }

    // ================================================================
    // CONC-005a: per-cage fd exhaustion isolation.
    //
    // Black-box counterpart:
    //   tests/unit-tests/process_tests/deterministic/
    //       conc_005_fd_exhaustion_isolation.c
    //
    // The C test can only observe errno from a saturated cage. These
    // pin down the properties underneath it: that the cap is per-cage
    // rather than global, that both allocators report EMFILE (and not
    // EBADF, which rawposix's fcntl(F_DUPFD) used to translate it to),
    // and that saturation is fully reversible with no refcount residue.
    // ================================================================

    /// Reserved cage-id block for CONC-005. 0xC0C5 == "CONC-005".
    /// Disjoint from threei::TESTING_CAGEID0..15 and from every other
    /// block used in this module.
    const C5_A: u64 = 0x0000_0000_C0C5_0000; // the cage driven to its cap
    const C5_B: u64 = 0x0000_0000_C0C5_0001; // the bystander
    const C5_CHILD: u64 = 0x0000_0000_C0C5_0010; // fork copy of C5_A

    /// A dedicated fdkind plus a disjoint underfd window guarantees no
    /// FDCOUNT key ever aliases with another test's leftovers: refresh()
    /// clears FDTABLE and CLOSEHANDLERTABLE but never FDCOUNT, so refcount
    /// state leaks across every test in this binary.
    const C5_FDKIND: u32 = 0x7E57_0005;
    const C5_UNDERFD_BASE: u64 = 0x5000_0000;

    static C5_UNDERFD_SEQ: AtomicU64 = AtomicU64::new(0);
    fn c5_next_underfd() -> u64 {
        C5_UNDERFD_BASE + C5_UNDERFD_SEQ.fetch_add(1, Ordering::SeqCst)
    }

    static C5_LAST: AtomicU64 = AtomicU64::new(0);
    static C5_HANDLER_ERRS: AtomicU64 = AtomicU64::new(0);

    // Close handlers are plain `fn` pointers and cannot capture, so all
    // bookkeeping lives in the statics above. They never panic: a panic
    // inside fdtables' own teardown path is nearly impossible to
    // attribute, so a violated expectation is recorded for the main
    // thread to assert on instead.
    fn c5_last_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C5_FDKIND || remaining != 0 {
            C5_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        C5_LAST.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn c5_intermediate_close(entry: FDTableEntry, remaining: u64) -> Result<(), i32> {
        if entry.fdkind != C5_FDKIND || remaining == 0 {
            C5_HANDLER_ERRS.fetch_add(1, Ordering::SeqCst);
        }
        Ok(())
    }

    /// Must be called *after* refresh(): refresh() clears
    /// CLOSEHANDLERTABLE, so a registration that ran only once would
    /// silently lose the handlers on the next test that calls refresh().
    fn c5_setup() {
        register_close_handlers(C5_FDKIND, c5_intermediate_close, c5_last_close);
        C5_LAST.store(0, Ordering::SeqCst);
        C5_HANDLER_ERRS.store(0, Ordering::SeqCst);
    }

    /// Fills `cageid` to FD_PER_PROCESS_MAX with fresh underfds and
    /// returns the virtual fds in allocation order. Asserts the table is
    /// exactly full, never over or under.
    fn c5_fill(cageid: u64) -> Vec<u64> {
        let mut vfds = Vec::with_capacity(FD_PER_PROCESS_MAX as usize);
        for expected in 0..FD_PER_PROCESS_MAX {
            let v = get_unused_virtual_fd(cageid, C5_FDKIND, c5_next_underfd(), false, 0).unwrap();
            // Allocation is lowest-available, so a full sweep from an
            // empty table must hand back 0, 1, 2, ... in order. This is
            // what lets the C test reason about fd numbers without ever
            // printing one.
            assert_eq!(v, expected);
            vfds.push(v);
        }
        vfds
    }

    #[test]
    /// The core CONC-005a claim: the fd cap belongs to the cage, not the
    /// system. A saturated cage A must not consume any of B's budget, and
    /// B must be able to fill its own table completely while A is still
    /// holding all 1024 of its own.
    ///
    /// A global cap (which is what TOTAL_FD_MAX would introduce if it were
    /// ever wired up; see the ENFILE note at the top of this file) shows
    /// up here as B failing partway through its own fill.
    fn conc_005_fd_limit_is_per_cage() {
        let _lock = c2_test_guard();
        refresh();
        c5_setup();

        init_empty_cage(C5_A);
        init_empty_cage(C5_B);

        // A takes its entire budget.
        let a_vfds = c5_fill(C5_A);
        assert_eq!(
            get_unused_virtual_fd(C5_A, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );

        // B, whose table has not been touched, is unaffected: it can
        // still allocate a full 1024 of its own.
        let b_vfds = c5_fill(C5_B);
        assert_eq!(
            get_unused_virtual_fd(C5_B, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );

        // Exhaustion is reversible, and only for the cage that cleans up.
        for v in &a_vfds {
            close_virtualfd(C5_A, *v).unwrap();
        }
        // A allocates again, and gets slot 0 back: releasing the table
        // restores lowest-available allocation rather than leaving a
        // high-water mark behind.
        assert_eq!(
            get_unused_virtual_fd(C5_A, C5_FDKIND, c5_next_underfd(), false, 0),
            Ok(0)
        );
        // B is still exactly as full as it was; A's cleanup did not
        // hand B any headroom either.
        assert_eq!(
            get_unused_virtual_fd(C5_B, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );

        for v in &b_vfds {
            close_virtualfd(C5_B, *v).unwrap();
        }

        remove_cage_from_fdtable(C5_A);
        remove_cage_from_fdtable(C5_B);
        assert_eq!(C5_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        refresh();
    }

    #[test]
    /// Both allocators must report EMFILE on a full table, and neither
    /// may report anything else.
    ///
    /// The start-fd variant is the one that matters most here: it backs
    /// fcntl(F_DUPFD)/F_DUPFD_CLOEXEC, and rawposix used to translate its
    /// error into EBADF, which makes "your table is full" indistinguishable
    /// from "you passed a bad descriptor". Every start offset must give
    /// EMFILE, including 0 (where it aliases plain allocation) and
    /// FD_PER_PROCESS_MAX - 1 (where only one slot could ever satisfy it).
    fn conc_005_exhausted_allocators_report_emfile() {
        let _lock = c2_test_guard();
        refresh();
        c5_setup();

        init_empty_cage(C5_A);
        let vfds = c5_fill(C5_A);

        assert_eq!(
            get_unused_virtual_fd(C5_A, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );
        for start in [0u64, 1, 50, FD_PER_PROCESS_MAX / 2, FD_PER_PROCESS_MAX - 1] {
            assert_eq!(
                get_unused_virtual_fd_from_startfd(
                    C5_A,
                    C5_FDKIND,
                    c5_next_underfd(),
                    false,
                    0,
                    start
                ),
                Err(threei::Errno::EMFILE as u64),
                "start={start}"
            );
        }

        // Free exactly one slot in the middle. Lowest-available then makes
        // the outcome a pure function of the start offset: a request at or
        // below the hole is satisfied by it, one above it still fails.
        let hole = FD_PER_PROCESS_MAX / 2;
        close_virtualfd(C5_A, hole).unwrap();
        assert_eq!(
            get_unused_virtual_fd_from_startfd(
                C5_A,
                C5_FDKIND,
                c5_next_underfd(),
                false,
                0,
                hole + 1
            ),
            Err(threei::Errno::EMFILE as u64)
        );
        assert_eq!(
            get_unused_virtual_fd_from_startfd(C5_A, C5_FDKIND, c5_next_underfd(), false, 0, hole),
            Ok(hole)
        );

        for v in &vfds {
            close_virtualfd(C5_A, *v).unwrap();
        }
        remove_cage_from_fdtable(C5_A);
        assert_eq!(C5_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        refresh();
    }

    #[test]
    /// Cage lifecycle still works while a cage is saturated: the case
    /// the C test covers by forking B only after A has already hit its
    /// cap.
    ///
    /// A fork from a full cage produces a child that is itself immediately
    /// full (fork copies the whole table, so the child inherits the
    /// saturation, not a fresh budget), every inherited entry is a second
    /// reference rather than a new one, and tearing the child down
    /// releases exactly the child's share, leaving the parent's 1024
    /// references intact.
    fn conc_005_fork_and_teardown_from_exhausted_cage() {
        let _lock = c2_test_guard();
        refresh();
        c5_setup();

        init_empty_cage(C5_A);
        let vfds = c5_fill(C5_A);

        // Forking a saturated cage succeeds: there is no aggregate check
        // (copy_fdtable_for_cage's ENFILE case is documented but
        // unimplemented), so this pins current behaviour deliberately.
        copy_fdtable_for_cage(C5_A, C5_CHILD).unwrap();

        // The child inherited the saturation, not a fresh budget.
        assert_eq!(
            get_unused_virtual_fd(C5_CHILD, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );

        // Every entry is now doubly referenced, so tearing the child down
        // fires no `last` close at all.
        remove_cage_from_fdtable(C5_CHILD);
        assert_eq!(C5_LAST.load(Ordering::SeqCst), 0);

        // ...and the parent is untouched: still exactly full, still able
        // to translate every one of its descriptors.
        assert_eq!(
            get_unused_virtual_fd(C5_A, C5_FDKIND, c5_next_underfd(), false, 0),
            Err(threei::Errno::EMFILE as u64)
        );
        for v in &vfds {
            translate_virtual_fd(C5_A, *v).unwrap();
        }

        // Dropping the last cage releases every key exactly once.
        remove_cage_from_fdtable(C5_A);
        assert_eq!(C5_LAST.load(Ordering::SeqCst), FD_PER_PROCESS_MAX);
        assert_eq!(C5_HANDLER_ERRS.load(Ordering::SeqCst), 0);

        refresh();
    }
}
