#!/usr/bin/env bash
# Generate libz.marshal.json for zlib. Unlike glibc, zlib builds with the plain
# wasm32 sysroot, and its public export surface is the ZEXPORT decls in zlib.h.
#
# Usage: tools/marshal-infer/infer_zlib.sh [zlib-src-dir] [out.json]
#   defaults: src dir = /home/lind/lind-wasm-apps/zlib, out = <repo>/libz.marshal.json
set -Eeuo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
MI="${SCRIPT_DIR}/build/marshal-infer"
ZLIB="${1:-/home/lind/lind-wasm-apps/zlib}"
OUT="${2:-${REPO_ROOT}/libz.marshal.json}"
SR="${REPO_ROOT}/build/sysroot"; [[ -d "$SR" ]] || SR="${REPO_ROOT}/src/glibc/sysroot"

[[ -x "${MI}" ]] || { echo "build marshal-infer first: tools/marshal-infer/build.sh" >&2; exit 1; }
[[ -f "${ZLIB}/zlib.h" ]] || { echo "zlib.h not found in ${ZLIB}" >&2; exit 1; }

WORK="$(mktemp -d)"; trap 'rm -rf "${WORK}"' EXIT
BC="${WORK}/bc"; mkdir -p "${BC}"

# Public API = the names declared ZEXPORT in zlib.h.
grep -oE 'ZEXPORT[ ]+[A-Za-z_][A-Za-z0-9_]*' "${ZLIB}/zlib.h" \
  | awk '{print $2}' | sort -u > "${WORK}/exports.txt"
echo "[1/3] $(wc -l < "${WORK}/exports.txt") exported functions (zlib.h ZEXPORT)"

echo "[2/3] emit wasm32 bitcode for $(ls "${ZLIB}"/*.c | wc -l) TUs"
for f in "${ZLIB}"/*.c; do
  clang --target=wasm32-unknown-wasi --sysroot="${SR}" -g -O1 -emit-llvm -c \
    "$f" -o "${BC}/$(basename "${f%.c}").bc" -I"${ZLIB}" 2>/dev/null || echo "  skip $(basename "$f")"
done

echo "[3/3] infer + filter to exports -> ${OUT}"
# shellcheck disable=SC2046
"${MI}" --json --module libz --exports "${WORK}/exports.txt" -o "${OUT}" $(find "${BC}" -name '*.bc')
echo "OK: ${OUT}"
