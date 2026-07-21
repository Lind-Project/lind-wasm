# Example 03 — buffers (caller-allocated output buffers / copy-out).
#
# libbufdemo.so exposes four functions that write into caller-allocated buffers,
# each with a different copy-back-length contract (return value / NUL / capacity /
# out-param). The real work runs inside the lind/wasmtime sandbox; the host stub
# allocates the buffer in guest memory, runs the call, and copies the result back.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libbufdemo.so
#   make run-native run the demo against a real native libbufdemo.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make gen        regenerate stub/src/lib.rs from functions.txt

LIB         := bufdemo
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# The guest uses real libc (snprintf/toupper/malloc), so preload libc/libm — same
# modules lind_run preloads, passed as HOST paths since the .so isn't chrooted.
PRELOAD := env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm

# No `check` target: `lind_run --call` only passes scalar i32 args, so it can't drive
# pointer/buffer-taking exports — that needs the marshalling the .so stub performs.
