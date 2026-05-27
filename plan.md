# Library-Level 3i Implementation Plan

## Core Principle

Minimize trusted (host) code. Reuse existing threei infrastructure wherever possible.
Policy and routing logic live in user-level grate code (guest WASM), not in the host runtime.

---

## How It Maps onto Existing 3i

Syscall 3i (existing):
```
app cage syscall
  → make_syscall(cage_id, syscall_num, arg1..6)
  → HANDLERTABLE[cage_id][syscall_num] → (handler_cage, fn_ptr)
  → _call_grate_func → trampoline → grate handler WASM function
```

Library-level 3i (new):
```
app cage library call
  → portal stub in instance_dylink  (only installed for registered symbols)
  → make_syscall(cage_id, call_id, arg1..6)    ← call_id is LIBCALL_BASE + N
  → HANDLERTABLE[cage_id][call_id] → (handler_cage, fn_ptr)  ← same table, fake syscall num
  → _call_grate_func → trampoline → grate handler WASM function
```

The portal stub is the library analogue of `make_syscall` for syscalls — it is the
interception point, installed per-symbol at `instance_dylink` time instead of statically
in glibc.

---

## Reused Infrastructure (no changes needed)

| Component | Reused as-is |
|-----------|-------------|
| `HANDLERTABLE` | stores lib handlers keyed by fake call_id |
| `_call_grate_func` + trampoline | cross-cage invocation of handler WASM function |
| `copy_data_between_cages` (syscall 1002) | cross-cage pointer marshaling in handler code |
| `copy_handler_table_to_cage` (syscall 1003) | handler table inheritance on fork |
| `make_syscall` dispatch | lib call dispatch (portal calls make_syscall with call_id) |

---

## New Trusted Code (minimal)

### 1. Fake syscall: `register_lib_handler` (syscall 1004)

Does two things:
1. Stores `(cage_id, symbol_name) → call_id` in a new `LIB_SYMBOL_TABLE` — used by
   `instance_dylink` to know which symbols to intercept and what call_id to assign.
2. Calls `register_handler_impl(cage_id, call_id, handler_cage_id, handler_fn_ptr)` to
   put the handler into the existing `HANDLERTABLE`.

```
LIB_SYMBOL_TABLE: Mutex<HashMap<u64 /*cage_id*/, HashMap<String /*symbol*/, u64 /*call_id*/>>>
```

No new dispatch table. No new trampoline. Just one new HashMap and one new fake syscall.

### 2. Portal stub in `instance_dylink`

For each symbol exported from a loaded module, check `LIB_SYMBOL_TABLE[cage_id][symbol]`.
If found, install a host closure (portal stub) instead of the direct function link.

The portal stub is minimal trusted code:
```rust
// pseudo-code
let portal = move |caller, raw_args, results| {
    let args = extract_raw_args(raw_args);   // up to 6 u64s from WASM ValRaw
    let ret = make_syscall(
        cage_id, call_id, /*syscall_name=*/0, cage_id,
        args[0], 0, args[1], 0, args[2], 0,
        args[3], 0, args[4], 0, args[5], 0,
    );
    results[0] = pack_result(ret, wasm_return_type);
};
```

That is the entirety of the new trusted code in `instance_dylink`.

---

## New Untrusted Code (guest grate)

The grate C program that sets up interposition:

```c
// grate coordinator: runs before app cage starts

// Register handler for libz.so:crc32
uint64_t call_id = LIBCALL_BASE + 1;
register_lib_handler(app_cage_id, "libz.so", "crc32", call_id,
                     grate_cage_id, (uint64_t)&my_crc32_handler);

// The handler itself (in grate WASM):
int64_t my_crc32_handler(uint64_t app_cage_id, uint64_t arg0,
                          uint64_t arg1, uint64_t arg2, ...) {
    // arg1 is a pointer into app cage's memory — use copy_data_between_cages to read it
    char buf[len];
    copy_data_between_cages(grate_cage_id, grate_cage_id,
                             arg1, app_cage_id,   // src: app cage ptr
                             (uint64_t)buf, grate_cage_id, // dst: local buf
                             arg2, 0);
    // now do something with buf — forward to another cage, call local lib, etc.
    return crc32(arg0, buf, arg2);
}
```

Policy decisions (where to route, which cage to call, whether to forward over network)
are entirely in this guest code. The host has no knowledge of zlib, routing rules, etc.

---

## Files to Change

| File | Change |
|------|--------|
| `src/threei/src/threei_const.rs` | Add `REGISTER_LIB_HANDLER_SYSCALL = 1004`, `LIBCALL_BASE = 2000` |
| `src/threei/src/lib_symbol_table.rs` | **New.** `LIB_SYMBOL_TABLE`, `register_lib_symbol`, `get_lib_call_id` |
| `src/threei/src/threei.rs` | Add `register_lib_handler()` function (1004 handler); add `rm_cage_from_lib_symbol_table` call in cage cleanup |
| `src/threei/src/lib.rs` | Re-export new public functions |
| `src/glibc/lind_syscall/lind_syscall_num.h` | Add `REGISTER_LIB_HANDLER_SYSCALL 1004` |
| `src/glibc/lind_syscall/lind_syscall.c` | Add `register_lib_handler(...)` wrapper using `make_threei_call` |
| `src/glibc/lind_syscall/lind_syscall.h` | Declare `register_lib_handler` |
| `src/wasmtime/crates/wasmtime/src/runtime/linker.rs` | Extend `instance_dylink`: check `get_lib_call_id`, install portal stub via `make_syscall` |
| `tests/library-interposition-examples/inter-cage-zlib/` | **New.** Demo using the new API |

---

## Phases

### Phase 1 — Registration + Portal Installation

Goal: a grate registers a handler; calling the symbol from the app cage reaches the handler.

1. Add `REGISTER_LIB_HANDLER_SYSCALL = 1004` and `LIBCALL_BASE = 2000` to `threei_const.rs`.
2. Add `lib_symbol_table.rs`: `LIB_SYMBOL_TABLE`, `register_lib_symbol(cage_id, symbol, call_id)`, `get_lib_call_id(cage_id, symbol) -> Option<u64>`.
3. Add `register_lib_handler` in `threei.rs`: validates args, calls `register_lib_symbol` + `register_handler_impl`.
4. Add syscall number + glibc wrapper `register_lib_handler(target_cage, lib_name, symbol_name, call_id, handler_cage, handler_fn_ptr)`.
5. Extend `instance_dylink`: for each exported symbol, check `get_lib_call_id(cage_id, symbol)`; if found, install the minimal portal stub closure.

**Success criterion:** grate registers handler for `crc32`; app calling `crc32` invokes the
grate handler (args not yet usable, but control reaches it).

---

### Phase 2 — Arg Passing and Cross-Cage Memory

Goal: handler can read all args and marshal pointer data.

The portal stub already passes raw args as `arg1..arg6`. The handler receives them as
normal WASM function parameters. For pointer args, the handler calls the existing
`copy_data_between_cages` (syscall 1002) — no new host code needed.

1. Write a test handler that reads scalar args correctly.
2. Write a test handler that copies a pointer arg from the app cage using `copy_data_between_cages`.
3. Verify output pointer copy-back works (handler writes to app cage memory via `copy_data_between_cages` in the reverse direction).

**Success criterion:** a simple `crc32` handler reads the input buffer from the app cage,
computes the checksum locally, and returns the correct result.

---

### Phase 3 — Inter-Cage Execution

Goal: handler forwards the call to a library function running in a separate library-host cage.

The library-host cage is a normal cage with the target `.so` loaded. To call a function in
it, the handler grate uses the existing `_call_grate_func` mechanism — specifically, it
calls a WASM function exported from the library-host cage that wraps the real library call.

No new host primitives needed: the handler can call `make_syscall` with the library-host
cage's registered entry point, using the existing 3i dispatch.

**Success criterion:** Python calls `zlib.crc32`; handler grate forwards to a zlib-host
cage; correct result returned.

---

### Phase 4 — Python zlib Demo

Rebuild `tests/library-interposition-examples/zlib-compression/` using the new API:
- Remove `LIND_REMOTE_CONFIG` host config
- Add a grate coordinator C program that calls `register_lib_handler` for `crc32`/`adler32`
- Python binary and `zlib.py` script unchanged

---

## Known Limitations (accepted for now)

- **6-arg limit**: portal passes at most 6 raw args via `make_syscall`. Functions with >6
  args are not supported in Phase 1. This covers the vast majority of library functions.
- **Pre-start registration only**: handlers must be registered before `instance_dylink`
  runs for the app cage. Dynamic (post-start) patching is future work.
- **No ABI metadata in trusted code**: the handler is responsible for knowing which args
  are pointers and their sizes. ABI metadata lives in the grate code, not the host.
  Auto-marshaling (trusted portal doing pointer copies automatically) is deferred.

---

## Open Questions (deferred)

1. Handler table inheritance across `fork` — `copy_handler_table_to_cage` handles `HANDLERTABLE`; `LIB_SYMBOL_TABLE` needs a parallel `copy_lib_symbol_table_to_cage` call in the fork path.
2. `dlsym`-returned function pointers bypass the portal — separate issue, deferred.
3. Multiple stacked handlers per symbol — deferred.
4. Runtime (post-start) dynamic patching — requires GOT update under lock, deferred.
