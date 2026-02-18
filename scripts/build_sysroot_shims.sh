#!/bin/bash
# Build shim libs (fenv, eh_stub, lll_elision) for wasm32 and install into Lind sysroot (issue #245).
set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
GLIBC="${ROOT_DIR}/src/glibc"
SYSROOT="${GLIBC}/sysroot"
BUILD="${GLIBC}/build"
SHIM_DIR="${ROOT_DIR}/scripts/shim-libs"
LIB_DIR="${SYSROOT}/lib/wasm32-wasi"

# Compiler and flags matching issue #245 / glibc build
export CC="${CC:-clang}"
for v in 16 15 14; do
  if [ -x "/usr/lib/llvm-${v}/bin/clang" ]; then
    CC="/usr/lib/llvm-${v}/bin/clang"
    break
  fi
done

CFLAGS="--target=wasm32-unknown-wasi -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g -fPIE -ftls-model=local-exec"
CFLAGS="$CFLAGS -Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
CFLAGS="$CFLAGS -fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -fmath-errno"
INCS="-I${GLIBC}/include -I${BUILD}/nptl -I${BUILD} -I${GLIBC}/sysdeps/lind -I${GLIBC}/lind_syscall"
INCS="$INCS -I${GLIBC}/sysdeps/unix/sysv/linux/i386/i686 -I${GLIBC}/sysdeps/unix/sysv/linux/i386"
INCS="$INCS -I${GLIBC}/sysdeps/unix/sysv/linux/x86/include -I${GLIBC}/sysdeps/unix/sysv/linux/x86"
INCS="$INCS -I${GLIBC}/sysdeps/x86/nptl -I${GLIBC}/sysdeps/i386/nptl -I${GLIBC}/sysdeps/unix/sysv/linux/include"
INCS="$INCS -I${GLIBC}/sysdeps/unix/sysv/linux -I${GLIBC}/sysdeps/nptl -I${GLIBC}/sysdeps/pthread -I${GLIBC}/sysdeps/gnu"
INCS="$INCS -I${GLIBC}/sysdeps/unix/inet -I${GLIBC}/sysdeps/unix/sysv -I${GLIBC}/sysdeps/unix/i386 -I${GLIBC}/sysdeps/unix"
INCS="$INCS -I${GLIBC}/sysdeps/posix -I${GLIBC}/sysdeps/i386/fpu -I${GLIBC}/sysdeps/x86/fpu -I${GLIBC}/sysdeps/i386"
INCS="$INCS -I${GLIBC}/sysdeps/x86/include -I${GLIBC}/sysdeps/x86 -I${GLIBC}/sysdeps/wordsize-32 -I${GLIBC}/sysdeps/ieee754/float128"
INCS="$INCS -I${GLIBC}/sysdeps/ieee754/ldbl-96/include -I${GLIBC}/sysdeps/ieee754/ldbl-96 -I${GLIBC}/sysdeps/ieee754/dbl-64"
INCS="$INCS -I${GLIBC}/sysdeps/ieee754/flt-32 -I${GLIBC}/sysdeps/ieee754 -I${GLIBC}/sysdeps/generic -I${GLIBC} -I${GLIBC}/libio"
RESOURCE_DIR="$(${CC} --target=wasm32-unknown-wasi -print-resource-dir 2>/dev/null)"
SYS_INC="-nostdinc -isystem ${RESOURCE_DIR}/include -isystem /usr/include -isystem /usr/include/x86_64-linux-gnu"
DEFINES="-D_LIBC_REENTRANT -DMODULE_NAME=libc -include ${GLIBC}/include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"
# If libc-modules.h exists (from glibc build), use it
[ -f "${BUILD}/libc-modules.h" ] && DEFINES="-include ${BUILD}/libc-modules.h $DEFINES"

mkdir -p "${LIB_DIR}"
cd "${SHIM_DIR}"

compile() {
  local src="$1" obj="$2"
  $CC $CFLAGS $INCS $SYS_INC $DEFINES -c "$src" -o "$obj" || return 1
}

# fenv_shim needs fenv.h from sysroot or clang
if [ -d "${SYSROOT}/include/wasm32-wasi" ]; then
  SYS_INC="$SYS_INC -isystem ${SYSROOT}/include/wasm32-wasi"
fi

compile fenv_shim.c fenv_shim.o
compile eh_stub.c eh_stub.o
compile lll_elision_shim.c lll_elision_shim.o

AR="llvm-ar"
command -v llvm-ar-14 >/dev/null 2>&1 && AR="llvm-ar-14"
command -v llvm-ar-16 >/dev/null 2>&1 && AR="llvm-ar-16"
command -v $AR >/dev/null 2>&1 || AR="ar"

$AR rcs libfenv_shim.a fenv_shim.o
$AR rcs libeh_stub.a eh_stub.o
$AR rcs lll_shim.a lll_elision_shim.o

cp libfenv_shim.a libeh_stub.a lll_shim.a "${LIB_DIR}/"
echo "Shim libs installed to ${LIB_DIR}"
rm -f fenv_shim.o eh_stub.o lll_elision_shim.o
