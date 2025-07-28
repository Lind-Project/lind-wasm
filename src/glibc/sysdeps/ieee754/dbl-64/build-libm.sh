#!/bin/bash
set -e

CLANG="/home/alice/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang"
AR="/home/alice/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/llvm-ar"

INCLUDES="\
-I/home/alice/lind-wasm/src/glibc/include \
-I/home/alice/lind-wasm/src/glibc/build/nptl \
-I/home/alice/lind-wasm/src/glibc/build \
-I/home/alice/lind-wasm/src/glibc/sysdeps/lind \
-I/home/alice/lind-wasm/src/glibc/lind_syscall \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux/i386/i686 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux/i386 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux/x86/include \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux/x86 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/x86/nptl \
-I/home/alice/lind-wasm/src/glibc/sysdeps/i386/nptl \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux/include \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv/linux \
-I/home/alice/lind-wasm/src/glibc/sysdeps/nptl \
-I/home/alice/lind-wasm/src/glibc/sysdeps/pthread \
-I/home/alice/lind-wasm/src/glibc/sysdeps/gnu \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/inet \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/sysv \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix/i386 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/unix \
-I/home/alice/lind-wasm/src/glibc/sysdeps/posix \
-I/home/alice/lind-wasm/src/glibc/sysdeps/i386/fpu \
-I/home/alice/lind-wasm/src/glibc/sysdeps/x86/fpu \
-I/home/alice/lind-wasm/src/glibc/sysdeps/i386 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/x86/include \
-I/home/alice/lind-wasm/src/glibc/sysdeps/x86 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/wordsize-32 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754/float128 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754/ldbl-96/include \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754/ldbl-96 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754/dbl-64 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754/flt-32 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/ieee754 \
-I/home/alice/lind-wasm/src/glibc/sysdeps/generic \
-I/home/alice/lind-wasm/src/glibc \
-I/home/alice/lind-wasm/src/glibc/libio \
-I/home/alice/lind-wasm/src/glibc/math"

CFLAGS="--target=wasm32-unknown-wasi -v -Wno-int-conversion -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g \
-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition \
-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common \
-Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec \
-nostdinc -isystem /home/alice/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/include \
-isystem /usr/i686-linux-gnu/include \
-D_LIBC_REENTRANT -include /home/alice/lind-wasm/src/glibc/build/libc-modules.h \
-DMODULE_NAME=libc -include /home/alice/lind-wasm/src/glibc/include/libc-symbols.h \
-DPIC -DTOP_NAMESPACE=glibc"

# Compile
$CLANG $CFLAGS $INCLUDES -o e_exp2.o -c e_exp2.c
$CLANG $CFLAGS $INCLUDES -o e_fmod.o -c e_fmod.c
$CLANG $CFLAGS $INCLUDES -o math_err.o -c math_err.c
$CLANG $CFLAGS $INCLUDES -o e_exp_data.o -c e_exp_data.c

# Archive
$AR rcs libm.a e_fmod.o e_exp2.o e_exp_data.o math_err.o

# Copy
cp libm.a /home/alice/lind-wasm/src/glibc/sysroot/lib/wasm32-wasi/
