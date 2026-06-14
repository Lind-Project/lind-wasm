# Lind Logging System

## 1. Usage and Configuration

### 1.1 Quick Reference

The logging system is controlled by three environment variables set before launching `lind-boot`:

| Variable | Purpose | Default |
|----------|---------|---------|
| `LIND_LOG_OUTPUT` | Where `lind_log!` output is written | `$LIND_WASM_ROOT/LIND.log` (or `/tmp/LIND.log`) |
| `LIND_LOG_CATEGORIES` | Which log categories are emitted | `default` |
| `LIND_DEBUG_PANIC` | Behavior of `lind_debug_panic!` | `panic-and-exit` |

### 1.2 Common Configurations

**Development (default — nothing to set):**
```bash
# Logs go to $LIND_WASM_ROOT/LIND.log, only Default category, panics on unexpected conditions.
make lind-boot
./build/lind-boot program.wasm
```

**Print all logs to stderr and see every category:**
```bash
LIND_LOG_OUTPUT=stderr LIND_LOG_CATEGORIES=all ./build/lind-boot program.wasm
```

**Debug dynamic linking only:**
```bash
LIND_LOG_OUTPUT=stderr LIND_LOG_CATEGORIES=dylink ./build/lind-boot program.wasm
```

**Debug ThreeI syscall routing only:**
```bash
LIND_LOG_OUTPUT=stderr LIND_LOG_CATEGORIES=threei ./build/lind-boot program.wasm
```

**Debug both dynamic linking and ThreeI:**
```bash
LIND_LOG_OUTPUT=stderr LIND_LOG_CATEGORIES=dylink,threei ./build/lind-boot program.wasm
```

**Write logs to a custom file:**
```bash
LIND_LOG_OUTPUT=/var/log/lind.log LIND_LOG_CATEGORIES=all ./build/lind-boot program.wasm
# or equivalently:
LIND_LOG_OUTPUT=file:/var/log/lind.log LIND_LOG_CATEGORIES=all ./build/lind-boot program.wasm
```

**Deployment — best-effort continuation on unexpected conditions:**
```bash
LIND_LOG_OUTPUT=/var/log/lind.log LIND_DEBUG_PANIC=log-only ./build/lind-boot program.wasm
```

**CI — silence log noise but keep unexpected-condition detection as test failures:**
```bash
LIND_LOG_OUTPUT=none LIND_DEBUG_PANIC=panic-and-exit ./build/lind-boot program.wasm
```

**Maximum performance / benchmark — remove all logging code from the binary:**
```bash
make lind-boot NO_LOGGING=1
```

### 1.3 Environment Variable Reference

#### `LIND_LOG_OUTPUT`

Controls where `lind_log!` output is written. Has no effect on whether `lind_debug_panic!` panics.

| Value | Behavior |
|-------|---------|
| `stderr` | Write to standard error |
| `stdout` | Write to standard output |
| `none` | Discard all log output (does not suppress panics) |
| `file:/path/to/file` | Append to file at path; created if absent |
| `/path/to/file` | Same as `file:` prefix |

#### `LIND_LOG_CATEGORIES`

Controls which categories of `lind_log!` messages are emitted. Messages from disabled categories are discarded before any formatting occurs.

| Value | Categories enabled |
|-------|--------------------|
| `all` | All three categories |
| `none` | No categories (silences all `lind_log!`) |
| `default` | General / uncategorized messages |
| `dylink` | Dynamic linking messages |
| `threei` | ThreeI syscall routing messages |
| `dylink,threei` | Any comma-separated combination |

#### `LIND_DEBUG_PANIC`

Controls the behavior of `lind_debug_panic!`. Regardless of this setting, `lind_debug_panic!` does **not** use the log category filter — it always fires.

| Value | Behavior |
|-------|---------|
| `panic` or `panic-and-exit` | Log the message then call `panic!` |
| `log` or `log-only` | Log the message and return normally |
| `none` or `no-action` | Return immediately without logging or panicking |

### 1.4 Compile-Time Switch

When benchmarking or building for maximum performance, the entire logging system can be compiled out:

```bash
make lind-boot NO_LOGGING=1
```

With `NO_LOGGING=1`, both `lind_log!` and `lind_debug_panic!` expand to nothing. Formatting arguments are not evaluated — there is zero overhead at the call site.

---

## 2. The Three Diagnostic Levels

The logging system distinguishes three escalating levels of concern:

```
Is continued execution possible?
│
├─ No, always → panic!
│
└─ Yes, or maybe →
       Is this condition expected during normal operation?
       │
       ├─ Yes → lind_log!
       │
       └─ No — something that should never happen →
              lind_debug_panic!
```

### `lind_log!` — General Diagnostic Output

Use for conditions that are informational or mildly suspicious but do not indicate a failure. Examples: reporting which code path was taken, noting that an optional resource was absent but a fallback succeeded, tracing intermediate values for debuggability.

```rust
lind_log!("cage {} started", cageid);
lind_log!(DYLINK, "resolved symbol {} at 0x{:x}", sym, addr);
lind_log!(THREEI, "registered handler for call {}", call_id);
```

`lind_log!` **never panics**. If the category is disabled or the output is `None`, it returns immediately without evaluating format arguments.

### `lind_debug_panic!` — Soft Panic

Use for conditions that *should never happen* under correct operation and may indicate corrupted internal state, but where best-effort continuation is preferable to a hard crash in production.

```rust
lind_debug_panic!("signal_epoch_trigger: cage {} not found", cageid);
```

Does **not** take a category. Always fires regardless of the active `LIND_LOG_CATEGORIES`. Behavior is controlled by `LIND_DEBUG_PANIC`.

**Call sites must always supply explicit fallback control flow**, because `lind_debug_panic!` may return normally in `log-only`, `no-action`, and compiled-out modes:

```rust
// Good
let cage = match get_cage(cageid) {
    Some(c) => c,
    None => {
        lind_debug_panic!("cage {} not found", cageid);
        return; // required — macro may return normally
    }
};

// Bad — assumes lind_debug_panic! always diverges
let cage = get_cage(cageid).unwrap_or_else(|| {
    lind_debug_panic!("cage {} not found", cageid);
    unreachable!() // wrong
});
```

### `panic!` — Unrecoverable Failure

Use only when continued execution is truly impossible — a lock is poisoned, a required resource is missing at boot, or an invariant that can never be violated has been violated. `panic!` is never softened by the logging system.

---

## 3. Log Categories

| Category | Token | Scope |
|----------|-------|-------|
| `Default` | `lind_log!(...)` or `lind_log!(Default, ...)` | Uncategorized Lind diagnostics |
| `DYLINK` | `lind_log!(DYLINK, ...)` | dlopen/dlsym/dlclose, GOT updates, symbol resolution, library replay, weak imports |
| `THREEI` | `lind_log!(THREEI, ...)` | Syscall routing, handler registration, inter-cage call routing, ThreeI interposition |

Categories are checked before format arguments are evaluated. Disabled categories add no overhead beyond a single bitmask test.

---

## 4. Output Format

All log lines share a consistent prefix that includes the category, source file, line number, and Rust module path:

```
[LIND][DYLINK][src/dylink.rs:88 lind::dylink] resolved symbol malloc at 0x7f001000
[LIND][Default][src/cage.rs:42 lind::cage] cage 3 started
[LIND][THREEI][src/threei.rs:77 threei::threei] registered handler call_id=7
```

`lind_debug_panic!` uses a different prefix to make it easy to grep for unexpected conditions in logs:

```
# PanicAndExit mode — appears before the panic backtrace
[LIND][DEBUG PANIC][src/signal.rs:182 lind::signal] cage 3 not found

# LogOnly mode — execution continues after this line
[LIND][DEBUG PANIC continuing][src/signal.rs:182 lind::signal] cage 3 not found
```

---

## 5. Architecture

### 5.1 Compile-Time Gating

Both macros are defined in `src/sysdefs/src/logging.rs` and re-exported from the `sysdefs` crate root. The body of each macro is wrapped in `#[cfg(feature = "lind-logging")]`:

```rust
#[macro_export]
macro_rules! lind_log {
    (@cat $category:expr, $($arg:tt)*) => {
        {
            #[cfg(feature = "lind-logging")]
            {
                if $crate::logging::category_enabled($category) {
                    $crate::logging::log($category, format_args!($($arg)*), file!(), line!(), module_path!());
                }
            }
        }
    };
    // ... category dispatch arms
}
```

The `#[cfg]` is evaluated at the **call site's crate**, so every crate that uses the macros must declare and forward the feature:

```toml
# Cargo.toml of any crate that uses lind_log! or lind_debug_panic!
[features]
lind-logging = ["sysdefs/lind-logging"]
```

The root `lind-boot` crate activates `lind-logging` for the entire workspace. The Makefile handles this automatically and respects `NO_LOGGING=1`.

### 5.2 Global Logger State

The logger is stored in a `OnceLock<LindLogger>`:

```rust
static LIND_LOGGER: OnceLock<LindLogger> = OnceLock::new();

struct LindLogger {
    writer: Mutex<LogWriter>,      // output destination
    panic_behavior: PanicBehavior, // lind_debug_panic! behavior
    enabled_categories: LogCategorySet, // bitmask of active categories
}
```

`OnceLock` gives first-call-wins initialization with no locking overhead on subsequent reads. The `Mutex<LogWriter>` only serializes the actual write — the category check and feature gate happen before acquiring the lock.

If `init_lind_logger` is never called, all accesses fall through to a hardcoded default that writes to the file path derived from `LIND_WASM_ROOT` (or `/tmp/LIND.log`), panics on `lind_debug_panic!`, and enables only the `Default` category.

### 5.3 Initialization Sequence

`init_lind_logger` is called at the very top of `main()` in `lind-boot`, before CLI parsing, before `chroot`, and before `rawposix_start`. This ensures every subsequent log message — including those emitted during RawPOSIX or wasmtime initialization — picks up the environment-configured behavior:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = init_lind_logger(config_from_env().unwrap_or_default());
    // ... rest of startup
}
```

`config_from_env()` reads the three environment variables and builds a `LindLoggerConfig`. If the variable is absent, the corresponding field retains its default value. If the variable contains an unrecognised value, `config_from_env()` returns an error and `unwrap_or_default()` falls back to the compiled-in defaults silently.

### 5.4 Category Bitmask

`LogCategorySet` is a `u8` bitmask — one bit per category. The check in `category_enabled` is a single AND operation:

```rust
const BIT_DEFAULT: u8 = 0b001;
const BIT_DYLINK:  u8 = 0b010;
const BIT_THREEI:  u8 = 0b100;

pub fn contains(&self, category: LogCategory) -> bool {
    self.0 & bit != 0
}
```

Format arguments are inside the `if category_enabled(...)` check, so they are not evaluated for disabled categories.

### 5.5 File Output

When `LogOutput::File` is selected, the file is opened once during `init_lind_logger` with `OpenOptions::append(true).create(true)` and the handle is stored inside the `Mutex<LogWriter>`. Every `write_line` call acquires the mutex, writes a single line, and releases it. The file descriptor is never closed and reopened per message.

### 5.6 `LogOutput::None` vs `NO_LOGGING=1`

These are two distinct suppression mechanisms:

| | `LogOutput::None` | `NO_LOGGING=1` |
|---|---|---|
| When decided | Runtime | Compile time |
| `lind_log!` | Suppressed (no output) | Compiled out entirely |
| `lind_debug_panic!` logging | Suppressed | Compiled out entirely |
| `lind_debug_panic!` panic | Still happens (if `PanicAndExit`) | Compiled out |
| Use case | CI — silence noise, keep panics as test failures | Benchmarks, max-performance builds |
