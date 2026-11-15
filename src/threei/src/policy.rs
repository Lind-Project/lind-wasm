//! Cage reference policy (3i design option 2b).
//!
//! 3i decides which cages a cage may *reference*: name as the target of a
//! syscall, name as the cage that owns a syscall argument, read or write with
//! `copy_data_between_cages`, or reroute with `register_handler` and
//! `copy_handler_table_to_cage`. Referencing is all-or-nothing: a cage that
//! may reference another may do all of these to it.
//!
//! ## Rule
//!
//! A cage may reference
//! - itself,
//! - its descendants in the fork tree, and
//! - every cage it has been granted access to with `grant_cage_access`, along
//!   with that cage's descendants, including ones forked after the grant.
//!
//! Everything else is denied.
//!
//! ## Grants
//!
//! A cage may grant access to a cage it *owns*: itself or one of its
//! descendants. Access held through a grant cannot be granted on, so revoking
//! a grant removes all access that came from it. A grant can be revoked by its
//! grantee (giving it up), by its granter, or by any cage that owns the granter.
//! A cage below the granter cannot revoke it.
//!
//! ## Checking
//!
//! `check_syscall` runs on every syscall a cage issues, before 3i routes it,
//! with the id of the cage that actually made the call. It also runs when a
//! grate forwards the call, with the grate as the caller. Checking the original
//! call matters: a grate that forwards a request on behalf of a cage it serves
//! would otherwise lend that cage its own reach.
//!
//! A call is checked when it enters 3i. Revoking a grant does not abort calls
//! already past the check.
//!
//! ## Representation
//!
//! Every decision is precomputed into `ACL`, one bitmap per subject cage with a
//! bit per actor cage, so the syscall path does one atomic load per foreign
//! cage id it sees, and none when a cage names only itself. Fork, exec, grant
//! and revoke update the inputs under `POLICY` and recompute the affected rows.
//! They are rare next to syscalls.
//!
//! Exits change nothing here. Cage ids are never reused (see
//! `cage::alloc_cage_id`), an exited cage makes no more calls, and calls that
//! name it fail in `make_syscall`. Keeping its fork-tree link means its
//! ancestors still own the descendants it leaves behind.
use crate::threei_const;
use once_cell::sync::Lazy;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use sysdefs::constants::err_const::Errno;
use sysdefs::constants::lind_platform_const::{INIT_CAGEID, MAX_CAGEID};
use sysdefs::lind_log;

const NCAGES: usize = MAX_CAGEID as usize;
/// Words in one ACL row: one bit per cage id.
const ROW_WORDS: usize = (NCAGES + 63) / 64;
type Row = [u64; ROW_WORDS];

/// `ACL[subject * ROW_WORDS + actor / 64]`, bit `actor % 64`: whether `actor`
/// may reference `subject`. A cage referencing itself is allowed without
/// consulting the table.
static ACL: Lazy<Box<[AtomicU64]>> =
    Lazy::new(|| (0..NCAGES * ROW_WORDS).map(|_| AtomicU64::new(0)).collect());

/// `grantee` may reference the subject cage and its descendants because
/// `granter` said so.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grant {
    grantee: u64,
    granter: u64,
}

/// The inputs `ACL` is computed from.
struct PolicyState {
    /// Fork-tree parent of each cage; `None` for the initial cage and for ids
    /// not handed out yet.
    parent: Vec<Option<u64>>,
    /// Whether the cage has exec'd since it was forked.
    execd: Vec<bool>,
    /// Grants on each subject cage.
    grants: Vec<Vec<Grant>>,
}

static POLICY: Lazy<Mutex<PolicyState>> = Lazy::new(|| Mutex::new(PolicyState::new()));

/// Whether `id` can name a cage. Cage ids start at `INIT_CAGEID` (1); 0 and the
/// virtual ids of RawPOSIX, Wasmtime and 3i (all >= `MAX_CAGEID`) cannot.
#[inline]
fn is_cageid(id: u64) -> bool {
    id != 0 && id < NCAGES as u64
}

fn load_row(cage: u64) -> Row {
    let base = cage as usize * ROW_WORDS;
    let mut row = [0; ROW_WORDS];
    for (i, word) in row.iter_mut().enumerate() {
        *word = ACL[base + i].load(Ordering::Acquire);
    }
    row
}

fn store_row(cage: u64, row: &Row) {
    let base = cage as usize * ROW_WORDS;
    for (i, word) in row.iter().enumerate() {
        ACL[base + i].store(*word, Ordering::Release);
    }
}

fn set_bit(row: &mut Row, cage: u64) {
    row[cage as usize / 64] |= 1 << (cage % 64);
}

impl PolicyState {
    fn new() -> Self {
        PolicyState {
            parent: vec![None; NCAGES],
            execd: vec![false; NCAGES],
            grants: vec![Vec::new(); NCAGES],
        }
    }

    /// Whether `actor` owns `subject`: is `subject` or one of its fork-tree
    /// ancestors. A grant does not make a cage an owner.
    fn owns(&self, actor: u64, subject: u64) -> bool {
        if !is_cageid(actor) || !is_cageid(subject) {
            return false;
        }
        let mut cur = subject;
        // Bounded so a corrupt parent chain cannot loop forever.
        for _ in 0..NCAGES {
            if cur == actor {
                return true;
            }
            match self.parent[cur as usize] {
                Some(p) => cur = p,
                None => return false,
            }
        }
        false
    }

    /// Whether `cage` is or was a cage: the initial cage or a forked one.
    fn is_known(&self, cage: u64) -> bool {
        is_cageid(cage) && (cage == INIT_CAGEID || self.parent[cage as usize].is_some())
    }

    /// The ACL row of `cage`: its parent's row plus the parent, plus its own
    /// grantees. Reads the parent's row from `ACL`, so the parent must be
    /// current.
    fn compute_row(&self, cage: u64) -> Row {
        let mut row = match self.parent[cage as usize] {
            Some(p) => {
                let mut row = load_row(p);
                set_bit(&mut row, p);
                row
            }
            None => [0; ROW_WORDS],
        };
        for g in &self.grants[cage as usize] {
            set_bit(&mut row, g.grantee);
        }
        row
    }

    /// Recompute the rows of `root` and all its descendants, parents first.
    fn recompute_subtree(&self, root: u64) {
        let mut children: Vec<Vec<u64>> = vec![Vec::new(); NCAGES];
        for (cage, parent) in self.parent.iter().enumerate() {
            if let Some(p) = parent {
                children[*p as usize].push(cage as u64);
            }
        }
        let mut seen = vec![false; NCAGES];
        let mut stack = vec![root];
        while let Some(cage) = stack.pop() {
            if std::mem::replace(&mut seen[cage as usize], true) {
                continue;
            }
            store_row(cage, &self.compute_row(cage));
            stack.extend_from_slice(&children[cage as usize]);
        }
    }
}

/// Whether `actor` may reference `subject`. This is the check on the syscall
/// path; it takes no lock.
#[inline]
pub fn may_reference(actor: u64, subject: u64) -> bool {
    if actor == subject {
        return true;
    }
    if !is_cageid(actor) || !is_cageid(subject) {
        return false;
    }
    let word = ACL[subject as usize * ROW_WORDS + actor as usize / 64].load(Ordering::Acquire);
    word & (1 << (actor % 64)) != 0
}

/// Whether `actor` owns `subject`: is `subject` or one of its fork-tree
/// ancestors. Only owners may grant access to a cage.
pub fn is_owner(actor: u64, subject: u64) -> bool {
    POLICY.lock().unwrap().owns(actor, subject)
}

/// Whether `child` was forked by `parent` and has not exec'd since, so it still
/// runs `parent`'s code.
pub fn is_unexecd_child_of(child: u64, parent: u64) -> bool {
    if !is_cageid(child) {
        return false;
    }
    let st = POLICY.lock().unwrap();
    st.parent[child as usize] == Some(parent) && !st.execd[child as usize]
}

/// Record that `parent` forked `child`. RawPOSIX calls this from fork before
/// the child runs. The child may be referenced by `parent` and by every cage
/// that may reference `parent`.
pub fn on_fork(parent: u64, child: u64) {
    if !is_cageid(parent) || !is_cageid(child) || parent == child {
        lind_log!(
            THREEI,
            "[3i|policy] on_fork ignored: parent={} child={}",
            parent,
            child
        );
        return;
    }
    let mut st = POLICY.lock().unwrap();
    st.parent[child as usize] = Some(parent);
    st.execd[child as usize] = false;
    st.grants[child as usize].clear();
    store_row(child, &st.compute_row(child));
}

/// Record that `cage` replaced its image with exec.
pub fn on_exec(cage: u64) {
    if is_cageid(cage) {
        POLICY.lock().unwrap().execd[cage as usize] = true;
    }
}

/// Let `grantee` reference `subject` and its descendants, on behalf of `actor`.
///
/// `actor` must own `subject`. `grantee` must be a cage other than `subject`.
///
/// ## Returns
/// - 0 on success, including when `actor` already made the same grant.
/// - `ELINDEPERM` if `actor` does not own `subject`.
/// - `ELINDAPIABORTED` if `grantee` is not a cage or is `subject`.
pub fn grant(actor: u64, subject: u64, grantee: u64) -> i32 {
    let mut st = POLICY.lock().unwrap();
    if !st.owns(actor, subject) {
        lind_log!(
            THREEI,
            "[3i|policy] grant denied: cage {} does not own cage {}",
            actor,
            subject
        );
        return threei_const::ELINDEPERM as i32;
    }
    if !st.is_known(grantee) || grantee == subject {
        return threei_const::ELINDAPIABORTED as i32;
    }
    let g = Grant {
        grantee,
        granter: actor,
    };
    if !st.grants[subject as usize].contains(&g) {
        st.grants[subject as usize].push(g);
        st.recompute_subtree(subject);
    }
    0
}

/// Withdraw the grants that let `grantee` reference `subject`, on behalf of
/// `actor`.
///
/// `actor` removes every such grant it may revoke: all of them if `actor` is
/// the grantee, otherwise those whose granter `actor` owns. A grant made by a
/// cage above `actor` stays, and so does the access it gives.
///
/// ## Returns
/// - 0 if at least one grant was removed.
/// - `ELINDEPERM` if `actor` is neither the grantee nor an owner of
///   `subject`, or may revoke none of the grants.
/// - `ELINDAPIABORTED` if `grantee` holds no grant on `subject`.
pub fn revoke(actor: u64, subject: u64, grantee: u64) -> i32 {
    if !is_cageid(subject) {
        return threei_const::ELINDAPIABORTED as i32;
    }
    let mut st = POLICY.lock().unwrap();
    // Only the grantee and the owners of `subject` learn whether a grant exists.
    if actor != grantee && !st.owns(actor, subject) {
        return threei_const::ELINDEPERM as i32;
    }
    let s = subject as usize;
    if !st.grants[s].iter().any(|g| g.grantee == grantee) {
        return threei_const::ELINDAPIABORTED as i32;
    }
    let mut grants = std::mem::take(&mut st.grants[s]);
    let before = grants.len();
    grants.retain(|g| !(g.grantee == grantee && (actor == grantee || st.owns(actor, g.granter))));
    let removed = grants.len() < before;
    st.grants[s] = grants;
    if !removed {
        lind_log!(
            THREEI,
            "[3i|policy] revoke denied: cage {} owns no granter of cage {} on cage {}",
            actor,
            grantee,
            subject
        );
        return threei_const::ELINDEPERM as i32;
    }
    st.recompute_subtree(subject);
    0
}

/// Forget all lineage and grants. For runtime teardown and tests.
pub fn reset_policy() {
    let mut st = POLICY.lock().unwrap();
    *st = PolicyState::new();
    for word in ACL.iter() {
        word.store(0, Ordering::Release);
    }
}

/// Whether `syscall_num` is one of 3i's own calls, which return raw ELIND
/// codes instead of `-errno`.
fn is_threei_call(syscall_num: u64) -> bool {
    matches!(
        syscall_num,
        threei_const::REGISTER_HANDLER_SYSCALL
            | threei_const::COPY_DATA_BETWEEN_CAGES_SYSCALL
            | threei_const::COPY_HANDLER_TABLE_TO_CAGE_SYSCALL
            | threei_const::GRANT_CAGE_ACCESS_SYSCALL
            | threei_const::REVOKE_CAGE_ACCESS_SYSCALL
    )
}

/// Decide whether `caller` may make this syscall, before 3i routes it.
///
/// `caller` must be the cage that actually issued the call; the runtime
/// authenticates it (see `make_syscall_from_cage`). An ordinary syscall names
/// cages in `target_cageid` and in the `argN_cageid` slots. `target_cageid`
/// must be referenceable. An `argN_cageid` must be referenceable if it can name
/// a cage at all; unused slots carry values that cannot (0, `UNUSED_ID`).
///
/// 3i's own calls carry cage ids in other slots, laid out by glibc's wrappers
/// in `lind_syscall.c`, and ignore `target_cageid`:
/// - `register_handler`: the caller must be able to reference `targetcage`
///   (arg1) and the handler cage (arg2_cageid), unless it deregisters. A forked
///   child that has not exec'd may also route its own calls to its parent,
///   since it still runs the parent's code; grates register this way between
///   fork and exec.
/// - `copy_data_between_cages`: source (arg1_cageid) and destination
///   (arg2_cageid) must both be referenceable.
/// - `copy_handler_table_to_cage`: source (arg1) and target (arg1_cageid) must
///   both be referenceable. Otherwise a cage could copy in the table of the
///   grate above it and step around that grate.
/// - `grant_cage_access`: the caller must own the subject (arg1).
/// - `revoke_cage_access`: the caller must own the subject (arg1) or be the
///   grantee (arg2). The call itself decides which grants go.
///
/// ## Returns
/// `Ok(())` to proceed, or `Err(code)` with what the call returns instead:
/// `ELINDEPERM` for 3i's own calls, `-EPERM` for the rest.
pub fn check_syscall(
    caller: u64,
    syscall_num: u64,
    target_cageid: u64,
    arg1: u64,
    arg1_cageid: u64,
    arg2: u64,
    arg2_cageid: u64,
    arg3_cageid: u64,
    arg4_cageid: u64,
    arg5_cageid: u64,
    arg6_cageid: u64,
) -> Result<(), i32> {
    let allowed = match syscall_num {
        threei_const::REGISTER_HANDLER_SYSCALL => {
            let (targetcage, handlefunccage) = (arg1, arg2_cageid);
            may_reference(caller, targetcage)
                && (handlefunccage == threei_const::THREEI_DEREGISTER
                    || may_reference(caller, handlefunccage)
                    || (targetcage == caller && is_unexecd_child_of(caller, handlefunccage)))
        }
        threei_const::COPY_DATA_BETWEEN_CAGES_SYSCALL => {
            may_reference(caller, arg1_cageid) && may_reference(caller, arg2_cageid)
        }
        threei_const::COPY_HANDLER_TABLE_TO_CAGE_SYSCALL => {
            may_reference(caller, arg1) && may_reference(caller, arg1_cageid)
        }
        threei_const::GRANT_CAGE_ACCESS_SYSCALL => is_owner(caller, arg1),
        threei_const::REVOKE_CAGE_ACCESS_SYSCALL => caller == arg2 || is_owner(caller, arg1),
        _ => {
            may_reference(caller, target_cageid)
                && [
                    arg1_cageid,
                    arg2_cageid,
                    arg3_cageid,
                    arg4_cageid,
                    arg5_cageid,
                    arg6_cageid,
                ]
                .iter()
                .all(|&c| c == caller || !is_cageid(c) || may_reference(caller, c))
        }
    };
    if allowed {
        return Ok(());
    }
    lind_log!(
        THREEI,
        "[3i|policy] denied: cage {} syscall {} target {} arg1 {} cages [{} {} {} {} {} {}]",
        caller,
        syscall_num,
        target_cageid,
        arg1,
        arg1_cageid,
        arg2_cageid,
        arg3_cageid,
        arg4_cageid,
        arg5_cageid,
        arg6_cageid
    );
    if is_threei_call(syscall_num) {
        Err(threei_const::ELINDEPERM as i32)
    } else {
        Err(-(Errno::EPERM as i32))
    }
}
