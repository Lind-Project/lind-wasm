// Typed adapter and registration tests (issue #22). Proves
// wasmtime_lind_3i::v2_adapter's resolve/validate/cache/dynamic-call
// mechanism against real Wasmtime `Instance`s.
//
// Fixtures are raw WAT text, not C compiled through the Lind toolchain: this
// needs a combinatorial set of DELIBERATELY mismatched modules (missing
// export, wrong arity/types, a missing or wrong manifest-version marker,
// and -- for the "unsupported type" case -- a v128 param, which the C
// toolchain cannot produce at all). See Cargo.toml's dev-dependency comment
// and CLAUDE.md's toolchain-exception clause; same justification already
// accepted for src/wasmtime/crates/lib3i-portal-signature-check.
use threei::{V2Registration, V2Signature, V2ValueType};
use wasmtime::{Engine, Instance, Linker, Module, Store, Val};
use wasmtime_lind_3i::v2_adapter::{
    MANIFEST_VERSION_EXPORT, V2AdapterCache, V2AdapterRejection, call_v2_adapter,
};

const MANIFEST_VERSION: u32 = 2;

fn wat_type(t: V2ValueType) -> &'static str {
    match t {
        V2ValueType::I32 => "i32",
        V2ValueType::I64 => "i64",
        V2ValueType::F32 => "f32",
        V2ValueType::F64 => "f64",
    }
}

fn zero_const(t: V2ValueType) -> &'static str {
    match t {
        V2ValueType::I32 => "i32.const 0",
        V2ValueType::I64 => "i64.const 0",
        V2ValueType::F32 => "f32.const 0",
        V2ValueType::F64 => "f64.const 0",
    }
}

/// A self-contained module (no imports) whose adapter export drops every
/// argument and pushes a fixed zero-ish value per declared result -- valid
/// for ANY param/result type mix, used by every test that only exercises
/// resolution/validation and never actually calls the adapter. `params`/
/// `results` are the LOGICAL shape; every real V2 adapter's actual wasm
/// signature additionally has two leading `i64` params (source_cage,
/// grate_cage -- see `V2_ADAPTER_LEADING_PARAMS`), prepended here too so
/// `resolve`'s stripping logic has something real to strip.
fn gen_trivial_adapter_wat(
    export_name: &str,
    params: &[V2ValueType],
    results: &[V2ValueType],
    manifest_version: u32,
) -> String {
    let full_types: Vec<&str> = std::iter::repeat("i64")
        .take(2)
        .chain(params.iter().map(|t| wat_type(*t)))
        .collect();
    let param_decl = format!("(param {})", full_types.join(" "));
    let result_decl = if results.is_empty() {
        String::new()
    } else {
        format!(
            "(result {})",
            results
                .iter()
                .map(|t| wat_type(*t))
                .collect::<Vec<_>>()
                .join(" ")
        )
    };
    let drops: String = (0..full_types.len())
        .map(|_| "drop\n    ".to_string())
        .collect::<String>();
    let pushes: String = results
        .iter()
        .map(|t| format!("{}\n    ", zero_const(*t)))
        .collect::<String>();
    format!(
        r#"(module
  (func (export "{MANIFEST_VERSION_EXPORT}") (result i32) i32.const {manifest_version})
  (func (export "{export_name}") {param_decl} {result_decl}
    {args_setup}
    {drops}{pushes})
)"#,
        args_setup = (0..full_types.len())
            .map(|i| format!("local.get {i}"))
            .collect::<Vec<_>>()
            .join("\n    "),
    )
}

/// A module whose adapter export XOR-folds N logical `i64` parameters into
/// one `i64` result entirely in Wasm, no import needed -- used to prove
/// real values reach and return from a genuinely executed adapter at
/// arbitrary arity (0, 1, 6, 7, 14, and larger). Two leading `i64` params
/// (source_cage, grate_cage) come first, per the same real convention
/// `gen_trivial_adapter_wat` follows, and are never read by the fold body
/// (unused wasm params need no explicit handling).
fn gen_fold_adapter_wat(export_name: &str, argc: usize, manifest_version: u32) -> String {
    let param_decl = format!("(param {})", vec!["i64"; 2 + argc].join(" "));
    let mut body = String::from("i64.const 0\n");
    for i in 0..argc {
        // Logical arg i lives at local index i+2 (indices 0,1 are the
        // leading source_cage/grate_cage context params).
        body.push_str(&format!("    local.get {}\n    i64.xor\n", i + 2));
    }
    format!(
        r#"(module
  (func (export "{MANIFEST_VERSION_EXPORT}") (result i32) i32.const {manifest_version})
  (func (export "{export_name}") {param_decl} (result i64)
    {body})
)"#
    )
}

fn instantiate_no_imports(wat: &str) -> (Store<()>, Instance) {
    let engine = Engine::default();
    let module = Module::new(&engine, wat).unwrap();
    let mut store = Store::new(&engine, ());
    let instance = Instance::new(&mut store, &module, &[]).unwrap();
    (store, instance)
}

fn registration(export: &str, params: &[V2ValueType], results: &[V2ValueType]) -> V2Registration {
    V2Registration {
        grate_cage: 1,
        adapter_export: export.to_string(),
        manifest_version: MANIFEST_VERSION,
        signature: V2Signature {
            params: params.to_vec(),
            results: results.to_vec(),
        },
        callback_params: vec![],
    }
}

#[test]
fn resolves_and_calls_scalar_adapters_at_varying_arity() {
    for &argc in &[0usize, 1, 6, 7, 14, 40] {
        let export = format!("adapter_{argc}");
        let wat = gen_fold_adapter_wat(&export, argc, MANIFEST_VERSION);
        let (mut store, instance) = instantiate_no_imports(&wat);
        let params = vec![V2ValueType::I64; argc];
        let results = vec![V2ValueType::I64];
        let reg = registration(&export, &params, &results);

        let mut cache = V2AdapterCache::new();
        let adapter = cache
            .resolve(&mut store, &instance, &reg)
            .unwrap_or_else(|e| panic!("argc={argc}: expected resolution to succeed, got {e}"));
        assert_eq!(adapter.signature.params.len(), argc);

        let args: Vec<Val> = (0..argc).map(|i| Val::I64(i as i64 + 1)).collect();
        let expected: i64 = (1..=argc as i64).fold(0, |a, b| a ^ b);
        let results = call_v2_adapter(&mut store, adapter, 111, 222, &args).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].i64().unwrap(), expected, "argc={argc}");

        // A second resolve for the same export must hit the cache rather
        // than fail or re-derive -- cheap to assert indirectly: it still
        // succeeds and produces the identical result.
        let adapter2 = cache.resolve(&mut store, &instance, &reg).unwrap();
        let results2 = call_v2_adapter(&mut store, adapter2, 111, 222, &args).unwrap();
        assert_eq!(results2[0].i64().unwrap(), expected);
    }
}

#[test]
fn argument_7_and_14_each_independently_affect_the_result() {
    let argc = 20;
    let export = "adapter_20";
    let wat = gen_fold_adapter_wat(export, argc, MANIFEST_VERSION);
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration(export, &vec![V2ValueType::I64; argc], &[V2ValueType::I64]);
    let mut cache = V2AdapterCache::new();
    let adapter = cache.resolve(&mut store, &instance, &reg).unwrap();

    let base_args: Vec<Val> = (0..argc).map(|i| Val::I64(i as i64 + 1)).collect();
    let base_result = call_v2_adapter(&mut store, adapter, 111, 222, &base_args).unwrap()[0]
        .i64()
        .unwrap();

    for &flip in &[6usize, 13] {
        let mut args = base_args.clone();
        args[flip] = Val::I64(args[flip].i64().unwrap() ^ 0xFF);
        let result = call_v2_adapter(&mut store, adapter, 111, 222, &args).unwrap()[0]
            .i64()
            .unwrap();
        assert_ne!(
            result, base_result,
            "flipping argument index {flip} must change the result"
        );
    }
}

#[test]
fn missing_export_is_rejected() {
    let wat = gen_trivial_adapter_wat("real_export", &[V2ValueType::I32], &[], MANIFEST_VERSION);
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration("totally_different_name", &[V2ValueType::I32], &[]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::MissingExport { export }) => {
            assert_eq!(export, "totally_different_name")
        }
        other => panic!("expected MissingExport, got {other:?}"),
    }
}

#[test]
fn missing_version_marker_is_rejected() {
    let wat = r#"(module
  (func (export "adapter") (param i32) (result i32)
    local.get 0)
)"#;
    let (mut store, instance) = instantiate_no_imports(wat);
    let reg = registration("adapter", &[V2ValueType::I32], &[V2ValueType::I32]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::MissingVersionMarker) => {}
        other => panic!("expected MissingVersionMarker, got {other:?}"),
    }
}

#[test]
fn manifest_version_mismatch_is_rejected() {
    let wat = gen_trivial_adapter_wat("adapter", &[V2ValueType::I64], &[V2ValueType::I64], 99);
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration("adapter", &[V2ValueType::I64], &[V2ValueType::I64]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::VersionMismatch { expected, found }) => {
            assert_eq!(expected, MANIFEST_VERSION);
            assert_eq!(found, 99);
        }
        other => panic!("expected VersionMismatch, got {other:?}"),
    }
}

#[test]
fn wrong_signature_is_rejected() {
    // Module really takes (i64, i64) -> i64; registration claims (i64) -> i64.
    let wat = gen_trivial_adapter_wat(
        "adapter",
        &[V2ValueType::I64, V2ValueType::I64],
        &[V2ValueType::I64],
        MANIFEST_VERSION,
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration("adapter", &[V2ValueType::I64], &[V2ValueType::I64]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::SignatureMismatch { .. }) => {}
        other => panic!("expected SignatureMismatch, got {other:?}"),
    }
}

#[test]
fn stale_id_after_a_rebuilt_symbol_changes_shape_is_rejected() {
    // Simulates a registration produced against an OLDER build of a symbol
    // (params: i32) being resolved against a module rebuilt with a
    // genuinely different real signature (params: i32, i32) -- the
    // registration's signature id is now stale relative to reality.
    let old_reg = registration("daxpy_like", &[V2ValueType::I32], &[V2ValueType::I64]);
    let rebuilt_wat = gen_trivial_adapter_wat(
        "daxpy_like",
        &[V2ValueType::I32, V2ValueType::I32],
        &[V2ValueType::I64],
        MANIFEST_VERSION,
    );
    let (mut store, instance) = instantiate_no_imports(&rebuilt_wat);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &old_reg) {
        Err(V2AdapterRejection::SignatureMismatch {
            expected, found, ..
        }) => {
            assert_ne!(expected.id(), found.id());
        }
        other => panic!("expected SignatureMismatch (stale id), got {other:?}"),
    }
}

#[test]
fn unsupported_v128_param_is_rejected() {
    let wat = format!(
        r#"(module
  (func (export "{MANIFEST_VERSION_EXPORT}") (result i32) i32.const {MANIFEST_VERSION})
  (func (export "adapter") (param v128) (result i32)
    i32.const 0)
)"#
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration("adapter", &[V2ValueType::I32], &[V2ValueType::I32]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::UnsupportedType { detail, .. }) => {
            assert!(detail.contains("v128"), "{detail}");
        }
        other => panic!("expected UnsupportedType, got {other:?}"),
    }
}

#[test]
fn too_many_results_is_rejected() {
    let wat = format!(
        r#"(module
  (func (export "{MANIFEST_VERSION_EXPORT}") (result i32) i32.const {MANIFEST_VERSION})
  (func (export "adapter") (result i32 i32)
    i32.const 0
    i32.const 0)
)"#
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let reg = registration("adapter", &[], &[V2ValueType::I32]);
    let mut cache = V2AdapterCache::new();
    match cache.resolve(&mut store, &instance, &reg) {
        Err(V2AdapterRejection::UnsupportedType { detail, .. }) => {
            assert!(detail.contains("result"), "{detail}");
        }
        other => panic!("expected UnsupportedType, got {other:?}"),
    }
}

#[test]
fn changed_signature_invalidates_cache() {
    // Real export genuinely takes (i64) -> i64. A first, correct
    // registration resolves and caches it; a SECOND registration under the
    // SAME adapter_export but a different (wrong, relative to the real
    // export) signature must be independently rejected -- not served the
    // first registration's cached Ok resolution just because the export
    // name repeats.
    let export = "adapter";
    let wat = gen_trivial_adapter_wat(
        export,
        &[V2ValueType::I64],
        &[V2ValueType::I64],
        MANIFEST_VERSION,
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let mut cache = V2AdapterCache::new();

    let reg_a = registration(export, &[V2ValueType::I64], &[V2ValueType::I64]);
    assert!(cache.resolve(&mut store, &instance, &reg_a).is_ok());

    let reg_b = registration(
        export,
        &[V2ValueType::I64, V2ValueType::I64],
        &[V2ValueType::I64],
    );
    match cache.resolve(&mut store, &instance, &reg_b) {
        Err(V2AdapterRejection::SignatureMismatch { .. }) => {}
        other => panic!("expected SignatureMismatch for the changed registration, got {other:?}"),
    }

    // The cache must still serve the ORIGINAL, still-valid registration
    // correctly after the failed re-resolution above -- a failed lookup for
    // a different registration must never corrupt or evict the still-good
    // cached entry.
    assert!(cache.resolve(&mut store, &instance, &reg_a).is_ok());
}

#[test]
fn changed_manifest_version_invalidates_cache() {
    // Real export's version marker reports MANIFEST_VERSION. A first
    // registration (expecting MANIFEST_VERSION) resolves and caches; a
    // second registration under the same adapter_export but a DIFFERENT
    // expected manifest_version must be independently rejected with
    // VersionMismatch, not served the first registration's cached Ok.
    let export = "adapter";
    let wat = gen_trivial_adapter_wat(
        export,
        &[V2ValueType::I64],
        &[V2ValueType::I64],
        MANIFEST_VERSION,
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let mut cache = V2AdapterCache::new();

    let reg_a = registration(export, &[V2ValueType::I64], &[V2ValueType::I64]);
    assert!(cache.resolve(&mut store, &instance, &reg_a).is_ok());

    let mut reg_b = registration(export, &[V2ValueType::I64], &[V2ValueType::I64]);
    reg_b.manifest_version = MANIFEST_VERSION + 1;
    match cache.resolve(&mut store, &instance, &reg_b) {
        Err(V2AdapterRejection::VersionMismatch { expected, found }) => {
            assert_eq!(expected, MANIFEST_VERSION + 1);
            assert_eq!(found, MANIFEST_VERSION);
        }
        other => panic!("expected VersionMismatch for the changed registration, got {other:?}"),
    }
}

#[test]
fn registration_replacement_with_same_shape_still_revalidates() {
    // Two registrations that are IDENTICAL in every field the real export's
    // shape could possibly disagree with (same signature, same manifest
    // version) but differ in `grate_cage` -- a field the cache has no way
    // to derive from the module itself. `V2Registration` is compared by
    // full value, so this must still be treated as a distinct registration
    // (re-validated, not silently reused) rather than the cache assuming
    // "same shape" implies "same registration".
    let export = "adapter";
    let wat = gen_trivial_adapter_wat(
        export,
        &[V2ValueType::I64],
        &[V2ValueType::I64],
        MANIFEST_VERSION,
    );
    let (mut store, instance) = instantiate_no_imports(&wat);
    let mut cache = V2AdapterCache::new();

    let reg_a = registration(export, &[V2ValueType::I64], &[V2ValueType::I64]);
    let adapter_a = cache.resolve(&mut store, &instance, &reg_a).unwrap();
    assert_eq!(adapter_a.signature, reg_a.signature);

    let mut reg_b = reg_a.clone();
    reg_b.grate_cage = reg_a.grate_cage + 1;
    assert_ne!(reg_a, reg_b);
    // Re-validates against the real module rather than reusing reg_a's
    // cached entry, and succeeds (the real export's shape didn't change).
    let adapter_b = cache.resolve(&mut store, &instance, &reg_b).unwrap();
    assert_eq!(adapter_b.signature, reg_b.signature);
}

#[test]
fn rejection_never_calls_the_real_handler() {
    // A module that genuinely imports and calls a host-provided "real"
    // function, wired to a call counter -- proves a rejected resolution
    // never lets a caller reach the point of invoking the adapter (and
    // through it, the real handler) at all.
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let calls = Arc::new(AtomicUsize::new(0));
    let engine = Engine::default();
    let wat = format!(
        r#"(module
  (import "env" "real" (func $real (param i64) (result i64)))
  (func (export "{MANIFEST_VERSION_EXPORT}") (result i32) i32.const {MANIFEST_VERSION})
  (func (export "adapter") (param i64) (result i64)
    local.get 0
    call $real)
)"#
    );
    let module = Module::new(&engine, &wat).unwrap();
    let mut store = Store::new(&engine, ());
    let mut linker: Linker<()> = Linker::new(&engine);
    let calls_clone = calls.clone();
    linker
        .func_wrap("env", "real", move |x: i64| -> i64 {
            calls_clone.fetch_add(1, Ordering::SeqCst);
            x
        })
        .unwrap();
    let instance = linker.instantiate(&mut store, &module).unwrap();

    // Registration deliberately wrong (arity mismatch) so resolution rejects.
    let reg = registration(
        "adapter",
        &[V2ValueType::I64, V2ValueType::I64],
        &[V2ValueType::I64],
    );
    let mut cache = V2AdapterCache::new();
    assert!(cache.resolve(&mut store, &instance, &reg).is_err());
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "real handler must not have run"
    );
}
