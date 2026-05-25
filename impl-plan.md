# Cross-Cage Library Interposition — Implementation Plan

## Overview

Evolve the current remote-library-call prototype into a Lind-native cross-cage library
interposition system. The design mirrors the grate/3i model: a main orchestrator program
spawns both the application cage and the library cage, then issues a syscall to register
interposition rules between them.

The key insight: **portal stubs are installed universally at library load time**. A stub
checks the routing table on every call and either falls through locally or dispatches
cross-cage. Changing or removing an interposition rule is a pure table write — no GOT
patching, no touching linear memory, works even after the target cage is already running.

> **Scope note:** This design covers **inter-cage** library interposition only — both the
> application cage and the library cage live inside the same Lind process. For
> **inter-process** (separate OS processes on the same machine) and **inter-machine**
> (remote host over TCP) use cases, the existing config-file-driven routing from
> `lind-remote-lib` (TCP and Unix socket transports) remains the supported path and is
> unchanged by this work.

---

## Architecture Summary

```
At library load time (instance_dylink, for interceptable libraries):
  Every imported symbol gets a portal stub (host function closure):

    stub for "crc32" in cage A:
      match LIBRARY_HANDLER_TABLE[(cage_A, "crc32")] {
          Some(lib_cage_id) => dispatch_cross_cage_call(lib_cage_id, ...),
          None              => original_func.call_nested(...)   // local, direct
      }

At any time — before or after the cage is running:
  lind_interpose_symbol(source_cage, "crc32", library_cage)
    → LIBRARY_HANDLER_TABLE.insert((source_cage, "crc32"), library_cage)
    → pure table write, no memory patching, no cage coordination needed
    → next call to "crc32" in source_cage routes cross-cage

To remove interposition:
  lind_remove_interposition(source_cage, "crc32")
    → LIBRARY_HANDLER_TABLE.remove((source_cage, "crc32"))
    → next call falls through to local implementation
```

Analogy to grate/3i:

| 3i / grate concept | Library interposition equivalent |
|---|---|
| 3i handler table | `LIBRARY_HANDLER_TABLE` |
| `register_handler` syscall | `lind_interpose_symbol` syscall |
| `harsh_cage_exit` cleanup | deregister all rules for dead cage |
| `GrateHandler` worker pool | `LibraryCageHandler` worker pool |
| `copy_data_between_cages` | reused directly for pointer copy-in/copy-out |

---

## Key Design Decisions

**1. Portal stubs are universal for interceptable libraries.**
When a library is marked interceptable (listed in the config or flagged at load time),
every one of its exported symbols gets a portal stub at `instance_dylink` time — not just
pre-configured symbols. The stub does a runtime `LIBRARY_HANDLER_TABLE` lookup on every
call. Unregistered symbols pay only a `DashMap` lookup returning `None`, then call through
to the original function directly. Registered symbols dispatch cross-cage.

This means:
- No need to know which symbols will be intercepted at library load time.
- Interposition can be added or removed at any point during execution.
- No `CrossCage` route decision at link time — the stub handles all routing decisions at
  call time.

**2. `lind_interpose_symbol` is a 3i syscall — pure table write.**
The syscall updates `LIBRARY_HANDLER_TABLE` and returns. It does not modify the GOT, does
not touch linear memory, and requires no coordination with the target cage's execution.
The portal stub already installed in the target cage will pick up the new rule on its next
invocation. Equally, `lind_remove_interposition` deletes the row and the next call falls
through locally.

**3. Library cage is a pure library WASM instance — no wrapper program.**
A library cage worker directly instantiates the library's `.cwasm` (the same shared library
file already used as a preloaded dependency in regular cages). There is no wrapper C/WASM
program in front of it. The Rust host calls the library's exported functions by name via
Wasmtime's typed function API (`instance.get_typed_func(&store, "crc32")`).

The worker's Linker satisfies the library's imports (libc, env) by replicating the same
preload chain used in regular cages — libc is loaded first, then the target library —
mirroring how `module_with_preload` works today, applied per-worker at init time.

This is consistent with how grate workers ARE the grate module rather than wrappers around
it: a library cage worker IS the library instance. Any `.cwasm` shared library works without
modification and no per-library wrapper code is needed.

A wrapper program (a small C/WASM shim that `dlopen`s the library) remains an escape hatch
for libraries with complex initialization or opaque handle requirements, but is out of scope
for the current design.

**4. Pointer marshaling stays in Rust host code.**
Existing `ArgSpec` / `PtrSizeSpec` metadata from `lind-remote-lib` is reused as-is.
Copy-in/copy-out is done with `copy_data_between_cages` from threei. Each worker has a
pre-allocated scratch buffer in the library cage's linear memory for staging pointer args.

**5. Config file pre-populates the table at startup.**
The routing config (extending `routing.json`) can specify which libraries to mark
interceptable and which initial interposition rules to register. At startup, `init_routing`
calls `lind_interpose_symbol` for each configured rule. Runtime WASM programs can later
add or remove rules via the same syscall.

---

## Component Breakdown

### A. `LIBRARY_HANDLER_TABLE` (in threei)

Global table mapping `(source_cage_id, symbol_name)` → `library_cage_id`.

```rust
// src/threei/src/library_handler.rs
static LIBRARY_HANDLER_TABLE: DashMap<(u64, String), u64> = ...;

pub fn register_library_handler(source_cage: u64, symbol: &str, library_cage: u64)
pub fn get_library_route(source_cage: u64, symbol: &str) -> Option<u64>
pub fn deregister_library_handler(source_cage: u64, symbol: &str)
pub fn deregister_all_for_cage(cage_id: u64)  // called on cage exit
```

Lifecycle: `deregister_all_for_cage` is hooked into `harsh_cage_exit` and
`trigger_harsh_cage_exit` to clean up rules when either the source cage or library cage dies.

---

### B. `lind_interpose_symbol` and `lind_remove_interposition` syscalls

New syscall numbers registered with `WASMTIME_CAGEID` (host-side handlers, like other
lind-specific syscalls).

```
lind_interpose_symbol(source_cage_id, symbol_ptr, symbol_ptr_cageid, library_cage_id)
  → reads symbol string from source_cage memory via symbol_ptr
  → calls register_library_handler(source_cage_id, symbol, library_cage_id)
  → returns 0 on success, -ESRCH if either cage is dead

lind_remove_interposition(source_cage_id, symbol_ptr, symbol_ptr_cageid)
  → reads symbol string
  → calls deregister_library_handler(source_cage_id, symbol)
  → returns 0
```

These follow the same pattern as existing lind-specific syscalls: registered via
`register_handler` with `WASMTIME_CAGEID` at startup, callable from any cage through
`make_syscall`.

---

### C. Portal stub (in `instance_dylink`)

For libraries marked interceptable, the existing direct wasm-to-wasm link is replaced with
a host function closure that does a runtime routing lookup:

```rust
// installed at instance_dylink time for each symbol of an interceptable library
Extern::Func(Func::new(&mut store, func_ty, move |mut caller, args, results| {
    match get_library_route(cage_id, &symbol) {
        Some(library_cage_id) => {
            let src_mem_base = get_mem_base(&mut caller);
            let raw_args = args_to_u64(args);
            let ret = dispatch_cross_cage_call(
                cage_id, library_cage_id, &symbol, &raw_args, src_mem_base,
            )?;
            results[0] = Val::I64(ret as i64);
        }
        None => {
            original_func.call_nested(&mut caller, args, results)?;
        }
    }
    Ok(())
}))
```

"Interceptable" is determined at `instance_dylink` time by whether the library appears in
the interceptable set (populated from config or a CLI flag). Non-interceptable libraries
continue to be linked directly — no overhead.

No new `RouteDecision` variant needed. The existing `Local` / `Remote` decisions are
unchanged; the portal stub is a third path triggered purely by the library being marked
interceptable.

---

### D. `LibraryCageHandler` worker pool (in `lind-3i`)

Modeled directly on `GrateHandler`. Manages reusable workers for one library cage.

```rust
struct LibraryCageHandler {
    library_cage_id: u64,
    inner: Mutex<VecDeque<LibraryWorker>>,
    cv: Condvar,
    active_calls: AtomicUsize,
    shutting_down: AtomicBool,
}

struct LibraryWorker {
    store: Store<HostCtx>,
    instance: Instance,      // library module instantiated
    scratch_base: u32,       // pre-allocated scratch offset in this worker's linear memory
    scratch_size: usize,
}
```

Global registry: `LIBRARY_CAGE_HANDLERS: DashMap<u64, LibraryCageHandler>`.

`submit(symbol, adjusted_args)`: leases a worker, resets scratch pointer, calls the typed
exported function by name, returns worker to pool.

---

### E. `dispatch_cross_cage_call`

```rust
pub fn dispatch_cross_cage_call(
    source_cage_id: u64,
    library_cage_id: u64,
    symbol: &str,
    raw_args: &[u64],       // WASM values from caller
    src_mem_base: *mut u8,  // base of source cage linear memory
) -> anyhow::Result<u64>
```

Steps:
1. Look up `ArgSpec` list for `symbol` from `meta_table` (reused from `lind-remote-lib`).
2. Lease a `LibraryWorker` from `LIBRARY_CAGE_HANDLERS[library_cage_id]`.
3. For each `In`/`InOut` pointer arg: copy data from source cage into worker scratch buffer
   using `copy_data_between_cages`. Replace pointer value in arg array with scratch offset.
4. Call the typed exported function `symbol` on `worker.instance`.
5. For each `Out`/`InOut` pointer arg: copy results from scratch buffer back to source cage.
6. Return worker to pool. Return scalar result.

---

### F. Library cage creation

```rust
fn create_library_cage(
    engine: &Engine,
    library_path: &str,
    n_workers: usize,
) -> anyhow::Result<u64>  // returns library_cage_id
```

Steps:
1. Allocate a new cage ID.
2. Compile the library `.cwasm` module (same file used as a preloaded shared library in
   regular cages — no wrapper or modification required).
3. Build a worker Linker: load the library's dependencies (libc, etc.) in the same order
   as `module_with_preload` does for regular cages, so all imports are satisfied.
4. Spin up N `LibraryWorker`s. Each worker gets its own `Store + Instance` by instantiating
   the library module through the worker Linker — the library's exports (`crc32`, `adler32`,
   etc.) are then directly callable via `instance.get_typed_func(&store, symbol)`.
5. Per worker: reserve a scratch region at a fixed high-address offset in the worker's
   linear memory for pointer arg staging (avoids collision with the library's heap/stack).
6. Register in `LIBRARY_CAGE_HANDLERS` and `LIBRARY_HANDLER_TABLE` (cage is live).
6. Return `library_cage_id`.

---

### G. Config schema

Extend `routing.json`:

```json
{
  "library_cages": {
    "zlib-cage": {
      "library": "/lib/libz.so",
      "n_workers": 4
    }
  },
  "interpose": [
    {
      "source_cage": "self",
      "symbol": "crc32",
      "library_cage": "zlib-cage",
      "args": [
        { "type": "scalar" },
        { "type": "ptr", "direction": "in", "size_arg": 2 },
        { "type": "scalar" }
      ]
    }
  ]
}
```

At startup, `init_routing`:
1. Creates each library cage via `create_library_cage`.
2. Marks listed libraries as interceptable (so `instance_dylink` installs portal stubs).
3. Calls `register_library_handler` for each `interpose` rule (equivalent to issuing
   `lind_interpose_symbol` at startup before the application cage begins executing).

---

## API Design

The API is organized in three layers, mirroring the threei pattern:

```
Layer 1 — Internal Rust API    (threei/library_handler.rs)
Layer 2 — Syscall-level API    (RawCallFunc handlers, registered like register_handler)
Layer 3 — C / WASM-side API   (glibc wrappers callable from user WASM programs)
```

---

### Syscall Numbers (`sysdefs/src/constants/sys_const.rs`)

Following the existing lind-specific range (1001–1003 are taken by threei):

```rust
pub const LIND_INTERPOSE_SYMBOL_SYSCALL:          u64 = 1004;
pub const LIND_REMOVE_INTERPOSITION_SYSCALL:       u64 = 1005;
pub const LIND_CREATE_LIBRARY_CAGE_SYSCALL:        u64 = 1006;
pub const LIND_DESTROY_LIBRARY_CAGE_SYSCALL:       u64 = 1007;
pub const LIND_COPY_INTERPOSITION_TABLE_SYSCALL:   u64 = 1008;
```

---

### Layer 1 — Internal Rust API (`threei/src/library_handler.rs`)

Pure Rust functions that directly manipulate `LIBRARY_HANDLER_TABLE`.
Analogous to threei's internal `register_handler_impl`, `_get_handler`, etc.

```rust
/// Register that calls to `symbol` from `source_cage` should route to `library_cage`.
/// Returns 0 on success, -ESRCH if either cage is dead/exiting.
/// Analogous to: register_handler_impl
pub fn interpose_symbol(
    source_cage: u64,
    symbol: &str,
    library_cage: u64,
) -> i32

/// Remove the interposition rule for `symbol` in `source_cage`.
/// Returns 0 on success, -ENOENT if no rule exists.
pub fn remove_interposition(
    source_cage: u64,
    symbol: &str,
) -> i32

/// Look up the library cage currently handling `symbol` for `source_cage`.
/// Analogous to: _get_handler
pub fn get_library_route(
    source_cage: u64,
    symbol: &str,
) -> Option<u64>  // returns library_cage_id

/// Copy all interposition rules from `src_cage` to `dst_cage`.
/// Called by fork machinery so the child inherits the parent's rules.
/// Analogous to: copy_handler_table_to_cage_impl
pub fn copy_interposition_table(
    src_cage: u64,
    dst_cage: u64,
) -> i32

/// Remove all interposition rules where `cage_id` is either source or library cage.
/// Called on cage exit. Analogous to: _rm_cage_from_handler / _rm_grate_from_handler
pub fn remove_all_for_cage(cage_id: u64)
```

---

### Layer 2 — Syscall-level API

Functions following the `RawCallFunc` signature, registered during cage init.
Table-level syscalls (1004, 1005, 1008) are registered under `THREEI_CAGEID` (same as
`register_handler` and `copy_data_between_cages`). Cage-lifecycle syscalls (1006, 1007)
are registered under `WASMTIME_CAGEID` (same as fork/exec handlers) because they involve
Wasmtime store creation.

#### `lind_interpose_symbol` — syscall 1004

```rust
/// Register an interposition rule: calls to `symbol` in `target_cageid` route to `library_cage_id`.
/// `symbol_ptr` is a WASM pointer to a null-terminated string in `symbol_cageid`'s memory.
pub fn lind_interpose_symbol(
    target_cageid:    u64,  // source cage to apply the rule to
    symbol_ptr:       u64,  // WASM ptr to null-terminated symbol name
    symbol_cageid:    u64,  // cage that owns the symbol string
    library_cage_id:  u64,  // library cage to route to
    _arg2_cageid:     u64,
    _arg3: u64, _arg3_cageid: u64,
    _arg4: u64, _arg4_cageid: u64,
    _arg5: u64, _arg5_cageid: u64,
    _arg6: u64, _arg6_cageid: u64,
) -> i32  // 0 on success, -ESRCH if cage dead
```

#### `lind_remove_interposition` — syscall 1005

```rust
/// Remove the interposition rule for `symbol` in `target_cageid`.
pub fn lind_remove_interposition(
    target_cageid:  u64,
    symbol_ptr:     u64,
    symbol_cageid:  u64,
    _arg2: u64, _arg2_cageid: u64,
    _arg3: u64, _arg3_cageid: u64,
    _arg4: u64, _arg4_cageid: u64,
    _arg5: u64, _arg5_cageid: u64,
    _arg6: u64, _arg6_cageid: u64,
) -> i32  // 0 on success, -ENOENT if no rule
```

#### `lind_create_library_cage` — syscall 1006

```rust
/// Create a library cage that hosts the library at `path_ptr`.
/// `n_workers` controls the worker pool size (0 = use default).
/// Returns the new library_cage_id on success (positive i32), or -errno on failure.
pub fn lind_create_library_cage(
    _target_cageid: u64,
    path_ptr:       u64,  // WASM ptr to null-terminated library path
    path_cageid:    u64,  // cage that owns the path string
    n_workers:      u64,  // worker pool size (0 = default)
    _arg2_cageid:   u64,
    _arg3: u64, _arg3_cageid: u64,
    _arg4: u64, _arg4_cageid: u64,
    _arg5: u64, _arg5_cageid: u64,
    _arg6: u64, _arg6_cageid: u64,
) -> i32  // library_cage_id on success, -errno on failure
```

#### `lind_destroy_library_cage` — syscall 1007

```rust
/// Shut down a library cage: drain in-flight calls, destroy workers, free cage ID.
pub fn lind_destroy_library_cage(
    _target_cageid:  u64,
    library_cage_id: u64,
    _arg1_cageid:    u64,
    _arg2: u64, _arg2_cageid: u64,
    _arg3: u64, _arg3_cageid: u64,
    _arg4: u64, _arg4_cageid: u64,
    _arg5: u64, _arg5_cageid: u64,
    _arg6: u64, _arg6_cageid: u64,
) -> i32  // 0 on success, -ESRCH if cage not found
```

#### `lind_copy_interposition_table` — syscall 1008

```rust
/// Copy all interposition rules from `src_cage` to `dst_cage`.
/// Called by the fork machinery; not typically called directly by user code.
/// Analogous to: copy_handler_table_to_cage (syscall 1003)
pub fn lind_copy_interposition_table(
    src_cage:     u64,  // parent cage
    dst_cage:     u64,  // child cage
    _arg1_cageid: u64,
    _arg2: u64, _arg2_cageid: u64,
    _arg3: u64, _arg3_cageid: u64,
    _arg4: u64, _arg4_cageid: u64,
    _arg5: u64, _arg5_cageid: u64,
    _arg6: u64, _arg6_cageid: u64,
) -> i32  // 0 on success
```

---

### Registration (`rawposix/src/init.rs`)

Following the pattern of `register_threei_syscall`, a new `register_library_interpose_syscalls`
function registers all five handlers during cage init:

```rust
pub fn register_library_interpose_syscalls(self_cageid: u64) -> i32 {
    // Table operations → THREEI_CAGEID (like register_handler)
    register_handler(..., self_cageid, LIND_INTERPOSE_SYMBOL_SYSCALL,    THREEI_CAGEID, fp_interpose, ...);
    register_handler(..., self_cageid, LIND_REMOVE_INTERPOSITION_SYSCALL, THREEI_CAGEID, fp_remove,    ...);
    register_handler(..., self_cageid, LIND_COPY_INTERPOSITION_TABLE_SYSCALL, THREEI_CAGEID, fp_copy, ...);

    // Cage lifecycle → WASMTIME_CAGEID (like fork/exec handlers)
    register_handler(..., self_cageid, LIND_CREATE_LIBRARY_CAGE_SYSCALL,  WASMTIME_CAGEID, fp_create,  ...);
    register_handler(..., self_cageid, LIND_DESTROY_LIBRARY_CAGE_SYSCALL, WASMTIME_CAGEID, fp_destroy, ...);
}
```

---

### Layer 3 — C / WASM-side API

Thin glibc wrappers that call `make_syscall` with the appropriate syscall number.
A WASM orchestrator program calls these like any other libc function.

```c
/* Register: route calls to `symbol` in `source_cage` to `library_cage`.
 * Returns 0 on success, -1 on error (errno set). */
int lind_interpose_symbol(uint64_t source_cage,
                          const char *symbol,
                          uint64_t library_cage);

/* Remove an interposition rule.
 * Returns 0 on success, -1 / ENOENT if no rule existed. */
int lind_remove_interposition(uint64_t source_cage, const char *symbol);

/* Create a library cage hosting the library at `path`.
 * `n_workers` == 0 uses the default pool size.
 * Returns library_cage_id (> 0) on success, -1 on error (errno set). */
int lind_create_library_cage(const char *path, int n_workers);

/* Destroy a library cage. Returns 0 on success, -1 on error. */
int lind_destroy_library_cage(uint64_t library_cage_id);
```

---

### Internal dispatch API (not syscall-callable)

Called only by the portal stub closure inside `instance_dylink`. Not exposed as a syscall.

```rust
/// Argument metadata for one library symbol — reused from lind-remote-lib.
/// Registered at config-parse time, looked up at call time by the portal stub.
pub fn register_symbol_args(symbol: &str, args: Vec<ArgSpec>)
pub fn get_symbol_args(symbol: &str) -> Option<&[ArgSpec]>

/// Execute a cross-cage library call.
/// Called by the portal stub when LIBRARY_HANDLER_TABLE has a live route.
pub fn dispatch_library_call(
    source_cage:     u64,
    library_cage:    u64,
    symbol:          &str,
    raw_args:        &[u64],     // WASM arg values from the caller
    src_mem_base:    *mut u8,    // base of source cage linear memory
) -> anyhow::Result<u64>
```

---

### Example usage (WASM orchestrator)

```c
// Orchestrator main program — sets up cross-cage zlib interposition

// 1. Create the library cage
int lib_cage = lind_create_library_cage("/lib/libz.so", 4);

// 2. Register interposition rules for this process
lind_interpose_symbol(getpid(), "crc32",    lib_cage);
lind_interpose_symbol(getpid(), "adler32",  lib_cage);
lind_interpose_symbol(getpid(), "compress2",lib_cage);

// 3. Run normally — zlib calls transparently execute in lib_cage
run_application();

// 4. Tear down
lind_destroy_library_cage(lib_cage);
```

---

## Milestones

### Milestone 1 — `LIBRARY_HANDLER_TABLE` + syscalls

Files: `src/threei/src/library_handler.rs` (new), `src/threei/src/lib.rs`,
`src/lind-boot/src/lind_wasmtime/trampoline.rs`

Tasks:
- [ ] Add `LIBRARY_HANDLER_TABLE: DashMap<(u64, String), u64>`
- [ ] Implement `register_library_handler`, `get_library_route`, `deregister_library_handler`, `deregister_all_for_cage`
- [ ] Hook `deregister_all_for_cage` into `harsh_cage_exit` / `trigger_harsh_cage_exit`
- [ ] Add syscall numbers for `lind_interpose_symbol` and `lind_remove_interposition`
- [ ] Implement host-side handlers, register with `WASMTIME_CAGEID`
- [ ] Unit tests: register, lookup, deregister, dead-cage rejection, concurrent access

---

### Milestone 2 — Portal stubs in `instance_dylink`

Files: `src/wasmtime/crates/wasmtime/src/runtime/linker.rs`, `src/lind-remote-lib/src/lib.rs`

Tasks:
- [ ] Add interceptable-library set (populated from config at startup)
- [ ] In `instance_dylink`: if library is interceptable, install portal stub for every symbol instead of direct link
- [ ] Portal stub: runtime `get_library_route` lookup → cross-cage dispatch or `call_nested` fallthrough
- [ ] Existing `Remote` (TCP/Unix) path unchanged
- [ ] Test: load interceptable library, verify stub installed; call with no rule → local; add rule → cross-cage

---

### Milestone 3 — `LibraryCageHandler` worker pool

Files: `src/wasmtime/crates/lind-3i/src/lib.rs` (extend) or `src/lind-library-portal/` (new crate)

Tasks:
- [ ] Define `LibraryWorker` (Store, Instance, scratch_base, scratch_size)
- [ ] Define `LibraryCageHandler` (worker pool, Condvar, shutdown, active_calls)
- [ ] Implement `create_library_cage_handler(module, engine, n_workers)`
- [ ] Implement `submit(symbol, adjusted_args)` → `i64`
- [ ] Global `LIBRARY_CAGE_HANDLERS: DashMap<u64, LibraryCageHandler>`
- [ ] Unit test: instantiate handler with a toy library, call a scalar function, verify result

---

### Milestone 4 — `dispatch_cross_cage_call` with pointer marshaling

Files: `src/lind-remote-lib/src/lib.rs`

Tasks:
- [ ] Implement `dispatch_cross_cage_call`
- [ ] Reuse `ArgSpec` / `PtrSizeSpec` size resolution
- [ ] Copy-in via `copy_data_between_cages`; copy-out after call
- [ ] Handle null pointers, `NullTerminated` strings
- [ ] Unit tests: scalar call, in-ptr, out-ptr, inout-ptr

---

### Milestone 5 — Library cage creation + config wiring

Files: `src/lind-boot/src/lind_wasmtime/execute.rs`, `src/lind-remote-lib/src/lib.rs`

Tasks:
- [ ] Implement `create_library_cage(engine, library_path, n_workers)`
- [ ] Extend `routing.json` schema (`library_cages`, `interpose` blocks)
- [ ] `init_routing`: create library cages, mark interceptable libraries, pre-register rules
- [ ] Wire into lind-boot startup before application cage begins loading libraries

---

### Milestone 6 — End-to-end zlib demo

Files: `examples/cross-cage-zlib/` (new)

Tasks:
- [ ] Config for `crc32`, `adler32` intercepted to zlib cage
- [ ] Python `zlib.py` test (same as current remote-calls-zlib baseline)
- [ ] `run.sh`: no-config run (local) vs config run (cross-cage), compare output
- [ ] Verify identical Python output in both cases

---

### Milestone 7 — Performance comparison

Tasks:
- [ ] Timing in `dispatch_cross_cage_call` and portal stub
- [ ] Benchmark: local direct → portal stub (no rule) → cross-cage → TCP remote
- [ ] Measure for scalar calls (`crc32`), buffer calls (`compress`, `uncompress`)

---

## Open Questions

1. **Interceptable library granularity.** Mark whole libraries as interceptable (all symbols
   get stubs), or allow symbol-level opt-in? Whole-library is simpler; symbol-level reduces
   overhead for hot uninterposed symbols (e.g. `memcpy` from libc).

2. **Scratch buffer size.** Fixed per-worker size (e.g. 1 MB) covers most buffer-oriented
   library calls; very large compress buffers may need dynamic allocation as follow-on.

3. **Worker count.** Default 4 per library cage for the prototype. Backpressure via
   `Condvar` means the source cage blocks if all workers are busy — acceptable for now.

4. **Fork inheritance.** When the application cage forks, the child inherits
   `LIBRARY_HANDLER_TABLE` rules (same rules apply). The library cage workers are shared
   (stateless for zlib). Stateful libraries are deferred to future work.

5. **`lind_interpose_symbol` from WASM.** The syscall is already callable from any cage
   via `make_syscall`. A WASM main program can therefore set up or tear down interposition
   rules at runtime without any additional changes — full grate-model parity is inherent
   in the design.

---

## Files Changed / Created (summary)

| File | Change |
|------|--------|
| `src/threei/src/library_handler.rs` | New — `LIBRARY_HANDLER_TABLE`, registration API, cage-exit cleanup |
| `src/threei/src/lib.rs` | Expose library_handler module, hook into cage exit |
| `src/lind-boot/src/lind_wasmtime/trampoline.rs` | New syscall handlers for `lind_interpose_symbol` / `lind_remove_interposition` |
| `src/wasmtime/crates/wasmtime/src/runtime/linker.rs` | Portal stub installation in `instance_dylink` for interceptable libraries |
| `src/lind-remote-lib/src/lib.rs` | Add interceptable-library set, `dispatch_cross_cage_call` |
| `src/wasmtime/crates/lind-3i/src/lib.rs` | New `LibraryCageHandler` + `LibraryWorker` |
| `src/lind-boot/src/lind_wasmtime/execute.rs` | Wire library cage creation at startup |
| `examples/cross-cage-zlib/` | New end-to-end demo |
