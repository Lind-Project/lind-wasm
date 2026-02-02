#!/bin/bash
#
# Create static archive files for multiple glibc modules after the glibc build process creates object files
# IMPORTANT NOTES:
# - call from source code repository root directory
# - expects `clang` and other llvm binaries on $PATH
# - expects GLIBC source in $PWD/src/glibc
#
set -x

CC="clang"
HOME_DIR=$(dirname "${SCRIPT_DIR}")
GLIBC="$HOME_DIR/src/glibc"
BUILD="$GLIBC/build"
SYSROOT="$GLIBC/sysroot"
SYSROOT_ARCHIVE="$SYSROOT/lib/wasm32-wasi/libc.a"
LIB_PATH="$SYSROOT/lib/wasm32-wasi"


# 1. Get the directory where this script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

# Function that creates .a file from a list of objects read from arg1
create_archive() {
    local input_list_name="$1"    # e.g., "libc_objects.txt"
    local output_archive="$2"     # e.g., "libc.a" (full path or relative)

    local list_file_path="$SCRIPT_DIR/object_lists/$input_list_name"
    local response_file="$SCRIPT_DIR/object_lists_final/$input_list_name"

    echo "------------------------------------------------"
    echo "Processing: $input_list_name -> $output_archive"

    # Check if input list exists
    if [ ! -f "$list_file_path" ]; then
        echo "Error: List file not found at $list_file_path"
        return 1
    fi

    # Read the file into an array (skipping comments # and empty lines)
    mapfile -t WANTED_OBJECTS < <(grep -v '^#' "$list_file_path" | grep -v '^$')

    # clean previous response file
    rm -f "$response_file"

    local count=0
    for obj in "${WANTED_OBJECTS[@]}"; do
        FULL_PATH="$BUILD/$obj"
        
        if [ -f "$FULL_PATH" ]; then
            echo "$FULL_PATH" >> "$response_file"
            ((count++))            
        else
            echo "Warning: Object not found: $obj"
        fi
    done

    # Create archive using the response file (@ syntax)
    if [ -s "$response_file" ]; then
        echo "Archiving $count objects..."
        rm -f "$output_archive" # Remove old archive to ensure clean state
        llvm-ar rcs "$output_archive" @"$response_file"
        
        if [ $? -eq 0 ]; then
            echo "SUCCESS: Created $output_archive"
        else
            echo "FAILED: llvm-ar returned an error."
            return 1
        fi
    else
        echo "Error: No valid objects found to archive."
        return 1
    fi
}

rm -rf $SCRIPT_DIR/object_lists_final
mkdir $SCRIPT_DIR/object_lists_final
# Loop through all files in the object_lists directory
for file in "$SCRIPT_DIR/object_lists/"*; do
    # Skip if directory is empty or file doesn't exist
    [ -e "$file" ] || continue

    filename=$(basename "$file")

    # CASE 1: Ends with "_objects_shared.txt, then create $LIBNAME_pic.a"
    if [[ "$filename" == *"_objects_shared.txt" ]]; then
        # Extract LIBNAME (remove suffix)
        LIBNAME="${filename%_objects_shared.txt}"
        # Construct Output Path (add _pic suffix)
        OUTPUT_ARCHIVE="$LIB_PATH/${LIBNAME}_pic.a"

        # Call function
        create_archive "$filename" "$OUTPUT_ARCHIVE"

    # CASE 2: Ends with "_objects.txt, then create $LIBNAME.a"
    elif [[ "$filename" == *"_objects.txt" ]]; then
        # Extract LIBNAME (remove suffix)
        LIBNAME="${filename%_objects.txt}"
        # Construct Output Path (standard .a)
        OUTPUT_ARCHIVE="$LIB_PATH/${LIBNAME}.a"

        # Call function
        create_archive "$filename" "$OUTPUT_ARCHIVE"

    else
        echo "Skipping unrecognized file: $filename"
    fi
done

#libpthread.a is created as a placeholder since -pthread flag is used for lind_compile
llvm-ar crs "$GLIBC/sysroot/lib/wasm32-wasi/libpthread.a"

# Check if llvm-ar succeeded
if [ $? -eq 0 ]; then
  echo "SUCCESS: Created archive libpthread.a"
else
  echo "Failed to create the archive libpthread.a"
  return 1
fi
