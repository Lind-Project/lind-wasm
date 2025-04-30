script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
glibc_base="$script_dir/src/glibc"
wasmtime_base="$script_dir/src/wasmtime"
rawposix_base="$script_dir/src/RawPOSIX"

CC="${CLANG:=/home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04}/bin/clang"

# Compilation flags
CFLAGS="--target=wasm32-unknown-wasi -v -Wno-int-conversion -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g"
WARNINGS="-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
EXTRA_FLAGS="-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common"
EXTRA_FLAGS+=" -Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec"
INCLUDE_PATHS="-I../include \
    -I$glibc_base/build/nptl \
    -I$glibc_base/build \
    -I../sysdeps/lind \
    -I../lind_syscall \
    -I../sysdeps/unix/sysv/linux/i386/i686 \
    -I../sysdeps/unix/sysv/linux/i386 \
    -I../sysdeps/unix/sysv/linux/x86/include \
    -I../sysdeps/unix/sysv/linux/x86 \
    -I../sysdeps/x86/nptl \
    -I../sysdeps/i386/nptl \
    -I../sysdeps/unix/sysv/linux/include \
    -I../sysdeps/unix/sysv/linux \
    -I../sysdeps/nptl \
    -I../sysdeps/pthread \
    -I../sysdeps/gnu \
    -I../sysdeps/unix/inet \
    -I../sysdeps/unix/sysv \
    -I../sysdeps/unix/i386 \
    -I../sysdeps/unix \
    -I../sysdeps/posix \
    -I../sysdeps/i386/fpu \
    -I../sysdeps/x86/fpu \
    -I../sysdeps/i386 \
    -I../sysdeps/x86/include \
    -I../sysdeps/x86 \
    -I../sysdeps/wordsize-32 \
    -I../sysdeps/ieee754/float128 \
    -I../sysdeps/ieee754/ldbl-96/include \
    -I../sysdeps/ieee754/ldbl-96 \
    -I../sysdeps/ieee754/dbl-64 \
    -I../sysdeps/ieee754/flt-32 \
    -I../sysdeps/ieee754 \
    -I../sysdeps/generic \
    -I.. \
    -I../libio \
    -I../li \
"
SYS_INCLUDE="-nostdinc -isystem $CLANG/lib/clang/16/include -isystem /usr/i686-linux-gnu/include"
DEFINES="-D_LIBC_REENTRANT -include $glibc_base/build/libc-modules.h -DMODULE_NAME=libc"
EXTRA_DEFINES="-include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"

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

compile_pthread_create="$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $glibc_base/build/nptl/pthread_create.o \
    -c pthread_create.c -MD -MP -MF $glibc_base/build/nptl/pthread_create.o.dt \
    -MT $glibc_base/build/nptl/pthread_create.o"

# Compiles lind syscall 
compile_lind_syscall="$CC $CFLAGS $WARNINGS $EXTRA_FLAGS \
    $INCLUDE_PATHS $SYS_INCLUDE $DEFINES $EXTRA_DEFINES \
    -o $glibc_base/build/lind_syscall.o \
    -c $glibc_base/lind_syscall/lind_syscall.c"

compile_wasi_thread_start="$CC --target=wasm32-wasi-threads \
    -matomics -o $glibc_base/build/csu/wasi_thread_start.o \
    -c $glibc_base/csu/wasm32/wasi_thread_start.s"

compile_set_stack_pointer="$CC --target=wasm32-wasi-threads -matomics \
    -o $glibc_base/build/csu/set_stack_pointer.o \
    -c $glibc_base/csu/wasm32/set_stack_pointer.s"

# Making glibc, renamed "make_cmd" and added compiling lind_syscall 
# Calls the Makefile, which will call the individual make files,
# Compiles pthread_create and lind_syscall separately,
# calls the gen_sysroot.sh script
make_glibc_cmd='
  cd "$glibc_base" && \
  rm -rf build && \
  ./wasm-config.sh ;

  cd build && \
  make -j8 --keep-going THREAD_MODEL=posix 2>&1 | tee check.log ;

  cd ../nptl && \
  '"$compile_pthread_create"' ;

  cd ../lind_syscall && \
  '"$compile_lind_syscall"' ;

  cd ../ && \
  '"$compile_wasi_thread_start"' && \
  '"$compile_set_stack_pointer"' ;

  ./gen_sysroot.sh
'

RED='\033[31m'
GREEN='\033[32m'
RESET='\033[0m'
