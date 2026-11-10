# Rust Style Guide

Rust code in `lind-wasm` follows the [default Rust style](https://doc.rust-lang.org/style-guide/index.html) and should be auto-formatted:

```bash
cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
cargo fmt --all --manifest-path src/lind-boot/Cargo.toml
```

## Constant placement

Define constants in the smallest scope where they are needed. The appropriate location depends on how widely they are used.

### Local constants

If a constant is only needed within one function, define it there. If it is used throughout a single file, keep it near the top of that file and private to the module. For example, `EPOCH_NORMAL`, `EPOCH_SIGNAL`, and `EPOCH_KILLED` are defined near the top of `src/cage/src/signal/signal.rs` and used within that file.

### Crate-wide constants

If a constant is used broadly within one crate but not by other crates, place it in a dedicated constants module within that crate. For example, the 3i crate defines `THREEI_DEREGISTER` and `ELINDAPIABORTED` in `src/threei/src/threei_const.rs`. These constants are used across the 3i implementation, including `threei.rs` and the handler-table modules.

### Shared constants

Constants used across multiple crates should generally be grouped by subsystem in a shared directory. In `lind-wasm`, one shared location for these constants is `src/sysdefs/src/constants/`, with modules such as `fs_const.rs` for filesystem constants, `net_const.rs` for networking, `err_const.rs` for errors, and `syscall_const.rs` for syscall numbers. The `mod.rs` file declares these modules and re-exports their constants. For example, `MAX_CAGEID` is defined in `lind_platform_const.rs` and used across several crates, including `cage`, `threei`, `rawposix`, `typemap`, and `lind-boot`.
