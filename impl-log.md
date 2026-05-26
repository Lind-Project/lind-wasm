# Library-Level 3i Implementation Log

## Session start

Goal: implement library-level 3i so a grate can register handlers for a child cage's
dynamic library calls, and get a working grate-test example running.

---

## Architecture decisions

### Reuse HANDLERTABLE with fake call_ids
Library handlers are stored in the existing `HANDLERTABLE` using fake syscall numbers
in the `LIBCALL_BASE = 2000` range. No separate dispatch table needed. This means
`make_syscall(cage_id, call_id, ...)` with a libcall call_id dispatches through the
exact same grate trampoline path as real syscalls.

### LIB_SYMBOL_TABLE
A separate small table `LIB_SYMBOL_TABLE[(cage_id, lib_name, symbol_name)] -> call_id`
is needed only so `instance_dylink` knows which symbols to intercept and what call_id
to use. It does not participate in dispatch — dispatch uses `HANDLERTABLE` only.

### Portal stub calls make_syscall directly
The portal stub is a Rust host closure inside `instance_dylink`. It calls
`threei::make_syscall(cage_id, call_id, 0, cage_id, arg0, 0, arg1, 0, ...)` directly
as a Rust function call. No new trampoline needed.

### i32 return limitation (Phase 1)
`make_syscall` returns `i32`. Library functions returning `i64`/`f32`/`f64` are not
fully supported in Phase 1. The portal casts the `i32` result to the WASM return type.
For `i32`-returning functions (the test case), this is exact.

### String args in register_lib_handler
`lib_name` and `symbol_name` are passed as host pointers (translated via
`TRANSLATE_GUEST_POINTER_TO_HOST` in glibc). The Rust side reads them with
`CStr::from_ptr`. This follows the same pattern as other syscalls that take string args.

---

## Implementation log

### Challenge: WASM import module name is "env", not library name

In lind-wasm, all preloaded libraries are loaded under the `"env"` namespace
(from `--preload env=/lib/libtoy.cwasm`). This means the cage binary's WASM
import section records `(import "env" "toy_add" ...)` — not `(import "libtoy.so" "toy_add" ...)`.

Consequence: `register_lib_handler` must use `lib_name = "env"` (the WASM module name),
not the filesystem library name like `"libtoy.so"`.

### Challenge: portal stubs must be installed at import resolution, not library load time

Initially, portal stubs were installed in `instance_dylink` (when a library's exports
are added to the linker). This requires the library to be preloaded. But:
1. The test does not preload libtoy — it just intercepts the import completely.
2. Even when preloaded, timing issues arise: the stub must be installed AFTER the
   handler is registered by the grate child.

Fix: added `define_lib_interpose_stubs(module, cage_id)` to `Linker<T>`. This iterates
the main module's UNRESOLVED imports and installs portal stubs for any that have a
registered handler in `LIB_SYMBOL_TABLE`. Called in `execute.rs` before
`define_unknown_imports_as_traps`, so portal stubs take priority over trap stubs.

This means libtoy does NOT need to be preloaded — the portal intercepts the import
entirely, and the real library never runs.

### Challenge: cage 2 did not have syscall 1004 (REGISTER_LIB_HANDLER) in its handler table

`register_lib_handler` is itself invoked as syscall 1004 via `make_threei_call`. When
cage 1 forks to create cage 2, `copy_handler_table_to_cage(1, 2)` copies all of cage 1's
handlers (including syscall 1004) to cage 2. But the previous `lind-boot` binary was
built before syscall 1004 was registered in `init.rs`. Rebuilding fixed this.

### Handler fn_ptr and cageid semantics

The grate's child (cage 2) calls `register_lib_handler` with:
- `target_cage = cageid (2)` — the cage whose imports will be intercepted
- `handler_cage = grateid (1)` — the grate that owns the handler function
- `handler_fn = &toy_add_handler` — function pointer in the grate's WASM address space

`register_lib_handler` in threei.rs calls both:
1. `register_lib_symbol(2, "env", "toy_add", 2001)` — so `define_lib_interpose_stubs` can find it
2. `register_handler_impl(2, 2001, 1, toy_add_handler_ptr)` — so dispatch works via HANDLERTABLE

When the portal fires: `make_syscall(2, 2001, ...)` → HANDLERTABLE[2][2001] → grate cage 1
→ `pass_fptr_to_wt(toy_add_handler_ptr, ...)` → `toy_add_handler` returns `(a+b)*2`.

### Final result

Test passes end-to-end:
```
[Grate|lib-interpose] registering toy_add handler for cage 2
[3i|register_lib_handler] cage=2 lib=env sym=toy_add call_id=2001 handler_cage=1 fn=...
[Grate|lib-interpose] toy_add_handler: cage=1 a=3 b=4
[Cage] toy_add(3, 4) = 14
[Grate|lib-interpose] toy_mul_handler: cage=1 a=5 b=6
[Cage] toy_mul(5, 6) = 11
[Cage] PASS
[Grate|lib-interpose] PASS
```

