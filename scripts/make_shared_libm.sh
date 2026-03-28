#!/bin/bash
#
# Build glibc and generate a sysroot for clang to cross-compile lind programs
#
# IMPORTANT NOTES:
# - call from source code repository root directory
# - expects `clang` and other llvm binaries on $PATH
# - expects GLIBC source in $PWD/src/glibc
#
set -e

CC="clang"
REPO_ROOT="$PWD"
SCRIPTS_DIR="$REPO_ROOT/scripts"
GLIBC="$PWD/src/glibc"
BUILD="$GLIBC/build"
SYSROOT="$GLIBC/sysroot"
SYSROOT_ARCHIVE="$SYSROOT/lib/wasm32-wasi/libc.a"

symbols=$($SCRIPTS_DIR/extract_glibc_symbols.sh $GLIBC $SCRIPTS_DIR/extract_versions.py --flags --paths-file $SCRIPTS_DIR/math-path.txt)

# --import-memory, --shared-memory: to make memory shared across wasm module
# --export-dynamic, --experimental-pic, --unresolved-symbols=import-dynamic, -shared: flags for dynamic build of libraries
# --export-if-defined: manually export the symbol if found. symbol in glibc has hidden visibility by default, we have to manually export it
wasm-ld \
    --import-memory \
    --shared-memory \
    --export-dynamic \
    --experimental-pic \
    --unresolved-symbols=import-dynamic \
    -shared \
    --whole-archive \
    "$SYSROOT/lib/wasm32-wasi/libm_pic.a" \
    --no-whole-archive \
    $symbols \
    --export=__tls_base \
    -o "$SYSROOT/lib/wasm32-wasi/libm.so" $REPO_ROOT/src/glibc/build/csu/errno.o "$SYSROOT/lib/wasm32-wasi/lind_utils.o"

# append `__wasm_apply_tls_relocs`, `__wasm_apply_global_relocs` and `__stack_pointer` export
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libm.so" "$SYSROOT/lib/wasm32-wasi/libm.so" __wasm_apply_global_relocs func __wasm_apply_global_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libm.so" "$SYSROOT/lib/wasm32-wasi/libm.so" __stack_pointer global __stack_pointer

mkdir -p $REPO_ROOT/lindfs/lib

# apply wasm-opt
$REPO_ROOT/tools/binaryen/bin/wasm-opt --enable-bulk-memory --enable-threads --epoch-injection --pass-arg=epoch-import --asyncify --pass-arg=asyncify-import-globals -O2 --debuginfo $SYSROOT/lib/wasm32-wasi/libm.so -o $REPO_ROOT/lindfs/lib/libm.wasm

# do precompile
rm -f $REPO_ROOT/lindfs/lib/libm.cwasm
$REPO_ROOT/scripts/lind_compile --precompile-only $REPO_ROOT/lindfs/lib/libm.wasm
