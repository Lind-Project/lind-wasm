LINDFS_ROOT ?= lindfs
BUILD_DIR ?= build
SYSROOT_DIR ?= $(BUILD_DIR)/sysroot
# Prebuilt libc++ headers/libs; merged into $(SYSROOT_DIR) by sync-sysroot when present.
ARTIFACTS_DIR ?= artifacts
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

# fdtables backend selector. One of: dashmaparray (default), dashmapvec,
# muthashmax, vanilla. Threaded through lind-boot -> rawposix -> fdtables
# via Cargo features.
#
# Examples:
#   make                                  # default (dashmaparray)
#   make build FDTABLES_IMPL=muthashmax   # full build with muthashmax
#   make lind-boot FDTABLES_IMPL=vanilla  # rebuild only lind-boot with vanilla
#   make fpcast FDTABLES_IMPL=dashmapvec  # fpcast variant with dashmapvec
#   make lind-debug FDTABLES_IMPL=muthashmax   # debug build with muthashmax
#   FDTABLES_IMPL=muthashmax make         # also works via env var
FDTABLES_IMPL ?= dashmaparray

.PHONY: lind-boot
lind-boot: build-dir
	# Build lind-boot with `--release` flag for faster runtime (e.g. for tests)
	cargo build --manifest-path src/lind-boot/Cargo.toml --release \
		--no-default-features --features fdtables-$(FDTABLES_IMPL)
	cp src/lind-boot/target/release/lind-boot $(LINDBOOT_BIN)

.PHONY: lind-boot-debug
lind-boot-debug: build-dir
	# Build lind-boot in debug mode for development/debugging.
	cargo build --manifest-path src/lind-boot/Cargo.toml \
		--no-default-features --features fdtables-$(FDTABLES_IMPL)
	cp src/lind-boot/target/debug/lind-boot $(LINDBOOT_BIN)

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
	cargo build --manifest-path src/lind-boot/Cargo.toml \
	    --no-default-features --features "lind_debug fdtables-$(FDTABLES_IMPL)"
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

# After copying src/glibc/sysroot → $(SYSROOT_DIR), optionally merge libc++ from
# $(ARTIFACTS_DIR) when that tree is present (same layout as Docker E2E).
# Remove any existing c++ tree first: it may be a symlink into src/glibc/sysroot
# (e.g. read-only in Docker), and cp -r into that path would hit EROFS.

.PHONY: sync-sysroot
sync-sysroot:
	$(RM) -r $(SYSROOT_DIR)
	cp -R src/glibc/sysroot $(SYSROOT_DIR)
	@if [ -d "$(ARTIFACTS_DIR)/include/wasm32-wasi/c++" ] \
	    && [ -f "$(ARTIFACTS_DIR)/lib/wasm32-wasi/libc++.a" ] \
	    && [ -f "$(ARTIFACTS_DIR)/lib/wasm32-wasi/libc++abi.a" ]; then \
	  echo "Merging libc++ from $(ARTIFACTS_DIR) into $(SYSROOT_DIR)"; \
	  mkdir -p $(SYSROOT_DIR)/include/wasm32-wasi; \
	  mkdir -p $(SYSROOT_DIR)/lib/wasm32-wasi; \
	  $(RM) -r $(SYSROOT_DIR)/include/wasm32-wasi/c++; \
	  cp -r $(ARTIFACTS_DIR)/include/wasm32-wasi/c++ $(SYSROOT_DIR)/include/wasm32-wasi/; \
	  $(RM) -f $(SYSROOT_DIR)/lib/wasm32-wasi/libc++.a $(SYSROOT_DIR)/lib/wasm32-wasi/libc++abi.a; \
	  cp $(ARTIFACTS_DIR)/lib/wasm32-wasi/libc++.a $(ARTIFACTS_DIR)/lib/wasm32-wasi/libc++abi.a $(SYSROOT_DIR)/lib/wasm32-wasi/; \
	fi

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
# Single-grate examples:
#   make test-grate GRATE=ipc-grate
#   make test-grate GRATE=chroot-grate GRATE_ARGS="--chroot-dir /tmp"
#   make test-grate GRATE=ipc-grate RUN=process_tests
#   make test-grate GRATE=ipc-grate TESTFILES=tests/unit-tests/process_tests/deterministic/hello.c
# Grate chain (mutually exclusive with GRATE / GRATE_ARGS — the string is passed
# verbatim before the test wasm, so any grate-side group syntax goes here too):
#   make test-grate GRATE_PREFIX="grates/fs-routing-clamp.cwasm --prefix /tmp grates/imfs-grate.cwasm"
# Build the grate(s) first:  cd ../lind-wasm-example-grates && make rust/<name>
# Run timeout defaults to 90s under any grate (vs 30s without), override with TIMEOUT=N.
.PHONY: test-grate
GRATE ?=
GRATE_ARGS ?=
GRATE_PREFIX ?=
TESTFILES ?=
RUN ?=
TIMEOUT ?=
test-grate:
	@if [ -z "$(GRATE)" ] && [ -z "$(GRATE_PREFIX)" ]; then \
		echo "Usage: make test-grate GRATE=<name> [GRATE_ARGS='...'] [RUN=folder | TESTFILES=path/to/test.c]"; \
		echo "   or: make test-grate GRATE_PREFIX='<raw chain>' [RUN=...]"; \
		echo ""; \
		echo "Examples:"; \
		echo "  make test-grate GRATE=ipc-grate"; \
		echo "  make test-grate GRATE=chroot-grate GRATE_ARGS=\"--chroot-dir /tmp\""; \
		echo "  make test-grate GRATE=ipc-grate RUN=process_tests"; \
		echo "  make test-grate GRATE=ipc-grate TESTFILES=tests/unit-tests/process_tests/deterministic/hello.c"; \
		echo "  make test-grate GRATE_PREFIX=\"grates/fs-routing-clamp.cwasm --prefix /tmp grates/imfs-grate.cwasm\""; \
		echo ""; \
		echo "Build the grate(s) first:  cd ../lind-wasm-example-grates && make rust/<name>"; \
		exit 1; \
	fi
	@if [ -n "$(GRATE)" ] && [ -n "$(GRATE_PREFIX)" ]; then \
		echo "GRATE and GRATE_PREFIX are mutually exclusive"; exit 1; \
	fi
	LIND_WASM_BASE=. LINDFS_ROOT=$(LINDFS_ROOT) \
	python3 ./scripts/harnesses/wasmtestreport.py \
		--allow-pre-compiled \
		$(if $(GRATE),--grate grates/$(GRATE).cwasm) \
		$(if $(GRATE_ARGS),--grate-args "$(GRATE_ARGS)") \
		$(if $(GRATE_PREFIX),--grate-prefix "$(GRATE_PREFIX)") \
		$(if $(TIMEOUT),--timeout $(TIMEOUT)) \
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
	rustfmt --check src/fdtables/src/dashmaparrayglobal.rs \
	    src/fdtables/src/dashmapvecglobal.rs \
	    src/fdtables/src/muthashmaxglobal.rs \
	    src/fdtables/src/vanillaglobal.rs
	# Note: --all-features can't be used here because it enables every
	# fdtables-* impl simultaneously, which trips the fdtables impl-mutex
	# compile_error guard. Enumerate the non-fdtables features explicitly
	# and pin to the default fdtables impl.
	cargo clippy \
	    --manifest-path src/lind-boot/Cargo.toml \
	    --features "disable_signals secure lind_debug debug-dylink debug-grate-calls fdtables-dashmaparray" \
	    --keep-going \
	    -- \
	    -A warnings \
	    -A clippy::not_unsafe_ptr_arg_deref \
	    -A clippy::absurd_extreme_comparisons

.PHONY: format
format:
	cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
	cargo fmt --all --manifest-path src/lind-boot/Cargo.toml
	rustfmt src/fdtables/src/dashmaparrayglobal.rs \
	    src/fdtables/src/dashmapvecglobal.rs \
	    src/fdtables/src/muthashmaxglobal.rs \
	    src/fdtables/src/vanillaglobal.rs
 

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
