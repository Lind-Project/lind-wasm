#!/bin/bash
# Build libc++ and libc++abi for wasm32-unknown-wasi against Lind sysroot (issue #245).
# Requires: llvm-project at repo root (e.g. git clone --branch llvmorg-16.0.4 https://github.com/llvm/llvm-project.git)
# Apply libc++ patches (filesystem_common.h, xlocale.h) before running.
set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LLVM_SRC="${ROOT_DIR}/llvm-project"
INSTALL_PREFIX="${ROOT_DIR}/libcxx-wasi-install"
SYSROOT="${ROOT_DIR}/src/glibc/sysroot"
TOOLCHAIN="${ROOT_DIR}/Toolchain-WASI.cmake"

# Prefer versioned clang if present
export CLANG_BIN="${CLANG_BIN:-/usr/bin}"
for v in 16 15 14; do
  if [ -x "/usr/lib/llvm-${v}/bin/clang" ]; then
    CLANG_BIN="/usr/lib/llvm-${v}/bin"
    break
  fi
done
export CMAKE_SYSROOT="${SYSROOT}"

[ -d "${LLVM_SRC}/runtimes" ] || { echo "Missing ${LLVM_SRC}/runtimes. Clone llvm-project (e.g. branch llvmorg-16.0.4)."; exit 1; }
mkdir -p "${ROOT_DIR}/libcxx-build"
cd "${ROOT_DIR}"

cmake -B libcxx-build -S "${LLVM_SRC}/runtimes" \
  -DCMAKE_TOOLCHAIN_FILE="${TOOLCHAIN}" \
  -DCLANG_BIN="${CLANG_BIN}" \
  -DLLVM_PATH="${LLVM_SRC}/llvm" \
  -DLLVM_ENABLE_RUNTIMES="libcxx;libcxxabi" \
  -DLLVM_TARGETS_TO_BUILD="X86" \
  -DLLVM_DEFAULT_TARGET_TRIPLE="x86_64-unknown-linux-gnu" \
  -DLLVM_HOST_TRIPLE="wasm32-unknown-wasi" \
  -DLIBCXX_ENABLE_SHARED=OFF \
  -DLIBCXX_ENABLE_STATIC=ON \
  -DLIBCXX_ENABLE_EXCEPTIONS=OFF \
  -DLIBCXX_USE_COMPILER_RT=ON \
  -DLIBCXX_ENABLE_RTTI=ON \
  -DLIBCXXABI_ENABLE_SHARED=OFF \
  -DLIBCXXABI_ENABLE_STATIC=ON \
  -DLIBCXXABI_ENABLE_EXCEPTIONS=OFF \
  -DLIBCXXABI_USE_LLVM_UNWINDER=OFF \
  -DLIBCXXABI_ENABLE_STATIC_UNWINDER=OFF \
  -DLIBCXX_ENABLE_UNWIND_TABLES=OFF \
  -DLIBCXXABI_ENABLE_UNWIND_TABLES=OFF \
  -DLIBCXXABI_USE_COMPILER_RT=ON \
  -DLIBCXXABI_ENABLE_RTTI=ON \
  -DLIBCXXABI_LIBCXX_PATH="${LLVM_SRC}/libcxx" \
  -DLIBCXX_HAS_MUSL_LIBC=OFF \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="${INSTALL_PREFIX}" \
  -DCMAKE_CXX_COMPILER_WORKS=1 \
  -DCMAKE_C_COMPILER_WORKS=1

cmake --build libcxx-build --target install

# Copy to Lind sysroot (issue #245): C++ under include/c++/v1 for --sysroot to find
mkdir -p "${SYSROOT}/include/c++" "${SYSROOT}/lib/wasm32-wasi"
cp -r "${INSTALL_PREFIX}/include/c++"/v1 "${SYSROOT}/include/c++/"
cp "${INSTALL_PREFIX}/lib/libc++.a" "${INSTALL_PREFIX}/lib/libc++abi.a" "${SYSROOT}/lib/wasm32-wasi/" 2>/dev/null || true

# Install fix_std_maxmin shim into sysroot (issue #245)
mkdir -p "${SYSROOT}/include/c++/v1/__algorithm"
cp "${ROOT_DIR}/scripts/shim-headers/__algorithm/fix_std_maxmin.h" "${SYSROOT}/include/c++/v1/__algorithm/"

echo "Libc++ installed to ${SYSROOT}; fix_std_maxmin.h installed."
