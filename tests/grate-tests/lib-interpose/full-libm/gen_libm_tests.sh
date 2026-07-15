#!/bin/bash
# Generates glibc's own libm correctness-test sources: one .c per function family, a
# small per-(precision-type, family) wrapper .c that selects the type before
# #including the family file, the libm-test-ulps.h header, and per-type
# libm-test-support wrapper .c files.
#
# This is pure host-side C-level generation with no wasm/lind involvement at all, so
# instead of hand-duplicating glibc's own function/type lists we just invoke glibc's
# own math/Makefile target for it directly (`gen-libm-test-sources`, added in
# src/glibc/math/Makefile) — the same way build/Makefile's own `regen-ulps` target
# invokes math/Makefile directly, bypassing the top-level Makefile's goal-recognition
# (glibc's own `make tests` doesn't work in this tree: it additionally tries to
# bootstrap a native-execution testroot that isn't wired up for this cross-compiled
# port). See ../LIBM_INTERPOSITION.md for the full writeup.
#
# The Makefile target has to run against the real glibc build dir (objdir=...) since
# it needs that dir's config.make/config.status — it can't write straight into this
# repo's generated/csrc. So: run it there, then copy the results into generated/csrc
# (wiping any stale contents first) so everything downstream (compile_libm_tests.sh)
# keeps reading from the same place it always has. Re-run any time; idempotent.
set -euo pipefail

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../../.." && pwd)"
GLIBC_MATH="$REPO_ROOT/src/glibc/math"
BUILD="$REPO_ROOT/src/glibc/build"
CSRC="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/generated/csrc"

make -C "$GLIBC_MATH" objdir="$BUILD" gen-libm-test-sources

rm -rf "$CSRC"
mkdir -p "$CSRC"
cp "$BUILD/math"/test-*.c "$BUILD/math"/libm-test-*.c "$BUILD/math"/libm-test-ulps.h "$CSRC/"

n=$(ls "$CSRC"/test-*.c | wc -l)
echo "[gen] done: $n test wrappers in $CSRC"
