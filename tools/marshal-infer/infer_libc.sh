#!/usr/bin/env bash
# Generate libc.marshal.json: argument-marshalling inference for the functions
# exported by the built libc (libc.cwasm / libc.so). Best-effort, library-side.
#
# Pipeline:
#   1. export list  = defined functions in lindfs/lib/libc.so
#   2. TU list      = glibc sources that were compiled (from build/*.o[.dt])
#   3. emit wasm32 bitcode (+DWARF) per TU, in parallel (skip TUs that don't build)
#   4. marshal-infer over all bitcode, filtered to the export list -> JSON
#
# Usage: tools/marshal-infer/infer_libc.sh [out.json]
# Output default: <repo>/libc.marshal.json
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
GLIBC="${REPO_ROOT}/src/glibc"
BUILD="${GLIBC}/build"
MI="${SCRIPT_DIR}/build/marshal-infer"
LIBC_SO="${REPO_ROOT}/lindfs/lib/libc.so"
OUT="${1:-${REPO_ROOT}/libc.marshal.json}"
JOBS="$(nproc)"

[[ -x "${MI}" ]] || { echo "build marshal-infer first: tools/marshal-infer/build.sh" >&2; exit 1; }
[[ -d "${BUILD}" ]] || { echo "glibc build dir missing: ${BUILD}" >&2; exit 1; }
[[ -f "${LIBC_SO}" ]] || { echo "libc.so missing: ${LIBC_SO}" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT
BCDIR="${WORK}/bc"; mkdir -p "${BCDIR}"

echo "[1/4] export list from libc.so"
llvm-nm "${LIBC_SO}" 2>/dev/null | awk '$2=="T"{print $3}' | sort -u > "${WORK}/exports.txt"
echo "      $(wc -l < "${WORK}/exports.txt") exported functions"

echo "[2/4] enumerate compiled translation units"
# (a) exact sources from dependency files (handles sysdeps overrides)
find "${BUILD}" -name '*.o.dt' -print0 2>/dev/null | while IFS= read -r -d '' dt; do
  sed -e ':a;N;$!ba;s/\\\n//g' "$dt" 2>/dev/null | sed 's/^[^:]*://' \
    | tr ' \t' '\n\n' | grep -m1 '\.c$' || true
done > "${WORK}/srcs.raw"
# (b) path-map remaining objects: build/<rel>.o -> src/glibc/<rel>.c
find "${BUILD}" -name '*.o' -printf '%P\n' 2>/dev/null | sed 's/\.o$/.c/' \
  | while read -r rel; do [[ -f "${GLIBC}/${rel}" ]] && echo "${GLIBC}/${rel}" || true; done \
  >> "${WORK}/srcs.raw" || true
sort -u "${WORK}/srcs.raw" | grep -E '\.c$' \
  | while read -r s; do [[ -f "$s" ]] && echo "$s" || true; done \
  > "${WORK}/srcs.txt" || true
echo "      $(wc -l < "${WORK}/srcs.txt") unique source TUs"

echo "[3/4] emit wasm32 bitcode (parallel x${JOBS}; failures skipped)"
RESOURCE_DIR="$(clang --target=wasm32-unknown-wasi -print-resource-dir)"
export GLIBC BUILD BCDIR RESOURCE_DIR
emit_one() {
  local src="$1"
  local name; name="$(echo "${src#"${GLIBC}/"}" | tr '/' '_')"
  local out="${BCDIR}/${name}.bc"
  local FLAGS=(--target=wasm32-unknown-wasi -Wno-int-conversion -DNO_HIDDEN -std=gnu11
    -fgnu89-inline -matomics -mbulk-memory -O1 -g -fPIC -fno-stack-protector
    -fno-common -Wp,-U_FORTIFY_SOURCE -fmath-errno -ftls-model=local-exec)
  local INC=(-I../include -I"${BUILD}/nptl" -I"${BUILD}" -I../sysdeps/lind -I../lind_syscall
    -I../sysdeps/unix/sysv/linux/i386/i686 -I../sysdeps/unix/sysv/linux/i386
    -I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86
    -I../sysdeps/x86/nptl -I../sysdeps/i386/nptl -I../sysdeps/unix/sysv/linux/include
    -I../sysdeps/unix/sysv/linux -I../sysdeps/nptl -I../sysdeps/pthread -I../sysdeps/gnu
    -I../sysdeps/unix/inet -I../sysdeps/unix/sysv -I../sysdeps/unix/i386 -I../sysdeps/unix
    -I../sysdeps/posix -I../sysdeps/i386/fpu -I../sysdeps/x86/fpu -I../sysdeps/i386
    -I../sysdeps/x86/include -I../sysdeps/x86 -I../sysdeps/wordsize-32
    -I../sysdeps/ieee754/float128 -I../sysdeps/ieee754/ldbl-96/include
    -I../sysdeps/ieee754/ldbl-96 -I../sysdeps/ieee754/dbl-64 -I../sysdeps/ieee754/flt-32
    -I../sysdeps/ieee754 -I../sysdeps/generic -I.. -I../libio -I.)
  local SYSINC=(-nostdinc -isystem "${RESOURCE_DIR}/include" -isystem /usr/i686-linux-gnu/include)
  local DEF=(-D_LIBC_REENTRANT -include "${BUILD}/libc-modules.h" -DMODULE_NAME=libc
    -include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc)
  ( cd "${BUILD}" && timeout 60 clang "${FLAGS[@]}" "${INC[@]}" "${SYSINC[@]}" "${DEF[@]}" \
      -emit-llvm -c "$src" -o "$out" ) 2>/dev/null || { rm -f "$out"; return 0; }
}
export -f emit_one
xargs -a "${WORK}/srcs.txt" -P "${JOBS}" -I{} bash -c 'emit_one "$@"' _ {} || true
NBC=$(find "${BCDIR}" -name '*.bc' | wc -l)
echo "      ${NBC} TUs emitted bitcode"

echo "[4/4] infer + filter to exports -> ${OUT}"
# Single invocation (flattened .bc names have no spaces) so -o isn't overwritten
# by an xargs split.
# shellcheck disable=SC2046
"${MI}" --json --module libc --exports "${WORK}/exports.txt" -o "${OUT}" \
  $(find "${BCDIR}" -name '*.bc')
echo "OK: ${OUT}"
