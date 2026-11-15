// The policy and handler tables are global, so these tests run one at a time.
use serial_test::serial;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{
    RAWPOSIX_CAGEID, THREEI_CAGEID, UNUSED_ARG, UNUSED_ID, WASMTIME_CAGEID,
};
use threei::policy::{
    check_syscall, grant, is_owner, may_reference, on_exec, on_fork, reset_policy, revoke,
};
use threei::threei_const::{
    COPY_DATA_BETWEEN_CAGES_SYSCALL, COPY_HANDLER_TABLE_TO_CAGE_SYSCALL, ELINDAPIABORTED,
    ELINDEPERM, GRANT_CAGE_ACCESS_SYSCALL, REGISTER_HANDLER_SYSCALL, REVOKE_CAGE_ACCESS_SYSCALL,
    THREEI_DEREGISTER,
};
use threei::{grant_cage_access, make_syscall_from_cage, register_handler};
mod common;
use common::*;

const PERM: i32 = ELINDEPERM as i32;
const ABORTED: i32 = ELINDAPIABORTED as i32;
const EPERM: i32 = -(Errno::EPERM as i32);
const SYSCALL_READ: u64 = 0;

fn fresh() {
    clear_globals();
    reset_policy();
}

/// An ordinary syscall from `caller` on behalf of `target` whose only
/// argument lives in `arg1_cageid`.
fn plain_call(caller: u64, target: u64, arg1_cageid: u64) -> Result<(), i32> {
    check_syscall(
        caller,
        SYSCALL_READ,
        target,
        3,
        arg1_cageid,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ID,
        UNUSED_ID,
        UNUSED_ID,
        UNUSED_ID,
    )
}

/// `register_handler` as glibc lays it out: targetcage in arg1, handler cage
/// in arg2_cageid.
fn register_call(caller: u64, targetcage: u64, handlefunccage: u64) -> Result<(), i32> {
    check_syscall(
        caller,
        REGISTER_HANDLER_SYSCALL,
        handlefunccage,
        targetcage,
        SYSCALL_READ,
        UNUSED_ARG,
        handlefunccage,
        UNUSED_ID,
        UNUSED_ID,
        UNUSED_ID,
        UNUSED_ID,
    )
}

// ---------- lineage ----------

#[test]
#[serial]
fn cage_may_reference_itself_and_descendants_only() {
    fresh();
    // 1 -> 2 -> 3, and 1 -> 4
    on_fork(1, 2);
    on_fork(2, 3);
    on_fork(1, 4);

    assert!(may_reference(2, 2));
    assert!(may_reference(1, 2));
    assert!(may_reference(1, 3));
    assert!(may_reference(2, 3));
    assert!(may_reference(1, 4));

    // Not upward, not sideways.
    assert!(!may_reference(2, 1));
    assert!(!may_reference(3, 2));
    assert!(!may_reference(3, 1));
    assert!(!may_reference(2, 4));
    assert!(!may_reference(4, 3));
}

#[test]
#[serial]
fn ids_that_are_not_cages_are_never_referenceable() {
    fresh();
    on_fork(1, 2);
    for id in [
        0,
        UNUSED_ID,
        RAWPOSIX_CAGEID,
        WASMTIME_CAGEID,
        THREEI_CAGEID,
    ] {
        assert!(!may_reference(1, id), "id {}", id);
    }
}

#[test]
#[serial]
fn ownership_ignores_grants() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    assert_eq!(grant(1, 3, 2), 0);
    assert!(may_reference(2, 3));
    assert!(is_owner(1, 3));
    assert!(is_owner(3, 3));
    assert!(!is_owner(2, 3));
}

// ---------- grant ----------

#[test]
#[serial]
fn grant_needs_ownership() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);

    // Siblings cannot grant themselves access to each other.
    assert_eq!(grant(2, 3, 2), PERM);
    assert!(!may_reference(2, 3));

    // The parent can, and a cage can share itself.
    assert_eq!(grant(1, 3, 2), 0);
    assert!(may_reference(2, 3));
    assert_eq!(grant(2, 2, 3), 0);
    assert!(may_reference(3, 2));
}

#[test]
#[serial]
fn grant_rejects_non_cages_and_self() {
    fresh();
    on_fork(1, 2);
    assert_eq!(grant(1, 2, 0), ABORTED);
    assert_eq!(grant(1, 2, UNUSED_ID), ABORTED);
    assert_eq!(grant(1, 2, 2), ABORTED);
    // 9 was never forked.
    assert_eq!(grant(1, 2, 9), ABORTED);
    // The initial cage is a valid grantee.
    on_fork(2, 3);
    assert_eq!(grant(2, 3, 1), 0);
}

#[test]
#[serial]
fn grant_is_idempotent() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    assert_eq!(grant(1, 3, 2), 0);
    assert_eq!(grant(1, 3, 2), 0);
    // One revoke undoes it.
    assert_eq!(revoke(1, 3, 2), 0);
    assert!(!may_reference(2, 3));
}

#[test]
#[serial]
fn grant_covers_existing_and_later_descendants() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    on_fork(3, 4);

    assert_eq!(grant(1, 3, 2), 0);
    assert!(may_reference(2, 3));
    assert!(may_reference(2, 4));

    on_fork(4, 5);
    assert!(may_reference(2, 5));
    // Still not upward from the subject.
    assert!(!may_reference(2, 1));

    assert_eq!(revoke(1, 3, 2), 0);
    for c in [3, 4, 5] {
        assert!(!may_reference(2, c), "cage {}", c);
    }
    // Lineage is untouched.
    assert!(may_reference(1, 5));
    assert!(may_reference(3, 5));
}

#[test]
#[serial]
fn granted_access_cannot_be_passed_on() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    on_fork(1, 4);
    assert_eq!(grant(1, 3, 2), 0);
    assert_eq!(grant(2, 3, 4), PERM);
    assert!(!may_reference(4, 3));
}

// ---------- revoke ----------

#[test]
#[serial]
fn revoke_by_grantee_granter_or_their_owners() {
    fresh();
    // 1 -> 2 -> 3 (subject); 1 -> 4 (grantee)
    on_fork(1, 2);
    on_fork(2, 3);
    on_fork(1, 4);

    // The subject sits below the granter, so it cannot drop the grant.
    assert_eq!(grant(2, 3, 4), 0);
    assert_eq!(revoke(3, 3, 4), PERM);
    assert!(may_reference(4, 3));

    // An owner of the granter can.
    assert_eq!(revoke(1, 3, 4), 0);
    assert!(!may_reference(4, 3));

    // So can the granter and the grantee.
    assert_eq!(grant(2, 3, 4), 0);
    assert_eq!(revoke(2, 3, 4), 0);
    assert_eq!(grant(2, 3, 4), 0);
    assert_eq!(revoke(4, 3, 4), 0);
    assert!(!may_reference(4, 3));
}

#[test]
#[serial]
fn revoke_keeps_grants_made_higher_up() {
    fresh();
    on_fork(1, 2);
    on_fork(2, 3);
    on_fork(1, 4);

    assert_eq!(grant(1, 3, 4), 0);
    assert_eq!(grant(2, 3, 4), 0);

    // 2 removes its own grant; 1's still gives access.
    assert_eq!(revoke(2, 3, 4), 0);
    assert!(may_reference(4, 3));
    // 1's grant is out of 2's reach.
    assert_eq!(revoke(2, 3, 4), PERM);

    assert_eq!(revoke(1, 3, 4), 0);
    assert!(!may_reference(4, 3));
}

#[test]
#[serial]
fn revoke_errors() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    on_fork(1, 4);
    // No grant to revoke.
    assert_eq!(revoke(1, 3, 2), ABORTED);
    // An unrelated cage does not learn whether a grant exists.
    assert_eq!(grant(1, 3, 2), 0);
    assert_eq!(revoke(4, 3, 2), PERM);
    assert_eq!(revoke(1, 0, 2), ABORTED);
}

// ---------- check_syscall: ordinary syscalls ----------

#[test]
#[serial]
fn plain_syscalls_check_target_and_argument_cages() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);

    // A cage working on itself.
    assert_eq!(plain_call(2, 2, 2), Ok(()));
    // Unused or non-cage argument cage ids are ignored.
    assert_eq!(plain_call(2, 2, UNUSED_ID), Ok(()));
    assert_eq!(plain_call(2, 2, 0), Ok(()));
    // A grate acting for its child, with the child's memory.
    assert_eq!(plain_call(1, 2, 2), Ok(()));

    // Acting for a sibling, or passing a sibling's memory, is refused.
    assert_eq!(plain_call(2, 3, 2), Err(EPERM));
    assert_eq!(plain_call(2, 2, 3), Err(EPERM));
    // So is acting for a parent.
    assert_eq!(plain_call(2, 1, 2), Err(EPERM));
    // A target that is not a cage is refused.
    assert_eq!(plain_call(2, RAWPOSIX_CAGEID, 2), Err(EPERM));

    assert_eq!(grant(1, 3, 2), 0);
    assert_eq!(plain_call(2, 3, 3), Ok(()));
}

// ---------- check_syscall: 3i calls ----------

#[test]
#[serial]
fn register_handler_rules() {
    fresh();
    // grate 1 -> cage 2 (not exec'd yet) -> cage 3; 1 -> 4
    on_fork(1, 2);
    on_fork(2, 3);
    on_fork(1, 4);

    // A grate routes its descendant's calls to itself.
    assert_eq!(register_call(1, 3, 1), Ok(()));
    // Deregistering only needs the target.
    assert_eq!(register_call(2, 3, THREEI_DEREGISTER), Ok(()));
    // No rerouting of a parent or a sibling.
    assert_eq!(register_call(2, 1, 2), Err(PERM));
    assert_eq!(register_call(2, 4, 2), Err(PERM));
    // No routing to a cage the caller may not reference.
    assert_eq!(register_call(2, 3, 4), Err(PERM));
    // No routing to native code.
    for native in [RAWPOSIX_CAGEID, WASMTIME_CAGEID, THREEI_CAGEID] {
        assert_eq!(register_call(1, 2, native), Err(PERM));
    }

    // Before exec, a child may route its own calls to its parent...
    assert_eq!(register_call(2, 2, 1), Ok(()));
    // ...but not someone else's.
    assert_eq!(register_call(2, 3, 1), Err(PERM));
    // After exec it runs its own code and loses that.
    on_exec(2);
    assert_eq!(register_call(2, 2, 1), Err(PERM));

    // With a grant, a cage may route to the grantor.
    assert_eq!(grant(4, 4, 2), 0);
    assert_eq!(register_call(2, 3, 4), Ok(()));
}

#[test]
#[serial]
fn copy_data_checks_source_and_destination() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    let copy = |caller, src, dest| {
        check_syscall(
            caller,
            COPY_DATA_BETWEEN_CAGES_SYSCALL,
            caller,
            0x1000,
            src,
            0x2000,
            dest,
            UNUSED_ID,
            UNUSED_ID,
            UNUSED_ID,
            UNUSED_ID,
        )
    };
    assert_eq!(copy(1, 2, 1), Ok(()));
    assert_eq!(copy(1, 1, 2), Ok(()));
    assert_eq!(copy(1, 2, 3), Ok(()));
    assert_eq!(copy(2, 3, 2), Err(PERM));
    assert_eq!(copy(2, 2, 1), Err(PERM));
    assert_eq!(copy(2, 0, 2), Err(PERM));
}

#[test]
#[serial]
fn copy_handler_table_refuses_tables_from_above() {
    fresh();
    on_fork(1, 2);
    on_fork(2, 3);
    let copy_table = |caller, src, target| {
        check_syscall(
            caller,
            COPY_HANDLER_TABLE_TO_CAGE_SYSCALL,
            caller,
            src,
            target,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ID,
            UNUSED_ID,
            UNUSED_ID,
            UNUSED_ID,
        )
    };
    // Copying the grate's table over its own would step around the grate.
    assert_eq!(copy_table(3, 1, 3), Err(PERM));
    assert_eq!(copy_table(3, 3, 2), Err(PERM));
    assert_eq!(copy_table(2, 2, 3), Ok(()));
    assert_eq!(copy_table(1, 3, 2), Ok(()));
}

#[test]
#[serial]
fn grant_and_revoke_entry_checks() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    let call = |caller, num, subject, grantee| {
        check_syscall(
            caller, num, caller, subject, UNUSED_ID, grantee, UNUSED_ID, UNUSED_ID, UNUSED_ID,
            UNUSED_ID, UNUSED_ID,
        )
    };
    assert_eq!(call(1, GRANT_CAGE_ACCESS_SYSCALL, 3, 2), Ok(()));
    assert_eq!(call(2, GRANT_CAGE_ACCESS_SYSCALL, 3, 2), Err(PERM));
    // A grantee may always ask to give up its access.
    assert_eq!(call(2, REVOKE_CAGE_ACCESS_SYSCALL, 3, 2), Ok(()));
    assert_eq!(call(2, REVOKE_CAGE_ACCESS_SYSCALL, 3, 1), Err(PERM));
}

// ---------- make_syscall_from_cage ----------

#[test]
#[serial]
fn denied_call_is_not_routed() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    // Cage 2 has no handler table: a routed call would find no handler. The
    // policy answers first.
    let ret = make_syscall_from_cage(
        2,
        SYSCALL_READ,
        0,
        3,
        3,
        3,
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
    assert_eq!(ret, EPERM);
}

#[test]
#[serial]
fn grant_through_the_syscall_path() {
    fresh();
    on_fork(1, 2);
    on_fork(1, 3);
    for cage in [1, 2] {
        assert_eq!(
            register_handler(
                0,
                0,
                cage,
                GRANT_CAGE_ACCESS_SYSCALL,
                0,
                THREEI_CAGEID,
                grant_cage_access as *const () as u64,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
            ),
            0
        );
    }
    let grant_call = |caller, subject, grantee| {
        make_syscall_from_cage(
            caller,
            GRANT_CAGE_ACCESS_SYSCALL,
            0,
            caller,
            subject,
            UNUSED_ID,
            grantee,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
            UNUSED_ARG,
            UNUSED_ID,
        )
    };

    assert_eq!(grant_call(2, 3, 2), PERM);
    assert!(!may_reference(2, 3));

    assert_eq!(grant_call(1, 3, 2), 0);
    assert!(may_reference(2, 3));
}
