script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
glibc_base="$script_dir/src/glibc"
wasmtime_base="$script_dir/src/wasmtime"
rawposix_base="$script_dir/src/RawPOSIX"

CC="${CLANG:=/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang"

precompile_wasm="$wasmtime_base/target/debug/wasmtime compile [input] -o [output]"

compile_test_cmd_fork_test="$CC -pthread --target=wasm32-unknown-wasi \
--sysroot $glibc_base/sysroot \
-Wl,--import-memory,--export-memory,--max-memory=67108864,--export="__stack_pointer",--export=__stack_low \
[input] -g -O0 -o [output] && \
 $script_dir/tools/binaryen/bin/wasm-opt --epoch-injection --asyncify -O2 --debuginfo [output] -o [output]"

run_cmd="$wasmtime_base/target/debug/wasmtime run --wasi \
threads=y \
--wasi preview2=n [target]"

run_cmd_precompile="$wasmtime_base/target/debug/wasmtime run \
--allow-precompiled \
--wasi threads=y \
--wasi preview2=n [target]"

run_cmd_debug="gdb --args $wasmtime_base/target/debug/wasmtime run \
-D debug-info \
-O opt-level=0 \
--wasi threads=y \
--wasi preview2=n [target]"

compile_wasmtime_cmd="cd $wasmtime_base && cargo build"
compile_rawposix_cmd="cd $rawposix_base && cargo build"

compile_pthread_create="$CC --target=wasm32-unkown-wasi \
-v -Wno-int-conversion pthread_create.c \
-c -std=gnu11 \
-fgnu89-inline \
 -matomics \
 -mbulk-memory \
 -O0 -g -Wall \
 -Wwrite-strings \
 -Wundef \
 -fmerge-all-constants \
-ftrapping-math \
 -fno-stack-protector  \
 -fno-common \
 -Wp,-U_FORTIFY_SOURCE \
 -Wstrict-prototypes \
 -Wold-style-definition \
 -fmath-errno \
 -fPIE \
 -ftls-model=local-exec \
 -I../include -I$glibc_base/build/nptl \
 -I$glibc_base/build \
 -I../sysdeps/lind  \
 -I../lind_syscall  \
 -I../sysdeps/unix/sysv/linux/i386/i686  \
 -I../sysdeps/unix/sysv/linux/i386  \
 -I../sysdeps/unix/sysv/linux/x86/include \
 -I../sysdeps/unix/sysv/linux/x86  \
 -I../sysdeps/x86/nptl  \
 -I../sysdeps/i386/nptl  \
 -I../sysdeps/unix/sysv/linux/include \
 -I../sysdeps/unix/sysv/linux  \
 -I../sysdeps/nptl  \
 -I../sysdeps/pthread  \
 -I../sysdeps/gnu  \
 -I../sysdeps/unix/inet  \
 -I../sysdeps/unix/sysv  \
 -I../sysdeps/unix/i386  \
 -I../sysdeps/unix  \
 -I../sysdeps/posix  \
 -I../sysdeps/i386/fpu  \
 -I../sysdeps/x86/fpu  \
 -I../sysdeps/i386  \
 -I../sysdeps/x86/include \
 -I../sysdeps/x86  \
 -I../sysdeps/wordsize-32  \
 -I../sysdeps/ieee754/float128  \
 -I../sysdeps/ieee754/ldbl-96/include \
 -I../sysdeps/ieee754/ldbl-96  \
 -I../sysdeps/ieee754/dbl-64  \
 -I../sysdeps/ieee754/flt-32  \
 -I../sysdeps/ieee754  \
 -I../sysdeps/generic  \
 -I.. \
 -I../libio \
 -I. -nostdinc \
 -isystem $CLANG/lib/clang/16/include \
 -isystem /usr/i686-linux-gnu/include \
 -D_LIBC_REENTRANT \
 -include $glibc_base/build/libc-modules.h \
 -DMODULE_NAME=libc \
 -include ../include/libc-symbols.h  \
 -DPIC     \
 -DTOP_NAMESPACE=glibc \
 -o $glibc_base/build/nptl/pthread_create.o \
 -MD -MP -MF $glibc_base/build/nptl/pthread_create.o.dt \
 -MT $glibc_base/build/nptl/pthread_create.o"

compile_wasi_thread_start="$CC --target=wasm32-wasi-threads \
-matomics -o $glibc_base/build/csu/wasi_thread_start.o \
-c $glibc_base/csu/wasm32/wasi_thread_start.s"
make_cmd="cd $glibc_base && rm -rf build && ./wasm-config.sh && \
cd build && \
make -j8 --keep-going 2>&1 THREAD_MODEL=posix | \
tee check.log && cd ../nptl && \
$compile_pthread_create && \
cd ../ && \
$compile_wasi_thread_start && \
./gen_sysroot.sh"

RED='\033[31m'
GREEN='\033[32m'
RESET='\033[0m'
