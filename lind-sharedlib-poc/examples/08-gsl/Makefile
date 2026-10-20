# Example 08 — GSL (GNU Scientific Library) as a sandboxed native .so.
#
# Same loading model as 06-openblas (Option A): GSL is a big prebuilt library that
# dispatches to its own code through internal function pointers, so it can't be the MAIN
# module — it's loaded as a PRELOAD behind a small shim.
#
#   MAIN module = gsl_lind_shim.c — exports guest_malloc/guest_free + lind_gsl_* forwarders.
#   PRELOAD     = plain GSL (built by lind-wasm-apps/gsl-2.8/compile_gsl.sh) + libc/libm.
#
#   make            build shim + stub .so + demo
#   make shim       build only the resident main-module shim (-> lindfs)
#   make host       build only the stub cdylib (libgsl.so)
#   make run        run the demo (shim main + GSL preload)
#   make clean      remove build artifacts

EXAMPLE_DIR := $(abspath $(dir $(lastword $(MAKEFILE_LIST))))
REPO_ROOT   := $(abspath $(EXAMPLE_DIR)/../../..)
LINDFS_DIR  := $(REPO_ROOT)/lindfs
LIND_COMPILE := $(REPO_ROOT)/scripts/lind_compile

CC          ?= cc
STUB_DIR    := $(EXAMPLE_DIR)/stub
CDYLIB_DIR  := $(STUB_DIR)/target/release
CDYLIB      := $(CDYLIB_DIR)/libgsl.so
BUILD       := $(EXAMPLE_DIR)/build

# Resident MAIN module: the shim. Its gsl_*/malloc/free calls become dynamic imports
# (default lind_compile is a dynamic build), resolved from the preloads at runtime.
LINDFS_SUBDIR := sharedlib-poc/gsl
SHIM_SRC    := $(EXAMPLE_DIR)/gsl_lind_shim.c
SHIM_CWASM  := $(LINDFS_DIR)/$(LINDFS_SUBDIR)/gsl_lind_shim.cwasm

# Plain GSL wasm library (built by compile_gsl.sh), loaded as a PRELOAD.
GSL_CWASM ?= $(LINDFS_DIR)/lib/libgsl.so

# Preload set (host paths — the .so isn't chrooted): libc, libm, then GSL.
PRELOAD ?= env=$(LINDFS_DIR)/lib/libc.cwasm,env=$(LINDFS_DIR)/lib/libm.cwasm,env=$(GSL_CWASM)

# Engine fpcast OFF: GSL's fpcast is baked in at compile time (same as OpenBLAS), so the
# engine must not double-apply it. The shim is built --fpcast-emu to match the guest.
FPCAST      ?= 0
SHIM_FLAGS  ?= --fpcast-emu

.DEFAULT_GOAL := build
.PHONY: build shim host demo run clean FORCE

build: shim host demo
	@echo "built shim + stub + demo — run with 'make run'"

shim: $(SHIM_CWASM)
$(SHIM_CWASM): $(SHIM_SRC)
	$(LIND_COMPILE) $(SHIM_FLAGS) --output-dir $(LINDFS_SUBDIR) $(SHIM_SRC)

host: $(CDYLIB)
$(CDYLIB): FORCE
	cargo build --release --manifest-path $(STUB_DIR)/Cargo.toml

demo: $(BUILD)/demo
$(BUILD)/demo: demo.c $(CDYLIB) | $(BUILD)
	$(CC) demo.c -L$(CDYLIB_DIR) -lgsl -o $@

run: $(BUILD)/demo $(SHIM_CWASM)
	@echo "================ GSL (wasm-sandboxed) ================="
	@LIND_MODULE=$(SHIM_CWASM) LIND_PRELOAD="$(PRELOAD)" LIND_ENABLE_FPCAST=$(FPCAST) \
		LD_LIBRARY_PATH=$(CDYLIB_DIR) $(BUILD)/demo

$(BUILD):
	mkdir -p $@

clean:
	rm -rf $(BUILD) $(STUB_DIR)/target
	rm -f $(SHIM_SRC:.c=.wasm) $(SHIM_SRC:.c=.cwasm)
	rm -rf $(LINDFS_DIR)/$(LINDFS_SUBDIR)
