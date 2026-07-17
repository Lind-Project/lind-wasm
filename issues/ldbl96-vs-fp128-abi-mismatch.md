# Issue: glibc's long-double implementation (ldbl-96) doesn't match the actual compiler type (fp128) on wasm32

## Status

**Fixed** — `sysdeps/lind/Implies` swapped to `ieee754/ldbl-128` as proposed below.
`long double` math functions now compute correctly with no grate/interposition
involved at all, confirmed. Not a marshalling/interposition bug to begin with — this
reproduced identically with **no grate at all**. Found while investigating why
131/142 libm-suite crashes under interposition were long-double-tier: those tests
turned out to crash identically in baseline too, tracing back to this.

**This fix is independent of, and does not resolve,** `marshal-infer`'s separate
`fp128`-argument `force_local` rule (see
`issues/fix-complex-and-ldbl-abi-marshalling.md` — the 2-raw-wasm-slot
argument-splitting gap, invisible at the LLVM-IR level, that the interposition/
marshalling layer still can't safely represent). `long double` functions now
compute correctly *when they run un-interposed* (the `force_local` path), which
was not true before this fix — but whether they can be safely *interposed* is
still blocked on the marshalling-side gap, unchanged by this.

## Background

`sysdeps/lind/Implies` selects this port's long-double implementation:

```
ieee754/float128
ieee754/ldbl-96
ieee754/dbl-64
ieee754/flt-32
ieee754
```

`ieee754/ldbl-96` implements `long double` as the classic x86 80-bit-extended-precision
value stored in 96 bits (12 bytes): 1 sign bit + 15 exponent bits + 16 bits padding +
a 64-bit **explicit** mantissa (no implicit leading bit). Its bit-manipulation macros
(`sysdeps/ieee754/ldbl-96/math_ldbl.h`, e.g. `GET_LDOUBLE_EXP`/`SET_LDOUBLE_WORDS`) work
by type-punning a `long double` through this union:

```c
typedef union {
  long double value;
  struct {
    uint32_t lsw;
    uint32_t msw;
    int sign_exponent:16;
    unsigned int empty:16;
  } parts;
} ieee_long_double_shape_type;
```

But wasm32 has no x87 hardware to emulate 80-bit extended precision, so clang's wasm32
target represents `long double` as **`fp128`** (genuine IEEE binary128/quad precision,
128 bits, 16 bytes, 112-bit mantissa **with** an implicit leading bit) — confirmed
directly:

```
$ echo 'long double x;' | clang --target=wasm32-unknown-wasi -xc - -S -emit-llvm -o -
@x = hidden global fp128 ...  align 16
```

So every `ieee_long_double_shape_type`-based macro overlays a **12-byte, x87-shaped**
struct on top of a value that's actually a **16-byte IEEE-quad** number. The
exponent/mantissa fields these macros extract are not merely imprecise — they're
reading bytes at the wrong offsets in the wrong format entirely.

## Why this manifests as a crash, not just wrong answers

`GET_LDOUBLE_EXP` and friends are used pervasively in argument-reduction code, e.g.
`sysdeps/ieee754/ldbl-96/e_rem_pio2l.c`, which extracts the exponent to decide how many
reduction iterations/how much precision-splitting work to do. A garbage exponent value
from the mismatched union read is exactly the kind of input that can make a
reduction/recursion loop never converge. Observed symptom matches:

```
$ scripts/lind_run test-ldouble-cos.wasm
wasm trap: call stack exhausted
```

Confirmed this is unrelated to marshalling/grates:
- Compiled a minimal reproducer doing only `x + y` / `x * y` on `long double` values
  (no glibc math calls) — this works correctly (compiler-rt's `fp128` arithmetic
  routines, e.g. `__addtf3`/`__multf3`, are fine). The bug is specifically in glibc's
  own hand-rolled bit-manipulation code, not in `long double` support generally.
- `test-ldouble-cos` crashes identically with and without any grate loaded — same
  trap, same point of failure.
- Confirmed via the full 411-test libm suite: all 131 long-double-tier test crashes
  match baseline exactly, post the complex/fp128-marshalling fix (which correctly
  `force_local`s long-double-involving functions rather than attempting to marshal
  them — so they run the real, unmodified glibc implementation either way, and both
  baseline and interposed hit this same underlying implementation bug).

## Proposed fix

This glibc source tree already has a complete, ready alternative:
`sysdeps/ieee754/ldbl-128/` (116 files vs. `ldbl-96`'s 88 — including all the same
transcendental functions: `s_cosl.c`, `s_sinl.c`, `e_rem_pio2l.c`, `k_cosl.c`, etc.).
Its `math_ldbl.h` uses 64-bit words (`parts64`), consistent with a genuine 128-bit
layout, rather than `ldbl-96`'s 32-bit-word/96-bit layout.

Proposed change: swap `sysdeps/lind/Implies`:
```diff
 ieee754/float128
-ieee754/ldbl-96
+ieee754/ldbl-128
 ieee754/dbl-64
 ieee754/flt-32
 ieee754
```

## Complication / risk (as assessed before the fix landed)

This was a glibc **build-configuration** change inside the vendored `src/glibc/`
tree, not a marshalling change — scope and risk were meaningfully different from
the `marshal-infer`/`gen_grate.py` work. The `sysdeps/lind/Implies` swap has since
landed and `long double` computation is confirmed working. The specific sub-risks
below were the pre-fix checklist; **not individually re-confirmed in this note** —
worth a quick check if not already covered elsewhere before relying on them:

- Whether anything else in this glibc port implicitly assumed `ldbl-96`'s
  12-byte/96-bit storage/layout (printf's `%Lg` formatting, `scanf`, struct layouts
  embedding a `long double` field, ABI-visible `sizeof(long double)`) and needed
  updating for `ldbl-128`'s genuine 16-byte size.
- Whether the full baseline (no grate) libm suite was re-run post-swap, not just the
  previously-crashing long-double tests — to catch any currently-passing test this
  broad a sysdeps change might have affected in either direction.
- `tools/marshal-gen/gen_grate.py`'s `fp128` force_local rule (see
  `issues/fix-complex-and-ldbl-abi-marshalling.md`) is **unaffected by this fix** —
  the *argument*-marshalling gap (2-slot arguments, invisible at the LLVM-IR level)
  is independent of which long-double sysdeps implementation this port uses, and
  remains open. `long double` functions stay `force_local`'d for interposition
  purposes even now that they compute correctly when run un-interposed.

## Impact

Resolved the crash for all long-double-tier libm functions in *both* baseline and
(via `force_local`, correctly) interposed runs (previously 131/142 crashes in the
interposed suite, all matching an identical baseline crash) — a correctness fix
independent of, and prior to, any further interposition work on `long double`.
The remaining `long double` gap is now purely the marshalling-side one (see above).
