# Clippy and static analysis

This page documents how [Clippy](https://doc.rust-lang.org/clippy/) is used in the
`lind-wasm` repository: CI integration, which checks are enforced or suppressed, and how
to reproduce CI locally.

Related tracking issues: [#1267](https://github.com/Lind-Project/lind-wasm/issues/1267),
[#243](https://github.com/Lind-Project/lind-wasm/issues/243), [#242](https://github.com/Lind-Project/lind-wasm/issues/242),
[#380](https://github.com/Lind-Project/lind-wasm/issues/380).

## Summary

| Question | Answer |
| --- | --- |
| Is Clippy in CI? | **Yes** — GitHub Actions workflow [`lint.yml`](https://github.com/Lind-Project/lind-wasm/blob/main/.github/workflows/lint.yml) on push to `main` and on non-draft PRs. |
| What gets checked? | The `lind-boot` crate and its **dependency graph** (Lind syscall layers, fdtables, embedded wasmtime crates, etc.), not the full wasmtime workspace. |
| Does CI fail on Clippy warnings? | **No** — `-A warnings` is passed so only **errors** fail the job. Workspace `[workspace.lints.clippy]` in `src/wasmtime/Cargo.toml` is latent until that suppression is removed ([#380](https://github.com/Lind-Project/lind-wasm/issues/380)). |
| Upstream wasmtime CI? | `cargo clippy --workspace` in `src/wasmtime/.github/workflows/main.yml` is **not** run by lind-wasm’s top-level workflows. |

## CI workflow (`lint.yml`)

The Lint job runs, in order:

1. **`Swatinem/rust-cache`** — caches Cargo artifacts for Clippy (placed before build steps).
2. **`cargo fmt --check`** — wasmtime and lind-boot manifests.
3. **Rust toolchain** — pinned to `nightly-2025-06-08` with the `clippy` component ([#242](https://github.com/Lind-Project/lind-wasm/issues/242): newer nightlies have triggered `clippy::not_unsafe_ptr_arg_deref` in upstream wasmtime VM code).
4. **Clippy** — via [`scripts/clippy-ci.sh`](https://github.com/Lind-Project/lind-wasm/blob/main/scripts/clippy-ci.sh).

### Why `lind-boot` and explicit features?

[PR #234](https://github.com/Lind-Project/lind-wasm/pull/234) originally ran Clippy on
`src/wasmtime/Cargo.toml` with `--all-features`. That enabled every `fdtables-*` backend at
once and hit fdtables’ mutual-exclusion `compile_error!`. CI now builds through
`src/lind-boot/Cargo.toml` with an explicit feature set matching a typical Lind build
(default fdtables backend, logging, debug, secure mode, etc.).

### Clippy flags (after `--`)

| Flag | Status | Rationale |
| --- | --- | --- |
| `-A warnings` | **Still enabled** | Large backlog of rustc/Clippy warnings across wasmtime and Lind code ([#380](https://github.com/Lind-Project/lind-wasm/issues/380)). |
| `-A clippy::not_unsafe_ptr_arg_deref` | **Still enabled** | Raw POSIX shims and wasmtime VM code; tied to nightly/toolchain constraints ([#242](https://github.com/Lind-Project/lind-wasm/issues/242)). |
| `-A clippy::absurd_extreme_comparisons` | **Removed** ([#243](https://github.com/Lind-Project/lind-wasm/issues/243)) | Fixed invalid unsigned `< 0` checks in `typemap` `validate_cageid` (see below). |
| `--all-targets` | **Enabled** ([#243](https://github.com/Lind-Project/lind-wasm/issues/243)) | Includes tests, examples, and benches in the `lind-boot` dependency graph. |
| `--workspace` (full wasmtime tree) | **Not in CI** | Heavier; deferred ([#243](https://github.com/Lind-Project/lind-wasm/issues/243)). |

### Code fix for `absurd_extreme_comparisons`

`validate_cageid` in
[`src/wasmtime/crates/typemap/src/cage_helpers.rs`](https://github.com/Lind-Project/lind-wasm/blob/main/src/wasmtime/crates/typemap/src/cage_helpers.rs)
compared `u64` cage IDs with `< 0`, which Clippy flags as always false. The redundant
checks were removed; upper-bound checks against `MAX_CAGEID` remain.

## Workspace lint policy (wasmtime)

In [`src/wasmtime/Cargo.toml`](https://github.com/Lind-Project/lind-wasm/blob/main/src/wasmtime/Cargo.toml),
`[workspace.lints.clippy]` sets `all = allow` and enables a **small selective set** of
lints (`clone_on_copy`, `uninlined_format_args`, `manual_strip`, etc.). Member crates
inherit these via `[lints] workspace = true`. They take effect when CI stops passing
`-A warnings`.

The standalone [`fdtables`](https://github.com/Lind-Project/lind-wasm/tree/main/src/wasmtime/crates/fdtables)
crate uses stricter crate-level `#![warn(clippy::all, clippy::pedantic, ...)]` in
`src/lib.rs`; CI still allows warnings globally until [#380](https://github.com/Lind-Project/lind-wasm/issues/380).

## Local development vs CI

| | CI (`lint.yml`) | [`rust-toolchain.toml`](https://github.com/Lind-Project/lind-wasm/blob/main/rust-toolchain.toml) |
| --- | --- | --- |
| Channel | `nightly-2025-06-08` (pinned) | `nightly` (rolling) |
| Components | `clippy`, `rustfmt` | `clippy`, `rustfmt` |

For parity with CI, use the script (Linux/Ubuntu recommended — same as GHA):

```bash
rustup toolchain install nightly-2025-06-08 --component clippy
./scripts/clippy-ci.sh
```

Override features or toolchain when experimenting:

```bash
LIND_BOOT_FEATURES="disable_signals secure fdtables-dashmaparray" RUST_TOOLCHAIN=nightly-2025-06-08 ./scripts/clippy-ci.sh
```

On non-Linux hosts, some crates (for example `sysdefs`) may not build; use the dev
Docker image or WSL2/Ubuntu as described in
[Native Linux setup](running-on-native-linux.md).

## Roadmap

1. ~~Re-enable `clippy::absurd_extreme_comparisons`~~ (done for known `typemap` hit; watch CI for others).
2. Re-enable `clippy::not_unsafe_ptr_arg_deref` with targeted fixes or a nightly bump ([#242](https://github.com/Lind-Project/lind-wasm/issues/242)).
3. Remove `-A warnings` and burn down the warning backlog ([#380](https://github.com/Lind-Project/lind-wasm/issues/380)); optional HTML report ([#384](https://github.com/Lind-Project/lind-wasm/issues/384)).
4. Consider `--workspace` Clippy over full wasmtime when cost and fdtables feature selection are addressed ([#243](https://github.com/Lind-Project/lind-wasm/issues/243)).

## Historical context

- [#220](https://github.com/Lind-Project/lind-wasm/issues/220) — Clippy results were missing from older CI artifacts.
- [PR #234](https://github.com/Lind-Project/lind-wasm/pull/234) — Added Clippy to GHA with temporary `-A` suppressions.
