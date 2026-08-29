# Example 06 — OpenBLAS as a sandboxed native .so.
#
# Loading model (Option A): a real shared library can't be the sandbox's MAIN module —
# a `-shared` wasm library's internal function tables only get set up on the LIBRARY
# (preload) path, so an indirect kernel dispatch (e.g. gemv) traps "uninitialized
# element" if OpenBLAS is loaded as main. Instead:
#
#   MAIN module = a small shim (openblas_lind_shim.c) — imports cblas_* from OpenBLAS
#                 and re-exports lind_cblas_* wrappers (+ guest_malloc/guest_free).
#   PRELOAD     = plain OpenBLAS (built by compile_openblas.sh WITHOUT the shim) + libc/libm.
#
# So OpenBLAS goes through the proven preload path; the shim (a normal dynamic module,
# like the 01–05 guests) is a valid main module and just trampolines into it.
#
#   make            build shim + stub .so + demo
#   make shim       build only the resident main-module shim (-> lindfs)
#   make host       build only the stub cdylib (libopenblas.so)
#   make run        run the demo (shim main + OpenBLAS preload)
#   make clean      remove build artifacts

EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO_ROOT   := $(abspath $(EXAMPLE_DIR)/../../..)
LINDFS_DIR  := $(REPO_ROOT)/lindfs
LIND_COMPILE := $(REPO_ROOT)/scripts/lind_compile

CC          ?= cc
STUB_DIR    := $(EXAMPLE_DIR)/stub
CDYLIB_DIR  := $(STUB_DIR)/target/release
CDYLIB      := $(CDYLIB_DIR)/libopenblas.so
BUILD       := $(EXAMPLE_DIR)/build

# Resident MAIN module: the shim. Its cblas_*/malloc/free calls become dynamic imports
# (default lind_compile is a dynamic build), resolved from the preloads at runtime.
LINDFS_SUBDIR := sharedlib-poc/openblas
SHIM_SRC    := $(EXAMPLE_DIR)/openblas_lind_shim.c
SHIM_CWASM  := $(LINDFS_DIR)/$(LINDFS_SUBDIR)/openblas_lind_shim.cwasm

# Plain OpenBLAS wasm library (built WITHOUT the shim linked in), loaded as a PRELOAD.
OPENBLAS_CWASM ?= $(LINDFS_DIR)/lib/libopenblas.so

# Preload set (host paths — the .so isn't chrooted): libc, libm, then OpenBLAS.
PRELOAD ?= env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm,env=$(OPENBLAS_CWASM)

# Engine fpcast must be OFF: OpenBLAS's fpcast is baked in at compile time, so enabling
# the engine's fpcast double-applies it and traps "indirect call type mismatch".
FPCAST      ?= 0

.DEFAULT_GOAL := build
.PHONY: build run host shim clean FORCE

build: shim $(BUILD)/demo
	@echo "built shim + stub + demo — run with 'make run'"

# Build the resident main-module shim. lind_compile's default (dynamic) build leaves
# cblas_*/malloc/free undefined -> dynamic imports; writes .wasm/.cwasm next to the
# source and copies the .cwasm into lindfs/$(LINDFS_SUBDIR)/.

#
# --fpcast-emu: the shim is the MAIN module and sets the cage's fpcast mode, so it must
# match the preloaded OpenBLAS (built with fpcast-emu). Without it, OpenBLAS's indirect
# kernel dispatch traps "indirect call type mismatch". Override SHIM_FLAGS to change.
SHIM_FLAGS ?= --fpcast-emu

shim: $(SHIM_CWASM)
$(SHIM_CWASM): $(SHIM_SRC)
	$(LIND_COMPILE) $(SHIM_FLAGS) --output-dir $(LINDFS_SUBDIR) $(SHIM_SRC)

host: $(CDYLIB)
$(CDYLIB): FORCE
	cargo build --release --manifest-path $(STUB_DIR)/Cargo.toml

$(BUILD)/demo: demo.c $(CDYLIB) | $(BUILD)
	$(CC) demo.c -L$(CDYLIB_DIR) -lopenblas -lm -o $@

run: shim $(BUILD)/demo
	@echo "======== LIND (OpenBLAS preloaded behind resident shim) ========"
	@test -f "$(SHIM_CWASM)"     || { echo "shim not built: run 'make shim'"; exit 1; }
	@test -f "$(OPENBLAS_CWASM)" || { echo "OpenBLAS guest not found: $(OPENBLAS_CWASM) (build plain OpenBLAS, set OPENBLAS_CWASM)"; exit 1; }
	@LIND_MODULE=$(SHIM_CWASM) LIND_PRELOAD="$(PRELOAD)" LIND_ENABLE_FPCAST=$(FPCAST) \
	  LD_LIBRARY_PATH=$(CDYLIB_DIR) $(BUILD)/demo

$(BUILD):
	mkdir -p $@

clean:
	rm -rf $(BUILD) $(STUB_DIR)/target
	rm -f $(SHIM_SRC:.c=.wasm) $(SHIM_SRC:.c=.cwasm)
	rm -rf $(LINDFS_DIR)/$(LINDFS_SUBDIR)
