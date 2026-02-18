#!/bin/bash
# Copy glibc target/include and target/lib into sysroot (issue #245).
# Run after make_glibc_and_sysroot.sh has populated target/ (or when target/ exists from install).
set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GLIBC="${ROOT_DIR}/src/glibc"
SYSROOT="${GLIBC}/sysroot"

[ -d "${GLIBC}/target/include" ] || { echo "Missing ${GLIBC}/target/include. Run make_glibc_and_sysroot.sh first or copy glibc headers."; exit 1; }

mkdir -p "${SYSROOT}/include/wasm32-wasi" "${SYSROOT}/lib/wasm32-wasi"
cp -r "${GLIBC}/target/include/"* "${SYSROOT}/include/wasm32-wasi/"
[ -d "${GLIBC}/target/lib" ] && cp -n "${GLIBC}/target/lib/"*.a "${SYSROOT}/lib/wasm32-wasi/" 2>/dev/null || true
[ -f "${GLIBC}/lind_syscall/crt1.o" ] && cp -n "${GLIBC}/lind_syscall/crt1.o" "${SYSROOT}/lib/wasm32-wasi/" 2>/dev/null || true
[ -f "${GLIBC}/lind_syscall/lind_syscall.h" ] && cp -n "${GLIBC}/lind_syscall/lind_syscall.h" "${SYSROOT}/include/wasm32-wasi/" 2>/dev/null || true
echo "Sysroot prepared from glibc target."
ls -la "${SYSROOT}/lib/wasm32-wasi/" 2>/dev/null || true
