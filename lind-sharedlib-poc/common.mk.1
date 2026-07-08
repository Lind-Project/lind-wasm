# Shared build engine for the sandboxed-library examples.
#
# An example Makefile sets `LIB` and `EXAMPLE_DIR`, then `include`s this file:
#
#     LIB         := add_sub
#     EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
#     include ../../common.mk
#
# Every example has the same shape on disk:
#     guest.c        the library source        -> guest.cwasm  (runs in the sandbox)
#     demo.c         unmodified native caller
#     functions.txt  stub manifest             -> stub/src/lib.rs (via `make gen`)
#     stub/          the cdylib crate          -> lib$(LIB).so
#
# Targets: native (baseline) | lind (sandboxed) | gen | guest | host | clean | help.

COMMON_MK := $(abspath $(lastword $(MAKEFILE_LIST)))
POC_DIR   := $(patsubst %/,%,$(dir $(COMMON_MK)))
REPO_ROOT := $(abspath $(POC_DIR)/..)

CC           ?= cc
LIND_COMPILE := $(REPO_ROOT)/scripts/lind_compile
LIND_RUN     := $(REPO_ROOT)/scripts/lind_run
GEN          := $(POC_DIR)/tools/gen_stubs.sh

# Per-example sources (fixed filenames).
GUEST_SRC     := $(EXAMPLE_DIR)/guest.c
DEMO_SRC      := $(EXAMPLE_DIR)/demo.c
FUNCS         := $(EXAMPLE_DIR)/functions.txt
STUB_DIR      := $(EXAMPLE_DIR)/stub
STUB_LIB      := $(STUB_DIR)/src/lib.rs
STUB_MANIFEST := $(STUB_DIR)/Cargo.toml
CDYLIB_DIR    := $(STUB_DIR)/target/release
CDYLIB        := $(CDYLIB_DIR)/lib$(LIB).so

# Guest module. lind_compile (full mode) writes .wasm + .cwasm next to the source
# AND copies the .cwasm into lindfs/, because lind can only locate modules inside
# lindfs/ (lind_run chroots into it). `--output-dir` puts each example's module in
# its own lindfs subdir so the per-example `guest.cwasm` names don't collide.
# NOTE: no inline comments on these value lines — Make would keep the whitespace
# before the `#` as part of the path.
GUEST_WASM      := $(EXAMPLE_DIR)/guest.wasm
# .cwasm written next to the source:
GUEST_CWASM_SRC := $(EXAMPLE_DIR)/guest.cwasm
LINDFS_DIR      := $(REPO_ROOT)/lindfs
# per-example subdir under lindfs/ (avoids guest.cwasm name collisions):
LINDFS_SUBDIR   := sharedlib-poc/$(LIB)
# host path to the lindfs copy (what LIND_MODULE / the .so reads):
GUEST_MODULE    := $(LINDFS_DIR)/$(LINDFS_SUBDIR)/guest.cwasm
# path as lind_run sees it after chrooting into lindfs/:
GUEST_LINDPATH  := $(LINDFS_SUBDIR)/guest.cwasm

# Build outputs.
BUILD      := $(EXAMPLE_DIR)/build
NATIVE_DIR := $(BUILD)/native
LIND_DIR   := $(BUILD)/lind

.DEFAULT_GOAL := build
.PHONY: build run run-native compare gen guest host clean help FORCE

# `make` (default) only BUILDS — it produces every artifact but runs nothing.
# This matters because build and run can happen on different machines: the guest
# module + cdylib build anywhere, but *running* needs the full Linux lind runtime.
# Use `make run` / `make run-native` to execute.
build: $(NATIVE_DIR)/demo $(LIND_DIR)/demo $(GUEST_MODULE)
	@echo "built native + sandboxed demos — run with 'make run' (or 'make run-native')"

# Regenerate the extern "C" stubs from functions.txt. 
gen:
	$(GEN) $(FUNCS) > $(STUB_LIB)
	@echo "generated $(STUB_LIB)"

# --------------------------------------------------------------------------
# Native baseline: compile guest.c as an ordinary shared library and link the
# unmodified demo against it. The control case — no sandbox.
# --------------------------------------------------------------------------
run-native: $(NATIVE_DIR)/demo
	@echo "=================== NATIVE (baseline) ==================="
	@LD_LIBRARY_PATH=$(NATIVE_DIR) $(NATIVE_DIR)/demo

# -w silences the wasm-only `export_name` attribute warning on native targets.
$(NATIVE_DIR)/lib$(LIB).so: $(GUEST_SRC) | $(NATIVE_DIR)
	$(CC) -shared -fPIC -w -o $@ $(GUEST_SRC)

$(NATIVE_DIR)/demo: $(DEMO_SRC) $(NATIVE_DIR)/lib$(LIB).so | $(NATIVE_DIR)
	$(CC) $(DEMO_SRC) -L$(NATIVE_DIR) -l$(LIB) -o $@

# --------------------------------------------------------------------------
# Sandboxed path: compile guest.c to wasm, build the wasm-backed cdylib, and link
# the SAME demo against it. The guest functions run inside the lind/wasmtime cage.
# --------------------------------------------------------------------------
run: $(LIND_DIR)/demo $(GUEST_MODULE)
	@echo "================ LIND (wasm-sandboxed) ================="
	@LIND_MODULE=$(GUEST_MODULE) LD_LIBRARY_PATH=$(CDYLIB_DIR) $(LIND_DIR)/demo

# Run both, back to back, for comparison.
compare: run-native run

# Compile the guest and land it in its lindfs subdir. lind_compile writes the
# .cwasm next to the source and copies it into lindfs/$(LINDFS_SUBDIR)/.
guest: $(GUEST_MODULE)
$(GUEST_MODULE): $(GUEST_SRC)
	$(LIND_COMPILE) --output-dir $(LINDFS_SUBDIR) $(GUEST_SRC)

# Host shim .so (embeds wasmtime + lind). FORCE-built so cargo — not make —
# decides what needs rebuilding across the whole lind-boot dependency graph.
host: $(CDYLIB)
$(CDYLIB): FORCE
	cargo build --release --manifest-path $(STUB_MANIFEST)

$(LIND_DIR)/demo: $(DEMO_SRC) $(CDYLIB) | $(LIND_DIR)
	$(CC) $(DEMO_SRC) -L$(CDYLIB_DIR) -l$(LIB) -o $@

$(NATIVE_DIR) $(LIND_DIR):
	mkdir -p $@

clean:
	rm -rf $(BUILD)
	rm -f $(GUEST_WASM) $(GUEST_CWASM_SRC)
	rm -rf $(LINDFS_DIR)/$(LINDFS_SUBDIR)

help:
	@echo "make            - build everything (runs nothing)"
	@echo "make run        - run the demo against the wasm-sandboxed lib$(LIB).so"
	@echo "make run-native - run the demo against a real native lib$(LIB).so (baseline)"
	@echo "make compare    - run native then sandboxed, back to back"
	@echo "make gen        - regenerate stub/src/lib.rs from functions.txt"
	@echo "make guest      - compile guest.c -> guest.cwasm only"
	@echo "make host       - build the cdylib (lib$(LIB).so) only"
	@echo "make clean      - remove build artifacts"
