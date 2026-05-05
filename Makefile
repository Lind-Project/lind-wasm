LINDFS_ROOT ?= lindfs
BUILD_DIR ?= build
SYSROOT_DIR ?= $(BUILD_DIR)/sysroot
LINDBOOT_BIN ?= $(BUILD_DIR)/lind-boot
LINDBOOT_DEBUG_BIN ?= $(BUILD_DIR)/lind-boot-debug
LINDFS_DIRS := \
	       bin \
	       dev \
	       etc \
	       grates \
	       lib \
	       sbin \
	       tmp \
	       usr/bin \
	       usr/lib \
	       usr/lib/locale \
	       usr/local/bin \
	       usr/share/zoneinfo \
	       var \
	       var/log \
	       var/run

WITH_FPCAST ?=

.PHONY: build 
build: lindfs lind-boot sysroot
	@echo "Build complete"

.PHONY: all
all: build

.PHONY: fpcast
fpcast:
	$(MAKE) build WITH_FPCAST=1

.PHONY: sysroot
sysroot: build-dir
	./scripts/make_glibc_and_sysroot.sh $(if $(WITH_FPCAST),--with-fpcast)
	$(MAKE) sync-sysroot

.PHONY: lind-boot
lind-boot: build-dir
	# Build lind-boot with `--release` flag for faster runtime (e.g. for tests)
	cargo build --manifest-path src/lind-boot/Cargo.toml --release
	cp src/lind-boot/target/release/lind-boot $(LINDBOOT_BIN)

.PHONY: lindfs
lindfs:
	@for d in $(LINDFS_DIRS); do \
		mkdir -p $(LINDFS_ROOT)/$$d; \
	done
	touch $(LINDFS_ROOT)/dev/null
	cp -rT scripts/lindfs-conf/etc $(LINDFS_ROOT)/etc
	cp -rT scripts/lindfs-conf/usr/lib/locale $(LINDFS_ROOT)/usr/lib/locale
	cp -rT scripts/lindfs-conf/usr/share/zoneinfo $(LINDFS_ROOT)/usr/share/zoneinfo
	@if [ -d /usr/share/zoneinfo ]; then \
		cp -r /usr/share/zoneinfo/* $(LINDFS_ROOT)/usr/share/zoneinfo/; \
	fi

.PHONY: clean-lindfs
clean-lindfs:
	@# Remove user files from lindfs while preserving preloaded system files.
	@# Keeps: lib/ (shared libs), etc/, usr/, dev/null, directory structure
	find $(LINDFS_ROOT) -maxdepth 1 -type f -delete
	rm -rf $(LINDFS_ROOT)/bin/* $(LINDFS_ROOT)/sbin/* $(LINDFS_ROOT)/tmp/*
	rm -rf $(LINDFS_ROOT)/home $(LINDFS_ROOT)/testfiles

.PHONY: lind-debug
lind-debug: lindfs build-dir
	# Build lind-boot with the lind_debug feature enabled
	cargo build --manifest-path src/lind-boot/Cargo.toml --features lind_debug
	cp src/lind-boot/target/debug/lind-boot $(LINDBOOT_BIN)

	# Build glibc with LIND_DEBUG enabled (by setting the LIND_DEBUG variable)
	$(MAKE) build_glibc LIND_DEBUG=1
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
test: lindfs
	# Unified harness entry point (run all discovered harnesses for e2e signal)
	alias_path='$(LIND_RUNTIME_LINDFS_ALIAS)'; \
	prebuilt_lindfs_root='$(PREBUILT_LINDFS_ROOT)'; \
	cleanup() { \
	  if [ -n "$$alias_path" ]; then \
	    alias_parent="$$(dirname "$$alias_path")"; \
	    rm -f "$$alias_path"; \
	    rmdir "$$alias_parent" 2>/dev/null || true; \
	    rmdir "$$(dirname "$$alias_parent")" 2>/dev/null || true; \
	  fi; \
	}; \
	if [ -n "$$alias_path" ]; then \
	  mkdir -p "$$(dirname "$$alias_path")"; \
	  rm -f "$$alias_path"; \
	  case '$(LINDFS_ROOT)' in \
	    /*) lindfs_root='$(LINDFS_ROOT)' ;; \
	    *) lindfs_root="$$PWD/$(LINDFS_ROOT)" ;; \
	  esac; \
	  ln -s "$$lindfs_root" "$$alias_path"; \
	  trap cleanup EXIT; \
	fi; \
	if [ -n "$$prebuilt_lindfs_root" ] && [ -d "$$prebuilt_lindfs_root/lib" ]; then \
	  mkdir -p "$(LINDFS_ROOT)/lib"; \
	  cp -a "$$prebuilt_lindfs_root/lib/." "$(LINDFS_ROOT)/lib/"; \
	fi; \
	if LIND_WASM_BASE=. LINDFS_ROOT=$(LINDFS_ROOT) \
	python3 ./scripts/test_runner.py --export-report report.html && \
	find reports -maxdepth 1 -name '*.json' -print -exec cat {} \; && \
	if [ "$(LIND_DEBUG)" = "1" ]; then \
	  python3 ./scripts/check_reports.py --debug; \
	else \
	  python3 ./scripts/check_reports.py; \
	fi; then \
	  echo "E2E_STATUS=pass" > e2e_status; \
	else \
	  echo "E2E_STATUS=fail" > e2e_status; \
	  mkdir -p reports; \
	  if [ ! -f report.html ]; then \
	    printf '%s\n' '<!DOCTYPE html><html><body><h1>E2E failed before report generation</h1></body></html>' > report.html; \
	  fi; \
	  if [ ! -f reports/report.html ]; then cp report.html reports/report.html; fi; \
	  if [ ! -f reports/wasm.json ]; then printf '%s\n' '{"number_of_failures":1,"results":[],"error":"missing wasm report"}' > reports/wasm.json; fi; \
	  if [ ! -f reports/grates.json ]; then printf '%s\n' '{"number_of_failures":1,"results":[],"error":"missing grate report"}' > reports/grates.json; fi; \
	fi; \
	exit 0

# Run wasmtestreport with a grate prefix.
# Examples:
#   make test-grate GRATE=ipc-grate
#   make test-grate GRATE=ipc-grate RUN=process_tests
#   make test-grate GRATE=ipc-grate TESTFILES=tests/unit-tests/process_tests/deterministic/hello.c
# Build the grate first:  cd ../lind-wasm-example-grates && make rust/<name>
.PHONY: test-grate
GRATE ?=
TESTFILES ?=
RUN ?=
test-grate:
	@if [ -z "$(GRATE)" ]; then \
		echo "Usage: make test-grate GRATE=<name> [RUN=folder | TESTFILES=path/to/test.c]"; \
		echo ""; \
		echo "Examples:"; \
		echo "  make test-grate GRATE=ipc-grate"; \
		echo "  make test-grate GRATE=ipc-grate RUN=process_tests"; \
		echo "  make test-grate GRATE=ipc-grate TESTFILES=tests/unit-tests/process_tests/deterministic/hello.c"; \
		echo ""; \
		echo "Build the grate first:  cd ../lind-wasm-example-grates && make rust/<name>"; \
		exit 1; \
	fi
	LIND_WASM_BASE=. LINDFS_ROOT=$(LINDFS_ROOT) \
	python3 ./scripts/harnesses/wasmtestreport.py \
		--grate grates/$(GRATE).cwasm \
		--allow-pre-compiled \
		$(if $(TESTFILES),--testfiles $(TESTFILES)) \
		$(if $(RUN),--run $(RUN))

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
	rustfmt --check src/fdtables/src/muthashmaxglobal.rs \
	    src/fdtables/src/vanillaglobal.rs \
	    src/fdtables/src/dashmapvecglobal.rs
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
	rustfmt src/fdtables/src/muthashmaxglobal.rs \
	    src/fdtables/src/vanillaglobal.rs \
	    src/fdtables/src/dashmapvecglobal.rs
 

.PHONY: docs-serve
docs-serve:
	mkdocs serve

.PHONY: clean
clean:
	@echo "cleaning glibc artifacts"
	# Remove only generated sysroot and intermediate .o files,
	# but KEEP required objects used by subsequent builds.
	$(RM) -r src/glibc/sysroot
	@echo "removing build artifacts"
	$(RM) -r $(BUILD_DIR)
	@echo "removing lindfs root"
	$(RM) -r $(LINDFS_ROOT)
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
	$(RM) -r reports || true
	$(RM) -r $(LINDFS_ROOT)/testfiles || true
	find tests -type f \( -name '*.wasm' -o -name '*.cwasm' -o -name '*.o' \) -delete
