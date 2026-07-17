# Issue: marshal-infer mis-describes byval/sret-lowered arguments and returns (complex, long double)

**Status: implemented for byval arguments, sret/fp128 returns, AND fp128
arguments — all four now marshal via a general "WebAssembly ABI lowering"
layer in `Infer.cpp`/`ParamTree.h`/`main.cpp`. One remaining caveat: the
runtime's own multi-slot argument capture (wasmtime/Rust side) has not been
end-to-end verified — see "What's still not fixed" below.**

## Background

`_Complex double` and `long double` are currently either excluded from interposition
(`complex`, via `is_marshalable()` in `gen_grate.py`) or interposed-but-crashing
(`long double`, ~131/142 of the current libm-suite crashes under
`libm_full_grate.cwasm` — see `tests/grate-tests/lib-interpose/LIBM_INTERPOSITION.md`).
Both are instances of the *same* underlying gap in `marshal-infer`, not two separate bugs.

Neither type is architecturally special. Both are >8-byte values (16 bytes: two
doubles for complex; 16-byte storage for this build's ldbl-96 long double) that don't
fit in a single wasm scalar slot (`i32`/`i64`/`f32`/`f64`). The wasm32 C ABI's
universal rule for any value like this — complex, long double, or an ordinary
`struct` passed/returned by value — is to lower it to **indirect passing**: an
argument becomes a hidden pointer (LLVM's `byval` attribute), and a return becomes a
hidden pointer written to instead of a real return value (LLVM's `sret` attribute,
return type becomes `void`/`nil` in the IR).

Confirmed via `wasm-objdump -x` on `build/sysroot/lib/wasm32-wasi/libm.so`:

| function | C signature | actual wasm signature |
|---|---|---|
| `sin` | `double sin(double)` | `(f64) -> f64` |
| `cabs` | `double cabs(complex)` | `(i32) -> f64` — complex arg is a pointer |
| `csqrt` | `complex csqrt(complex)` | `(i32, i32) -> nil` — arg0 is an sret pointer for the return, arg1 a pointer to the complex arg |
| `cpow` | `complex cpow(complex, complex)` | `(i32, i32, i32) -> nil` — sret return pointer + 2 argument pointers |
| `__fpclassifyl` | `int __fpclassifyl(long double)` | `(i64, i64) -> i32` — the single long double arg spans 2 raw slots |
| `cosl` | `long double cosl(long double)` | `(i32, i64, i64) -> nil` — sret return pointer + 2-slot arg |

`marshal-infer` builds its spec from the C/DWARF-level type, not the ABI-lowered LLVM
IR type, so it never notices any of this.

## Correction: complex and long double are NOT the same mechanism at the IR level

The original framing above ("both are instances of the same underlying gap") turned
out to be imprecise in a way that matters for the fix. Verified directly by compiling
both through the exact `-emit-llvm -c` pipeline `marshal-infer` actually consumes,
then separately disassembling the real compiled wasm object:

- **`_Complex double`**: the `.bc` `marshal-infer` reads **already shows the
  indirection as real LLVM IR attributes**:
  ```llvm
  define hidden void @cexp_test(ptr sret({ double, double }) %0, ptr byval({ double, double }) %1)
  ```
  `hasByValAttr()` / `hasStructRetAttr()` find this directly — clang's frontend
  performs the ABI lowering during `-emit-llvm` itself, before marshal-infer ever
  sees the module.

- **`long double`** (`fp128` in LLVM): the `.bc` shows a **completely ordinary
  direct-value signature, no attributes at all**:
  ```llvm
  define hidden fp128 @cosl_test(fp128 %0)
  ```
  `hasStructRetAttr()`/`hasByValAttr()` both return `false` — there is nothing to
  check. The real indirect behavior (confirmed via `wasm-objdump`: arg0 is a real
  sret pointer, and the `fp128` argument arrives as two raw `i64` values stored
  directly, never loaded through any pointer) is produced **only** by the wasm32
  backend's SelectionDAG type-legalization pass, which runs *after* `-emit-llvm`
  has already produced the bitcode this tool consumes. There is no IR attribute for
  `Infer.cpp` to find; by the time it looks, that information doesn't exist yet in
  the artifact it's given.

This means the fix needs **two different mechanisms**, not one:
1. **Attribute-based** (`hasByValAttr()` / `hasStructRetAttr()`) — complete and
   correct for `_Complex` and for ordinary large `struct`-by-value
   arguments/returns, since clang's frontend already makes the indirection
   IR-visible for both.
2. **Hardcoded target-ABI fact** for `fp128` specifically — not a bug-detector,
   just a constant, always-true statement: *"an `fp128`-typed value, on wasm32,
   is always sret-returned, and always occupies two raw `i64` slots as an
   argument."* Keyed on the LLVM type being literally `fp128`, not on any attribute.

## What was implemented

### 1. Byval arguments → const-sized IN pointer (`tools/marshal-infer/src/Infer.cpp`)

In `inferFunction`'s per-argument loop, before the existing `NodeKind::Pointer`
dispatch: if the real `Argument` has `hasByValAttr()`, the existing (DWARF-correct,
but wrongly-`Scalar`-or-`Struct`-tagged) `TreeNode` is wrapped as the pointee of a
synthetic `Pointer` node — `dir=In` (byval means the callee gets its own copy;
writes never reach the caller, so no copy-back is attempted regardless of what
`analyzeAccess` might otherwise infer from the callee's own writes to its local
copy), `sizeKind=Const`, sized via `getParamByValType()` + `DataLayout::
getTypeAllocSize()` (not the DWARF size, to stay correct even if they ever
diverge). If the pointee is a composite (an ordinary byval struct, not complex),
`annotateComposite` still runs on it for full field-level resolution, and the
existing global "non-mappable component" safety net still force_locals the whole
function if any field comes back unresolved — exactly as it already does for a
named struct-pointer argument. No new marshalling primitive was needed; this
reuses the exact `LIND_ARG_PTR` shape a real `T*` argument already produces.

### 2. sret-shaped / fp128 returns → synthetic leading OUT-pointer argument, `retKind` stays/becomes `Void`

This turned out to need **no new `RetKind`, no `gen_grate.py` changes beyond
removing the obsolete exclusion, and no `lind_marshal.h` runtime changes at all** —
a simpler design than originally proposed below. The key realizations:

- `gen_grate.py`'s `emit_function_spec` builds `spec->args[]` **generically** from
  whatever `args` the JSON contains, in order, with `nargs = len(args)` — it has no
  assumption tying the count/order to the C-source-level parameter list. It also
  emits fully signature-agnostic K&R externs (`extern long name();`) for the real
  function's address — the exact C-level type never matters for compilation.
- The runtime's `pass_fptr_to_wt` interception already captures **every raw
  wasm-level call argument** generically (`a1..a6`), with no notion of "which ones
  are real C arguments vs compiler-synthesized ones" — for an sret-returning
  function, the caller's own hidden sret pointer is already sitting in `a1`
  (wasm ABI position 0), captured for free.
- `lind_marshal_dispatch`'s existing generic `LIND_ARG_PTR` pre/post-call logic
  already does exactly "allocate a shadow buffer, call the real function with its
  address, copy the buffer back to the caller's original pointer afterward" for
  *any* `OUT`-direction pointer argument — which is precisely what an sret buffer
  needs, with zero new code.

So the sret/fp128-return case is represented as **one more argument**, not a return
kind: `FunctionTrees::retSretArg` (`ParamTree.h`) holds a `TreeNode`
(`kind=Pointer, dir=Out, sizeKind=Const, constSize=<real return size>`), built in
`Infer.cpp` when either `sretOffset==1` (true sret, IR-visible, sized via
`getParamStructRetType()`) or `F.getReturnType()->isFP128Ty()` (hardcoded rule,
sized via `DataLayout::getTypeAllocSize()` on the `fp128` type — always 16 bytes).
`retKind` is forced to `Void` for the `fp128` case (true sret already naturally
computes `Void`, since `F.getReturnType()` really is `void` there). `main.cpp`
splices `retSretArg` in as `args[0]` **only at JSON-emission time** — `ft.params`
itself is never reindexed, so none of the file's existing DWARF-index arithmetic
(`dwarfIndexOf`/`sretOffset`, used throughout endptr/return-alias detection)
needed to change to accommodate it.

**`gen_grate.py`**: the `type == "complex"` exclusion in `is_marshalable()` is
removed — it's now correctly represented as an ordinary `kind:"ptr"` entry the
existing `LIND_ARG_PTR` path already handles, and `"type"` is purely an
informational label on the pointee, not a kind affecting marshalling.

### Verified

Synthetic tests, compiled through the real `-emit-llvm -c` pipeline:

| function | shape | result |
|---|---|---|
| `cexp_test(complex) -> complex` | byval arg + sret return | `marshal`: `args[0]` = synthetic sret OUT ptr (const_size=16), `args[1]` = byval complex IN ptr (const_size=16, no copy-back) |
| `cabs_test(complex) -> double` | byval arg only | `marshal`: `args[0]` = byval complex IN ptr; `ret` unaffected (`scalar`) |
| `cosl_test(long double) -> long double` | fp128 arg + fp128 return | `force_local` (fp128 argument, see below) — but note the return-side `retSretArg` synthesis DOES fire and is recorded, in case a future fix unlocks the argument side |
| `fpclassify_test(long double) -> int` | fp128 arg only | `force_local` (fp128 argument) |

Also re-verified against the real glibc source: `sysdeps/ieee754/dbl-64/s_sin.c`
(plain `double`, unaffected) still classifies `sin`/`cos`/aliases correctly — no
regression to the non-complex/non-fp128 path. Full regression pass across every
earlier synthetic test suite from this project's `marshal-infer` work (cursor mode,
comparators, NSS struct fields, return-to-static, variadic A–Z, incomplete-composite
handles) showed no new misfires.

## `long double` / `fp128` **arguments** — now unlocked via a WASM ABI lowering layer

Originally deferred (see git history for this doc's earlier revision) with the
concern that `lind_marshal_dispatch`'s fixed `lind_handler6_t = uint64_t(*)
(uint64_t×6)` call shape captures **one raw 64-bit slot per logical argument**,
while an `fp128` argument genuinely needs **two**. Revisited once it became clear
this concern doesn't actually apply: `pass_fptr_to_wt`/`lind_marshal_dispatch`
already capture raw wasm-level call arguments **positionally and generically**
(`a1..a6`), with zero notion of "which raw slots belong to which C-level
argument" — exactly the same insight that made the sret-return case free (see
above). A `long double` argument splitting into 2 raw `i64` slots is, from the
interception layer's point of view, indistinguishable from "two ordinary scalar
arguments in a row" — no new runtime capability was needed, only a way for
`marshal-infer` to *describe* it that way.

### Implementation: `TreeNode::abiSlots` + an explicit lowering layer

`Infer.cpp` now has a small block of named helpers (`lowerFp128Arg`,
`lowerByvalArg`, `lowerAbiReturn`) under a "WebAssembly ABI lowering layer"
comment, unifying the two ABI-lowering mechanisms described above (frontend
byval/sret vs. backend fp128 scalar-splitting) as one concept applied per
argument/return during inference:

- `TreeNode` (`ParamTree.h`) gained `uint32_t abiSlots = 1` — how many raw
  wasm-level call-site slots this one logical (DWARF-level) parameter occupies.
  `lowerFp128Arg` sets `abiSlots = 2` for any argument whose real LLVM type is
  `fp128`; `dir`/`sizeKind` are meaningless for `abiSlots>1` (pure raw-bits
  passthrough, never an address — nothing to translate).
- `main.cpp`'s `jsonFunction` expands any `abiSlots>1` node into N consecutive
  plain-`"kind":"scalar"` JSON `args` entries at emission time (labeled `(lo)`/
  `(hi)`, matching the confirmed wasm32 low-then-high slot order). Exactly like
  `retSretArg`, `ft.params`'s own length/indexing is never changed to
  accommodate this — only the final JSON array shifts.

### The remapping bug this surfaced (found and fixed in the same pass)

`sizeArgIndex` (buf/len pairing) and `intoArgIndex` (endptr-style
`ptr_into_arg`) are computed in `Infer.cpp` in terms of `ft.params`'s *internal*
index, but consumed by the runtime (`_lind_compute_size`'s
`LIND_SIZE_FROM_ARG` case, reading `raw_or_sibling[as->size_arg_index]`) as a
position into the *final* JSON `args` array — which `gen_grate.py` builds 1:1
with `raw_args[]`. Once either `retSretArg` or an `abiSlots>1` argument shifts
the JSON array out of 1:1 correspondence with `ft.params`'s indexing (which the
sret-return work above already did, latently, for any sret-shaped function with
a `size_arg_index`/`intoArgIndex` reference — this was never triggered in
practice before, since alias/cursor return kinds are mutually exclusive with a
sret/fp128-forced-`Void` return), the emitted index would point at the wrong
argument.

Fixed by computing a `ft.params`-index → final-JSON-position remap vector once
per function in `jsonFunction`, threaded through `jsonNode`'s recursive calls,
and applied at both emission sites (`size_arg_index` and `into_arg`) whenever
the referencing node is not itself a struct field (`isField` — a struct field's
`size_field_index` is a same-struct sibling-field index, never a top-level
argument index, and is never remapped). Verified against:
- a synthetic `mix2(long double x, char *buf, unsigned long len)` calling
  `memset(buf, 0, len)` — `size_arg_index` now correctly reads `3` (post-fp128-
  expansion position of `len`), not the pre-fix `2`.
- the real libc `strtold(const char *nptr, char **endptr)` — `into_arg` now
  correctly reads `1` (post-sret-splice position of `nptr`), not the pre-fix `0`
  (which would have pointed at the sret pointer itself).
- full regression pass: every function with neither an sret return nor an
  `abiSlots>1` argument (`strtol`, `strsep`, `memchr`, `memcpy`, and the full
  pre-existing synthetic suite) emits byte-identical indices to before.

### What's still not verified

The claim that `pass_fptr_to_wt`'s generic `a1..a6` capture actually preserves
two adjacent raw slots for a split `fp128` argument, in the correct low-then-high
order, all the way through the real wasmtime call-interception path — as opposed
to this session's testing, which only exercised `marshal-infer`'s own JSON
output (structural/positional correctness of the generated *spec*) — has **not**
been confirmed by actually compiling a grate and running a real `long double`-
argument libm call end-to-end through it. The reasoning that this should already
work (the capture is genuinely positional and slot-count-agnostic, proven by the
sret case working correctly today) is strong, but is still an inference from
reading the dispatch code, not an observed runtime result.

## Impact

- The `_Complex` exclusion is gone: any function with a `_Complex` argument or
  return (across libc/libm/libz) is now correctly marshalled instead of being
  unconditionally excluded.
- Ordinary byval/sret-lowered plain `struct`-by-value arguments/returns are fixed
  by the same general mechanism, if any exist elsewhere in the marshalled
  libraries (not verified against a concrete example in this codebase, but the
  detector is general — not complex-specific).
- The long-double **return** path and **argument** path are both fixed. Full
  corpus regeneration (`tools/marshal-infer/infer_libm.sh`,
  `tools/marshal-infer/infer_libc.sh`): libm's exported-symbol corpus (492
  covered functions) now has **zero** force_local decisions, including 91
  functions with a genuine fp128 argument (`atan2l`, `log1pl`, `jnl`, ...); libc
  flips exactly 2 functions (`strfroml`, `strfromf64x`, both `long double`-
  argument) from force_local to marshal, landing at 1382 marshal / 465
  force_local (up from 1380/467) — a clean, isolated, fully-accounted-for delta
  with no other regressions.
- Pending the runtime-capture verification noted above, this closes the
  representation gap behind the majority of the ~131/142 libm-suite crashes
  cited in `LIBM_INTERPOSITION.md` (most affected functions take a `long
  double` argument, not just return one) — but does not yet *prove* those
  crashes are fixed, since that requires an end-to-end run.
