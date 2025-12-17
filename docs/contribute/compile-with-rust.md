# Compiling Rust Code with `lind-glibc` (Shared Memory Enabled)

To compile Rust programs against **`lind-glibc`**, you must configure Cargo to:

1. Use the custom `config.toml` configuration file
2. Use the custom `wasip1-clang` linker wrapper
3. Rebuild the Rust standard library (`std`) with these features enabled

---

## 1. Cargo Configuration (`.cargo/config.toml`)

Create or modify `.cargo/config.toml` as follows (based on
`scripts/rust/config.toml.template`):

```toml
[build]
# Compile all Rust code for WASI Preview 1
target = "wasm32-wasip1"

[target.wasm32-wasip1]
# Use lind’s custom clang wrapper for glibc-based WASI linking
linker = "/home/lind-wasm/scripts/wasip1-clang.sh"

rustflags = [
  # Do not use Rust’s built-in self-contained WASI linker
  "-C", "link-self-contained=no",

  # Enable WebAssembly features required for shared memory
  # - atomics: required for multi-threading
  # - bulk-memory: required for memory.copy / memory.fill
  # - crt-static: ensure static runtime linking
  "-C", "target-feature=+crt-static,+atomics,+bulk-memory",

  # Import and export the linear memory so the runtime can control it
  "-C", "link-arg=-Wl,--import-memory",
  "-C", "link-arg=-Wl,--export-memory",

  # Enable shared linear memory (threads proposal)
  "-C", "link-arg=-Wl,--shared-memory",

  # Set maximum memory size (64 MiB)
  "-C", "link-arg=-Wl,--max-memory=67108864",

  # Export stack symbols required by lind runtime
  "-C", "link-arg=-Wl,--export=__stack_pointer",
  "-C", "link-arg=-Wl,--export=__stack_low",
]
```

### Why this is necessary

* Shared memory requires `atomics + bulk-memory`
* These features must be enabled for both your crate and `std`
* Rust’s prebuilt `std` does *not* include these features by default
* Therefore, `std` must be rebuilt explicitly

---

## 2. Custom Linker Wrapper (`scripts/wasip1-clang.sh`)

The following script replaces Cargo’s default linker and ensures that:

* `lind-glibc` is used instead of WASI libc
* The correct `crt1.o` startup object is injected
* `pthread` and glibc are linked properly
* All Rust-provided linker arguments are preserved

### Linker Script

```bash
#!/usr/bin/env bash
set -euo pipefail

# Path to lind-glibc sysroot
SYSROOT=/home/lind-wasm/src/glibc/sysroot
LIBDIR="$SYSROOT/lib/wasm32-wasi"
CRT1="$LIBDIR/crt1.o"

# Sanity checks (fail early with a clear message)
[ -r "$CRT1" ] || { echo "Missing $CRT1"; exit 1; }
[ -d "$LIBDIR" ] || { echo "Missing $LIBDIR"; exit 1; }

# Base clang invocation
cmd=(
  clang
  --target=wasm32-unknown-wasip1
  --sysroot="$SYSROOT"
  -nostartfiles        # Prevent clang from injecting its own crt1.o
)

# Forward all arguments from rustc unchanged
cmd+=("$@")

# Inject lind-glibc startup object and libraries
cmd+=(
  "$CRT1"
  -L"$LIBDIR"
  -lc                  # lind-glibc
  -pthread             # enable pthread support
)

# Print the final command for debugging (stderr keeps rustc output clean)
echo "[clang wrapper exec]" "${cmd[@]}" 1>&2

# Execute the linker
exec "${cmd[@]}"
```

### Why this script exists

Rust’s default WASI linker:

* Uses WASI-libc instead of glibc
* Does not support lind’s threading and memory model
* Cannot inject a custom `crt1.o`

This wrapper ensures full control over startup, libc, and threading behavior.
---

## 3. Build Rust with a Custom `std`

After configuring Cargo, compile your Rust project using **nightly** and rebuild the standard library:

```bash
cargo build -Z build-std=std,panic_abort
```

### What this does

* Forces Rust to rebuild `std` for `wasm32-wasip1`
* Applies your `rustflags` to `std` itself
* Enables atomics + bulk-memory inside `libstd`
* Avoids linker errors such as:

```
--shared-memory is disallowed because std was not compiled with atomics
```

> ⚠️ This requires a **nightly toolchain**, which you already have.
