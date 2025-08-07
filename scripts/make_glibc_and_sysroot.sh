#!/bin/bash
#
# Build glibc and generate a sysroot for clang to cross-compile lind programs
#
# IMPORTANT NOTES:
# - call from source code repository root directory
# - expects `clang` and other llvm binaries on $PATH
# - expects GLIBC source in $PWD/src/glibc
#
set -x

CC="clang"
GLIBC="$PWD/src/glibc"
BUILD="$GLIBC/build"
SYSROOT="$GLIBC/sysroot"
SYSROOT_ARCHIVE="$SYSROOT/lib/wasm32-wasi/libc.a"

# Define common flags
CFLAGS="--target=wasm32-unknown-wasi -v -Wno-int-conversion -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g -fPIC"
WARNINGS="-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
EXTRA_FLAGS="-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common"
EXTRA_FLAGS+=" -Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec"
INCLUDE_PATHS="
    -I../include
    -I$BUILD/nptl
    -I$BUILD
    -I../sysdeps/lind
    -I../lind_syscall
    -I../sysdeps/unix/sysv/linux/i386/i686
    -I../sysdeps/unix/sysv/linux/i386
    -I../sysdeps/unix/sysv/linux/x86/include
    -I../sysdeps/unix/sysv/linux/x86
    -I../sysdeps/x86/nptl
    -I../sysdeps/i386/nptl
    -I../sysdeps/unix/sysv/linux/include
    -I../sysdeps/unix/sysv/linux
    -I../sysdeps/nptl
    -I../sysdeps/pthread
    -I../sysdeps/gnu
    -I../sysdeps/unix/inet
    -I../sysdeps/unix/sysv
    -I../sysdeps/unix/i386
    -I../sysdeps/unix
    -I../sysdeps/posix
    -I../sysdeps/i386/fpu
    -I../sysdeps/x86/fpu
    -I../sysdeps/i386
    -I../sysdeps/x86/include
    -I../sysdeps/x86
    -I../sysdeps/wordsize-32
    -I../sysdeps/ieee754/float128
    -I../sysdeps/ieee754/ldbl-96/include
    -I../sysdeps/ieee754/ldbl-96
    -I../sysdeps/ieee754/dbl-64
    -I../sysdeps/ieee754/flt-32
    -I../sysdeps/ieee754
    -I../sysdeps/generic
    -I..
    -I../libio
    -I.
"
SYS_INCLUDE="-nostdinc -isystem $CLANG/lib/clang/18/include -isystem /usr/i686-linux-gnu/include"
DEFINES="-D_LIBC_REENTRANT -include $BUILD/libc-modules.h -DMODULE_NAME=libc"
EXTRA_DEFINES="-include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"


# Build glibc
rm -rf $BUILD
mkdir -p $BUILD
cd $BUILD

../configure \
  --disable-werror \
  --disable-hidden-plt \
  --disable-profile \
  --with-headers=/usr/i686-linux-gnu/include \
  --prefix=$GLIBC/target \
  --host=i686-linux-gnu \
  --build=i686-linux-gnu \
  CFLAGS=" -matomics -mbulk-memory -O2 -g" \
  CC="clang --target=wasm32-unknown-wasi -v -Wno-int-conversion"

make -j$(($(nproc) * 2)) --keep-going 2>&1 THREAD_MODEL=posix | tee check.log

# Build extra
cd ../nptl
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/nptl/pthread_create.o \
    -c pthread_create.c -MD -MP -MF $BUILD/nptl/pthread_create.o.dt \
    -MT $BUILD/nptl/pthread_create.o

$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/lind_syscall.o \
    -c $GLIBC/lind_syscall/lind_syscall.c

# Compile elision-lock.c
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $GLIBC/build/nptl/elision-lock.o \
    -c $GLIBC/sysdeps/unix/sysv/linux/x86/elision-lock.c \
    -MD -MP -MF $GLIBC/build/nptl/elision-lock.o.dt \
    -MT $GLIBC/build/nptl/elision-lock.o

# Compile elision-unlock.c
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $GLIBC/build/nptl/elision-unlock.o \
    -c $GLIBC/sysdeps/unix/sysv/linux/x86/elision-unlock.c \
    -MD -MP -MF $GLIBC/build/nptl/elision-unlock.o.dt \
    -MT $GLIBC/build/nptl/elision-unlock.o

# Compile assembly files
cd ../
$CC --target=wasm32-wasi-threads -matomics \
    -o $BUILD/csu/wasi_thread_start.o \
    -c $GLIBC/csu/wasm32/wasi_thread_start.s

$CC --target=wasm32-wasi-threads -matomics \
    -o $BUILD/csu/set_stack_pointer.o \
    -c $GLIBC/csu/wasm32/set_stack_pointer.s

# Generate sysroot
# First, remove the existing sysroot directory to start cleanly
rm -rf "$SYSROOT"

# Find all .o files recursively in the source directory, ignoring stamp.o
object_files=$(find "$BUILD" -type f -name "*.o" ! \( \
  -name "stamp.o" -o \
  -name "argp-pvh.o" -o \
  -name "repertoire.o" -o \
  -name "static-stubs.o" \
  -name "zic.o" -o \
  -name "xmalloc.o" -o \
  -name "list.o" -o \
  -name "ldconfig.o" -o \
  -name "sln.o" \
\))


# Check if object files were found
if [ -z "$object_files" ]; then
  echo "No suitable .o files found in '$BUILD'."
  exit 1
fi

# Create the sysroot directory structure
mkdir -p "$SYSROOT/include/wasm32-wasi" "$SYSROOT/lib/wasm32-wasi"

# Pack all found .o files into a single .a archive
llvm-ar rcs "$SYSROOT_ARCHIVE" $object_files
llvm-ar crs "$GLIBC/sysroot/lib/wasm32-wasi/libpthread.a"

# Check if llvm-ar succeeded
if [ $? -eq 0 ]; then
  echo "Successfully created $SYSROOT_ARCHIVE with the following .o files:"
  echo "$object_files"
else
  echo "Failed to create the archive."
  exit 1
fi

# Copy all files from the external include directory to the new sysroot include directory
cp -r "$GLIBC/target/include/"* "$SYSROOT/include/wasm32-wasi/"

# Copy the crt1.o file into the new sysroot lib directory
cp "$GLIBC/lind_syscall/crt1.o" "$SYSROOT/lib/wasm32-wasi/"
