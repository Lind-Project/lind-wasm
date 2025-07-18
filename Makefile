
.PHONY: build 
build: sysroot wasmtime
	@echo "Build complete"

.PHONY: all
all: build

.PHONY: sysroot
sysroot:
	./scripts/make_glibc_and_sysroot.sh

.PHONY: wasmtime
wasmtime:
	# Build wasmtime with `--release` flag for faster runtime (e.g. for tests)
	cargo build --manifest-path src/wasmtime/Cargo.toml --release

.PHONY: test
test:
	# NOTE: `grep` workaround required for lack of meaningful exit code in wasmtestreport.py
	LIND_WASM_BASE=. LIND_FS_ROOT=src/RawPOSIX/tmp \
	./scripts/wasmtestreport.py && \
	cat results.json && \
	! grep '"number_of_failures": [^0]' results.json

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
	@echo "Removing glibc artifacts"
	$(RM) -r src/glibc/build src/glibc/sysroot src/glibc/target

	@echo "cargo clean (wasmtime)"
	cargo clean --manifest-path src/wasmtime/Cargo.toml

	@echo "Deleting test outputs"
	$(RM) -f results.json report.html
	$(RM) -r src/RawPOSIX/tmp/testfiles || true

	@echo "Purging compiled test artifacts"
	find tests -type f \( -name '*.wasm' -o -name '*.cwasm' -o -name '*.o' \) -delete

	@echo "Clean done."
