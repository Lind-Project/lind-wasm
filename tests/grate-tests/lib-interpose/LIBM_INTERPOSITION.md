# libm interposition — viability findings

Status: **first interposition run complete — zero new regressions vs. the baseline.**
This records (1) the investigation into whether a full-libm interposition grate is a
viable next target (after full-libc/libz proved hard — see `INTERPOSE_TEST_RESULTS.md`),
(2) a comprehensive correctness baseline for libm *without any interposition*, using
glibc's own test suite, and (3) the first actual interposition grate build + full-corpus
run, diffed against that baseline.

---

## Why libm, and why it looked promising

Every category that dominates the current libc failures is structurally **absent** from libm:

| category that sinks libc coverage | present in libm? |
|---|---|
| file descriptors (deferred — see `INTERPOSE_TEST_RESULTS.md` Group B) | none — math functions never touch fds |
| control-flow terminators (`exit`/`abort`) | none |
| stdio streams (`puts`/`fputs`/…) | none |
| startup/TLS-init functions (`__libc_setup_tls`/`__ctype_init`/…) | none — those are libc-startup symbols, not exported by libm, so they're not even candidates for this grate |
| variadic (`printf`/`err`/…) | none |
| locale | none |
| `setjmp`/`longjmp`/`exec` | none |

## The numbers

`libm.marshal.json` already exists (866 functions catalogued by `marshal-infer`) —
**every single one is `decision:"marshal"`, zero `force_local`** at the inference stage
(compare libc: 1468/1847 marshal, 379 force_local).

Arg-kind distribution across all 866 functions: **1157 scalar args, 111 pointer args.**
Of the 111 pointer args:

| `(size_kind, pointee_kind)` | count | example |
|---|---|---|
| `(const, scalar)` | 98 | `modf`/`sincos`/`remquo`-style fixed-size scalar OUT param |
| `(cstr, scalar)` | 7 | `nan(const char *tagp)` |
| `(const, struct)` | 6 | `fegetenv`/`fesetenv`/`feholdexcept`/`feupdateenv` (`fenv_t`, 28B) and `fegetmode`/`fesetmode` (`femode_t`, 8B) |

All of these are shapes the runtime already handles correctly (fixed-size, no nested
pointers, no cursor semantics) — this is about as clean a marshalling surface as exists
anywhere in glibc.

---

## The one real gap found: complex-number functions are misclassified

**~154/866 (~18%) of the catalog are C99 `_Complex` functions** (`csqrt`, `cabs`, `cpow`,
`clog`, `ctanh`, …, and their `f`/`f32`/`f64`/`f32x`/`f64x`/`l` suffix variants).

`marshal-infer` reports these as a plain scalar:
```json
{"kind": "scalar", "type": "complex", "size": 16}
```
This is the **C/DWARF-level view** (`double complex` passed "by value") — but it does not
match the *actual compiled wasm ABI*. Verified directly against the compiled
`build/sysroot/lib/wasm32-wasi/libm.so` with `wasm-objdump -x`:

```
csqrt:  sig=4 = (i32, i32) -> nil     # sret output pointer + indirect input pointer
csqrtf: sig=4 = (i32, i32) -> nil     # SAME — even though float complex is only 8 bytes
```

**Both the argument and the return are indirect (pointer) parameters** — the wasm32 ABI
lowers *every* `_Complex` value to pointer-indirect passing, regardless of size (confirmed
this isn't a "too big for a register" threshold effect: `csqrtf`'s complex value is only
8 bytes — would fit a single register — and it's *still* passed indirectly).

**Why this matters:** if a `LIND_ARG_SCALAR` spec (the current mapping for `kind:"scalar"`
regardless of `size`) were used for these, the dispatcher would pass the caller's raw
pointer value straight through with **no cross-cage translation** — the callee, running in
a different cage, would dereference a foreign-cage address (wrong data, fault, or silent
corruption depending on what happens to live at that address in the callee's memory).

**This is a real gap in the inference tool** (describing the pre-ABI-lowering C type
rather than the actual wasm-level calling convention), not something specific to this
grate. Likely affects any C type that the wasm32 ABI lowers to indirect/sret regardless of
its declared size — `_Complex` is the only instance found in libm, but the same class of
bug could in principle affect other multi-word by-value aggregates elsewhere in the
catalog (not yet audited outside libm).

**Recommendation:** exclude complex-number functions from the first interposition pass —
a clean, precise, structural exclusion (the JSON already tags them `"type":"complex"`, so
this is a one-line filter on that field, not a name-list hack) — and interpose the
remaining **~712** functions, which have no known structural issues.

---

## Secondary finding: no errno propagation (documented, currently inconsequential)

`lind_marshal.h` has **zero** errno-handling logic today. A domain/range error
(e.g. `sqrt(-1)` setting `EDOM`) would set `errno` in the *grate's* TLS if `sqrt` is
interposed, not the app's — the app-side `errno` would remain unset/stale.

Checked: **no test in `tests/unit-tests/math_tests/` currently asserts on `errno`**, so
this gap won't affect the pass/fail count of the existing suite. Documented so it isn't
"discovered" again later if a future test (or a different test suite) does check errno
after a math call.

---

---

## No-interposition baseline: glibc's own libm test suite

**Goal:** before touching interposition at all, establish ground truth — does *this port's*
libm behave correctly under plain lind (no grate)? Rather than rely on the 3 hand-written
`tests/unit-tests/math_tests/`, we found and used glibc's own canonical correctness suite
(`src/glibc/math/`), which is comprehensive by construction (auto-generated from a
706K-line, MPFR-verified reference corpus) and already present in this repo's vendored
glibc source — see prior conversation for how it was located and validated (`gen-libm-test.py`
is host-runnable with just Python 3 stdlib; confirmed by generating a real, correct
2235-line `cos` test driver).

### Scope

glibc's own suite has 131 function families (56 "auto" data-driven + 75 "noauto"
hand-written, deduplicated) + 6 narrow-conversion families (`add`/`div`/`fma`/`mul`/`sqrt`/`sub`).
Each family is compiled once per precision type via a tiny generated wrapper
(`#include <test-double.h>` / `libm-test-<fn>.c`), matching glibc's own recipe exactly.

Verified via `wasm-objdump` symbol-alias checks (e.g. `cosf32`/`cosf64`/`cosf32x` all map to
the *same function index* as `cosf`/`cos`) that testing **double + float + long double**
(glibc's own `test-types-basic`) fully covers the `f32`/`f64`/`f32x`/`f64x`-suffixed
aliases' behavior — they're literally the same compiled code. Total scope: 131 × 3 + 6 × 3
narrow type-pairs (float-double/float-ldouble/double-ldouble) = **411 test binaries**.

Documented as explicit follow-up (not blocking): 7 additional float128-narrow type-pairs
(confirmed some, e.g. `f64addf128`, are genuinely distinct compiled bodies — not aliases)
and the float32/float32x/float64 narrow pairs (confirmed pure aliases of float-double, so
lower priority).

### Build pipeline (glibc-internal, not the public sysroot)

This test code is glibc's *internal* test machinery, not ordinary userspace code — it needs
glibc's internal include paths (`sysdeps/...`) and macros (`_LIBC_REENTRANT`,
`-include libc-symbols.h`, etc.), which `scripts/lind_compile` (designed for the public
sysroot) doesn't provide. glibc's own `make tests` target does have this machinery wired up,
but additionally tries to bootstrap `testroot.pristine` — a native-execution install tree
glibc's test infrastructure assumes it can build and run tests against directly on the
host — which isn't wired up for this cross-compiled-to-wasm32 port and fails deep in the
dependency chain (`csu/wasi_thread_start.s.o` has no build rule).

**Resolution:** extracted the real internal `-I`/`-D` flags from a live (partial) `make
tests` trace, and compiled+linked each of the 411 wrappers directly:
1. `clang --target=wasm32-unknown-wasi` + glibc's internal `-I sysdeps/...` chain +
   `-D_LIBC_REENTRANT -include libc-symbols.h` etc. → `.o`
2. Link against `libm-test-support-<type>.o` (also needs a per-type wrapper, same pattern)
   + `lind_utils.o` + `-lm`, with lind's static-build flags (`-pthread` is required — its
   absence causes `undefined symbol: __tls_align/__tls_size` at link time).
3. `scripts/lind-wasm-opt --static` (the epoch/signal_callback injection pass — skipping
   this causes `Failed to find epoch global export!` at runtime).
4. Run via `scripts/lind_run` (no grate).

### Results

All 411 test binaries, one bucket each — **PASS** or **FAIL**, with FAIL split by root cause.

| | count | share |
|---|---|---|
| **PASS** | **246** | **60%** |
| FAIL | 165 | 40% |
| **Total** | **411** | |

#### PASS — 246 / 411

| sub-category | count | meaning |
|---|---|---|
| Clean (every check, including FP-exception-flag checks) | 106 | |
| Numerically correct; only FP-exception-flag/errno checks fail | 140 | wasm32 has no hardware FP exception flags (the compiler itself warns of this at compile time) — not a bug |

#### FAIL — 165 / 411

| sub-category | count | root cause |
|---|---|---|
| Missing libm symbol (didn't build) | 138 | see *Missing symbols* below |
| Real correctness bug | 20 | see *Correctness bugs* below |
| Crash (`wasm trap: call stack exhausted`) | 6 | see *Crashes* below |
| Binaryen `wasm-opt` parse failure | 1 | `test-double-scalb` — `[parse exception: popping from empty stack]`; `scalb` also has a separate, real correctness bug in its `float` variant (below), so this function looks generally unhealthy on this target |

**Missing symbols (138)** — not a harness problem, these functions genuinely don't exist
in this build. 122 share one root cause: `libm-test-support-ldouble.o` (linked into every
ldouble test) references `ceill`, which is absent from both the compiled `libm.so` **and**
`libm.marshal.json` (confirmed via `wasm-objdump` and the catalog) — this single missing
symbol blocks nearly the entire ldouble bucket. Also individually confirmed missing:
`floorl`, `truncl`, `rintl`, `logbl`, `expm1l`, `atanl`, and `significand`/`significandf`/
`significandl` (all three precisions — likely an intentionally-omitted legacy/BSD
function). `compat_totalorder`/`compat_totalordermag` (all precisions) are also missing,
but that's *expected*: legacy ABI-versioned compatibility shims with no relevance to a
brand-new port that has no old ABI to stay compatible with.

**Correctness bugs (20)** — verified real (not exception-flag artifacts) by inspecting
full failure context, not just summary counts:

- **`fma` (double, float)**: loses the fused-multiply-add precision benefit. Example:
  `fma(0x1.0000002p+0, 0xf.fffffep-4, -0x1p-300)` returns `1.0` but should return
  `0.99999999999999989` — off by exactly 1 ULP, consistent with the multiply being rounded
  *before* the add (ordinary `x*y+z`) rather than a true single-rounding fused op. `fma`
  has **no entry in the x86_64 `libm-test-ulps` file**, meaning exact (0-ULP) results are
  expected even on x86_64 — a genuine implementation gap, not a legitimate wasm32 rounding
  difference needing its own ULP tolerance.
- **`nexttoward` (double, float)**: doesn't propagate a NaN `to` argument.
  `nexttoward(1.1, qNaN)` returns `1.1` unchanged instead of `NaN`. Also has no x86_64 ulps
  entry (0-ULP/exact expected) — genuine bug.
- **`scalb` (float)**: failures around `±0`/`±inf` argument combinations. (The `double`
  variant fails earlier, at the `wasm-opt` stage — see above.)
- **Narrow conversions — `add`/`sub`/`mul`/`div`/`fma`, nearly every type-pair** (15 of the
  20 failing tests): a *systematic* pattern — the narrowed result loses low-order bits of
  the **first argument itself**, even when the second argument is astronomically
  insignificant. Example: `add_double(-0x1.000001p+0, -0x4p-1024)` (essentially
  `-1.000001 + ~0`) returns exactly `-1.0` — the `2^-23` correction from the *first*
  argument's own value is dropped, not just the negligible second one. Suggests the
  narrowing-add implementation isn't computing at full intermediate precision before the
  final narrowing round. (Narrow `sqrt` passed — flag-only — where it built; see *Crashes*
  for the two type-pairs where it didn't build at all.)

**Crashes (6)** — genuine, not a test-data-size artifact (`libm-test-fmax.c`/
`libm-test-fmin.c` are only ~130 lines, tiny compared to e.g. `cos`'s 2235): `fmax`/`fmin`
(double, float) and narrow `sqrt` (double-ldouble, float-ldouble). Looks like a real
stack-exhaustion bug (likely runaway recursion) in these implementations on this platform —
not yet root-caused, flagged for follow-up.

### Takeaways for the interposition plan

- **246/411 (60%) pass outright**, and of the 165 that don't, **138 are simply missing
  symbols** (nothing to interpose) — so of what's actually *present and buildable*
  (272 binaries), **246/272 (90%) is numerically correct** under plain lind. Strong
  foundation: the interposition grate's own correctness (once built) can be compared
  directly against this baseline per-function.
- The **20 correctness bugs + 6 crashes + 1 opt-failure are pre-existing libm defects**,
  independent of interposition — worth fixing (or at minimum, known and excluded from
  later interposition-vs-baseline diffs so they aren't mistaken for interposition bugs).
- **ldouble support is incomplete** in this build (missing `ceill` and several sibling
  `*l` functions) — this bounds what "the entire libm" can mean for this port: functions
  that don't exist in the compiled binary can't be interposed, tested, or missed.

---

## First interposition run

Built the actual grate and re-ran the same 272 buildable baseline binaries under it,
diffing per-test against the no-interposition results above.

### Building the grate

`gen_grate.py libm.marshal.json --lib-name libm --freestanding` (complex-type exclusion
from the viability phase already in place) → **702 marshalable handlers** (164 dropped:
~154 complex + a few others), reduced to **680 in the final grate** after fixing build
issues discovered along the way:

1. **`--compile-grate` requires a *static* build.** A dynamic-mode attempt built fine but
   failed at runtime (`unknown import: env::register_lib_handler has not been defined`) —
   the host imports `--compile-grate` needs aren't wired up for dynamic/dlopen-style
   modules. Every working grate in this repo (libc, libz) is static for the same reason;
   confirmed libm needs it too.
2. **Static linking pulls in `libm.a`'s *internal* cross-references, not just what the
   grate calls directly** — the same class of gap the no-interposition baseline already
   found (missing `ceill`/`floorl`/etc.), but now cascading through implementation
   objects. Root-caused via `llvm-nm` on each blocking `.o` member (not guesswork) and
   excluded exactly the symbols involved, including weak `f64x`/`f32x` aliases that
   resolve to the same blocked long-double body:
   `acosl`/`acosf64x`, `fmodl`/`fmodf64x`, `remainder`/`drem`/`remainderf32x`/`remainderf64`,
   `gammal`/`lgammal`/`lgammal_r`/`tgammal`/`lgammaf64x`/`lgammaf64x_r`/`tgammaf64x`,
   `powl`/`powf64x`, `remquol`/`remquof64x`.
3. **`scalb`/`scalbf`/`scalbl` trip a Binaryen `wasm-opt` parser bug** (`popping from
   empty stack`) — same failure class as the standalone `test-double-scalb` opt-failure
   found in the baseline. Excluded (the long-exponent `scalbln` family is unaffected).

All of the above are recorded in `gen_grate.py`'s `NEVER_INTERPOSE` set with comments
explaining each, so they're not accidentally "rediscovered" as marshalling bugs later.

Smoke-tested against `tests/unit-tests/math_tests/deterministic/math_tests.c`: clean pass,
correctly formatted `%f` output — notably **without** the startup-init exclusion the libc
grate needed (`__libc_setup_tls`/`__ctype_init`/`__wasi_init_tp`), since those are
libc-startup symbols libm never touches. Confirms that fix was specific to interposing
*all of libc*, not a general cost of interposition.

### Result: diffed against the 272-binary baseline

| | count |
|---|---|
| **Identical classification to baseline** (same pass/fail bucket, same failure content) | **272 / 272** |
| New regressions introduced by interposition | **0** |

Every one of the 106 clean, 140 flag-only-limited, and 20 real-bug tests produced the
*exact same* classification under the grate as under plain lind — the 20 pre-existing
bugs (`fma`, `nexttoward`, `scalb`, narrow-conversion precision loss) are unchanged by
interposition, neither better nor worse.

**One nuance, not a new bug:** the 6 known `fmax`/`fmin`/narrow-`sqrt` stack-exhaustion
crashes still crash under the grate, but with a different signature — no
`wasm trap: call stack exhausted` message, just a silent `exit(1)` right after the test
driver's startup print. Traced to `libm-test-driver.c`'s `check_ulp()` (an internal sanity
check called immediately after startup, before the main test loop) — it exercises
`fmax`/`fmin` directly, so this is the *same* underlying recursion bug, not a different
one. The difference: interposition adds real per-call stack depth (marshalling, cross-cage
dispatch), which apparently pushes an already-marginal recursion from "wasmtime's own
guarded wasm-stack limit trips cleanly" to "the host's native process stack overflows and
the process dies silently, with no wasm-level trap to report." **Worth flagging as a
general property of interposition**: a latent recursion bug in an interposed function can
become a harder failure once interposed, not just a marshalled one.

### Takeaway

For the 680 functions interposed in this first pass, the grate is **behaviorally
equivalent to no interposition at all** — a clean, strong result. Remaining work is
tracked below, not blocking: the excluded complex functions (~154, pending the ABI-lowering
fix), the handful of static-link-blocked functions (~18, genuine libm gaps on this port,
not marshalling issues), and `scalb`/`scalbf`/`scalbl` (Binaryen bug, not marshalling).

---

## Next steps (not yet done)

1. Consider a follow-up pass on the excluded complex functions once `marshal-infer` (or a
   `gen_grate.py`-side override) correctly models the indirect/sret ABI lowering for
   `_Complex` args/returns.
2. Separately (not interposition-related): investigate the `fma`/`nexttoward`/narrow-conversion
   precision bugs, the `fmax`/`fmin`/narrow-`sqrt` stack-exhaustion crashes, and the
   Binaryen `wasm-opt` bug on `scalb` — real, pre-existing defects on this port/toolchain.
3. Expand scope: the ldouble tier (once the missing `ceill`/`floorl`/etc. symbols are
   addressed) and the 139 tests currently blocked at the harness/link level would extend
   coverage beyond today's 272.
