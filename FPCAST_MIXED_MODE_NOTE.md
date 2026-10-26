# Mixed fpcast mode vs full fpcast mode

**A** = every module is built with `--fpcast-emu` (full fpcast).
**B** = fpcast and native modules are mixed in one process (mixed mode).

All numbers below come from microbenchmarks and a bash run done on one machine. They are rough, not a full benchmark. Code is on branch `fix-cross-fpcast` (commits `8c7505425`, `8d9d20a80`, `830a32c20`, `9e14b6e56`, `a19396d45`).

## Case 1: function pointers resolved by symbol (not passed as data)

**A:** nothing to do. Every pointer is already a thunk.

**B (implemented):**
1. A function exported by a module can have two table slots: the native function and its `$fpcast_emu$` thunk. A `GOT.func` cell used to be shared by all importers. It is now split: fpcast importers get their own cells (`GOT.func.fpcast`, keyed `$fpcast_emu$<name>`), native importers keep the normal ones. Each importer receives the entry matching its own ABI. The fpcast mode of a module is detected from its exports (`Module::fpcast_mode()`), not from the engine-wide `--enable-fpcast` flag.
2. If an fpcast importer needs a thunk that the provider does not export (for example the stock native libc), the loader generates one at load time. The thunk is a tiny generated wasm adapter, compiled once per signature, with the same conversions as binaryen's thunks. Forked children and new threads replay the same table appends so function-pointer values stay identical.

**Cost:** a small one-time cost per synthesized thunk at load time (not measured). No per-call cost afterwards, because the pointer already has the right form.

## Case 2: function pointers passed as data (`qsort`, `bsearch`, `atexit`, signal handlers, thread entry, `dlsym`)

**A:** nothing to do.

**B:** the loader cannot see these pointers (they are just integers in memory), so only a runtime fix works.

- **Rejected: catch the trap and repair it.** A wasm trap unwinds the stack and cannot be resumed reliably.
- **Implemented: change the compiler.** In Cranelift, a signature mismatch in `call_indirect` no longer traps. A cold block calls a new builtin libcall, `indirect_call_mismatch`, which returns a function reference of the expected type. The matching path is unchanged.
  - Native pointer called as an fpcast thunk: wrap the callee in a thunk.
  - fpcast thunk called as a native pointer: wrap it in a native-typed adapter. It widens the arguments to `i64`, zero-fills the unused parameters, calls the thunk, and narrows the result.
  - Any other mismatch (two different native types, or more than 18 parameters) still traps.
- **First call at each distinct (callee, expected type):** creates the adapter. This is the only expensive step; its cost was not measured separately.
- **Every later crossing call:** still goes through the builtin, because the table slot cannot be rewritten without breaking callers of the other ABI. A 256-entry direct-mapped cache in the store makes this about 4 ns per call (about 14 ns before the cache, when a std `HashMap` lookup dominated).

**Switches:** `LIND_LENIENT_INDIRECT_CALL=1` must be set at `--precompile` time and at run time (it changes generated code). Adding the tunable changed the serialized settings, so every existing `.cwasm` must be recompiled.

## Performance

Overhead over native (i.e. without fpcast), per indirect call. This is a fixed per-call cost, so it is given in ns; the percentage in brackets is only for the shellsort benchmark, where each call does ~10 ns of work, and would be smaller for a bigger function body.

**Non-crossing**
1. Approach A: **~+1 ns** per indirect call (+9% in the sort; +22% worst case, a loop of nothing but indirect calls)
2. Approach B: **0** between native modules. Between fpcast modules the same as A, ~+1 ns.

**Crossing**
1. Approach A: **~+1 ns** (same as non-crossing; A has no special crossing cost)
2. Approach B:
   - raw called as fpcast: first time ~0.07-1 ms (adapter created); subsequent **~+4 ns** (+43%)
   - fpcast called as raw: first time ~0.07-1 ms (adapter created); subsequent **~+4.5 ns** (+47%)

Notes:
- "First time" is per (callee, expected type) in a process. It is the cost of compiling and instantiating the small adapter module; another callback with the same signature costs ~3-11 µs. The timer in lind has 0.6-1.5 µs resolution and the first-time figure varied a lot across runs (66 µs-1 ms), so treat it as an order of magnitude.
- Before the 256-entry adapter cache, the subsequent cost was ~+14 ns per call (+145% in the sort), dominated by a `HashMap` lookup.
- Break-even: B wins over A only if crossings are fewer than roughly a quarter of all indirect calls (4 ns per crossing vs 1 ns per indirect call), and the native modules make many indirect calls.
- Derivation: pure indirect-call loop 1028 ms native vs 1257 ms fpcast over 212.5M calls (+1.1 ns); shellsort 360 ms native, 390 ms both fpcast (+0.9 ns), 515 ms and 530 ms with every call crossing, over 36.8M calls (+4.2 / +4.6 ns).
- Other costs: `libc.cwasm` grows 20 KB (+0.13%) for the handler; all existing `.cwasm` must be recompiled.

## Real-world check: bash

Setup: dynamic fpcast bash on the native libc (every libc-to-bash callback is a crossing), with counters enabled (`LIND_COUNT_INDIRECT_CALLS=1` at precompile, `LIND_INDIRECT_STATS=1` at run time).

| workload | total indirect calls | crossings |
|---|---|---|
| startup only | 713 | 421 |
| 200k-iteration arithmetic loop | 202k | 421 |
| string loop | 162k | 421 |
| associative array, 30k entries | 62k | 421 |
| 400 command substitutions (forks) | 31k | 421 |

- The 421 crossings are a fixed startup cost: bash sorts its builtin table with `qsort` (`builtins/common.c:910`). Steady-state work adds none.
- Cost in absolute terms: 421 × ~4 ns ≈ 2 µs for B, versus ~0.1-0.2 ms of per-call overhead for A over 62k-202k indirect calls. Both are negligible against run times of 0.6-7 s.
- Across 43 small unit-test programs: 540 indirect calls in total, 4 crossings.

Wall-clock comparison (all times in **milliseconds**, from starting `lind-boot` to process exit, so they include startup, library loading and any forks), same dynamic fpcast bash in both columns, median of 3 runs. **A** runs on an fpcast libc/libm (built from the same sysroot link output with `lind-wasm-opt --fpcast-emu`; bash and libc compiled without mixed mode). **B** runs on the stock native libc/libm (compiled with `LIND_LENIENT_INDIRECT_CALL=1`). A static fpcast bash is included only as a reference for the cost of dynamic linking.

| workload (wall-clock time, ms) | A: dynamic, fpcast libc (ms) | B: dynamic, native libc, mixed (ms) | static fpcast bash, reference (ms) |
|---|---|---|---|
| startup (`-c true`) | 29 | 30 | 28 |
| arithmetic loop | 2198 | 2250 (+2%) | 1977 |
| string loop | 8135 | 8176 (+0.5%) | 7405 |
| associative array | 690 | 702 (+2%) | 620 |
| 400 command substitutions (forks) | 8116 | 6991 (-14%) | 1620 |

- **A vs B on the same dynamic bash:** within about 2% on compute workloads, which is at the noise level (one B string-loop run came in at 8828 ms). On the fork-heavy workload B was about 14% faster, plausibly because the native libc is smaller and cheaper to instantiate in each forked child (`libc.cwasm` 16.0 MB native vs 18.2 MB fpcast). The fork difference was not investigated further.
- **Static vs dynamic:** the ~10% compute gap and the 4-5x fork gap to the static bash appear in both A and B, so they come from dynamic linking, not from mixing or crossings. An earlier version of this note compared the static bash with dynamic mixed bash and wrongly attributed the gap to mixing.

## Does mixed mode ever beat full fpcast?

Not in anything measured here. Mixed mode (B) only saves the fpcast overhead (~1 ns per indirect call) inside native modules, and the programs tested make few indirect calls.

- bash, dynamic, same binary: B was within about 2-3% of A on compute workloads, slightly behind (arithmetic loop: A 2204 ms, A with the handler compiled in 2180 ms, B 2250 ms; median of 5). The handler's codegen is not the cause, since A with the handler compiled in is not slower than A. The remaining 2-3% gap is unexplained: bash makes only ~200k indirect calls and 421 crossings per run, far too few to account for ~50-70 ms.
- Libc-heavy loop with no crossings (3M iterations of malloc/memset/snprintf/strlen/strcmp/strdup/free): native libc 1240 ms, fpcast libc 1258 ms. Native is ~1.5% faster, whichever way the app is built. So fpcast costs libc about 1.5% on this kind of code, which is the most mixed mode can save here.
- The stock native `libc.so` is byte-identical to one rebuilt from the sysroot with `lind-wasm-opt --target=libc`, so the comparison above does not hide a build difference.

Conclusion: mixed mode is a compatibility feature (use fpcast and native modules together, no sysroot rebuild with `WITH_FPCAST=1`, callbacks work in both directions), not a performance feature. It could win only for a native module that makes a very large number of indirect calls of its own and crosses module boundaries rarely, and no such workload was measured.

## Caveats

- One real application (bash) plus toy tests. Programs that sort or search large data with callbacks are the likely worst case for B; they cross on every comparison.
- Not measured: the load-time cost of synthesizing GOT thunks. The first-crossing adapter cost was measured only with a low-resolution timer (0.6-1.5 µs resolution) and shows large variance (66 µs-1 ms).
- Not covered by B: pointer identity (one function can have two table indices, native and thunk, so `&foo` can differ across ABIs), and functions with more than 18 parameters (no thunk is possible).
- The full `make test` suite has not been run with mixed mode.
