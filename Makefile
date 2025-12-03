LIND_ROOT ?= src/tmp

.PHONY: build 
build: sysroot wasmtime
	@echo "Build complete"

.PHONY: prepare-lind-root
prepare-lind-root:
	mkdir -p $(LIND_ROOT)/dev
	touch $(LIND_ROOT)/dev/null

.PHONY: all
all: build

.PHONY: sysroot
sysroot:
	./scripts/make_glibc_and_sysroot.sh

.PHONY: wasmtime
wasmtime:
	# Build wasmtime with `--release` flag for faster runtime (e.g. for tests)
	cargo build --manifest-path src/wasmtime/Cargo.toml --release

.PHONY: wasmtime-debug
wasmtime-debug:
	# Build wasmtime in debug mode for faster iteration in devcontainer
	cargo build --manifest-path src/wasmtime/Cargo.toml

.PHONY: test
test: prepare-lind-root
	# NOTE: `grep` workaround required for lack of meaningful exit code in wasmtestreport.py
	LIND_WASM_BASE=. LIND_ROOT=$(LIND_ROOT) \
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
	cargo clippy \
	    --manifest-path src/wasmtime/Cargo.toml \
	    --all-features \
	    --keep-going \
	    -- \
	    -A warnings \
	    -A clippy::not_unsafe_ptr_arg_deref \
	    -A clippy::absurd_extreme_comparisons

.PHONY: format
format:
	cargo fmt --all --manifest-path src/wasmtime/Cargo.toml
 

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
	@echo "cargo clean (wasmtime)"
	cargo clean --manifest-path src/wasmtime/Cargo.toml

.PHONY: distclean
distclean: clean
	@echo "removing test outputs & temp files"
	$(RM) -f results.json report.html e2e_status
	$(RM) -r $(LIND_ROOT)/testfiles || true
	find tests -type f \( -name '*.wasm' -o -name '*.cwasm' -o -name '*.o' \) -delete
