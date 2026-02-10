LINDFS_ROOT ?= lindfs
BUILD_DIR ?= build
SYSROOT_DIR ?= $(BUILD_DIR)/sysroot
LINDBOOT_BIN ?= $(BUILD_DIR)/lind-boot
LINDBOOT_DEBUG_BIN ?= $(BUILD_DIR)/lind-boot-debug

.PHONY: build 
build: sysroot lind-boot
	@echo "Build complete"

.PHONY: prepare-lind-root
prepare-lind-root:
	mkdir -p $(LINDFS_ROOT)/dev
	touch $(LINDFS_ROOT)/dev/null

.PHONY: all
all: build

.PHONY: sysroot
sysroot: build-dir
	./scripts/make_glibc_and_sysroot.sh
	$(MAKE) sync-sysroot

.PHONY: lind-boot
lind-boot: build-dir
	# Build lind-boot with `--release` flag for faster runtime (e.g. for tests)
	cargo build --manifest-path src/lind-boot/Cargo.toml --release
	cp src/lind-boot/target/release/lind-boot $(LINDBOOT_BIN)


.PHONY: lind-debug
lind-debug: build-dir
	# Build glibc with LIND_DEBUG enabled (by setting the LIND_DEBUG variable)
	$(MAKE) build_glibc LIND_DEBUG=1
	
	# Build lind-boot with the lind_debug feature enabled
	cargo build --manifest-path src/lind-boot/Cargo.toml --features lind_debug
	cp src/lind-boot/target/debug/lind-boot $(LINDBOOT_BIN)
build_glibc:
	# build sysroot passing -DLIND_DEBUG if LIND_DEBUG is set
	if [ "$(LIND_DEBUG)" = "1" ]; then \
		echo "Building glibc with LIND_DEBUG enabled"; \
		./scripts/make_glibc_and_sysroot.sh; \
		$(MAKE) sync-sysroot; \
	fi

.PHONY: build-dir
build-dir:
	mkdir -p $(BUILD_DIR)

.PHONY: sync-sysroot
sync-sysroot:
	$(RM) -r $(SYSROOT_DIR)
	cp -R src/glibc/sysroot $(SYSROOT_DIR)

.PHONY: test
test: prepare-lind-root
	# NOTE: `grep` workaround required for lack of meaningful exit code in wasmtestreport.py
	LIND_WASM_BASE=. LINDFS_ROOT=$(LINDFS_ROOT) \
	./scripts/wasmtestreport.py && \
	cat results.json; \
	if grep -q '"number_of_failures": [^0]' results.json; then \
	  echo "E2E_STATUS=fail" > e2e_status; \
	else \
	  echo "E2E_STATUS=pass" > e2e_status; \
	fi; \
	exit 0


.PHONY: md_generation
OUT ?= .
REPORT ?= report.html

md_generation:
	python3 -m pip install --quiet jinja2
	REPORT_PATH=$(REPORT) OUT_DIR=$(OUT) python3 scripts/render_e2e_templates.py
	@echo "Wrote $(OUT)/e2e_comment.md"

	

.PHONY: lint
lint:
	cargo fmt --check --all --manifest-path src/wasmtime/Cargo.toml
	cargo fmt --check --all --manifest-path src/lind-boot/Cargo.toml
	cargo clippy \
	    --manifest-path src/lind-boot/Cargo.toml \
	    --all-features \
	    --keep-going \
	    -- \
	    -A warnings \
	    -A clippy::not_unsafe_ptr_arg_deref \
	    -A clippy::absurd_extreme_comparisons

.PHONY: format
format:
	cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
	cargo fmt --all --manifest-path src/lind-boot/Cargo.toml
 

.PHONY: docs-serve
docs-serve:
	mkdocs serve

.PHONY: clean
clean:
	@echo "cleaning glibc artifacts"
	# Remove only generated sysroot and intermediate .o files,
	# but KEEP required objects used by subsequent builds.
	$(RM) -r src/glibc/sysroot
	@find src/glibc -type f -name '*.o' \
	    ! -path 'src/glibc/csu/wasm32/wasi_thread_start.o' \
	    ! -path 'src/glibc/target/lib/Mcrt1.o' \
	    ! -path 'src/glibc/target/lib/Scrt1.o' \
	    ! -path 'src/glibc/target/lib/crt1.o' \
	    ! -path 'src/glibc/target/lib/crti.o' \
	    ! -path 'src/glibc/target/lib/crtn.o' \
	    ! -path 'src/glibc/target/lib/gcrt1.o' \
	    ! -path 'src/glibc/target/lib/grcrt1.o' \
	    ! -path 'src/glibc/target/lib/rcrt1.o' \
	    -exec rm -f {} +
	@echo "cargo clean (lind-boot)"
	cargo clean --manifest-path src/lind-boot/Cargo.toml

.PHONY: distclean
distclean: clean
	@echo "removing test outputs & temp files"
	$(RM) -f results.json report.html e2e_status
	$(RM) -r $(LINDFS_ROOT)/testfiles || true
	find tests -type f \( -name '*.wasm' -o -name '*.cwasm' -o -name '*.o' \) -delete
