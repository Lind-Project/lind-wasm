# Shared setup for gen_libm_tests.sh / compile_libm_tests.sh / run_single_libm_test.sh /
# run_libm_tests.sh. Not meant to be run directly — sourced by the others.
#
# This is glibc's *internal* test machinery, not ordinary userspace code, so it needs
# glibc's internal include paths/macros — scripts/lind_compile (built for the public
# sysroot) can't compile it directly. See ../LIBM_INTERPOSITION.md for why, and for the
# note that glibc's own `make tests` doesn't work here either (tries to bootstrap a
# native-execution testroot that isn't wired up for this cross-compiled port). The
# flags below were extracted from a live (partial) `make tests` trace on this repo's
# glibc build tree; regenerate by running `make tests` in src/glibc/build and grepping
# the clang invocation for a plain .c file if the glibc build config ever changes.

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd)"
GLIBC="$REPO_ROOT/src/glibc"
BUILD="$GLIBC/build"
SYSROOT="$REPO_ROOT/build/sysroot"
LLVM="$(ls -d "$REPO_ROOT"/clang+llvm-* 2>/dev/null | head -1)"
HERE="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
# gen_libm_tests.sh runs glibc's own math/Makefile target (which must write into its
# real objdir) and then copies the results here.
CSRC="$HERE/generated/csrc"
OBJ="$HERE/generated/obj"

CC="clang --target=wasm32-unknown-wasi -Wno-int-conversion"
# NOTE: no -ftls-model= override here (deliberately, after a real bug: glibc's own
# internal build uses -ftls-model=local-exec, which is only valid because that code
# ends up statically linked INTO the same final libc.so as the TLS variables it
# touches, e.g. errno. We compile this test code as a genuinely separate dynamic
# module that must reach into the real, separately-built libc.so's TLS block for
# errno at runtime — local-exec assumes same-module placement and produces a
# relocation wasm-ld can't satisfy (`R_WASM_MEMORY_ADDR_TLS_SLEB` against an
# undefined symbol). Omitting the flag lets clang use its default, cross-module-safe
# TLS model (matching lind_compile's own do_dynamic_compile, which never sets this
# flag either).
CFLAGS_INTERNAL="-std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O2 -g -fPIC -DLIND_EH_SETJMP -Wall -Wwrite-strings -Wundef -fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -Wstrict-prototypes -Wold-style-definition -fmath-errno -fPIE -DSTACK_PROTECTOR_LEVEL=0"
INCLUDES="-I$GLIBC/include -I$BUILD/math -I$BUILD -I$GLIBC/sysdeps/lind -I$GLIBC/sysdeps/ieee754/float128 -I$GLIBC/sysdeps/ieee754/ldbl-96/include -I$GLIBC/sysdeps/ieee754/ldbl-96 -I$GLIBC/sysdeps/ieee754/dbl-64 -I$GLIBC/sysdeps/ieee754/flt-32 -I$GLIBC/sysdeps/ieee754 -I$GLIBC/lind_syscall -I$GLIBC/sysdeps/unix/sysv/linux/i386/i686 -I$GLIBC/sysdeps/unix/sysv/linux/i386 -I$GLIBC/sysdeps/unix/sysv/linux/x86/include -I$GLIBC/sysdeps/unix/sysv/linux/x86 -I$GLIBC/sysdeps/x86/nptl -I$GLIBC/sysdeps/i386/nptl -I$GLIBC/sysdeps/unix/sysv/linux/include -I$GLIBC/sysdeps/unix/sysv/linux -I$GLIBC/sysdeps/nptl -I$GLIBC/sysdeps/pthread -I$GLIBC/sysdeps/gnu -I$GLIBC/sysdeps/unix/inet -I$GLIBC/sysdeps/unix/sysv -I$GLIBC/sysdeps/unix/i386 -I$GLIBC/sysdeps/unix -I$GLIBC/sysdeps/posix -I$GLIBC/sysdeps/i386/fpu -I$GLIBC/sysdeps/x86/fpu -I$GLIBC/sysdeps/i386 -I$GLIBC/sysdeps/x86/include -I$GLIBC/sysdeps/x86 -I$GLIBC/sysdeps/wordsize-32 -I$GLIBC/sysdeps/generic -I$GLIBC -I$GLIBC/libio -I$GLIBC/math -I$CSRC"
SYSINC="-nostdinc -isystem $LLVM/lib/clang/18/include -isystem /usr/i686-linux-gnu/include"
# MODULE_NAME=testsuite (not libc!) — this IS glibc's own sanctioned setting for
# compiling its test suite (see include/libc-symbols.h's IS_IN(testsuite)/_ISOMAC
# comment: "should be compiled against as close an approximation to the installed
# headers as possible"). Using MODULE_NAME=libc (copied from a trace of glibc's own
# *internal* object compiles, e.g. csu/init-first.c, which genuinely become part of
# libc.so) was the actual bug behind the errno link failure: it made errno.h treat
# this code as literally being part of libc.so and use a same-module-only TLS
# access (__libc_errno) instead of the safe, cross-module-compatible public one.
DEFS="-D_LIBC_REENTRANT -include $BUILD/libc-modules.h -DMODULE_NAME=testsuite -include $GLIBC/include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc"

# worklist [only]: prints "name rettype" pairs, one per generated test source in
# $CSRC, optionally filtered to a single test name.
#
# rettype is read directly out of each file's own first `#include <test-TYPE.h>` line
# (glibc's own math/Makefile recipe always emits that as the first line, for both
# plain and narrow-conversion wrappers — see gen-libm-test-sources in
# src/glibc/math/Makefile) rather than guessed from the filename, since narrow-
# conversion wrappers are named "test-<rettype>-<argtype>-<func>.c" with no "narrow"
# marker, which would otherwise collide with plain "test-<type>-<func>.c" names.
#
# Scope: only tests whose type(s) are double/float/ldouble are emitted — the same
# scope this test suite has always covered (matching libm-test-support-<t>.o, which is
# only built for those 3 types below). glibc's own Makefile now generates the full
# 7-type matrix (+float128/float32/float32x/float64/float64x) as part of the same
# generation step; skip those here rather than silently widening what gets compiled
# and run.
#
# Pure-bash (no basename/sed forks per file — with 1126 files generated by the
# Makefile matrix, that fork overhead alone used to dominate a single test's
# compile/run time).
worklist() {
  local only="${1:-}" f b t1 t2 line1 line2

  if [ -n "$only" ]; then
    f="$CSRC/$only.c"
    [ -f "$f" ] || return 0
    set -- "$f"
  else
    set -- "$CSRC"/test-*.c
  fi

  for f in "$@"; do
    b="${f##*/}"; b="${b%.c}"
    { read -r line1; read -r line2; } < "$f"
    [[ "$line1" =~ \<test-([A-Za-z0-9_]+)\.h\> ]] || continue
    t1="${BASH_REMATCH[1]}"
    case "$t1" in double|float|ldouble) ;; *) continue ;; esac
    t2=""
    if [[ "$line2" =~ \<test-arg-([A-Za-z0-9_]+)\.h\> ]]; then
      t2="${BASH_REMATCH[1]}"
      case "$t2" in double|float|ldouble) ;; *) continue ;; esac
    fi
    echo "$b $t1"
  done
}
