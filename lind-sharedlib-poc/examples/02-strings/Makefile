# Example 02 — strings (first marshalled argument).
#
# libstrdemo.so exposes str_len(const char*), whose real implementation runs inside
# the lind/wasmtime sandbox. Because a pointer can't cross the guest memory boundary
# as-is, the host stub copies the string into guest memory (via the guest allocator)
# and passes the guest offset — the first taste of marshalling.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libstrdemo.so
#   make run-native run the demo against a real native libstrdemo.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make gen        regenerate stub/src/lib.rs from functions.txt

LIB         := strdemo
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# The guest uses real libc malloc/free/strlen, so its module imports env::malloc etc.
# Those are resolved by preloading libc (and libm) into the sandbox — the same modules
# lind_run preloads, but passed as HOST paths since the .so isn't chrooted into lindfs.
PRELOAD := env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm


# NOTE: no `check` target here. `lind_run --call` only passes scalar i32 args, so it
# cannot drive a pointer-taking export like str_len — that needs the marshalling the
# .so stub performs. Use `make run`.
