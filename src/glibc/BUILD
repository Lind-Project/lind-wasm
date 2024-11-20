genrule(
    name = "make_all",
    srcs = glob(["**"]),
    outs = ["sysroot"],
    cmd = """
        set -e
        export GLIBC_BASE=./src/glibc
        export CLANG=/home/lind/test/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04
        export CC=/home/lind/test/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang
        cd src/glibc
        rm -rf build
        ./wasm-config.sh
        cd build
        make -j8 --keep-going 2>&1 THREAD_MODEL=posix | tee check.log
        # cd ../nptl
        # $$CC --target=wasm32-unkown-wasi -v -Wno-int-conversion pthread_create.c -c -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O0 -g -Wall -Wwrite-strings -Wundef -fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -Wstrict-prototypes -Wold-style-definition -fmath-errno -fPIE -ftls-model=local-exec -I../include -I$$GLIBC_BASE/build/nptl -I$$GLIBC_BASE/build -I../sysdeps/lind -I../lind_syscall -I../sysdeps/unix/sysv/linux/i386/i686 -I../sysdeps/unix/sysv/linux/i386 -I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86 -I../sysdeps/x86/nptl -I../sysdeps/i386/nptl -I../sysdeps/unix/sysv/linux/include -I../sysdeps/unix/sysv/linux -I../sysdeps/nptl -I../sysdeps/pthread -I../sysdeps/gnu -I../sysdeps/unix/inet -I../sysdeps/unix/sysv -I../sysdeps/unix/i386 -I../sysdeps/unix -I../sysdeps/posix -I../sysdeps/i386/fpu -I../sysdeps/x86/fpu -I../sysdeps/i386 -I../sysdeps/x86/include -I../sysdeps/x86 -I../sysdeps/wordsize-32 -I../sysdeps/ieee754/float128 -I../sysdeps/ieee754/ldbl-96/include -I../sysdeps/ieee754/ldbl-96 -I../sysdeps/ieee754/dbl-64 -I../sysdeps/ieee754/flt-32 -I../sysdeps/ieee754 -I../sysdeps/generic -I.. -I../libio -I. -nostdinc -isystem $$CLANG/lib/clang/16/include -isystem /usr/i686-linux-gnu/include -D_LIBC_REENTRANT -include $$GLIBC_BASE/build/libc-modules.h -DMODULE_NAME=libc -include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc -o $$GLIBC_BASE/build/nptl/pthread_create.o -MD -MP -MF $$GLIBC_BASE/build/nptl/pthread_create.o.dt -MT $$GLIBC_BASE/build/nptl/pthread_create.o
        # cd ../
        # $$CC --target=wasm32-wasi-threads -matomics -o $$GLIBC_BASE/build/csu/wasi_thread_start.o -c csu/wasm32/wasi_thread_start.s
        # ./gen_sysroot.sh
        # cp -r sysroot $(location sysroot)
    """,
)