use std::str;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // NB: duplicating a workaround in the wasmtime-fiber build script.
    custom_cfg("asan", cfg_is("sanitize", "address"));

    let unix = cfg("unix");
    let windows = cfg("windows");
    let miri = cfg("miri");

    let supported_os = (unix || windows) && cfg!(feature = "std");

    let has_host_compiler_backend = match std::env::var("CARGO_CFG_TARGET_ARCH").unwrap().as_str() {
        "x86_64" | "riscv64" | "s390x" | "aarch64" => true,
        _ => false,
    };

    let has_native_signals = !miri
        && (supported_os || cfg!(feature = "custom-native-signals"))
        && has_host_compiler_backend;
    let has_virtual_memory = supported_os || cfg!(feature = "custom-virtual-memory");
    let has_custom_sync = !cfg!(feature = "std")
        && cfg!(feature = "custom-sync-primitives")
        && cfg!(feature = "runtime");

    custom_cfg("has_native_signals", has_native_signals);
    custom_cfg("has_virtual_memory", has_virtual_memory);
    custom_cfg("has_custom_sync", has_custom_sync);
    custom_cfg("has_host_compiler_backend", has_host_compiler_backend);

    #[cfg(feature = "runtime")]
    if has_host_compiler_backend && (supported_os || cfg!(feature = "debug-builtins")) {
        build_c_helpers();
    }

    let default_target_pulley = !has_host_compiler_backend || miri;
    custom_cfg("default_target_pulley", default_target_pulley);
    if default_target_pulley {
        println!("cargo:rustc-cfg=feature=\"pulley\"");
    }
}

fn cfg(key: &str) -> bool {
    std::env::var(&format!("CARGO_CFG_{}", key.to_uppercase())).is_ok()
}

fn cfg_is(key: &str, val: &str) -> bool {
    std::env::var(&format!("CARGO_CFG_{}", key.to_uppercase()))
        .ok()
        .as_deref()
        == Some(val)
}

fn custom_cfg(key: &str, enabled: bool) {
    println!("cargo:rustc-check-cfg=cfg({key})");
    if enabled {
        println!("cargo:rustc-cfg={key}");
    }
}

#[cfg(feature = "runtime")]
fn build_c_helpers() {
    use wasmtime_versioned_export_macros::versioned_suffix;

    let mut build = cc::Build::new();
    build.warnings(true);
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    build.define(&format!("CFG_TARGET_OS_{os}"), None);
    build.define(&format!("CFG_TARGET_ARCH_{arch}"), None);
    build.define("VERSIONED_SUFFIX", Some(versioned_suffix!()));
    if std::env::var("CARGO_FEATURE_DEBUG_BUILTINS").is_ok() {
        build.define("FEATURE_DEBUG_BUILTINS", None);
    } else if os == "windows" {
        return;
    }

    println!("cargo:rerun-if-changed=src/runtime/vm/helpers.c");
    build.file("src/runtime/vm/helpers.c");
    build.compile("wasmtime-helpers");
}
