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

# wasm_eh_c_longjmp_tag.o introduces a wasm Tag section into libc.so.  The
# prebuilt add-export-tool binary cannot handle binaries with a Tag section
# (it miscounts globals, causing "exported global index out of bounds").  The
# tag is only needed in the static libc.a as a fallback for programs that
# never call setjmp themselves; in the shared build every user compilation
# unit that uses setjmp will define __c_longjmp in its own object, so exclude
# it here.
SHARED_ARCHIVE=$(mktemp /tmp/libc_shared_XXXXXX.a)
cp "$SYSROOT_ARCHIVE" "$SHARED_ARCHIVE"
llvm-ar d "$SHARED_ARCHIVE" wasm_eh_c_longjmp_tag.o 2>/dev/null || true
trap "rm -f $SHARED_ARCHIVE" EXIT

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
    "$SHARED_ARCHIVE" \
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
    $([ -z "${LIND_ASYNCIFY_SETJMP:-}" ] && printf '%s\n' \
        --export-if-defined=saveSetjmp \
        --export-if-defined=testSetjmp \
        --export-if-defined=getTempRet0 \
        --export-if-defined=setTempRet0 \
        --export-if-defined=__wasm_longjmp) \
    -o "$SYSROOT/lib/wasm32-wasi/libc.so" "$SYSROOT/lib/wasm32-wasi/lind_utils.o"

mkdir -p $REPO_ROOT/lindfs/lib

# append `__wasm_apply_tls_relocs`, `__wasm_apply_global_relocs` and `__stack_pointer` export
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libc.so" $REPO_ROOT/lindfs/lib/libc.so __wasm_apply_tls_relocs func __wasm_apply_tls_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.so $REPO_ROOT/lindfs/lib/libc.so __wasm_apply_global_relocs func __wasm_apply_global_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool $REPO_ROOT/lindfs/lib/libc.so $REPO_ROOT/lindfs/lib/libc.so __stack_pointer global __stack_pointer

# apply wasm-opt
$REPO_ROOT/scripts/lind-wasm-opt --target=library $FPCAST_FLAG $REPO_ROOT/lindfs/lib/libc.so -o $REPO_ROOT/lindfs/lib/libc.so

# do precompile (call lind-boot directly to avoid lind_compile copying to lindfs root)
rm -f $REPO_ROOT/lindfs/lib/libc.cwasm
$REPO_ROOT/build/lind-boot --precompile $REPO_ROOT/lindfs/lib/libc.so
