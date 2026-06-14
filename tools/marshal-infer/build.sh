#!/usr/bin/env bash
# Configure + build marshal-infer against the repo's clang+llvm-18.1.8 tree.
# Usage: tools/marshal-infer/build.sh [clean]
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="${SCRIPT_DIR}/build"

# Prefer the repo-shipped llvm-config if present, else whatever is on PATH.
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
SHIPPED_LLVM="${REPO_ROOT}/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04/bin/llvm-config"
if [[ -x "${SHIPPED_LLVM}" ]]; then
  LLVM_CONFIG="${SHIPPED_LLVM}"
else
  LLVM_CONFIG="$(command -v llvm-config)"
fi

if [[ "${1:-}" == "clean" ]]; then
  rm -rf "${BUILD_DIR}"
fi

mkdir -p "${BUILD_DIR}"
cmake -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release \
  -DLLVM_CONFIG="${LLVM_CONFIG}" \
  -S "${SCRIPT_DIR}" -B "${BUILD_DIR}"
make -C "${BUILD_DIR}" -j"$(nproc)"
echo "OK: ${BUILD_DIR}/marshal-infer"
