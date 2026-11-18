use serial_test::serial;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{RAWPOSIX_CAGEID, THREEI_CAGEID, WASMTIME_CAGEID};
use threei::access_table::ACCESS_TABLE;
use threei::handler_table::{_rm_cage_from_handler, _rm_grate_from_handler};
use threei::{make_syscall, register_handler, threei_const, EXITING_TABLE};

mod common;
use common::*;

const A: u64 = 11;
const B: u64 = 12;
const C: u64 = 13;
const D: u64 = 14;
const CALL: u64 = 34;
const OTHER_CALL: u64 = 35;
const RESULT: i64 = 0x1234_5678_9abc_def0;
const DENIED: i64 = -(Errno::EPERM as i64);
const UNUSED: u64 = 0xdead_beef_dead_beef;

extern "C" fn raw_handler(
    _target: u64,
    _arg1: u64,
    _owner1: u64,
    _arg2: u64,
    _owner2: u64,
    _arg3: u64,
    _owner3: u64,
    _arg4: u64,
    _owner4: u64,
    _arg5: u64,
    _owner5: u64,
    _arg6: u64,
    _owner6: u64,
) -> i64 {
    RESULT
}

extern "C" fn registration_handler(
    caller: u64,
    target: u64,
    arg1: u64,
    owner1: u64,
    arg2: u64,
    owner2: u64,
    arg3: u64,
    owner3: u64,
    arg4: u64,
    owner4: u64,
    arg5: u64,
    owner5: u64,
    arg6: u64,
    owner6: u64,
) -> i32 {
    register_handler(
        caller, target, arg1, owner1, arg2, owner2, arg3, owner3, arg4, owner4, arg5, owner5, arg6,
        owner6,
    )
}

extern "C" fn copy_handler(
    caller: u64,
    target: u64,
    arg1: u64,
    owner1: u64,
    arg2: u64,
    owner2: u64,
    arg3: u64,
    owner3: u64,
    arg4: u64,
    owner4: u64,
    arg5: u64,
    owner5: u64,
    arg6: u64,
    owner6: u64,
) -> i32 {
    threei::copy_handler_table_to_cage(
        caller, target, arg1, owner1, arg2, owner2, arg3, owner3, arg4, owner4, arg5, owner5, arg6,
        owner6,
    ) as i32
}

fn grant(handler: u64, target: u64, syscall: u64) {
    assert_eq!(register_simple(target, syscall, handler, 1, 1), 0);
}

fn setup() {
    clear_globals();
    for caller in [A, B, C, D] {
        assert_eq!(
            register_simple(
                caller,
                CALL,
                RAWPOSIX_CAGEID,
                raw_handler as *const () as usize as u64,
                1,
            ),
            0
        );
    }
}

fn call(caller: u64, target: u64, owners: [u64; 6]) -> i64 {
    make_syscall(
        caller, CALL, 0, target, 0, owners[0], 0, owners[1], 0, owners[2], 0, owners[3], 0,
        owners[4], 0, owners[5],
    )
}

#[test]
#[serial]
fn target_authority_is_directional_and_transitive() {
    setup();
    assert_eq!(call(A, A, [A; 6]), RESULT);
    assert_eq!(call(A, B, [A; 6]), DENIED);
    grant(A, B, OTHER_CALL);
    assert_eq!(call(A, B, [B; 6]), RESULT);
    assert_eq!(call(B, A, [B; 6]), DENIED);
    grant(B, C, OTHER_CALL);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    assert_eq!(call(A, D, [A; 6]), DENIED);
    assert_eq!(call(D, C, [D; 6]), DENIED);
}

#[test]
#[serial]
fn every_argument_owner_is_checked_even_for_self_targeted_calls() {
    setup();
    for index in 0..6 {
        let mut owners = [A; 6];
        owners[index] = B;
        assert_eq!(call(A, A, owners), DENIED, "argument {}", index + 1);
    }
    grant(A, B, OTHER_CALL);
    grant(B, C, OTHER_CALL);
    assert_eq!(call(A, A, [A, B, C, A, B, C]), RESULT);
    for index in 0..6 {
        let mut owners = [C; 6];
        owners[index] = D;
        assert_eq!(call(A, C, owners), DENIED, "argument {}", index + 1);
    }
}

#[test]
#[serial]
fn replacement_and_last_deregistration_revoke_transitive_authority() {
    setup();
    grant(A, B, OTHER_CALL);
    grant(A, B, OTHER_CALL + 1);
    grant(B, C, OTHER_CALL);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    assert_eq!(
        register_simple(B, OTHER_CALL, threei_const::THREEI_DEREGISTER, 0, 0),
        0
    );
    assert_eq!(call(A, C, [C; 6]), RESULT);
    grant(D, B, OTHER_CALL + 1);
    assert_eq!(call(A, B, [A; 6]), DENIED);
    assert_eq!(call(A, C, [A; 6]), DENIED);
    assert_eq!(call(D, C, [C; 6]), RESULT);
    assert_eq!(
        register_simple(B, OTHER_CALL + 1, threei_const::THREEI_DEREGISTER, 0, 0),
        0
    );
    assert_eq!(call(D, C, [D; 6]), DENIED);
}

#[test]
#[serial]
fn cycles_terminate_and_do_not_authorize_unrelated_cages() {
    setup();
    grant(A, B, OTHER_CALL);
    grant(B, C, OTHER_CALL);
    grant(C, A, OTHER_CALL);
    assert_eq!(call(A, C, [A, B, C, A, B, C]), RESULT);
    assert_eq!(call(D, C, [D; 6]), DENIED);
    _rm_grate_from_handler(B);
    assert_eq!(call(A, C, [A; 6]), DENIED);
}

#[test]
#[serial]
fn copied_handlers_replace_permissions_and_failed_copy_preserves_them() {
    setup();
    grant(A, B, OTHER_CALL);
    grant(D, C, OTHER_CALL);
    assert_eq!(call(D, C, [C; 6]), RESULT);
    assert_eq!(cpy(C, B), 0);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    assert_eq!(call(D, C, [D; 6]), DENIED);
    assert_eq!(cpy(C, C), 0);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    assert_eq!(cpy(C, 100), threei_const::ELINDAPIABORTED);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    assert_eq!(cpy(C, D), 0);
    assert_eq!(call(A, C, [A; 6]), DENIED);
}

#[test]
#[serial]
fn cage_and_grate_cleanup_revoke_paths() {
    setup();
    grant(A, B, OTHER_CALL);
    grant(B, C, OTHER_CALL);
    _rm_cage_from_handler(B);
    assert_eq!(call(A, B, [A; 6]), DENIED);
    assert_eq!(call(A, C, [A; 6]), DENIED);
    grant(A, B, OTHER_CALL);
    assert_eq!(call(A, C, [C; 6]), RESULT);
    _rm_grate_from_handler(A);
    assert_eq!(call(A, C, [A; 6]), DENIED);
}

#[test]
#[serial]
fn failed_registration_does_not_grant_or_revoke_authority() {
    setup();
    EXITING_TABLE.insert(B);
    assert_eq!(
        register_simple(B, OTHER_CALL, A, 1, 1),
        threei_const::ELINDESRCH as i32
    );
    EXITING_TABLE.remove(&B);
    assert_eq!(call(A, B, [A; 6]), DENIED);
    grant(A, B, OTHER_CALL);
    EXITING_TABLE.insert(D);
    assert_eq!(
        register_simple(B, OTHER_CALL, D, 1, 1),
        threei_const::ELINDESRCH as i32
    );
    assert_eq!(call(A, B, [B; 6]), RESULT);
}

#[test]
#[serial]
fn runtime_handlers_do_not_grant_authority_and_revoke_replaced_edges() {
    setup();
    for runtime in [RAWPOSIX_CAGEID, WASMTIME_CAGEID, THREEI_CAGEID] {
        grant(A, B, OTHER_CALL);
        grant(runtime, B, OTHER_CALL);
        assert!(!ACCESS_TABLE.read().unwrap().allows(runtime, B));
        assert_eq!(call(A, B, [A; 6]), DENIED);
    }
}

#[test]
#[serial]
fn unused_argument_sentinels_are_not_owners_but_invalid_ids_are_denied() {
    setup();
    assert_eq!(
        call(A, A, [UNUSED, UNUSED & !(1 << 63), A, A, A, A]),
        RESULT
    );
    let unused_id = sysdefs::constants::lind_platform_const::UNUSED_ID;
    assert_eq!(
        call(A, A, [unused_id, unused_id & !(1 << 63), A, A, A, A]),
        RESULT
    );
    // harsh_cage_exit passes 0 as filler in its argument cage slots.
    assert_eq!(call(A, A, [A, 0, 0, 0, 0, 0]), RESULT);
    assert_eq!(call(A, 0, [A; 6]), DENIED);
    for invalid in [2048, u64::MAX, RAWPOSIX_CAGEID] {
        assert_eq!(call(A, A, [invalid; 6]), DENIED);
        assert_eq!(call(A, invalid, [A; 6]), DENIED);
    }
}

#[test]
#[serial]
fn internal_rawposix_to_wasmtime_route_preserves_runtime_metadata() {
    setup();
    assert_eq!(
        register_simple(
            RAWPOSIX_CAGEID,
            CALL,
            WASMTIME_CAGEID,
            raw_handler as *const () as usize as u64,
            1,
        ),
        0
    );
    assert_eq!(
        call(
            RAWPOSIX_CAGEID,
            WASMTIME_CAGEID,
            [B, 123456, 0, C, D, UNUSED]
        ),
        RESULT
    );
    assert_eq!(call(A, WASMTIME_CAGEID, [A; 6]), DENIED);
    assert_eq!(call(RAWPOSIX_CAGEID, B, [B; 6]), DENIED);
}

#[test]
#[serial]
fn registration_syscall_metadata_is_not_checked_as_argument_owners() {
    setup();
    assert_eq!(
        register_simple(
            A,
            threei_const::REGISTER_HANDLER_SYSCALL,
            THREEI_CAGEID,
            registration_handler as *const () as usize as u64,
            1,
        ),
        0
    );
    assert_eq!(
        make_syscall(
            A,
            threei_const::REGISTER_HANDLER_SYSCALL,
            UNUSED,
            A,
            B,
            OTHER_CALL,
            UNUSED,
            A,
            1,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ),
        0
    );
    assert_eq!(call(A, B, [B; 6]), RESULT);
    assert_eq!(
        make_syscall(
            A,
            threei_const::REGISTER_HANDLER_SYSCALL,
            UNUSED,
            D,
            C,
            OTHER_CALL,
            UNUSED,
            A,
            1,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ),
        DENIED
    );
    assert_eq!(call(A, C, [A; 6]), DENIED);
}

#[test]
#[serial]
fn dead_target_errors_are_preserved_and_exit_calls_still_require_authority() {
    setup();
    EXITING_TABLE.insert(B);
    assert_eq!(call(A, B, [A; 6]), -(Errno::ESRCH as i64));
    assert_eq!(call(B, B, [B; 6]), RESULT);
    assert_eq!(
        make_syscall(
            A,
            threei::EXIT_SYSCALL,
            0,
            B,
            0,
            A,
            0,
            A,
            0,
            A,
            0,
            A,
            0,
            A,
            0,
            A,
        ),
        DENIED
    );
}

#[test]
#[serial]
fn handler_copy_syscall_preserves_metadata_abi_and_updates_authority() {
    setup();
    grant(B, A, OTHER_CALL);
    assert_eq!(
        register_simple(
            A,
            threei_const::COPY_HANDLER_TABLE_TO_CAGE_SYSCALL,
            THREEI_CAGEID,
            copy_handler as *const () as usize as u64,
            1,
        ),
        0
    );
    assert_eq!(
        make_syscall(
            A,
            threei_const::COPY_HANDLER_TABLE_TO_CAGE_SYSCALL,
            UNUSED,
            A,
            A,
            C,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ),
        0
    );
    assert_eq!(call(B, C, [C; 6]), RESULT);
    assert_eq!(call(A, C, [A; 6]), DENIED);
}

#[test]
#[serial]
fn data_copy_syscall_checks_real_argument_owners() {
    setup();
    assert_eq!(
        register_simple(
            A,
            threei_const::COPY_DATA_BETWEEN_CAGES_SYSCALL,
            RAWPOSIX_CAGEID,
            raw_handler as *const () as usize as u64,
            1,
        ),
        0
    );
    for owner in [B, C] {
        assert_eq!(
            make_syscall(
                A,
                threei_const::COPY_DATA_BETWEEN_CAGES_SYSCALL,
                UNUSED,
                A,
                0,
                A,
                0,
                owner,
                0,
                UNUSED,
                0,
                UNUSED,
                UNUSED,
                UNUSED,
                UNUSED,
                UNUSED,
            ),
            DENIED
        );
    }
    grant(A, B, OTHER_CALL);
    grant(B, C, OTHER_CALL);
    assert_eq!(
        make_syscall(
            A,
            threei_const::COPY_DATA_BETWEEN_CAGES_SYSCALL,
            UNUSED,
            A,
            0,
            B,
            0,
            C,
            0,
            UNUSED,
            0,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
            UNUSED,
        ),
        RESULT
    );
}
