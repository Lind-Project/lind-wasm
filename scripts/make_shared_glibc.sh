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

FPCAST_FLAG=""
if [[ "$1" == "--with-fpcast" ]]; then
    FPCAST_FLAG="--fpcast-emu"
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
    -o "$SYSROOT/lib/wasm32-wasi/libc.so" "$SYSROOT/lib/wasm32-wasi/lind_utils.o"

mkdir -p $REPO_ROOT/lindfs/lib

# append `__wasm_apply_tls_relocs`, `__wasm_apply_global_relocs` and `__stack_pointer` export
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libc.so" $REPO_ROOT/lindfs/lib/libc.so __wasm_apply_tls_relocs func __wasm_apply_tls_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.so $REPO_ROOT/lindfs/lib/libc.so __wasm_apply_global_relocs func __wasm_apply_global_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.so $REPO_ROOT/lindfs/lib/libc.so __stack_pointer global __stack_pointer

# apply wasm-opt
$REPO_ROOT/scripts/lind-wasm-opt --target=library $FPCAST_FLAG $REPO_ROOT/lindfs/lib/libc.so -o $REPO_ROOT/lindfs/lib/libc.opt.wasm

# do precompile (call lind-boot directly to avoid lind_compile copying to lindfs root)
rm -f $REPO_ROOT/lindfs/lib/libc.cwasm
$REPO_ROOT/build/lind-boot --precompile $REPO_ROOT/lindfs/lib/libc.opt.wasm
mv $REPO_ROOT/lindfs/lib/libc.opt.wasm $REPO_ROOT/lindfs/lib/libc.so
