// We mutate global tables (HANDLERTABLE/EXITING_TABLE) in tests.
// Rust runs tests in parallel by default, which can cause cross-test interference.
// `serial_test` lets us mark those tests #[serial] so they run one at a time.
use serial_test::serial;
use threei::handler_table::HANDLERTABLE;
use threei::threei_const;
use threei::EXITING_TABLE;
mod common;
use common::*;
// ---------- [Register_handler] ----------
const SYSCALL_FOO: u64 = 34;
const SYSCALL_BAR: u64 = 35;
const OP_ADD: u64 = 1;
const OP_REMOVE: u64 = 0;

#[test]
#[serial]
fn insert_new_handler_creates_mapping() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let handlefunc = 11;
    let dest = 99;

    let rc = reg(cage, callnum, handlefunc, dest, OP_ADD);
    assert_eq!(rc, 0);

    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(handlefunc, dest)]);
}

#[test]
#[serial]
fn re_register_same_mapping_is_idempotent() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let handlefunc = 11;
    let dest = 99;

    assert_eq!(reg(cage, callnum, handlefunc, dest, OP_ADD), 0);
    // Re-register exactly the same mapping
    assert_eq!(reg(cage, callnum, handlefunc, dest, OP_ADD), 0);

    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(handlefunc, dest)]);
}

#[test]
#[serial]
fn conflicting_mapping_returns_apiaborted() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let handlefunc = 11;
    let dest1 = 99;
    let dest2 = 100; // conflicting destination

    assert_eq!(reg(cage, callnum, handlefunc, dest1, OP_ADD), 0);
    // Same (cage, callnum, handlefunc) but different destination -> conflict
    let rc = reg(cage, callnum, handlefunc, dest2, OP_ADD);
    assert_eq!(rc, threei_const::ELINDAPIABORTED as i32);

    // Mapping should remain the original one
    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(handlefunc, dest1)]);
}

#[test]
#[serial]
fn deregister_entire_callnum_with_constant() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    // Install multiple handlers under the same (cage, callnum)
    assert_eq!(reg(cage, callnum, 1, 90, OP_ADD), 0);
    assert_eq!(reg(cage, callnum, 2, 91, OP_ADD), 0);
    assert_eq!(reg(cage, callnum, 3, 92, OP_ADD), 0);

    // Now remove the entire callnum entry:
    assert_eq!(
        reg(
            cage,
            callnum,
            12345,
            threei_const::THREEI_DEREGISTER,
            OP_REMOVE
        ),
        0
    );

    // No mappings should remain for that (cage, callnum)
    let m = mappings_for(cage, callnum);
    assert!(m.is_empty());

    // Idempotent: removing again is still success
    assert_eq!(
        reg(
            cage,
            callnum,
            9999,
            threei_const::THREEI_DEREGISTER,
            OP_REMOVE
        ),
        0
    );
}

#[test]
#[serial]
fn selective_deregister_with_handlefunc_zero_removes_only_matching_dest() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    // Two handlers pointing to different destinations
    // targetcage: u64, targetcallnum: u64, handlefunc: u64, handlefunccage: u64
    assert_eq!(reg(cage, callnum, 1, 90, OP_ADD), 0);
    assert_eq!(reg(cage, callnum, 2, 91, OP_ADD), 0);
    assert_eq!(reg(cage, callnum, 3, 90, OP_ADD), 0);

    // handlefunc == 0 -> remove all entries whose dest == 90
    assert_eq!(reg(cage, callnum, 1, 90, OP_REMOVE), 0);
    assert_eq!(reg(cage, callnum, 3, 90, OP_REMOVE), 0);

    let mut m = mappings_for(cage, callnum);
    m.sort_unstable();
    // Only the (2, 91) mapping should remain
    assert_eq!(m, vec![(2, 91)]);
}

#[test]
#[serial]
fn exiting_table_blocks_registration() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let handlefunc = 11;
    let dest = 99;

    // Put source cage into EXITING state
    EXITING_TABLE.insert(cage);
    let rc = reg(cage, callnum, handlefunc, dest, OP_ADD);
    assert_eq!(rc, threei_const::ELINDESRCH as i32);

    // Remove, then try blocking by destination exiting
    EXITING_TABLE.remove(&cage);
    EXITING_TABLE.insert(dest);

    let rc2 = reg(cage, callnum, handlefunc, dest, OP_ADD);
    assert_eq!(rc2, threei_const::ELINDESRCH as i32);

    // Ensure nothing was inserted
    assert!(mappings_for(cage, callnum).is_empty());
}

#[test]
#[serial]
fn cleanup_cage_removed_when_last_callnum_removed() {
    clear_globals();

    let cage = 7;

    // install entries under two callnums
    assert_eq!(reg(cage, SYSCALL_FOO, 1, 90, OP_ADD), 0);
    assert_eq!(reg(cage, SYSCALL_BAR, 2, 91, OP_ADD), 0);

    // remove SYSCALL_FOO entirely
    assert_eq!(
        reg(
            cage,
            SYSCALL_FOO,
            0,
            threei_const::THREEI_DEREGISTER,
            OP_REMOVE
        ),
        0
    );
    // cage still present because call_b remains
    #[cfg(feature = "hashmap")]
    {
        let tbl = HANDLERTABLE.lock().unwrap();
        assert!(tbl.get(&cage).is_some());
    }
    #[cfg(feature = "dashmap")]
    {
        assert!(HANDLERTABLE.get(&cage).is_some());
    }

    // remove SYSCALL_BAR entirely -> cage should be cleaned up (optional cleanup branch)
    assert_eq!(
        reg(
            cage,
            SYSCALL_BAR,
            0,
            threei_const::THREEI_DEREGISTER,
            OP_REMOVE
        ),
        0
    );

    #[cfg(feature = "hashmap")]
    {
        let tbl = HANDLERTABLE.lock().unwrap();
        assert!(tbl.get(&cage).is_none());
    }
    #[cfg(feature = "dashmap")]
    {
        assert!(HANDLERTABLE.get(&cage).is_none());
    }
}

#[test]
#[serial]
fn deregister_not_found_is_ok() {
    clear_globals();

    let cage = 123;
    let callnum = 456;

    // Nothing exists, but deregister should still succeed (idempotent)
    let rc = reg(cage, callnum, 0, threei_const::THREEI_DEREGISTER, OP_REMOVE);
    assert_eq!(rc, 0);
}

// // ---------- [Copy_handlers] ----------

#[test]
#[serial]
fn copy_into_empty_target_copies_all() {
    clear_globals();

    let src = 1001;
    let dst = 2001;

    // src: callnum SYSCALL_FOO -> {(11->90), (12->91)}
    assert_eq!(reg(src, SYSCALL_FOO, 11, 90, OP_ADD), 0);
    assert_eq!(reg(src, SYSCALL_FOO, 12, 91, OP_ADD), 0);

    let rc = cpy(dst, src);
    assert_eq!(rc, 0);

    let mut got = mappings_for(dst, SYSCALL_FOO);
    got.sort_unstable();
    assert_eq!(got, vec![(11, 90), (12, 91)]);

    // src unchanged
    let mut srcmap = mappings_for(src, SYSCALL_FOO);
    srcmap.sort_unstable();
    assert_eq!(srcmap, vec![(11, 90), (12, 91)]);
}

#[test]
#[serial]
fn copy_is_idempotent() {
    clear_globals();

    let src = 1002;
    let dst = 2002;

    assert_eq!(reg(src, SYSCALL_FOO, 11, 90, OP_ADD), 0);
    assert_eq!(cpy(dst, src), 0);
    assert_eq!(cpy(dst, src), 0); // second time no changes

    let mut got = mappings_for(dst, SYSCALL_FOO);
    got.sort_unstable();
    assert_eq!(got, vec![(11, 90)]);
}

#[test]
#[serial]
fn copy_does_not_overwrite_existing_handlers() {
    clear_globals();

    let src = 1003;
    let dst = 2003;

    // src has (11->99, 12->100)
    assert_eq!(reg(src, SYSCALL_FOO, 11, 99, OP_ADD), 0);
    assert_eq!(reg(src, SYSCALL_FOO, 12, 100, OP_ADD), 0);

    // dst already has (11->77) under the same callnum
    assert_eq!(reg(dst, SYSCALL_FOO, 11, 77, OP_ADD), 0);

    assert_eq!(cpy(dst, src), 0);

    // Expect (11->77) preserved, (12->100) added
    let mut got = mappings_for(dst, SYSCALL_FOO);
    got.sort_unstable();
    assert_eq!(got, vec![(11, 77), (12, 100)]);
}

// This test adds multiple callnums with different destinations
// Verify that when copying handler tables:
// - All callnums from the source cage are copied into the destination cage.
// - Existing handlers in the destination are preserved (not overwritten).
// - New handlers from the source are merged in alongside existing ones.
// In this case: callnum 34 is newly added, and callnum 35's new entry is merged
// without losing the old one.
#[test]
#[serial]
fn copy_merges_multiple_callnums() {
    clear_globals();

    let src = 1004;
    let dst = 2004;

    // src: two callnums (SYSCALL_FOO and SYSCALL_BAR)
    assert_eq!(reg(src, SYSCALL_FOO, 11, 90, OP_ADD), 0);
    assert_eq!(reg(src, SYSCALL_BAR, 21, 190, OP_ADD), 0);

    // dst already has one handler under callnum SYSCALL_BAR
    assert_eq!(reg(dst, SYSCALL_BAR, 22, 191, OP_ADD), 0);

    assert_eq!(cpy(dst, src), 0);

    let mut got34 = mappings_for(dst, SYSCALL_FOO);
    got34.sort_unstable();
    assert_eq!(got34, vec![(11, 90)]);

    let mut got35 = mappings_for(dst, SYSCALL_BAR);
    got35.sort_unstable();
    // (22,191) preserved; (21,190) added
    assert_eq!(got35, vec![(21, 190), (22, 191)]);
}

// Ensure that if the source cage does not have a handler table,
// attempting to copy from it fails with ELINDAPIABORTED.
// Also verify that the destination cage remains unaffected (still empty).
#[test]
#[serial]
fn copy_returns_error_if_src_missing_table() {
    clear_globals();

    let src = 1005; // no table for src
    let dst = 2005;

    let rc = cpy(dst, src);

    assert_eq!(rc, threei_const::ELINDAPIABORTED as u64);

    // Ensure dst stays empty.
    #[cfg(feature = "hashmap")]
    {
        let tbl = HANDLERTABLE.lock().unwrap();
        assert!(tbl.get(&dst).is_none());
    }
    #[cfg(feature = "dashmap")]
    {
        assert!(HANDLERTABLE.get(&dst).is_none());
    }
}

// Validate that copying fails with ELINDESRCH if either the source or the
// destination cage is marked as "exiting".
// The test covers both cases separately (src exiting, then dst exiting).
// After each case, the EXITING_TABLE is restored to keep tests independent.
#[test]
#[serial]
fn copy_returns_elindesrch_if_either_src_or_dst_exiting() {
    clear_globals();

    let src = 1006;
    let dst = 2006;

    // Prepare a valid source table
    assert_eq!(reg(src, SYSCALL_FOO, 11, 90, OP_ADD), 0);

    // Case 1: src exiting
    EXITING_TABLE.insert(src);
    let rc1 = cpy(dst, src);
    assert_eq!(rc1, threei_const::ELINDESRCH as u64);
    EXITING_TABLE.remove(&src);

    // Case 2: dst exiting
    EXITING_TABLE.insert(dst);
    let rc2 = cpy(dst, src);
    assert_eq!(rc2, threei_const::ELINDESRCH as u64);
    EXITING_TABLE.remove(&dst);
}
