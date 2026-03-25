#!/bin/bash
set -e

# --- Paths ---
export HOME_DIR="/home/lind/lind-wasm"
export LLVM_SRC="$HOME_DIR/llvm-project"
export INSTALL_DIR="$HOME_DIR/compiler-rt-install"
export BUILD_DIR="$HOME_DIR/compiler-rt-build"
export TOOLCHAIN_FILE="$HOME_DIR/Toolchain-WASI.cmake"

# --- Clean build ---
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"

# --- Configure ---
cmake -G Ninja "$LLVM_SRC/compiler-rt" \
  -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN_FILE" \
  -DCMAKE_BUILD_TYPE=Release \
  -DCMAKE_INSTALL_PREFIX="$INSTALL_DIR" \
  -DCOMPILER_RT_BUILD_BUILTINS=ON \
  -DCOMPILER_RT_DEFAULT_TARGET_ONLY=ON \
  -DCOMPILER_RT_INCLUDE_TESTS=OFF \
  -DCOMPILER_RT_BUILD_SANITIZERS=OFF \
  -DCOMPILER_RT_BUILD_XRAY=OFF \
  -DCOMPILER_RT_BUILD_LIBFUZZER=OFF \
  -DCOMPILER_RT_BUILD_PROFILE=OFF \
  -DCOMPILER_RT_BUILD_MEMPROF=OFF \
  -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY

# --- Build and install ---
ninja
ninja install
