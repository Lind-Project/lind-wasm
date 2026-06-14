#!/usr/bin/env bash
# Reproduce the full marshal-infer pipeline on demo.c, emitting every
# intermediate artifact. See README.md for the stage-by-stage explanation.
set -Eeuo pipefail

HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${HERE}/../../.." && pwd)"
MI="${REPO_ROOT}/tools/marshal-infer/build/marshal-infer"
SYSROOT="${REPO_ROOT}/build/sysroot"
[[ -d "${SYSROOT}" ]] || SYSROOT="${REPO_ROOT}/src/glibc/sysroot"
[[ -x "${MI}" ]] || { echo "build the tool first: tools/marshal-infer/build.sh" >&2; exit 1; }

cd "${HERE}"

echo "[stage 1] C -> wasm32 LLVM bitcode (+DWARF), the inference input"
#   exactly what lind_compile --emit-llvm does (clang at -O1, -g for DWARF).
clang --target=wasm32-unknown-wasi --sysroot="${SYSROOT}" -g -O1 \
      -emit-llvm -c demo.c -o demo.bc
#   also emit human-readable IR so you can read it.
llvm-dis demo.bc -o demo.ll
echo "         -> demo.bc, demo.ll"

echo "[stage 2] marshal-infer: human-readable inference tree (intermediate view)"
"${MI}" demo.bc > demo.tree.txt
echo "         -> demo.tree.txt"

echo "[stage 3] marshal-infer: the JSON sidecar (final delivery)"
"${MI}" --json --module demo -o demo.marshal.json demo.bc
echo "         -> demo.marshal.json"

echo "done. read README.md alongside these files."
