# Issue: lind-wasm's dylink loader cannot support cross-module symbol interposition (a preloaded library can never call back into the main module's exports)

## Status

**The primary issue was actually two independent bugs, found while building an
isolated reproducer that avoids one to exercise the other:**

1. **Loader/ordering (`--preload` instantiates library-before-main, so a
   library's genuine `env::foo` import can never see main's export) — FIXED.**
   See "Ordering fix (implemented)" below. `tests/playground/dylink_min/`
   still reproduces the *combined* symptom (both bugs stacked); the new,
   ordering-only reproducer is `tests/playground/dylink_ordering/`.
2. **`wasm-ld` gap: a symbol a module defines locally itself never becomes an
   `env` import at all, so nothing at runtime — old or new — ever gets a
   chance to redirect that call.** This is still open; it's *why*
   `dylink_min/` (and the real OpenBLAS `xerbla_` case) still fail even with
   the ordering fix in place — see "No workaround needed" below, unchanged.

**Secondary issue (setjmp/longjmp crash) is RESOLVED** — turned out to be a
`compile_openblas.sh` build-configuration gap, not a lind-wasm runtime bug. See
"Secondary bug" below for the fix.

## Ordering fix (implemented)

Fixed in `wasmtime::Linker` (`src/wasmtime/crates/wasmtime/src/runtime/linker.rs`)
and `wasmtime_lind_utils::LindGOT` (`src/wasmtime/crates/lind-utils/src/lib.rs`),
wired up from `module_with_preload`'s call site in
`src/lind-boot/src/lind_wasmtime/execute.rs`.

**Root cause (of this half):** `Instance::new` must satisfy every wasm import
synchronously, at the moment a module is instantiated. Preloaded libraries are
instantiated one at a time, strictly before the main module (see "Root cause"
below, point 1) — so a library's genuine, unresolved `env::foo` function
import (a real direct-call import, not `GOT.func`/`GOT.mem`) previously got
bound *permanently* to a hard-trap stub via
`Linker::define_unknown_imports_as_traps`, before main (the only possible
provider) had even been instantiated. Once bound, a wasm function import can't
be swapped out later — so the call was doomed regardless of what main went on
to export.

**Fix:** added `Linker::define_unknown_imports_as_deferred_calls`, used in
`module_with_preload` in place of the immediate hard trap for function-typed
imports. Instead of binding the import to a value now, it binds a small host
trampoline that defers the actual lookup to *call* time: it consults
`LindGOT::get_cached_symbol` (backed by `symbol_cache`, which is already
populated for every module's exports via `apply_GOT_relocs` — main module
included, regardless of preload order) for the target's shared
indirect-function-table index, fetches the funcref, and forwards the call. By
the time the guest program actually runs (and the library's call fires), main
has always already been instantiated and registered its exports — so this
reproduces the property that makes ELF symbol interposition work: resolution
is deferred until the full symbol table is known. If the symbol is genuinely
never defined anywhere, the stub falls back to the same trap behavior as
before.

This is a runtime loader fix; it does not touch `wasm-ld` and does nothing for
bug 2 above. It only helps when the compiler actually emitted a real `env`
import for the call in the first place.

**Verified:** `tests/playground/dylink_ordering/` — `lib.c`'s `do_work()`
calls an `xerbla_` it only declares `extern` (forcing a genuine `env::xerbla_`
import, sidestepping bug 2); `main.c` defines the only `xerbla_` and is built
with `-rdynamic` (wasm-ld doesn't export ordinary main-executable symbols by
default — a instance of bug 2 on the *export* side, routed around the same
way `lind_compile -rdynamic` was designed for). Before the fix:
`unknown import: \`env::xerbla_\` has not been defined` at call time. After:
`[main] override xerbla_: error` — the library's call reaches main's
definition. Confirmed both ways by building the pre-fix and post-fix
`lind-boot` binary against the identical reproducer (`git stash` around the
three changed files). No regression in the existing
`tests/unit-tests/dylink_tests/` suite (8/8 pass, including the
`dlopen`-based `rdynamic/rdynamic_main.c` test, which exercises the same
main-exports-a-symbol-a-library-calls-back-into pattern via `dlopen` instead
of `--preload` — that path was already unaffected since dlopen always runs
after main is instantiated).

## Background

Found while adding OpenBLAS to `lind-wasm-apps` and building its own `utest`/`utest_ext`
test suites as dylink executables that import BLAS/CBLAS symbols from `libopenblas.so`
at runtime via `lind_run --preload env=lib/libopenblas.so <binary>`, instead of
statically linking `libopenblas.a` into the test binaries.

`openblas_utest` (66 tests) passed cleanly this way. `openblas_utest_ext` (600 tests)
crashed at test 317/600 (`domatcopy:xerbla_colmajor_invalid_lda`) with:

```
failed to run main module

Caused by:
    0: failed to invoke command default
    1: thrown Wasm exception
```

216 of the 600 tests in `utest_ext` depend on the test suite **overriding OpenBLAS's
own `xerbla_` error handler** — a standard "does this routine correctly reject a bad
argument?" pattern:

- `interface/omatcopy.c` (and similarly many other BLAS/CBLAS entry points) calls
  `BLASFUNC(xerbla)` (i.e. `xerbla_`) when it detects an invalid parameter, then
  returns immediately.
- OpenBLAS's own default `xerbla_` (`driver/others/xerbla.c`) just prints a message
  and returns — it does not abort.
- The test suite's `utest/test_extensions/xerbla.c` defines its **own** `xerbla_` that
  records the error into global state instead, so `check_badargs()`-style helpers can
  assert the error was actually raised.
- This relies on ordinary C symbol interposition: the test's copy of `xerbla_` must
  take priority over the library's own copy, for calls that happen **inside the
  library's own code** (`omatcopy` calling `xerbla_` internally).

On a statically-linked build (one link unit), this trivially works — the linker uses
whichever definition is directly linked and never even extracts the library's own
`xerbla.o` from the archive. On the dylink build, it silently doesn't: `libopenblas.so`
was built as a separate, already-fully-linked module (via `--whole-archive`), so its
internal call to `xerbla_` was resolved to its own copy at the `.so`'s own build time,
long before the test binary exists. The override is simply never consulted, the
assertion (`ASSERT_EQUAL(TRUE, passed)`) genuinely fails for the first time in the
whole suite — and `ctest.h`'s `longjmp`-based "report this failure and continue to the
next test" path then crashes instead of unwinding gracefully (see "Secondary bug"
below), taking down the whole binary instead of reporting 216 failures and moving on.

## Root cause

Verified precisely, on both sides, rather than assumed:

### Native ELF: the override does work, and why

Reproduced directly (not via `LD_PRELOAD` — the override compiled straight into the
main executable):

```c
// main.c — defines its own xerbla_ instead of relying on the library's
int xerbla_(char *name, int *info, int length) {
    printf("OVERRIDE xerbla_ called: name=%.6s info=%d\n", name, *info);
    return 0;
}
// ... calls domatcopy_() with a deliberately invalid lda ...
```
```
$ gcc main.o -o test_dynamic -lopenblas
$ ./test_dynamic
OVERRIDE xerbla_ called: name=DOMATC info=7 (NOT exiting)
main: returned normally after domatcopy_ call
```

`nm -D` on the native `libopenblas.so` shows `xerbla_` marked **weak**:
```
00000000000eeab0 W xerbla_
```
— confirmed deliberate upstream design: `driver/others/xerbla.c` has
`int BLASFUNC(xerbla)(...) __attribute__((weak, alias ("__xerbla")))`, gated on
`#ifdef __ELF__` (non-ELF targets get a plain strong definition with no override
path at all — relevant below, since wasm32 does not define `__ELF__`).

`LD_DEBUG=symbols ./test_dynamic` confirms the mechanism: `symbol=xerbla_; lookup in
file=./test_dynamic [0]` — the dynamic linker's **global symbol scope** is searched in
a fixed order (main executable first, `LD_PRELOAD` libraries even earlier, then
`DT_NEEDED` dependencies), and it's this scope — not the calling module — that a
library's own internal PLT-bound call to `xerbla_` resolves against. A separate
`LD_PRELOAD` experiment (override compiled into a standalone `.so`, main executable
left vanilla) confirms the same mechanism independent of weak/strong marking or
static-into-main linking:

```
$ LD_PRELOAD=./override.so ./test_vanilla
PRELOADED OVERRIDE xerbla_ called: name=DOMATC info=7
```

Critically, ELF's resolution is **deferred**: PLT calls are bound lazily (or eagerly,
but still after all objects are mapped), against a scope that is only finalized once
mapping of every participating object (main + preloads + all dependencies) is
complete. Order of *mapping* doesn't matter for correctness; only the fixed *priority*
list used once resolution actually happens.

### lind-wasm: the override cannot work, by construction

Read the actual runtime implementation (not generic wasm-ld/WASI-SDK assumptions):

- Load orchestration: `src/lind-boot/src/lind_wasmtime/execute.rs:237-350` (preload
  loop), `:587-660` (`load_main_module`).
- Cross-module `env` symbol table (GOT): `src/wasmtime/crates/lind-utils/src/lib.rs`
  (`LindGOT`, `new_entry`/`cache_symbol` — first entry wins via
  `compare_exchange(0, val)`, later duplicates ignored).
- `env`-import binding: `src/wasmtime/crates/wasmtime/src/runtime/linker.rs`
  (`instance_dylink`, `module_with_preload`, `insert`).
- GOT relocation of a module's own exports:
  `src/wasmtime/crates/wasmtime/src/runtime/instance.rs` (`apply_GOT_relocs`).

Two properties, both confirmed against the source and then empirically:

1. **Preloaded modules are instantiated one at a time, in `--preload` order, strictly
   before the main module.** `apply_GOT_relocs`/`instance_dylink` runs for each
   preload as it loads; the main module's own run afterward, in `load_main_module`.
   GOT slots are claimed first-come-first-served — the *opposite* of ELF's
   main-executable-first priority.
2. **Only symbols a module actually imports (`GOT.func`/`env` function imports) go
   through this machinery at all.** A module's internal wasm `call` to a function
   *it itself defines* is a local function-index reference — it never appears in an
   import section, so nothing in the runtime ever sees it, let alone redirects it.
   Since `libopenblas.so`'s internal call to `xerbla_` was resolved directly by
   `wasm-ld` at the `.so`'s own build time (not left as an import), it was never a
   candidate for interposition at all, independent of point 1.

Fundamentally, wasm's `Instance::new(store, module, imports)` requires **every**
import to be satisfiable at the moment a module is instantiated — there is no
wasm-level equivalent of ELF's lazy/deferred PLT binding. Each module gets resolved
"now, completely," one at a time, rather than "later, once everyone is known."

## Verification

Two experiments, isolating the mechanism from all OpenBLAS/ctest complexity.

### 1. Baseline reproducer — live, at `tests/playground/dylink_min/`

`lib.c` (analogous to `libopenblas.so`) defines its own `xerbla_` and a `do_work()`
that calls it internally. `main.c` (analogous to `openblas_utest_ext`) defines its own
override and calls into the library. `build.sh` builds both via `lind_compile`
(`--compile-library` for `lib.c`, default dynamic build for `main.c`); `run.sh`
installs and runs via `lind_run --preload`.

```
$ ./build.sh && ./run.sh
[lib] default xerbla_: error
```

The library's own copy runs; `main`'s override (`"[main] override xerbla_: ..."`)
never prints. Confirmed identically whether built by hand (raw `clang`/`wasm-opt`/
`add-export-tool`/`lind-boot` invocations) or through the real `lind_compile` pipeline
— see "A note on `lind_compile`" below.

The same `lib.c`/`main.c`, built natively instead (`gcc -shared -fPIC lib.c -o lib.so`,
`gcc main.c -o main -L. -lfoo -Wl,-rpath,.`) gives the opposite result — main's
override wins there. That native-comparison script was written during the original
investigation but no longer exists on disk (this repo's `tests/playground/` is not
durable across sessions — see caveat at the end of this doc); the native mechanism
itself is independently reproduced and explained in "Root cause" below.

### 2. `-rdynamic` test — historical, scripts no longer on disk (see caveat)

Tested whether `lind_compile -rdynamic` — "export all symbols from the main module so
dynamically loaded libraries can resolve them at runtime" — closes the gap. Built a
library variant with **no** definition of `xerbla_` at all (so it must resolve as a
genuine unresolved `env` import), and a main module built with `-fvisibility=default
-Wl,--export-dynamic` (what `-rdynamic` adds). Result:

```
[main] calling do_work(1)...failed to run main module

Caused by:
    0: failed to invoke command default
    1: unknown import: `env::xerbla_` has not been defined
```

Confirms: the library's `xerbla_` correctly became a real unresolved import — but
main's `-rdynamic`-exported copy never got a chance to satisfy it. This is consistent
with the instantiation-order finding above: the library is instantiated (and all its
imports resolved) **before** the main module exists at all, so there is nothing for it
to import from yet, `-rdynamic` or not. `-rdynamic`'s actual intended use case is
almost certainly `dlopen()`-loaded libraries (loaded programmatically *after* `main`
has already started and its exports already exist in the GOT table) — not
`--preload`-based loading, where every preload is fully resolved before `main` is even
instantiated. **Not currently re-runnable** — the scripts for this specific variant
were not preserved; recreating it would mean: strip `xerbla_` out of `dylink_min/lib.c`
entirely, and add `-rdynamic` to the `lind_compile` call for `main.c` in `build.sh`.

### A note on `lind_compile` / `lind-wasm-opt`

The reproducer was originally built with hand-rolled `clang`/`wasm-opt`/
`add-export-tool`/`lind-boot` invocations (copied from `lind-wasm-apps`'s existing
`compile_*.sh` app scripts), not `lind_compile`. Re-verified with the real
`lind_compile --compile-library` / default-dynamic pipeline afterward — same
interposition result either way (as expected; that finding is independent of build
tooling). But this hand-rolling turned out to matter a lot for the "Secondary bug"
below: `lind_compile`'s default dynamic build adds `-fwasm-exceptions -mllvm
-wasm-enable-sjlj` (real wasm exception-handling-based `setjmp`/`longjmp`) and its
`wasm-opt` step goes through `scripts/bin/lind-wasm-opt`, which adds the matching
`--translate-to-exnref` conversion pass — neither of which `compile_openblas.sh` (or
this reproducer, originally) included. Once both were added, the crash described in
"Secondary bug" below turned out to be entirely a build-configuration gap, not a
runtime defect — see that section for the full resolution. **Lesson generalized:** use
`lind_compile`/`lind-wasm-opt` rather than hand-rolling `clang`/`wasm-opt` invocations
wherever the build shape allows it (a single `.c` file, or a post-link `.wasm` file) —
they encode real, load-bearing correctness requirements (flag combinations, pass
ordering) that are easy to silently get wrong by copying an older/incomplete pattern
from an existing app script instead.

## Why "instantiate main first" is not the fix

An intuitive fix would be: reverse the order, instantiate main before the libraries.
This doesn't work, and isn't what native ELF actually does. If `main` were instantiated
first, it would just fail for the opposite reason — `main` typically needs symbols
*from* the libraries (in the OpenBLAS case, `cblas_dgemm`, `domatcopy_`, etc.), which
wouldn't exist yet either. Whichever side goes first is missing something only the
other side has.

Native ELF's actual process is **three separate phases**, not a single order:
1. **Mapping** — main executable, `LD_PRELOAD` libraries, and all `DT_NEEDED`
   dependencies (transitively) all get mapped into memory; nothing executes yet.
2. **Relocation/resolution** — PLT-bound calls resolve (lazily or eagerly) against the
   complete global scope built from step 1, which by this point already contains
   everyone, so it doesn't matter who calls whom first.
3. **Initialization** — constructors run in dependency order (libraries' constructors
   before the thing that depends on them; `main()` last).

The property that makes overriding work is that **resolution (step 2) is deferred
until mapping (step 1) is complete** — by the time any cross-module call actually
executes, the full symbol table, including main's own symbols, already exists.

## Proposed fix (not implemented — scope note)

A genuine fix would need a second phase inserted before instantiation, not a
reordering of the existing one-shot instantiate-and-resolve-immediately flow:

1. Parse/link every participating module (main + all preloads) and register what each
   one *exports*, without yet resolving any of their own imports or running start code.
2. Build one complete symbol table from all of that.
3. *Then* instantiate every module against the complete table.

This is closer to introducing wasm-level PLT-style deferred/lazy binding into the
loader than to a simple ordering change — a real runtime feature, not a build-flag
tweak. Not attempted here; flagged for whoever owns the dylink loader.

## Secondary bug found along the way — RESOLVED (was a build-config gap, not a runtime bug)

`utest/ctest.h`'s assertion-failure recovery path
(`__CTEST_LONGJMP(ctest_err, 1)`, used by e.g. `ASSERT_EQUAL` on failure) is supposed
to gracefully unwind out of a failing test and report it, then continue to the next
one. Initially it didn't — every genuinely-failing assertion crashed the whole binary
(`thrown Wasm exception`, uncaught) instead of being reported and continuing, which is
what turned the 108 xerbla-interposition failures below into one hard crash at the
*first* one (test 317/600) instead of 108 reported failures plus 492 passes.

**Root cause:** `compile_openblas.sh` never enabled the flags this glibc port's
`setjmp`/`longjmp` actually requires — a straightforward build-configuration gap, not a
runtime defect. Confirmed by reading the source directly:

- `setjmp/wasm_eh_setjmp.c`'s header comment: "When user code is compiled with
  `-fwasm-exceptions -mllvm -wasm-enable-sjlj`, clang 18 transforms each
  `setjmp()`/`longjmp()` call site into: `try { ... } catch (__c_longjmp) { ... }`" —
  i.e. these flags aren't optional extras, they're what makes the call sites route
  through this glibc port's actual sjlj support at all.
- `setjmp/longjmp.c`: `__longjmp` unconditionally throws a wasm exception via
  `__wasm_longjmp`/`__builtin_wasm_throw` regardless of whether the flags were used.
  Without them, the corresponding `setjmp()` call site never got the matching
  try/catch installed, so the thrown exception had nothing to catch it and propagated
  all the way to the top, killing the process — exactly the observed signature.
- Adding just the clang flags surfaced a second, related gap: `lind-boot --precompile`
  then failed with `legacy_exceptions feature required for try instruction` — clang 18
  emits the *legacy* wasm-EH encoding, but Cranelift (lind-boot's compiler) only
  supports the newer standard/exnref-based proposal.
  `scripts/bin/lind-wasm-opt`'s own source names the fix in a comment:
  `--translate-to-exnref` converts one to the other, and must run *after* `--asyncify`.

**Fix applied** in `compile_openblas.sh`: added `-fwasm-exceptions -mllvm
-wasm-enable-sjlj` to the utest compile/link flags, and switched from raw `wasm-opt`
invocations to `scripts/bin/lind-wasm-opt --target=main` / `--static` (the wrapper that
already encodes the correct flag set and pass ordering for this — including the exnref
conversion — per target; this project should have been using it from the start instead
of hand-rolling `wasm-opt` flags).

**Result:** the crash is completely gone. `openblas_utest_ext` now runs all 600 tests
to completion:
```
RESULTS: 600 tests (492 ok, 108 failed, 0 skipped) ran in 85 ms
```
All 108 failures were individually confirmed to be the same, already-understood
xerbla-interposition pattern from "Root cause" above (`***** ILLEGAL VALUE OF
PARAMETER NUMBER N NOT DETECTED *****`) — no new or different failure types. This also
corrects the earlier estimate of "216 affected tests" (counted by name pattern) down to
the real, now-measured number: **108**.

## No workaround needed (revised from an earlier draft of this doc)

An earlier version of this document proposed excluding the interposition-dependent
tests from the dylink test registry, since at the time they caused a hard crash rather
than a reported failure. That's no longer necessary — with the `setjmp`/`longjmp` fix
above, `openblas_utest_ext` runs to completion and reports 108 genuine, expected
failures rather than crashing, so nothing needs to be special-cased at the registry
level. The 108 failures are simply the honest, visible cost of the dylink symbol
interposition limitation from "Root cause" above.

## Impact

Any future app in `lind-wasm-apps` (or elsewhere) that relies on the "library caller
provides a callback/override that the library calls internally" pattern — common for
error handlers, allocators, logging hooks, and similar library-provided extension
points — will hit this same wall under lind-wasm's current dylink model, whether
built via `--preload` (this issue) or, less certainly, via `dlopen()` (untested here,
but plausible given `-rdynamic`'s apparent intended use case). Worth being aware of
before designing any dylink-based test or integration strategy that depends on
symbol-interposition-style overriding.

## Reproducer

`tests/playground/dylink_min/` in this repo — minimal, 4 files:
- `lib.c` — defines `xerbla_` and a `do_work()` that calls it internally.
- `main.c` — defines its own `xerbla_` override and calls `do_work()`.
- `build.sh` — builds both via `lind_compile` (`--compile-library` for `lib.c`,
  default dynamic build for `main.c`), installs into `lindfs`.
- `run.sh` — runs the installed binary under lind-wasm with `lind_run --preload`.

```
$ ./build.sh && ./run.sh
[lib] default xerbla_: error
```

The library's own copy runs; `main`'s override (`"[main] override xerbla_: ..."`)
never prints — reproducing the core finding in isolation, in two source files,
independent of OpenBLAS/ctest.

**Caveat for whoever picks this up:** `tests/playground/` in this repo is not durable
across sessions — an earlier, more elaborate version of this reproducer (with a native
`gcc -shared` comparison script and the `-rdynamic` variant as separate files) was
built, verified, and documented, then disappeared from disk before this doc was
finalized (environment reset between turns, not a deliberate removal). Only
`dylink_min/`'s 4 files above currently exist; the native-comparison and `-rdynamic`
findings are preserved as transcripts in "Verification" above but are not independently
re-runnable right now. If continuing this work, consider moving durable artifacts
(reproducers, this doc) somewhere more persistent than `tests/playground/`, or expect
to recreate them.
