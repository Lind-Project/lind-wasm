// We mutate global tables (HANDLERTABLE/EXITING_TABLE) in tests. 
// Rust runs tests in parallel by default, which can cause cross-test interference. 
// `serial_test` lets us mark those tests #[serial] so they run one at a time.
use serial_test::serial;
use threei::{make_syscall, threei_const};
mod common;
use common::*;
/// Helper: pick IDs that won't collide with other tests.
const CAGE_A: u64 = 11;
const GRATE_G: u64 = 99;
const SYSCALL_FOO: u64 = 34;

#[test]
#[serial]
fn non_exit_syscall_falls_through_to_rawposix_path_when_not_interposed() {
    clear_globals();

    let unknown_syscall = 0xFFFF_FFFFu64;

    let rc = make_syscall(
        CAGE_A,
        unknown_syscall,
        0,
        CAGE_A,
        0, CAGE_A, 0, CAGE_A, 0, CAGE_A, 0, CAGE_A, 0, CAGE_A, 0, CAGE_A,
    );

    assert_eq!(rc, threei_const::ELINDAPIABORTED as i32);
}

/// If there is an interposition entry for (self_cageid, syscall_num),
/// make_syscall should call into the grate (via _call_grate_func) and return
/// that function's status code. This requires that the test setup installs
/// a test grate closure so _call_grate_func(Some(...)) returns a known value.
#[test]
#[ignore = "Requires a test grate closure so _call_grate_func returns Some(RETVAL)"]
fn interposed_syscall_invokes_grate_and_returns_its_value() {
    clear_globals();

    // Arrange: register (CAGE_A, SYSCALL_FOO) -> (handlefunc=7, grate=GRATE_G)
    let rc = reg(CAGE_A, SYSCALL_FOO, 7, GRATE_G);
    assert_eq!(rc, 0);
    
    // Act: call the interposed syscall from CAGE_A.
    let ret = make_syscall(
        CAGE_A,       // self_cageid
        SYSCALL_FOO,  // syscall_num (interposed)
        0,            // syscall name
        CAGE_A,       // target cage (unused by interposed path)
        1, CAGE_A, 2, CAGE_A, 3, CAGE_A, 4, CAGE_A, 5, CAGE_A, 6, CAGE_A,
    );

    assert_eq!(ret, 1234, "Should return the grate function's return value");
}
