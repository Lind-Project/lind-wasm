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

symbols=$($SCRIPTS_DIR/extract_glibc_symbols.sh $GLIBC $SCRIPTS_DIR/extract_versions.py --flags --paths-file $SCRIPTS_DIR/math-path.txt)

# fp128 (long double, on this port -- see sysdeps/lind/Implies) arithmetic needs
# compiler-rt soft-float helpers (__multf3, __divtf3, __lttf2, ...). wasm-ld is
# invoked directly here (not via the clang driver), so it never auto-links
# compiler-rt the way a normal clang link would -- link it explicitly (as a
# plain, non-whole archive, so only the object files libm_pic.a actually
# references get pulled in) so libm.so is self-sufficient for these rather than
# leaving them as unresolved `env` imports no module ends up providing.
CLANG_RT_BUILTINS="$($CC --target=wasm32-unknown-wasi -print-libgcc-file-name)"

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
    "$CLANG_RT_BUILTINS" \
    $symbols \
    --export=__tls_base \
    -o "$SYSROOT/lib/wasm32-wasi/libm.so" "$SYSROOT/lib/wasm32-wasi/lind_utils.o"

# append `__wasm_apply_tls_relocs`, `__wasm_apply_global_relocs` and `__stack_pointer` export
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libm.so" "$SYSROOT/lib/wasm32-wasi/libm.so" __wasm_apply_global_relocs func __wasm_apply_global_relocs
$REPO_ROOT/tools/add-export-tool/add-export-tool "$SYSROOT/lib/wasm32-wasi/libm.so" "$SYSROOT/lib/wasm32-wasi/libm.so" __stack_pointer global __stack_pointer

mkdir -p $REPO_ROOT/lindfs/lib

# apply wasm-opt
$REPO_ROOT/scripts/lind-wasm-opt --target=library $FPCAST_FLAG $SYSROOT/lib/wasm32-wasi/libm.so -o $REPO_ROOT/lindfs/lib/libm.so

# do precompile (call lind-boot directly to avoid lind_compile copying to lindfs root)
rm -f $REPO_ROOT/lindfs/lib/libm.cwasm
$REPO_ROOT/build/lind-boot --precompile $REPO_ROOT/lindfs/lib/libm.so
