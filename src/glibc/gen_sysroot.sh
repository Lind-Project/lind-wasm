#!/bin/bash

# Define the source directory for object files (change ./build to your desired path)
src_dir="./build"

# Define paths for copying additional resources
include_source_dir="$PWD/target/include"
crt1_source_path="$PWD/lind_syscall/crt1.o"
lind_syscall_path="$PWD/lind_syscall/lind_syscall.o" # Path to the lind_syscall.o file

# TARGET_TRIPLE = wasm32-wasi
TARGET_TRIPLE=wasm32-wasi-threads

# Define the output archive and sysroot directory
output_archive="sysroot/lib/wasm32-wasi/libc.a"
sysroot_dir="sysroot"

# First, remove the existing sysroot directory to start cleanly
rm -rf "$sysroot_dir"

# Find all .o files recursively in the source directory, ignoring stamp.o
object_files=$(find "$src_dir" -type f -name "*.o" ! \( -name "stamp.o" -o -name "argp-pvh.o" -o -name "repertoire.o" -o -name "static-stubs.o" \))

# Add the lind_syscall.o file to the list of object files
object_files="$object_files $lind_syscall_path"

# Check if object files were found
if [ -z "$object_files" ]; then
  echo "No suitable .o files found in '$src_dir'."
  exit 1
fi

# Create the sysroot directory structure
mkdir -p "$sysroot_dir/include/wasm32-wasi" "$sysroot_dir/lib/wasm32-wasi"

# Pack all found .o files into a single .a archive
${CLANG:=/home/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.0.4}/bin/llvm-ar rcs "$output_archive" $object_files
"$CLANG/bin/llvm-ar" crs "sysroot/lib/wasm32-wasi/libpthread.a"

# Check if llvm-ar succeeded
if [ $? -eq 0 ]; then
  echo "Successfully created $output_archive with the following .o files:"
  echo "$object_files"
else
  echo "Failed to create the archive."
  exit 1
fi

# Copy all files from the external include directory to the new sysroot include directory
cp -r "$include_source_dir"/* "$sysroot_dir/include/wasm32-wasi/"

# Copy the crt1.o file into the new sysroot lib directory
cp "$crt1_source_path" "$sysroot_dir/lib/wasm32-wasi/"
