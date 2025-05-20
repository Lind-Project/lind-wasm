# ------------------------------------------------------------------------------
# File: setup_clang.sh
# Description: This script checks if Clang is installed in the target directory.
#              If not found, it downloads, extracts, and copies it.
#              Additionally, if the Lind-wasm repository is present, it moves
#              the 'wasi' folder into the Clang installation directory.
#

# Usage:
#   Run the script with:
#      chmod +x setup_clang.sh
#      ./setup_clang.sh
#
# Dependencies:
#   - wget: To download Clang
#   - tar: To extract the archive
#
# Exit Codes:
#   0 - Success
#   1 - Error (e.g., download failure, missing directories)
#
# ------------------------------------------------------------------------------


#!/bin/bash

# Define variables
HOME_DIR="/home/lind/lind-wasm"
CLANG_DIR="$HOME_DIR/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04"
CLANG_TAR="clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04.tar.xz"
CLANG_URL="https://github.com/llvm/llvm-project/releases/download/llvmorg-18.1.8/$CLANG_TAR"
LIND_WASM_DIR="$HOME_DIR"
WASI_SRC="$HOME_DIR/src/glibc/wasi"
WASI_DEST="$CLANG_DIR/lib/clang/18/lib"

# Check if Clang already exists
if [ ! -d "$CLANG_DIR" ]; then
    echo "Clang not found. Downloading and extracting..."
    wget "$CLANG_URL" && \
    tar -xf "$CLANG_TAR"    
else
    echo "Clang is already installed in $CLANG_DIR."
fi

# Check if Lind-wasm exists and copy wasi folder
if [ -d "$LIND_WASM_DIR" ] && [ -d "$WASI_SRC" ]; then
    echo "Lind-wasm found. Copying wasi folder..."
    cp -r "$WASI_SRC" "$WASI_DEST"
else
    echo "Either lind-wasm directory or wasi source directory does not exist."
fi
