# Example 04 — in/out buffers (the guest reads and writes the caller's buffer).
#
# libinoutdemo.so exposes four in-place transforms. The real work runs inside the
# lind/wasmtime sandbox; the host stub copies the caller's buffer INTO guest memory,
# runs the call, and copies the result back out.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libinoutdemo.so
#   make run-native run the demo against a real native libinoutdemo.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make gen        regenerate stub/src/lib.rs from functions.txt

LIB         := inoutdemo
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# The guest uses real libc (strnlen/malloc), so preload libc/libm — same modules
# lind_run preloads, passed as HOST paths since the .so isn't chrooted.
PRELOAD := env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm

# No `check` target: `lind_run --call` only passes scalar i32 args, so it can't drive
# pointer/buffer-taking exports — that needs the marshalling the .so stub performs.
