# Example 07 — callbacks (guest -> host upcall).
#
# Proves the reverse direction: the guest calls back into a host handler the stub
# registered, including reading a guest buffer back out (reverse marshalling). This
# is the primitive OpenBLAS's xerbla error handler needs.
#
#   make            build everything (runs nothing)
#   make run        run the demo against the wasm-sandboxed libcallbacks.so
#   make run-native run the demo against a native libcallbacks.so (baseline)
#   make compare    run native then sandboxed, back to back
#   make check      in-host smoke test via `lind_run --call`

LIB         := callbacks
EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
include ../../common.mk

# In-host smoke test: --call passes scalar i32 args and runs a guest export. apply_cb
# exercises the scalar upcall path (slot 0); the runtime must have a handler for slot 0,
# so this only demonstrates the guest side — the full round trip needs the stub (make run).
.PHONY: check
check: $(GUEST_MODULE)
	$(LIND_RUN) --call apply_cb $(GUEST_LINDPATH) 21
