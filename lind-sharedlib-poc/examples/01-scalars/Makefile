# Example 01 — scalars.
#
# A trivial library (int add(int,int), int subtract(int,int)) exposed as a native
# .so whose symbols run inside the lind/wasmtime sandbox. Pure scalar i32 in/out,
# so nothing crosses the guest memory boundary — this proves the runtime +
# packaging plumbing in isolation, before any marshalling.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libadd_sub.so
#   make run-native run the demo against a real native libadd_sub.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make gen        regenerate stub/src/lib.rs from functions.txt
#   make check      in-host smoke test via `lind_run --call` (no .so packaging)

LIB         := add_sub
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# In-host smoke test of the Stage-1 mechanic. Function names + args are
# example-specific, so this lives here rather than in the shared engine.
# lind_run chroots into lindfs/, so the module is the lindfs-relative path.
.PHONY: check
check: $(GUEST_MODULE)
	$(LIND_RUN) --call add $(GUEST_LINDPATH) 2 3
	$(LIND_RUN) --call subtract $(GUEST_LINDPATH) 10 4
