# Rust Style Guide

Rust code in `lind-wasm` follows the [default Rust style](https://doc.rust-lang.org/style-guide/index.html) and should be auto-formatted:

```bash
cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
cargo fmt --all --manifest-path src/lind-boot/Cargo.toml
```

## Clippy

CI runs Clippy on the `lind-boot` dependency graph via
[`scripts/clippy-ci.sh`](https://github.com/Lind-Project/lind-wasm/blob/main/scripts/clippy-ci.sh).
See [Clippy / static analysis](clippy.md) for suppressions, toolchain pins, and how to match CI locally.

```bash
rustup toolchain install nightly-2025-06-08 --component clippy
./scripts/clippy-ci.sh
```
