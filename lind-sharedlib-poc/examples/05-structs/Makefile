# Example 05 — structs (const struct* input, marshalled field by field).
#
# libpersondemo.so exposes functions taking a `const struct Person *`. The real work
# runs inside the lind/wasmtime sandbox; the host stub reads the caller's struct with
# the host (LP64) layout and the engine repacks it into the guest (ILP32) layout,
# nested-allocating the char* field.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libpersondemo.so
#   make run-native run the demo against a real native libpersondemo.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make gen        regenerate stub/src/lib.rs from functions.txt

LIB         := persondemo
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# The guest uses real libc (strlen/malloc), so preload libc/libm — same modules
# lind_run preloads, passed as HOST paths since the .so isn't chrooted.
PRELOAD := env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm

# No `check` target: `lind_run --call` only passes scalar i32 args, so it can't drive
# pointer/struct-taking exports — that needs the marshalling the .so stub performs.
