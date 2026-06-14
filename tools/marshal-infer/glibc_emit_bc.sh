#!/usr/bin/env bash
# Emit wasm32 LLVM bitcode (+DWARF) for a single glibc translation unit, using
# the same flags scripts/make_glibc_and_sysroot.sh compiles glibc with. This is
# the scaling primitive for inferring marshal specs over libc: the build keeps no
# .bc, so we re-emit a TU's bitcode on demand for marshal-infer to consume.
#
# Usage:
#   tools/marshal-infer/glibc_emit_bc.sh <tu.c relative to src/glibc> [out.bc]
# Example:
#   tools/marshal-infer/glibc_emit_bc.sh string/strncpy.c /tmp/strncpy.bc
#
# Requires the glibc build dir (src/glibc/build) to exist with generated headers
# (libc-modules.h etc.) — i.e. glibc must have been built at least once.
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
GLIBC="${REPO_ROOT}/src/glibc"
BUILD="${GLIBC}/build"

[[ $# -ge 1 ]] || { echo "usage: $0 <tu.c relative to src/glibc> [out.bc]" >&2; exit 2; }
TU="$1"
SRC="${GLIBC}/${TU}"
[[ -f "${SRC}" ]] || { echo "error: TU not found: ${SRC}" >&2; exit 2; }
[[ -d "${BUILD}" ]] || { echo "error: glibc build dir missing (${BUILD}); build glibc first" >&2; exit 2; }
OUT="${2:-${SRC%.c}.bc}"

RESOURCE_DIR="$(clang --target=wasm32-unknown-wasi -print-resource-dir)"

# Mirrors make_glibc_and_sysroot.sh; -O1 (not -O2) to keep SSA readable for the
# def-use analyses, like KSplit analyzes at low opt.
FLAGS=(--target=wasm32-unknown-wasi -Wno-int-conversion -DNO_HIDDEN -std=gnu11
  -fgnu89-inline -matomics -mbulk-memory -O1 -g -fPIC -fno-stack-protector
  -fno-common -Wp,-U_FORTIFY_SOURCE -fmath-errno -ftls-model=local-exec)

# Include search path is relative to the build dir (the build cd's into it).
INC=(-I../include -I"${BUILD}/nptl" -I"${BUILD}" -I../sysdeps/lind -I../lind_syscall
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
SYSINC=(-nostdinc -isystem "${RESOURCE_DIR}/include" -isystem /usr/i686-linux-gnu/include)
DEF=(-D_LIBC_REENTRANT -include "${BUILD}/libc-modules.h" -DMODULE_NAME=libc
  -include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc)

# Compile from the build dir so the relative -I paths resolve as in the real build.
( cd "${BUILD}" && clang "${FLAGS[@]}" "${INC[@]}" "${SYSINC[@]}" "${DEF[@]}" \
    -emit-llvm -c "${SRC}" -o "${OUT}" )
echo "OK: ${OUT}"
