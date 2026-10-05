//! V2 (variable-width) library-call registration table.
//!
//! Parallel to `lib_handler_table.rs`'s V1 table, but the stored identity is
//! deliberately NOT a raw `(handler_cage_id, fn_ptr)` pair: a grate-linear-
//! memory address is meaningless across the FFI trampoline boundary a V2
//! call crosses, and would force that boundary to reinterpret an opaque
//! integer as a live Rust reference. An interned `u64` id resolved back
//! through this table's own registry sidesteps that entirely. This table
//! also carries the checked signature/version metadata a V1 registration
//! never needed, since V1's fixed six-slot shape made per-symbol signature
//! checking unnecessary.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::lib_call_v2::{V2Signature, V2ValueType};

/// One symbol's V2 registration: which grate cage owns it, the exported
/// adapter function's name inside that grate's module (resolved by name at
/// worker-creation time, never by address -- see the module doc), the
/// manifest version the registration was produced against, and the
/// authoritative lowered signature the resolved export must match exactly.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct V2Registration {
    pub grate_cage: u64,
    pub adapter_export: String,
    pub manifest_version: u32,
    pub signature: V2Signature,
    /// Gate 0 (cross-cage function-pointer callbacks) scaffolding: 0-based
    /// indices, into `signature.params`, of parameters the caller's raw i32
    /// is a table index into the CALLING cage's own indirect-function table
    /// rather than an ordinary scalar. Deliberately not part of
    /// `V2Signature`: that type's equality is the WASM-level value-type
    /// identity checked against a caller's independently-derived signature
    /// (`v2_signature_from_func_ty` in `linker.rs`), which has no way to
    /// know about callback semantics and would then never match. Empty for
    /// every registration that isn't callback-aware -- the normal case,
    /// checked once at portal-install time so calls to every other
    /// interposed function pay nothing for this.
    pub callback_params: Vec<u32>,
}

fn lib_handler_table_v2() -> &'static Mutex<HashMap<u64, HashMap<(String, String), u64>>> {
    static TABLE: OnceLock<Mutex<HashMap<u64, HashMap<(String, String), u64>>>> = OnceLock::new();
    TABLE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Interned registry backing every V2 registration's stable identity: a
/// plain `u64` id a portal captures at link time and later hands to
/// `dispatch_lib_call_v2`, which passes it across the `GrateTrampolineFnV2`
/// FFI boundary as `handler_id`. An id, not a raw `Arc` pointer cast to
/// `u64`, so that boundary never needs to reinterpret an opaque integer as a
/// live Rust reference -- the runtime-side trampoline looks the id back up
/// through this same safe table instead.
fn v2_registration_registry() -> &'static Mutex<HashMap<u64, Arc<V2Registration>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<u64, Arc<V2Registration>>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_v2_handler_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Which cages currently hold a LIVE reference to each V2 registration id,
/// and the reverse index used to release them all at once on cage exit.
///
/// A cage holds a reference to an id for either of two reasons, both tied to
/// that SAME cage's own lifetime:
///   - its own (lib_name, symbol_name) -> id table entry could still be
///     looked up by a future portal install (e.g. on exec/dlopen replay);
///   - it already installed a portal (`Linker::instance_dylink`'s V2 portal
///     in `linker.rs`) whose closure captured this id directly.
/// The second kind is invisible to (and outlives) the table: re-registering
/// the same (lib_name, symbol_name) key later never touches an
/// already-installed portal's captured id (see `register_lib_handler_v2_entry`'s
/// own doc), so a table overwrite must never be treated as releasing a
/// reference. Both kinds are therefore only ever added during a cage's
/// lifetime and released together, in bulk, at that cage's exit -- see
/// `add_v2_registration_ref`/`release_v2_registration_refs`.
struct V2RefState {
    /// id -> cage_ids holding a reference to it.
    holders: HashMap<u64, std::collections::HashSet<u64>>,
    /// cage_id -> ids it holds a reference to (reverse index).
    by_cage: HashMap<u64, std::collections::HashSet<u64>>,
}

fn v2_ref_state() -> &'static Mutex<V2RefState> {
    static STATE: OnceLock<Mutex<V2RefState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(V2RefState {
            holders: HashMap::new(),
            by_cage: HashMap::new(),
        })
    })
}

/// Records that `cage_id` now holds a live reference to registration `id`.
/// Idempotent -- safe to call more than once for the same (id, cage_id)
/// pair (e.g. a table-registration ref and a later install-time ref for the
/// same cage collapse into the same membership, since both die together at
/// that cage's exit anyway).
pub fn add_v2_registration_ref(id: u64, cage_id: u64) {
    let mut st = v2_ref_state().lock().unwrap();
    st.holders.entry(id).or_default().insert(cage_id);
    st.by_cage.entry(cage_id).or_default().insert(id);
}

/// Releases every reference `cage_id` holds -- from its own table entries
/// and from any portal it installed -- and reclaims the interned
/// registration for any id left with no remaining holder. Called once at
/// cage exit, alongside `rm_cage_from_lib_handler_table_v2`.
///
/// A registration is intentionally NOT reclaimed just because its owning
/// cage's table entries were removed or overwritten -- only cage exit
/// (this function) can drop a reference, since only cage exit guarantees no
/// live installed portal for that cage can still call it.
pub fn release_v2_registration_refs(cage_id: u64) {
    let mut reclaimable: Vec<u64> = Vec::new();
    {
        let mut st = v2_ref_state().lock().unwrap();
        if let Some(ids) = st.by_cage.remove(&cage_id) {
            for id in ids {
                if let Some(set) = st.holders.get_mut(&id) {
                    set.remove(&cage_id);
                    if set.is_empty() {
                        st.holders.remove(&id);
                        reclaimable.push(id);
                    }
                }
            }
        }
    }
    if !reclaimable.is_empty() {
        let mut registry = v2_registration_registry().lock().unwrap();
        for id in reclaimable {
            registry.remove(&id);
        }
    }
}

/// Test-only introspection: the current holder set for `id`, or `None` if
/// nothing references it (either never registered, or fully reclaimed).
#[cfg(test)]
fn v2_registration_holders(id: u64) -> Option<std::collections::HashSet<u64>> {
    v2_ref_state().lock().unwrap().holders.get(&id).cloned()
}

/// Register a (lib_name, symbol_name) -> V2Registration mapping for cage_id.
/// Each call mints a fresh handler id; a symbol registered again later gets
/// a new id and registration, matching V1's own "captured once at link
/// time" behavior -- a portal already installed against an older id keeps
/// working against the registration it was actually installed for.
pub fn register_lib_handler_v2_entry(
    cage_id: u64,
    lib_name: &str,
    symbol_name: &str,
    registration: V2Registration,
) {
    let id = next_v2_handler_id();
    v2_registration_registry()
        .lock()
        .unwrap()
        .insert(id, Arc::new(registration));
    add_v2_registration_ref(id, cage_id);

    let mut table = lib_handler_table_v2().lock().unwrap();
    table
        .entry(cage_id)
        .or_default()
        .insert((lib_name.to_string(), symbol_name.to_string()), id);
}

/// Look up the (handler id, V2 registration) for (cage_id, lib_name,
/// symbol_name). The id is what a portal passes on to
/// `dispatch_lib_call_v2`/`GrateTrampolineFnV2` as `handler_id`.
pub fn get_lib_handler_v2(
    cage_id: u64,
    lib_name: &str,
    symbol_name: &str,
) -> Option<(u64, Arc<V2Registration>)> {
    let id = {
        let table = lib_handler_table_v2().lock().unwrap();
        *table
            .get(&cage_id)?
            .get(&(lib_name.to_string(), symbol_name.to_string()))?
    };
    let registration = v2_registration_registry().lock().unwrap().get(&id)?.clone();
    Some((id, registration))
}

/// Resolve a handler id (as produced by `get_lib_handler_v2`, carried
/// through `dispatch_lib_call_v2`) back to its `V2Registration`. Called by
/// the runtime-side `GrateTrampolineFnV2` implementation.
pub fn get_v2_registration_by_id(id: u64) -> Option<Arc<V2Registration>> {
    v2_registration_registry().lock().unwrap().get(&id).cloned()
}

/// Remove all V2 lib handler entries for cage_id. Called on cage exit/cleanup.
///
/// Only removes this cage's OWN (lib_name, symbol_name) -> id mappings, not
/// the interned registrations those ids point to -- a registration id may
/// still be referenced by a portal already installed in some OTHER cage
/// (the caller cage that had this symbol interposed), so it is left in the
/// registry rather than invalidated here. `dispatch_lib_call_v2`'s existing
/// cage-liveness check (the handler/grate cage, not the registering cage)
/// is what actually rejects a call whose GRATE has exited.
pub fn rm_cage_from_lib_handler_table_v2(cage_id: u64) {
    let mut table = lib_handler_table_v2().lock().unwrap();
    table.remove(&cage_id);
}

/// Copy all V2 lib handler entries from src_cage_id to dst_cage_id. Called by
/// `fork_syscall` so a forked child cage inherits the parent's registered V2
/// handlers -- see `baseline-v1-library-call-transport.md`'s "Fork-copy and
/// teardown: two confirmed gaps" for why V1's own equivalent function exists
/// but was never actually wired into fork, a gap V2 does not repeat.
pub fn copy_lib_handler_table_v2_to_cage(src_cage_id: u64, dst_cage_id: u64) {
    let mut table = lib_handler_table_v2().lock().unwrap();
    if let Some(src_map) = table.get(&src_cage_id).cloned() {
        // The child cage is now itself a holder of every id it inherited --
        // without this, if only the PARENT later exits, `dst_cage_id`'s own
        // (now-copied) table entries and any portal it later installs from
        // them would reference an id that could already have been reclaimed.
        for &id in src_map.values() {
            add_v2_registration_ref(id, dst_cage_id);
        }
        table.insert(dst_cage_id, src_map);
    }
}

#[cfg(test)]
mod reclaim_tests {
    use super::*;

    // Every id minted by register_lib_handler_v2_entry is globally unique
    // (a process-wide AtomicU64 counter), so registrations from different
    // tests never collide even though these tests share the SAME process-
    // global tables/registry (cargo runs unit tests in this binary in
    // parallel by default). Cage ids only need to be unique WITHIN a test
    // to keep its own reasoning simple, so each test picks its own
    // disjoint small range via this counter.
    fn fresh_cage_id() -> u64 {
        static NEXT: AtomicU64 = AtomicU64::new(1_000_000);
        NEXT.fetch_add(1, Ordering::Relaxed)
    }

    fn dummy_registration() -> V2Registration {
        V2Registration {
            grate_cage: fresh_cage_id(),
            adapter_export: "__lind_v2_adapter_test".to_string(),
            manifest_version: 1,
            signature: V2Signature {
                params: vec![],
                results: vec![],
            },
            callback_params: vec![],
        }
    }

    #[test]
    fn registration_is_reclaimed_after_its_only_cage_exits() {
        let cage = fresh_cage_id();
        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (id, _reg) = get_lib_handler_v2(cage, "libtest", "sym_a").expect("just registered");
        assert!(get_v2_registration_by_id(id).is_some());
        assert_eq!(
            v2_registration_holders(id),
            Some([cage].into_iter().collect())
        );

        rm_cage_from_lib_handler_table_v2(cage);
        release_v2_registration_refs(cage);

        assert!(
            get_v2_registration_by_id(id).is_none(),
            "registration must be reclaimed"
        );
        assert_eq!(v2_registration_holders(id), None);
    }

    #[test]
    fn re_registration_keeps_the_old_id_alive_until_this_cage_exits() {
        // Mirrors register_lib_handler_v2_entry's own doc: re-registering
        // the same (cage, lib, symbol) mints a NEW id: an already-installed
        // portal captured the OLD id directly and keeps working against it,
        // so the old id must remain resolvable until THIS cage exits, not
        // be dropped the moment the table entry is overwritten.
        let cage = fresh_cage_id();
        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (old_id, _) = get_lib_handler_v2(cage, "libtest", "sym_a").unwrap();

        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (new_id, _) = get_lib_handler_v2(cage, "libtest", "sym_a").unwrap();
        assert_ne!(old_id, new_id);

        // The table now only points at new_id, but old_id -- as if an
        // earlier-installed portal still captured it -- must stay resolvable.
        assert!(get_v2_registration_by_id(old_id).is_some());
        assert!(get_v2_registration_by_id(new_id).is_some());

        rm_cage_from_lib_handler_table_v2(cage);
        release_v2_registration_refs(cage);

        assert!(get_v2_registration_by_id(old_id).is_none());
        assert!(get_v2_registration_by_id(new_id).is_none());
    }

    #[test]
    fn fork_copied_cage_keeps_registration_alive_after_parent_exits() {
        let parent = fresh_cage_id();
        let child = fresh_cage_id();
        register_lib_handler_v2_entry(parent, "libtest", "sym_a", dummy_registration());
        let (id, _) = get_lib_handler_v2(parent, "libtest", "sym_a").unwrap();

        copy_lib_handler_table_v2_to_cage(parent, child);
        assert!(get_lib_handler_v2(child, "libtest", "sym_a").is_some());

        // Parent exits first: the child inherited its own reference at
        // fork-copy time, so the registration must survive.
        rm_cage_from_lib_handler_table_v2(parent);
        release_v2_registration_refs(parent);
        assert!(
            get_v2_registration_by_id(id).is_some(),
            "child's inherited reference must keep the registration alive"
        );

        // Only once the child ALSO exits is the registration unreachable.
        rm_cage_from_lib_handler_table_v2(child);
        release_v2_registration_refs(child);
        assert!(get_v2_registration_by_id(id).is_none());
    }

    #[test]
    fn install_time_reference_survives_a_later_table_overwrite() {
        // Simulates: cage installs a portal for id X (add_v2_registration_ref,
        // as linker.rs does at portal-creation time), then the SAME cage's
        // table entry for that (lib, symbol) is later overwritten by a fresh
        // registration (id Y). X must remain resolvable until the cage
        // itself exits -- an already-installed portal's captured id must
        // never be silently invalidated by an unrelated table overwrite.
        let cage = fresh_cage_id();
        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (old_id, _) = get_lib_handler_v2(cage, "libtest", "sym_a").unwrap();
        add_v2_registration_ref(old_id, cage); // portal install, redundant with the table ref

        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (new_id, _) = get_lib_handler_v2(cage, "libtest", "sym_a").unwrap();
        assert_ne!(old_id, new_id);

        assert!(get_v2_registration_by_id(old_id).is_some());

        rm_cage_from_lib_handler_table_v2(cage);
        release_v2_registration_refs(cage);
        assert!(get_v2_registration_by_id(old_id).is_none());
        assert!(get_v2_registration_by_id(new_id).is_none());
    }

    #[test]
    fn repeated_cage_id_reuse_starts_clean() {
        // A cage id reused after a prior generation fully exited must not
        // inherit any stale reference bookkeeping from that earlier
        // generation.
        let cage = fresh_cage_id();
        register_lib_handler_v2_entry(cage, "libtest", "sym_a", dummy_registration());
        let (id1, _) = get_lib_handler_v2(cage, "libtest", "sym_a").unwrap();
        rm_cage_from_lib_handler_table_v2(cage);
        release_v2_registration_refs(cage);
        assert_eq!(v2_registration_holders(id1), None);

        // Same numeric cage id, fresh "generation".
        register_lib_handler_v2_entry(cage, "libtest", "sym_b", dummy_registration());
        let (id2, _) = get_lib_handler_v2(cage, "libtest", "sym_b").unwrap();
        assert_eq!(
            v2_registration_holders(id2),
            Some([cage].into_iter().collect())
        );
        rm_cage_from_lib_handler_table_v2(cage);
        release_v2_registration_refs(cage);
        assert!(get_v2_registration_by_id(id2).is_none());
    }
}

/// Parses one type character into a `V2ValueType`: `i`=I32, `l`=I64 ("long"),
/// `f`=F32, `d`=F64 (double). Case matters; anything else is unrecognized.
fn parse_v2_type_char(c: char) -> Option<V2ValueType> {
    match c {
        'i' => Some(V2ValueType::I32),
        'l' => Some(V2ValueType::I64),
        'f' => Some(V2ValueType::F32),
        'd' => Some(V2ValueType::F64),
        _ => None,
    }
}

/// Parses a compact signature descriptor string of the form
/// `"<manifest_version>:<params>:<results>[:<callback_params>]"`, where
/// `params`/`results` are each a (possibly empty) run of type characters
/// (see `parse_v2_type_char`) -- e.g. `"2:iid:d"` is manifest version 2,
/// params `[I32, I32, F64]`, results `[F64]`.
///
/// The optional 4th segment is Gate 0 (cross-cage function-pointer
/// callbacks) scaffolding: a comma-separated list of 0-based parameter
/// indices whose raw i32 is a table index into the CALLING cage's own
/// indirect-function table, e.g. `"1:i::0"` marks parameter 0 of a
/// one-param, void-result function. Absent or empty means no callback
/// parameters -- every pre-existing 3-segment descriptor continues to parse
/// identically to before this segment existed.
///
/// A raw `extern "C"` syscall (see `register_lib_handler_v2` below) has a
/// fixed six-raw-argument-pair shape, the exact width limitation V2 exists
/// to work around for LIBRARY calls -- it cannot itself carry a variable-
/// length params/results list as separate slots. Encoding the whole
/// signature as one compact string keeps registration a single extra
/// pointer argument, reusing the same "pointer to a string the syscall
/// dispatch layer already translates to a host address" mechanism
/// `lib_name_ptr`/`symbol_name_ptr` already rely on, instead of inventing a
/// new argument-passing mechanism just for this one call.
fn parse_v2_signature_desc(s: &str) -> Option<(u32, V2Signature, Vec<u32>)> {
    let mut parts = s.splitn(4, ':');
    let version: u32 = parts.next()?.parse().ok()?;
    let params_str = parts.next()?;
    let results_str = parts.next()?;
    let callback_params_str = parts.next().unwrap_or("");
    let params = params_str
        .chars()
        .map(parse_v2_type_char)
        .collect::<Option<Vec<_>>>()?;
    let results = results_str
        .chars()
        .map(parse_v2_type_char)
        .collect::<Option<Vec<_>>>()?;
    let callback_params = if callback_params_str.is_empty() {
        Vec::new()
    } else {
        callback_params_str
            .split(',')
            .map(|p| p.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()?
    };
    Some((version, V2Signature { params, results }, callback_params))
}

/// Register a V2 (variable-width) library-level handler for
/// (lib_name, symbol_name) in target_cage. The syscall-shaped counterpart to
/// V1's `register_lib_handler` (`threei::register_lib_handler`), following
/// the same make_syscall argument convention.
///
/// Arguments:
///   arg1 = target_cage_id     -- cage whose library calls are being intercepted
///   arg2 = lib_name_ptr       -- host pointer to a NUL-terminated library name
///   arg3 = symbol_name_ptr    -- host pointer to a NUL-terminated symbol name
///   arg4 = handler_cage_id    -- grate cage that will handle the call
///   arg5 = adapter_export_ptr -- host pointer to the generated adapter's
///                                exported wasm function name (NUL-terminated)
///   arg6 = signature_desc_ptr -- host pointer to a NUL-terminated signature
///                                descriptor (see `parse_v2_signature_desc`)
pub fn register_lib_handler_v2(
    _self_cageid: u64,
    _target_cageid: u64,
    target_cage_id: u64,
    _arg1cage: u64,
    lib_name_ptr: u64,
    _arg2cage: u64,
    symbol_name_ptr: u64,
    _arg3cage: u64,
    handler_cage_id: u64,
    _arg4cage: u64,
    adapter_export_ptr: u64,
    _arg5cage: u64,
    signature_desc_ptr: u64,
    _arg6cage: u64,
) -> i32 {
    if lib_name_ptr == 0
        || symbol_name_ptr == 0
        || adapter_export_ptr == 0
        || signature_desc_ptr == 0
    {
        eprintln!("[3i|register_lib_handler_v2] null string pointer");
        return -1;
    }

    let read_cstr = |ptr: u64, what: &str| -> Option<String> {
        match unsafe { std::ffi::CStr::from_ptr(ptr as *const i8).to_str() } {
            Ok(s) => Some(s.to_string()),
            Err(_) => {
                eprintln!("[3i|register_lib_handler_v2] invalid {what} UTF-8");
                None
            }
        }
    };

    let Some(lib_name) = read_cstr(lib_name_ptr, "lib_name") else {
        return -1;
    };
    let Some(symbol_name) = read_cstr(symbol_name_ptr, "symbol_name") else {
        return -1;
    };
    let Some(adapter_export) = read_cstr(adapter_export_ptr, "adapter_export") else {
        return -1;
    };
    let Some(signature_desc) = read_cstr(signature_desc_ptr, "signature_desc") else {
        return -1;
    };

    let Some((manifest_version, signature, callback_params)) =
        parse_v2_signature_desc(&signature_desc)
    else {
        eprintln!(
            "[3i|register_lib_handler_v2] malformed signature descriptor: {signature_desc:?}"
        );
        return -1;
    };

    register_lib_handler_v2_entry(
        target_cage_id,
        &lib_name,
        &symbol_name,
        V2Registration {
            grate_cage: handler_cage_id,
            adapter_export,
            manifest_version,
            signature,
            callback_params,
        },
    );

    0
}
