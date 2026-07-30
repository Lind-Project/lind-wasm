#!/bin/bash
#
# Build glibc and generate a sysroot for clang to cross-compile grateos programs
#
# IMPORTANT NOTES:
# - call from source code repository root directory
# - expects `clang` and other llvm binaries on $PATH
# - expects GLIBC source in $PWD/src/glibc
#

set -e

shared_script_args=()
if [[ "$1" == "--with-fpcast" ]]; then
    shared_script_args+=(--with-fpcast)
fi

CC="clang"
GLIBC="$PWD/src/glibc"
BUILD="$GLIBC/build"
SYSROOT="$GLIBC/sysroot"
SYSROOT_ARCHIVE="$SYSROOT/lib/wasm32-wasi/libc.a"
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

# Define common flags
CFLAGS="--target=wasm32-unknown-wasi -v -Wno-int-conversion  -DNO_HIDDEN -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g -fPIC"
WARNINGS="-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
EXTRA_FLAGS="-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common"
EXTRA_FLAGS+=" -Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec"
INCLUDE_PATHS="
    -I../include
    -I$BUILD/nptl
    -I$BUILD
    -I../sysdeps/grateos
    -I../grateos_syscall
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


RESOURCE_DIR="$(clang --target=wasm32-unknown-wasi -print-resource-dir)"
SYS_INCLUDE="-nostdinc -isystem ${RESOURCE_DIR}/include -isystem /usr/i686-linux-gnu/include"

#SYS_INCLUDE="-nostdinc -isystem $CLANG/lib/clang/18/include -isystem /usr/i686-linux-gnu/include"
DEFINES="-D_LIBC_REENTRANT -include $BUILD/libc-modules.h -DMODULE_NAME=libc"
EXTRA_DEFINES="-include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"

# Check if GRATEOS_DEBUG is defined (set by build.rs when `grateos_debug` is enabled)
if [ "$GRATEOS_DEBUG" ]; then
  DEFINES="$DEFINES -DGRATEOS_DEBUG"
fi

# Build glibc
rm -rf $BUILD
mkdir -p $BUILD
cd $BUILD

# In EH-based setjmp mode (default), compile glibc with -DGRATEOS_EH_SETJMP so
# that __longjmp (used by setjmp/longjmp) uses __wasm_longjmp (EH-based) and
# __libc_siglongjmp (used by sigsetjmp/siglongjmp) uses the asyncify
# grateos.grateos-longjmp import — signal handlers are invoked through a Rust host
# boundary where EH exceptions cannot propagate back, so asyncify is required
# for sigsetjmp/siglongjmp.  Skipped when GRATEOS_ASYNCIFY_SETJMP is set.
GLIBC_SETJMP_CFLAGS=""
if [[ -z "${GRATEOS_ASYNCIFY_SETJMP:-}" ]]; then
    GLIBC_SETJMP_CFLAGS="-DGRATEOS_EH_SETJMP"
fi

# do configure, we enable fPIC by default for dynamic build
../configure \
  --disable-werror \
  --disable-hidden-plt \
  --disable-profile \
  --disable-nscd \
  --with-headers=/usr/i686-linux-gnu/include \
  --prefix=$GLIBC/target \
  --host=i686-linux-gnu \
  --build=i686-linux-gnu \
  libc_cv_complocaledir='/usr/lib/locale' \
  CFLAGS=" -matomics -mbulk-memory -O2 -g -fPIC $GLIBC_SETJMP_CFLAGS" \
  CC="clang --target=wasm32-unknown-wasi -v -Wno-int-conversion"

make -j$(($(nproc) * 2)) --keep-going 2>&1 THREAD_MODEL=posix | tee check.log

# Build extra
cd ../nptl
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/nptl/pthread_create.o \
    -c pthread_create.c -MD -MP -MF $BUILD/nptl/pthread_create.o.dt \
    -MT $BUILD/nptl/pthread_create.o

# Compile grateos_syscall.c, which contains the make_threei, register_handler, 
# and copy_data_between_cages functions
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/grateos_syscall.o \
    -c $GLIBC/grateos_syscall/grateos_syscall.c

# Compile address translation module
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/addr_translation.o \
    -c $GLIBC/grateos_syscall/addr_translation.c
    
# Compile grateos debug module
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/grateos_debug.o \
    -c $GLIBC/grateos_syscall/grateos_debug.c

# Compile grateos utils module
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $BUILD/grateos_utils.o \
    -c $GLIBC/grateos_syscall/grateos_utils.c

# Compile crt1.c
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $GLIBC/grateos_syscall/crt1.o \
    -c $GLIBC/grateos_syscall/crt1/crt1.c \
 || { echo "ERROR: clang failed compiling crt1.c"; exit 1; }
 [ -f "$GLIBC/grateos_syscall/crt1.o" ] || { echo "ERROR: $GLIBC/grateos_syscall/crt1.o not produced"; exit 1; }

# Compile crt1.c for shared target
$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -DGRATEOS_DYLINK \
    -o $GLIBC/grateos_syscall/crt1_shared.o \
    -c $GLIBC/grateos_syscall/crt1/crt1.c \
 || { echo "ERROR: clang failed compiling crt1.c"; exit 1; }
 [ -f "$GLIBC/grateos_syscall/crt1.o" ] || { echo "ERROR: $GLIBC/grateos_syscall/crt1.o not produced"; exit 1; }

# Compile wasm EH setjmp/longjmp runtime (needs -fwasm-exceptions for __builtin_wasm_throw).
# Skipped when GRATEOS_ASYNCIFY_SETJMP is set (asyncify-based setjmp is used instead).
if [[ -z "${GRATEOS_ASYNCIFY_SETJMP:-}" ]]; then
    mkdir -p $BUILD/setjmp
    $CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
        $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
        -fwasm-exceptions -mllvm -wasm-enable-sjlj \
        -o $BUILD/setjmp/wasm_eh_setjmp.o \
        -c $GLIBC/setjmp/wasm_eh_setjmp.c

    # Compile the __c_longjmp tag anchor (must NOT use -fPIC/-fPIE; those flags
    # cause the LLVM SjLj pass to emit an import instead of a local weak Tag
    # definition, which would leave __c_longjmp undefined in programs that
    # don't call setjmp themselves).
    # EXTRA_FLAGS contains -fPIE (from line 29), so strip it before use here.
    CFLAGS_NO_PIC="--target=wasm32-unknown-wasi -Wno-int-conversion -DNO_HIDDEN -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g"
    EXTRA_FLAGS_NO_PIE="${EXTRA_FLAGS//-fPIE/}"
    clang $CFLAGS_NO_PIC $WARNINGS $EXTRA_FLAGS_NO_PIE \
        $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
        -fwasm-exceptions -mllvm -wasm-enable-sjlj \
        -o $BUILD/setjmp/wasm_eh_c_longjmp_tag.o \
        -c $GLIBC/setjmp/wasm_eh_c_longjmp_tag.c
fi

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

# Generate sysroot
# First, remove the existing sysroot directory to start cleanly
rm -rf "$SYSROOT"


# Create the sysroot directory structure
mkdir -p "$SYSROOT/include/wasm32-wasi" "$SYSROOT/lib/wasm32-wasi"
cp "$BUILD/grateos_utils.o" "$SYSROOT/lib/wasm32-wasi/"
cp "$BUILD/grateos_debug.o" "$SYSROOT/lib/wasm32-wasi/"

"$SCRIPT_DIR/make_archive.sh"
cd $SCRIPT_DIR
cd ../../
"$SCRIPT_DIR/make_shared_glibc.sh" "${shared_script_args[@]}"
"$SCRIPT_DIR/make_shared_libm.sh" "${shared_script_args[@]}"

# Copy all files from the external include directory to the new sysroot include directory
cp -r "$GLIBC/target/include/"* "$SYSROOT/include/wasm32-wasi/"

# Copy the crt1.o file into the new sysroot lib directory
cp "$GLIBC/grateos_syscall/crt1.o" "$SYSROOT/lib/wasm32-wasi/"
cp "$GLIBC/grateos_syscall/crt1_shared.o" "$SYSROOT/lib/wasm32-wasi/"
cp "$GLIBC/grateos_syscall/grateos_syscall.h" "$SYSROOT/include/wasm32-wasi/"
