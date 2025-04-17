#!/bin/bash

make_glibc() {
        echo "test" > $@
        
        export GLIBC_BASE=$$PWD/src/glibc
        export WORKSPACE=$$PWD
        
        export CLANG=$$PWD/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04
        export CC=$$CLANG/bin/clang
        
        echo $$GLIBC_BASE >> $@
        echo $$CLANG >> $@
        echo $$CC >> $@
        
        cd $$GLIBC_BASE
        rm -rf build
        ./wasm-config.sh
        cd build
        make -j8 --keep-going 2>&1 THREAD_MODEL=posix | tee check.log || true
        
        cd ../nptl
        
        # Define common flags
        CFLAGS="--target=wasm32-unknown-wasi -v -Wno-int-conversion -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O0 -g"
        WARNINGS="-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
        EXTRA_FLAGS="-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common"
        EXTRA_FLAGS+=" -Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec"
        INCLUDE_PATHS="
            -I../include
            -I$$GLIBC_BASE/build/nptl
            -I$$GLIBC_BASE/build
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
        SYS_INCLUDE="-nostdinc -isystem $$CLANG/lib/clang/16/include -isystem /usr/i686-linux-gnu/include"
        DEFINES="-D_LIBC_REENTRANT -include $$GLIBC_BASE/build/libc-modules.h -DMODULE_NAME=libc"
        EXTRA_DEFINES="-include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"
        
        $$CC $$CFLAGS $$WARNINGS $$EXTRA_FLAGS \
            $$INCLUDE_PATHS $$SYS_INCLUDE $$DEFINES $$EXTRA_DEFINES \
            -o $$GLIBC_BASE/build/nptl/pthread_create.o \
            -c pthread_create.c -MD -MP -MF $$GLIBC_BASE/build/nptl/pthread_create.o.dt \
            -MT $$GLIBC_BASE/build/nptl/pthread_create.o
        
        # Compile assembly files
        cd ../ && \
        $$CC --target=wasm32-wasi-threads -matomics \
            -o $$GLIBC_BASE/build/csu/wasi_thread_start.o \
            -c $$GLIBC_BASE/csu/wasm32/wasi_thread_start.s
        
        $$CC --target=wasm32-wasi-threads -matomics \
            -o $$GLIBC_BASE/build/csu/set_stack_pointer.o \
            -c $$GLIBC_BASE/csu/wasm32/set_stack_pointer.s
}

make_wasmtime() {
        echo "test" > $@
        
        export WASMTIME_BASE=$$PWD/src/wasmtime
        export WORKSPACE=$$PWD

        cd $$WASMTIME_BASE
        cargo build
}

case "$1" in
    make_glibc)
        make_glibc
        ;;
    make_wasmtime)
        make_wasmtime
        ;;
esac
