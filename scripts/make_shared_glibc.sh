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

WITH_FPCAST=""
if [[ "$1" == "--with-fpcast" ]]; then
    WITH_FPCAST="--fpcast-emu --pass-arg=relocatable-fpcast"
fi

symbols=$($SCRIPTS_DIR/extract_glibc_symbols.sh $GLIBC $SCRIPTS_DIR/extract_versions.py --flags --paths-file $SCRIPTS_DIR/version-path-minimal.txt)

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
    $SYSROOT_ARCHIVE \
    --no-whole-archive \
    $symbols \
    --export-if-defined=__libc_setup_tls \
    --export-if-defined=__wasi_init_tp \
    --export-if-defined=__ctype_init \
    --export-if-defined=__lind_init_addr_translation \
    --export-if-defined=__wasm_init_tls \
    --export-if-defined=environ \
    --export=__tls_base \
    --export-if-defined=copy_data_between_cages \
    --export-if-defined=copy_handler_table_to_cage \
    --export-if-defined=make_threei_call \
    --export-if-defined=register_handler \
    --export-if-defined=lind_debug_printf \
    --export-if-defined=__lind_debug_import \
    --export-if-defined=lind_debug_panic \
    -o "$SYSROOT/lib/wasm32-wasi/libc.so" "$SYSROOT/lib/wasm32-wasi/lind_utils.o"

mkdir -p $REPO_ROOT/lindfs/lib

# append `__wasm_apply_tls_relocs`, `__wasm_apply_global_relocs` and `__stack_pointer` export
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libc.so" $REPO_ROOT/lindfs/lib/libc.wasm __wasm_apply_tls_relocs func __wasm_apply_tls_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.wasm $REPO_ROOT/lindfs/lib/libc.wasm __wasm_apply_global_relocs func __wasm_apply_global_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.wasm $REPO_ROOT/lindfs/lib/libc.wasm __stack_pointer global __stack_pointer

# apply wasm-opt
$REPO_ROOT/tools/binaryen/bin/wasm-opt --enable-bulk-memory --enable-threads --epoch-injection --pass-arg=epoch-import --asyncify --pass-arg=asyncify-import-globals $WITH_FPCAST -O2 --debuginfo $REPO_ROOT/lindfs/lib/libc.wasm -o $REPO_ROOT/lindfs/lib/libc.wasm

# do precompile
rm -f $REPO_ROOT/lindfs/lib/libc.cwasm
$REPO_ROOT/scripts/lind_compile --precompile-only $REPO_ROOT/lindfs/lib/libc.wasm
