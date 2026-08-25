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
    let dest_grate = 99;
    let addr = 11;

    let rc = register_simple(cage, callnum, dest_grate, addr, OP_ADD);
    assert_eq!(rc, 0);

    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(dest_grate, addr)]);
}

#[test]
#[serial]
fn re_register_same_mapping_is_idempotent() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let dest_grate = 99;
    let addr = 11;

    assert_eq!(register_simple(cage, callnum, dest_grate, addr, OP_ADD), 0);
    assert_eq!(register_simple(cage, callnum, dest_grate, addr, OP_ADD), 0);

    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(dest_grate, addr)]);
}

#[test]
#[serial]
fn re_register_overwrites_existing_mapping() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let dest_grate = 99;
    let addr1 = 11;
    let addr2 = 12;

    assert_eq!(register_simple(cage, callnum, dest_grate, addr1, OP_ADD), 0);

    let rc = register_simple(cage, callnum, dest_grate, addr2, OP_ADD);
    assert_eq!(rc, 0);

    let m = mappings_for(cage, callnum);
    assert_eq!(m, vec![(dest_grate, addr2)]);
}

#[test]
#[serial]
fn deregister_entire_callnum_with_constant() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;

    assert_eq!(register_simple(cage, callnum, 90, 1, OP_ADD), 0);
    assert_eq!(register_simple(cage, callnum, 91, 2, OP_ADD), 0);
    assert_eq!(register_simple(cage, callnum, 92, 3, OP_ADD), 0);

    assert_eq!(
        register_simple(cage, callnum, threei_const::THREEI_DEREGISTER, 0, OP_REMOVE),
        0
    );

    assert!(mappings_for(cage, callnum).is_empty());

    assert_eq!(
        register_simple(cage, callnum, threei_const::THREEI_DEREGISTER, 0, OP_REMOVE),
        0
    );
}

#[test]
#[serial]
fn registering_different_handler_replaces_existing_mapping() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;

    assert_eq!(register_simple(cage, callnum, 90, 1001, OP_ADD), 0);
    assert_eq!(register_simple(cage, callnum, 91, 1002, OP_ADD), 0);
    assert_eq!(register_simple(cage, callnum, 92, 1003, OP_ADD), 0);

    // Only one handler is retained for each (cage, syscall) pair.
    assert_eq!(mappings_for(cage, callnum), vec![(92, 1003)]);
}

#[test]
#[serial]
fn exiting_table_blocks_registration() {
    clear_globals();

    let cage = 7;
    let callnum = SYSCALL_FOO;
    let dest_grate = 99;
    let addr = 11;

    EXITING_TABLE.insert(cage);
    let rc = register_simple(cage, callnum, dest_grate, addr, OP_ADD);
    assert_eq!(rc, threei_const::ELINDESRCH as i32);

    EXITING_TABLE.remove(&cage);
    EXITING_TABLE.insert(dest_grate);

    let rc2 = register_simple(cage, callnum, dest_grate, addr, OP_ADD);
    assert_eq!(rc2, threei_const::ELINDESRCH as i32);

    assert!(mappings_for(cage, callnum).is_empty());
}

#[test]
#[serial]
fn cleanup_cage_removed_when_last_callnum_removed() {
    clear_globals();

    let cage = 7;

    assert_eq!(register_simple(cage, SYSCALL_FOO, 90, 1, OP_ADD), 0);
    assert_eq!(register_simple(cage, SYSCALL_BAR, 91, 2, OP_ADD), 0);

    assert_eq!(
        register_simple(
            cage,
            SYSCALL_FOO,
            threei_const::THREEI_DEREGISTER,
            0,
            OP_REMOVE,
        ),
        0,
    );

    #[cfg(feature = "hashmap")]
    {
        let tbl = HANDLERTABLE.lock().unwrap();
        assert!(tbl.get(&cage).is_some());
    }
    #[cfg(feature = "dashmap")]
    {
        assert!(HANDLERTABLE.get(&cage).is_some());
    }

    assert_eq!(
        register_simple(
            cage,
            SYSCALL_BAR,
            threei_const::THREEI_DEREGISTER,
            0,
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

    let rc = register_simple(cage, callnum, threei_const::THREEI_DEREGISTER, 0, OP_REMOVE);
    assert_eq!(rc, 0);
}

// ---------- [Copy_handlers] ----------

#[test]
#[serial]
fn copy_into_empty_target_copies_all() {
    clear_globals();

    let source = 7;
    let target = 8;

    assert_eq!(register_simple(source, SYSCALL_FOO, 90, 11, OP_ADD), 0);
    assert_eq!(register_simple(source, SYSCALL_BAR, 91, 12, OP_ADD), 0);

    assert_eq!(cpy(target, source), 0);

    assert_eq!(mappings_for(target, SYSCALL_FOO), vec![(90, 11)]);
    assert_eq!(mappings_for(target, SYSCALL_BAR), vec![(91, 12)]);
}

#[test]
#[serial]
fn copy_is_idempotent() {
    clear_globals();

    let src = 1002;
    let dst = 2002;

    assert_eq!(register_simple(src, SYSCALL_FOO, 90, 11, OP_ADD), 0);
    assert_eq!(cpy(dst, src), 0);
    assert_eq!(cpy(dst, src), 0);

    let mut got = mappings_for(dst, SYSCALL_FOO);
    got.sort_unstable();
    assert_eq!(got, vec![(90, 11)]);
}

#[test]
#[serial]
fn copy_overwrites_existing_target_handlers() {
    clear_globals();

    let source = 7;
    let target = 8;

    assert_eq!(register_simple(target, SYSCALL_FOO, 77, 10, OP_ADD), 0);
    assert_eq!(register_simple(target, SYSCALL_BAR, 78, 13, OP_ADD), 0);

    assert_eq!(register_simple(source, SYSCALL_FOO, 100, 12, OP_ADD), 0);

    assert_eq!(cpy(target, source), 0);

    // Copy replaces the target cage's complete handler table.
    assert_eq!(mappings_for(target, SYSCALL_FOO), vec![(100, 12)]);
    assert!(mappings_for(target, SYSCALL_BAR).is_empty());
}

#[test]
#[serial]
fn copy_preserves_multiple_source_callnums() {
    clear_globals();

    let source = 7;
    let target = 8;

    assert_eq!(register_simple(source, SYSCALL_FOO, 190, 21, OP_ADD), 0);
    assert_eq!(register_simple(source, SYSCALL_BAR, 191, 22, OP_ADD), 0);

    assert_eq!(cpy(target, source), 0);

    assert_eq!(mappings_for(target, SYSCALL_FOO), vec![(190, 21)]);
    assert_eq!(mappings_for(target, SYSCALL_BAR), vec![(191, 22)]);
}

#[test]
#[serial]
fn copy_returns_error_if_src_missing_table() {
    clear_globals();

    let src = 1005;
    let dst = 2005;

    let rc = cpy(dst, src);

    assert_eq!(rc, threei_const::ELINDAPIABORTED as u64);

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

#[test]
#[serial]
fn copy_returns_elindesrch_if_either_src_or_dst_exiting() {
    clear_globals();

    let src = 1006;
    let dst = 2006;

    assert_eq!(register_simple(src, SYSCALL_FOO, 90, 11, OP_ADD), 0);

    EXITING_TABLE.insert(src);
    let rc1 = cpy(dst, src);
    assert_eq!(rc1, threei_const::ELINDESRCH as u64);
    EXITING_TABLE.remove(&src);

    EXITING_TABLE.insert(dst);
    let rc2 = cpy(dst, src);
    assert_eq!(rc2, threei_const::ELINDESRCH as u64);
    EXITING_TABLE.remove(&dst);
}
