#!/usr/bin/env bash
# Generate openblas.marshal.json: argument-marshalling inference for OpenBLAS's
# public API, sourced from lind-wasm-apps/openblas (a sibling checkout, built
# via that repo's own compile_openblas.sh). Best-effort, library-side.
#
# NOTE on TU discovery: unlike glibc (sysdeps-override TUs found via .o.d/
# .o.dt dependency files -- see infer_libc.sh/infer_libm.sh), OpenBLAS's build
# compiles DIRECTLY into archive members via GNU Make's archive-member
# implicit rule (`libopenblas.a(foo.o): foo.c`), so no loose .o or .d file is
# ever left on disk to enumerate. The real per-object compile command --
# including the exact -D name-mangling flags OpenBLAS's Makefile.tail
# generates per precision variant, e.g. -DNAME=daxpy_ -DCNAME=daxpy for the
# same source file compiled twice -- is instead recovered by tracing a forced
# dry run of the real library build (`make -n -B libs netlib`). This is
# side-effect-free here specifically because OpenBLAS's Makefiles never
# `include` an auto-generated dependency file the way glibc's do (confirmed:
# zero .d files anywhere in this source tree) -- unlike the glibc incident
# logged in local-notes/active/impl-log-inference.md, where `-n` alone still
# triggered real regeneration of an auto-included file; there is no analogous
# trap here for `-n -B` to fall into.
#
# NOTE on export list: OpenBLAS's static archive has no visibility distinction
# between public API and internal implementation (every object, from the real
# BLAS/CBLAS entry points down to the lowest-level micro-kernels, is an
# ordinary global "T" symbol -- confirmed via llvm-nm on the built archive).
# Scraping the whole archive's symbol table the way infer_libc.sh/infer_libm.sh
# scrape a real .so's dynamic export table would therefore pull in hundreds of
# internal kernel/driver helpers (sgemm_kernel, strsm_iutncopy, ...) that
# application code never calls directly -- only OpenBLAS's own interface/
# layer calls them. The EXPORT LIST (what ends up in the output JSON) is
# instead scoped to the interface/ directory (confirmed via cross-referencing
# cblas.h: every interface/*.c object corresponds to either a cblas_*
# declaration or the classic Fortran-callable BLAS name it wraps) plus a
# handful of driver/others/openblas_*.c runtime-config utilities that cblas.h
# also declares (openblas_set_num_threads, openblas_get_num_threads,
# openblas_get_num_procs, openblas_get_config, openblas_get_parallel,
# openblas_error_handle).
#
# BUT bitcode is emitted for EVERY resolvable archive member, not just the
# public-API-scoped ones -- driver/level2, driver/level3, and kernel/ objects
# are excluded from the EXPORT LIST (internal orchestration/micro-kernel code,
# the same category of exclusion as glibc's __ieee754_* internals) but their
# BODIES still need to be resident for marshal-infer's one-hop interprocedural
# length detection (Infer.cpp's detectDelegatedLength): a public wrapper like
# cblas_daxpy delegates its actual array walk to an internal kernel
# (kernel/riscv64/axpy.c's daxpy_k) that marshal-infer must be able to follow
# into and analyze, even though daxpy_k itself never appears in the output.
#
# NOTE on symbol resolution: rather than re-deriving each object's real
# defined name from its NAME/CNAME macros (interface/*.c's actual definition
# is an `#ifndef CBLAS ... NAME ... #else ... CNAME ... #endif` pick-one, so
# guessing from the flags alone requires replicating that logic exactly),
# each included object's real symbol is looked up directly from the ALREADY-
# BUILT libopenblas.a via `llvm-nm -A` (per-archive-member symbol listing) --
# ground truth, zero guessing.
#
# Pipeline:
#   1. trace the real library build (dry run) to recover exact per-object
#      compile commands
#   2. resolve every resolvable object's real symbol via llvm-nm -A on
#      libopenblas.a; mark interface/ + the public driver/others utilities as
#      the export list (everything else stays internal-only)
#   3. emit wasm32 bitcode (+DWARF) for EVERY resolvable TU, public or
#      internal, in parallel -- internal kernel/driver bodies are needed for
#      one-hop delegation lookups even though they're never emitted themselves
#   3.5. OPTIONAL: an offline LLM-prompt preview for LLM_PROMPT_FUNCTIONS,
#      if set -- see below. No network request, no model SDK; this is the SAME
#      resident bitcode step 4 analyzes, reused rather than re-traced/
#      re-compiled by a separate script, since this script's own bitcode
#      is destroyed (the whole ${WORK} directory) once it exits.
#   4. marshal-infer over all bitcode, filtered to the resolved export list -> JSON
#
# Usage: tools/marshal-infer/infer_openblas.sh [out.json]
# Output default: <repo>/openblas.marshal.json
#
# Env vars:
#   LLM_PROMPT_FUNCTIONS  comma-separated function names to write a Stage 1
#                         LLM prompt/manifest for (step 3.5), using the SAME
#                         bitcode this run already emitted -- e.g.
#                         LLM_PROMPT_FUNCTIONS=cblas_daxpy,cblas_dnrm2. Unset
#                         (the default) skips step 3.5 entirely; ordinary
#                         inference (step 4) is unaffected either way.
#   LLM_PROMPT_OUTPUT     directory for step 3.5's artifacts. Default:
#                         <repo>/llm-prompts/openblas (gitignored -- prompt
#                         artifacts are experimental output, never committed).
#   LLM_MAX_FUNCTIONS     overrides marshal-infer's --llm-max-functions (default
#                         8) for step 3.5 only. A function whose dispatch-table
#                         candidates (see resolveFunctionPointerTable,
#                         LlmPrompt.cpp) alone exceed the default budget --
#                         e.g. trmv/trsv/tpmv/tpsv/tbmv/tbsv's 8-entry
#                         trans/uplo/diag table -- needs a higher value to
#                         fully resolve. Unset keeps the tool's own default.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
MI="${SCRIPT_DIR}/build/marshal-infer"
APPS_ROOT="${LIND_WASM_APPS_ROOT:-$(cd "${REPO_ROOT}/.." && pwd)/lind-wasm-apps}"
OPENBLAS_SRC="${APPS_ROOT}/openblas"
BASE_SYSROOT="${REPO_ROOT}/src/glibc/sysroot"
LLVM_BIN="$(ls -d "${REPO_ROOT}"/clang+llvm-*/bin 2>/dev/null | head -n1)"
OUT="${1:-${REPO_ROOT}/openblas.marshal.json}"
JOBS="$(nproc)"

[[ -x "${MI}" ]] || { echo "build marshal-infer first: tools/marshal-infer/build.sh" >&2; exit 1; }
[[ -d "${OPENBLAS_SRC}" ]] || { echo "openblas source missing: ${OPENBLAS_SRC}" >&2; exit 1; }
[[ -f "${OPENBLAS_SRC}/libopenblas.a" ]] || { echo "libopenblas.a missing -- run ${OPENBLAS_SRC}/compile_openblas.sh first" >&2; exit 1; }
[[ -n "${LLVM_BIN}" && -x "${LLVM_BIN}/clang" ]] || { echo "LLVM not found under ${REPO_ROOT}" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT
BCDIR="${WORK}/bc"; mkdir -p "${BCDIR}"

# The bitcode analyzed here is deliberately compiled at a LOWER
# optimization level than the real -O2 release build compile_openblas.sh
# produces, and with loop-unrolling and vectorization explicitly disabled
# -- an analysis-specific compile profile, not an attempt to reproduce the
# shipped binary's own codegen. Optimization level does not change a
# well-defined program's semantics, only how the compiler expresses it, so
# analyzing a lower-optimization build is sound for recovering a
# function's real (length, stride) relationship; it just makes that
# relationship far easier for static analysis to prove, since transforms
# like runtime-unroll-with-remainder rewrite a loop's exit test into a
# form (an opaque equality check against a compiler-computed, sign-masked
# bound) that is NOT the loop's original source-level condition at all.
#
# Every other flag (COMMON_OPT/CFLAGS aside) still mirrors
# compile_openblas.sh's static (LIND_DYLINK=0, the default) build: any
# mismatch there changes what make believes is "current" and desyncs the
# traced commands' -D name-mangling flags from the real, already-built
# libopenblas.a this script resolves symbols against.
#
# This calls clang directly rather than scripts/lind_compile --emit-llvm, a
# concrete, structural toolchain gap: CC_WASI is substituted as OPENBLAS_MAKE_
# ARGS' CC below, so OpenBLAS's OWN Makefile constructs and invokes the full
# compile command (interspersing -D/-U flags with -c/-o in positions Make,
# not this script, decides -- see the compile_re comment further down).
# lind_compile has no calling convention that fits being invoked as another
# project's `CC=`: its CLI takes exactly one fixed source-file positional
# argument, with any extra clang flags only accepted trailing after `--`.
# lind_compile --emit-llvm also omits -pthread/-matomics/-mbulk-memory and
# has no way to disable vectorization, which this script needs explicitly.
#
# COMMON_OPT (not CFLAGS alone) carries the optimization level: OpenBLAS's
# own Makefile.system unconditionally appends `$(COMMON_OPT)` AFTER
# whatever CFLAGS is passed in (defaulting COMMON_OPT to -O2 if unset), so
# a bare `CFLAGS="-O1 ..."` override is silently overridden right back to
# -O2 by that append -- confirmed by tracing the actual compile command
# generated with CFLAGS alone. Setting COMMON_OPT here directly is what
# actually wins.
CC_WASI="${LLVM_BIN}/clang --target=wasm32-unknown-wasi --sysroot=${BASE_SYSROOT} -pthread -matomics -mbulk-memory"
OPENBLAS_MAKE_ARGS=(
  CC="${CC_WASI}" HOSTCC=cc FC=false
  AR="${LLVM_BIN}/llvm-ar" RANLIB="${LLVM_BIN}/llvm-ranlib" NM="${LLVM_BIN}/llvm-nm"
  TARGET=RISCV64_GENERIC BINARY=32 CROSS=1
  NOFORTRAN=1 NO_LAPACK=1 NO_LAPACKE=1 USE_THREAD=0 USE_OPENMP=0
  NO_SHARED=1 NEED_PIC=0 FIXED_LIBNAME=1
  BUILD_SINGLE=1 BUILD_DOUBLE=1 BUILD_COMPLEX=0 BUILD_COMPLEX16=0 BUILD_BFLOAT16=0
  CFLAGS="-g -fno-vectorize -fno-slp-vectorize -fno-unroll-loops" COMMON_OPT="-O1"
)

echo "[1/4] tracing the real library build (side-effect-free dry run)"
( cd "${OPENBLAS_SRC}" && make -n -B libs netlib "${OPENBLAS_MAKE_ARGS[@]}" ) > "${WORK}/trace.log" 2>&1

echo "[2/4] resolving real symbol names, marking the public-API subset"
"${LLVM_BIN}/llvm-nm" --defined-only -A "${OPENBLAS_SRC}/libopenblas.a" 2>/dev/null \
  | awk -F'[: ]+' '$4=="T"{print $2, $5}' > "${WORK}/archive_syms.txt"

PUBLIC_DRIVER_OTHERS='openblas_set_num_threads openblas_get_num_threads openblas_get_num_procs openblas_get_config openblas_get_parallel openblas_error_handle'

python3 - "${WORK}/trace.log" "${WORK}/archive_syms.txt" "${WORK}/jobs.tsv" "${WORK}/exports.txt" "${PUBLIC_DRIVER_OTHERS}" <<'PYEOF'
import re, sys

trace_path, syms_path, jobs_path, exports_path, public_others = sys.argv[1:6]
public_others = set(public_others.split())

# obj basename (e.g. "saxpy.o") -> [real defined T-symbols], ground truth from
# the already-built archive -- no guessing from -DNAME/-DCNAME macros needed.
obj_syms = {}
with open(syms_path) as f:
    for line in f:
        parts = line.split()
        if len(parts) != 2:
            continue
        obj, sym = parts
        obj_syms.setdefault(obj, []).append(sym)

cwd = None
jobs = []
exports = set()
n_public = 0
# Match ONLY the trailing "-o <obj>.o" -- some objects (e.g. smax.o/samax.o/
# ismax.o, all built from the SAME max.c with different -D/-U toggles) have
# extra -D/-U flags inserted BETWEEN "-c" and the source filename, so "-c"
# and the source are not always adjacent tokens. Whatever precedes " -o
# <obj>.o" (the compiler binary, all flags, "-c", the source file, in
# whatever order make emitted them) is kept as one opaque prefix and replayed
# verbatim with a different trailing "-o" -- no need to identify the source
# filename separately at all.
compile_re = re.compile(r'^(.*) -o (\S+\.o)\s*$')
enter_re = re.compile(r"Entering directory '([^']+)'")

with open(trace_path) as f:
    for line in f:
        line = line.rstrip('\n')
        m = enter_re.search(line)
        if m:
            cwd = m.group(1)
            continue
        if 'clang --target=wasm32' not in line or ' -c ' not in line:
            continue
        m = compile_re.match(line)
        if not m:
            continue
        prefix, obj = m.groups()
        in_interface = cwd is not None and cwd.rstrip('/').endswith('/interface')
        in_others = cwd is not None and cwd.rstrip('/').endswith('/driver/others')
        obj_base = obj.rsplit('.o', 1)[0]
        is_public = in_interface or (in_others and obj_base in public_others)
        syms = obj_syms.get(obj)
        if not syms:
            # Object never made it into the real archive (e.g. filtered out by
            # BUILD_COMPLEX=0/BUILD_COMPLEX16=0 despite make wanting to build
            # it) -- nothing to interpose (if public) or delegate into (if
            # internal) either way, skip.
            continue
        # Bitcode gets emitted for EVERY resolvable object, not just the
        # public-API ones: marshal-infer's one-hop interprocedural length
        # detection (see Infer.cpp's detectDelegatedLength) needs the BODY of
        # whatever internal kernel a public wrapper delegates its actual
        # array walk to (e.g. cblas_daxpy -> kernel/riscv64/axpy.c's
        # daxpy_k) resident in the same run -- without it, the one-hop
        # lookup can never find anything to follow into, regardless of how
        # correct the analysis itself is. --exports below still restricts
        # the JSON to just the public API; kernel/driver objects are
        # analyzed (so their bodies are available to follow into) but never
        # themselves emitted as top-level records.
        jobs.append((cwd, prefix, obj_base))
        if is_public:
            exports.update(syms)
            n_public += 1

with open(jobs_path, 'w') as f:
    for cwd, prefix, obj_base in jobs:
        f.write(f"{cwd}\t{prefix}\t{obj_base}\n")

with open(exports_path, 'w') as f:
    for s in sorted(exports):
        f.write(s + "\n")

print(f"      {n_public} public-API objects ({len(jobs)} total incl. internal, "
      f"for one-hop delegation lookups) -> {len(exports)} exported symbols",
      file=sys.stderr)
PYEOF

echo "[3/4] emit wasm32 bitcode (parallel x${JOBS})"
export BCDIR
emit_one() {
  local IFS=$'\t'
  read -r cwd prefix obj_base <<< "$1"
  local out="${BCDIR}/${obj_base}.bc"
  ( cd "${cwd}" && eval timeout 60 "${prefix}" -emit-llvm -g -o "${out}" ) 2>/dev/null \
    || { rm -f "${out}"; return 0; }
}
export -f emit_one
xargs -a "${WORK}/jobs.tsv" -d '\n' -P "${JOBS}" -I{} bash -c 'emit_one "$@"' _ {} || true
NBC=$(find "${BCDIR}" -name '*.bc' | wc -l)
echo "      ${NBC} TUs emitted bitcode"

if [[ -n "${LLM_PROMPT_FUNCTIONS:-}" ]]; then
  LLM_PROMPT_OUT="${LLM_PROMPT_OUTPUT:-${REPO_ROOT}/llm-prompts/openblas}"
  echo "[3.5/4] Stage 1 LLM-prompt preview -> ${LLM_PROMPT_OUT}"
  mkdir -p "${LLM_PROMPT_OUT}"
  IFS=',' read -r -a LLM_FN_ARRAY <<< "${LLM_PROMPT_FUNCTIONS}"
  # shellcheck disable=SC2046
  BC_FILES=($(find "${BCDIR}" -name '*.bc'))
  LLM_EXTRA_ARGS=()
  [[ -n "${LLM_MAX_FUNCTIONS:-}" ]] && LLM_EXTRA_ARGS+=(--llm-max-functions "${LLM_MAX_FUNCTIONS}")
  # A function named in LLM_PROMPT_FUNCTIONS was explicitly requested by the
  # caller -- failing to generate its prompt is a run failure, not a
  # warning to scroll past. Every requested function is still attempted
  # (not aborted at the first failure) so one bad name doesn't hide
  # diagnostics for the rest, but the script exits nonzero afterward.
  LLM_PROMPT_FAILED=0
  for fn in "${LLM_FN_ARRAY[@]}"; do
    if ! "${MI}" --llm-prompt-only --function "${fn}" --prompt-output "${LLM_PROMPT_OUT}" \
          "${BC_FILES[@]}" "${LLM_EXTRA_ARGS[@]}" 2>"${WORK}/llm_prompt_${fn}.err"; then
      echo "      ERROR: LLM prompt generation failed for '${fn}':" >&2
      cat "${WORK}/llm_prompt_${fn}.err" >&2
      LLM_PROMPT_FAILED=1
    fi
  done
  if [[ "${LLM_PROMPT_FAILED}" -eq 1 ]]; then
    echo "LLM prompt generation failed for one or more explicitly requested functions -- aborting." >&2
    exit 1
  fi
fi

echo "[4/4] infer + filter to exports -> ${OUT}"
# --config: the checked-in profile (see CONFIG.md) carries this library's
# coverage-threshold floor -- this is what would have caught issue #26's
# own regression (marshal count silently swinging from 86 to 18)
# automatically instead of requiring a human to notice. No contracts or
# relaxed-policy heuristics are needed here: the analysis-specific -O1
# compile above already recovers everything they used to cover, as
# unconditional exact proofs.
# shellcheck disable=SC2046
"${MI}" --json --module openblas --exports "${WORK}/exports.txt" \
  --config "${SCRIPT_DIR}/profiles/openblas.json" -o "${OUT}" \
  $(find "${BCDIR}" -name '*.bc')
echo "OK: ${OUT}"
