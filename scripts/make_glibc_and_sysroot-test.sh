#!/usr/bin/env bash
#
# Build glibc and generate a sysroot for clang to cross-compile lind programs
#
# Assumptions:
# - Run from repo root
# - clang/llvm in PATH (CLANG may also point to the toolchain root)
# - glibc source at ./src/glibc
#
set -euo pipefail

# -------- roots and dirs (absolute) --------
REPO_ROOT="$(pwd)"
GLIBC_SRC="${REPO_ROOT}/src/glibc"
BUILD_ROOT="${REPO_ROOT}/build"              # out-of-tree build root
GLIBC_BUILD="${BUILD_ROOT}/glibc"            # <== build outside the source tree
SYSROOT="${GLIBC_SRC}/sysroot"
SYSROOT_ARCHIVE="${SYSROOT}/lib/wasm32-wasi/libc.a"

mkdir -p "${GLIBC_BUILD}" "${SYSROOT}" "${SYSROOT}/lib/wasm32-wasi" "${SYSROOT}/include/wasm32-wasi"

# -------- toolchain/flags --------
CC="clang"
TARGET="wasm32-unknown-wasi"
CFLAGS_COMMON="-Wno-int-conversion -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g -fPIC"
CFLAGS="--target=${TARGET} ${CFLAGS_COMMON}"
WARNINGS="-Wall -Wwrite-strings -Wundef -Wstrict-prototypes -Wold-style-definition"
EXTRA_FLAGS="-fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -fmath-errno -fPIE -ftls-model=local-exec"

# Prefer LLVM binutils so probes don't choke on WASM
: "${CLANG:?CLANG env var must point to your clang+llvm dir}"
export AR="${CLANG}/bin/llvm-ar"
export NM="${CLANG}/bin/llvm-nm"
export RANLIB="${CLANG}/bin/llvm-ranlib"
export STRIP="${CLANG}/bin/llvm-strip"
export LD="${CLANG}/bin/wasm-ld"
export OBJDUMP="${CLANG}/bin/llvm-objdump"
export OBJCOPY="${CLANG}/bin/llvm-objcopy"

# Sanity check (absolute paths; don't rely on PATH)
for t in "$AR" "$NM" "$RANLIB" "$STRIP" "$LD" "$OBJDUMP" "$OBJCOPY"; do
  [[ -x "$t" ]] || { echo "Error: $t not found or not executable"; exit 1; }
done

# Header search paths (absolute, no relative ascents)
INCLUDE_PATHS="
    -I${GLIBC_SRC}/include
    -I${GLIBC_BUILD}/nptl
    -I${GLIBC_BUILD}
    -I${GLIBC_SRC}/sysdeps/lind
    -I${GLIBC_SRC}/lind_syscall
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux/i386/i686
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux/i386
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux/x86/include
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux/x86
    -I${GLIBC_SRC}/sysdeps/x86/nptl
    -I${GLIBC_SRC}/sysdeps/i386/nptl
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux/include
    -I${GLIBC_SRC}/sysdeps/unix/sysv/linux
    -I${GLIBC_SRC}/sysdeps/nptl
    -I${GLIBC_SRC}/sysdeps/pthread
    -I${GLIBC_SRC}/sysdeps/gnu
    -I${GLIBC_SRC}/sysdeps/unix/inet
    -I${GLIBC_SRC}/sysdeps/unix/sysv
    -I${GLIBC_SRC}/sysdeps/unix/i386
    -I${GLIBC_SRC}/sysdeps/unix
    -I${GLIBC_SRC}/sysdeps/posix
    -I${GLIBC_SRC}/sysdeps/i386/fpu
    -I${GLIBC_SRC}/sysdeps/x86/fpu
    -I${GLIBC_SRC}/sysdeps/i386
    -I${GLIBC_SRC}/sysdeps/x86/include
    -I${GLIBC_SRC}/sysdeps/x86
    -I${GLIBC_SRC}/sysdeps/wordsize-32
    -I${GLIBC_SRC}/sysdeps/ieee754/float128
    -I${GLIBC_SRC}/sysdeps/ieee754/ldbl-96/include
    -I${GLIBC_SRC}/sysdeps/ieee754/ldbl-96
    -I${GLIBC_SRC}/sysdeps/ieee754/dbl-64
    -I${GLIBC_SRC}/sysdeps/ieee754/flt-32
    -I${GLIBC_SRC}/sysdeps/ieee754
    -I${GLIBC_SRC}/sysdeps/generic
    -I${GLIBC_SRC}
    -I${GLIBC_SRC}/libio
    -I${GLIBC_BUILD}
"
SYS_INCLUDE="-nostdinc -isystem ${CLANG:-}/lib/clang/18/include -isystem /usr/i686-linux-gnu/include"
DEFINES="-D_LIBC_REENTRANT -include ${GLIBC_BUILD}/libc-modules.h -DMODULE_NAME=libc"
EXTRA_DEFINES="-include ${GLIBC_SRC}/include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"

# -------- clean build dir --------
rm -rf "${GLIBC_BUILD}"
mkdir -p "${GLIBC_BUILD}"

# -------- configure: preseed + restrict subdirs --------
# Limit glibc build to csu+libc (avoid elf/ unwinder etc. that assume ELF)
cat > "${GLIBC_BUILD}/configparms" <<'EOF'
subdirs = csu libc
build-werror = no
EOF

# Make configure’s ELF/ifunc probes harmless for WASM objects
cat > "${GLIBC_BUILD}/config.site" <<'EOF'
ac_cv_prog_READELF=true
libc_cv_have_ifunc=no
libc_cv_as_needed=no
EOF
export CONFIG_SITE="${GLIBC_BUILD}/config.site"
export READELF=true

# Harmless non-empty stub so compares don’t complain
printf '/* stub: generated elsewhere for WASI build */\n' > "${GLIBC_BUILD}/libc-modules.h"

# Configure from inside the build dir
cd "${GLIBC_BUILD}"

"${GLIBC_SRC}/configure" \
  --disable-werror \
  --disable-hidden-plt \
  --disable-profile \
  --with-headers=/usr/i686-linux-gnu/include \
  --prefix="${GLIBC_SRC}/target" \
  --host=i686-linux-gnu \
  --build=i686-linux-gnu \
  CFLAGS=" -matomics -mbulk-memory -O2 -g" \
  CC="clang --target=${TARGET} -Wno-int-conversion" \
  --cache-file="${GLIBC_BUILD}/config.cache" \
  --quiet

# Ensure only csu+libc even if config.status wrote a bigger list
sed -i 's/^subdirs *=.*/subdirs = csu libc/' config.make

# -------- build (only the subdirs we want; avoid top-level "all") --------
LOG="${GLIBC_BUILD}/build-glibc.log"
: > "${LOG}"
MAKEFLAGS="-j$(( $(nproc)*2 )) --keep-going -s --no-print-directory"

# (optional) install headers without dragging in elf
make -C "${GLIBC_BUILD}" ${MAKEFLAGS} install-headers >>"${LOG}" 2>&1 || true

# build only csu and libc (let top-level make create subdir dirs/makefiles)
if ! make -C "${GLIBC_BUILD}" ${MAKEFLAGS} csu/subdir_lib >>"${LOG}" 2>&1; then
  echo "make csu/subdir_lib failed — last 200 lines:" >&2; tail -200 "${LOG}" >&2; exit 1
fi
if ! make -C "${GLIBC_BUILD}" ${MAKEFLAGS} libc/subdir_lib >>"${LOG}" 2>&1; then
  echo "make libc/subdir_lib failed — last 200 lines:" >&2; tail -200 "${LOG}" >&2; exit 1
fi

# -------- extra objects (absolute paths, no cd ..) --------
mkdir -p "${GLIBC_BUILD}/nptl" "${GLIBC_BUILD}/csu"

# nptl/pthread_create.o
${CC} ${CFLAGS} ${WARNINGS} ${EXTRA_FLAGS} \
  ${INCLUDE_PATHS} ${SYS_INCLUDE} ${DEFINES} ${EXTRA_DEFINES} \
  -o "${GLIBC_BUILD}/nptl/pthread_create.o" \
  -c "${GLIBC_SRC}/nptl/pthread_create.c" \
  -MD -MP -MF "${GLIBC_BUILD}/nptl/pthread_create.o.dt" \
  -MT "${GLIBC_BUILD}/nptl/pthread_create.o" >>"${LOG}" 2>&1

# lind_syscall.o
${CC} ${CFLAGS} ${WARNINGS} ${EXTRA_FLAGS} \
  ${INCLUDE_PATHS} ${SYS_INCLUDE} ${DEFINES} ${EXTRA_DEFINES} \
  -o "${GLIBC_BUILD}/lind_syscall.o" \
  -c "${GLIBC_SRC}/lind_syscall/lind_syscall.c" >>"${LOG}" 2>&1

# elision-lock.o
${CC} ${CFLAGS} ${WARNINGS} ${EXTRA_FLAGS} \
  ${INCLUDE_PATHS} ${SYS_INCLUDE} ${DEFINES} ${EXTRA_DEFINES} \
  -o "${GLIBC_BUILD}/nptl/elision-lock.o" \
  -c "${GLIBC_SRC}/sysdeps/unix/sysv/linux/x86/elision-lock.c" \
  -MD -MP -MF "${GLIBC_BUILD}/nptl/elision-lock.o.dt" \
  -MT "${GLIBC_BUILD}/nptl/elision-lock.o" >>"${LOG}" 2>&1

# elision-unlock.o
${CC} ${CFLAGS} ${WARNINGS} ${EXTRA_FLAGS} \
  ${INCLUDE_PATHS} ${SYS_INCLUDE} ${DEFINES} ${EXTRA_DEFINES} \
  -o "${GLIBC_BUILD}/nptl/elision-unlock.o" \
  -c "${GLIBC_SRC}/sysdeps/unix/sysv/linux/x86/elision-unlock.c" \
  -MD -MP -MF "${GLIBC_BUILD}/nptl/elision-unlock.o.dt" \
  -MT "${GLIBC_BUILD}/nptl/elision-unlock.o" >>"${LOG}" 2>&1

# WASM thread startup asm
${CC} --target=wasm32-wasi-threads -matomics \
  -o "${GLIBC_BUILD}/csu/wasi_thread_start.o" \
  -c "${GLIBC_SRC}/csu/wasm32/wasi_thread_start.s" >>"${LOG}" 2>&1

${CC} --target=wasm32-wasi-threads -matomics \
  -o "${GLIBC_BUILD}/csu/set_stack_pointer.o" \
  -c "${GLIBC_SRC}/csu/wasm32/set_stack_pointer.s" >>"${LOG}" 2>&1

# -------- sysroot pack --------
rm -rf "${SYSROOT}"
mkdir -p "${SYSROOT}/include/wasm32-wasi" "${SYSROOT}/lib/wasm32-wasi"

# collect objects (exclude known junk)
mapfile -t OBJECTS < <(find "${GLIBC_BUILD}" -type f -name '*.o' ! \( \
  -name 'stamp.o' -o \
  -name 'argp-pvh.o' -o \
  -name 'repertoire.o' -o \
  -name 'static-stubs.o' -o \
  -name 'zic.o' -o \
  -name 'xmalloc.o' -o \
  -name 'list.o' -o \
  -name 'ldconfig.o' -o \
  -name 'sln.o' \
\))

if (( ${#OBJECTS[@]} == 0 )); then
  echo "No suitable .o files found in '${GLIBC_BUILD}'." >&2
  tail -200 "${LOG}" >&2 || true
  exit 1
fi

llvm-ar rcs "${SYSROOT_ARCHIVE}" "${OBJECTS[@]}"
# placeholder pthread archive (if you need it)
llvm-ar crs "${SYSROOT}/lib/wasm32-wasi/libpthread.a"

# headers (best-effort)
cp -r "${GLIBC_SRC}/target/include/"* "${SYSROOT}/include/wasm32-wasi/" 2>/dev/null || true
cp "${GLIBC_SRC}/lind_syscall/crt1.o" "${SYSROOT}/lib/wasm32-wasi/" 2>/dev/null || true

echo "OK: sysroot @ ${SYSROOT}"
echo "Log: ${LOG}"
