
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
	$(RM) -f results.json report.html
	$(RM) -r src/RawPOSIX/tmp/testfiles || true
	find tests -type f \( -name '*.wasm' -o -name '*.cwasm' -o -name '*.o' \) -delete

# wasmtime-tiny:
# - Uses Cargo profile env overrides (release) to optimize for size:
#     OPT_LEVEL=z      : smallest code size
#     LTO=fat          : link-time optimization across crates
#     CODEGEN_UNITS=1  : serial codegen, smaller binaries
#     PANIC=abort      : no unwinding code
#     DEBUG=false      : no debug info
#     STRIP=symbols    : cargo-native stripping (if supported)
#     CARGO_INCREMENTAL=0 : disables incremental artifacts
# - RUSTFLAGS:
#     -C debuginfo=0          : no DWARF info
#     -Wl,--gc-sections       : drop unused sections
#     -Wl,--as-needed         : don’t link unused libs
#     -fuse-ld=lld + --icf=all: if lld is installed, identical code folding
# - Features:
#     --no-default-features   : disables Wasmtime’s huge default set
#     -F run                  : keep only the `run` subcommand
#     -F winch                : enable Winch JIT (choose cranelift instead if preferred)
#     -F disable-logging      : compile out log statements
# - Strip step:
#     run system `strip` to remove leftover symbols
# - Optional UPX:
#     if UPX=1 is passed and `upx` is installed, compress the binary
# - Outputs final file size with `ls -lh`

.PHONY: wasmtime-tiny
wasmtime-tiny:
	CARGO_PROFILE_RELEASE_OPT_LEVEL=z \
	CARGO_PROFILE_RELEASE_LTO=fat \
	CARGO_PROFILE_RELEASE_CODEGEN_UNITS=1 \
	CARGO_PROFILE_RELEASE_PANIC=abort \
	CARGO_PROFILE_RELEASE_DEBUG=false \
	CARGO_PROFILE_RELEASE_STRIP=symbols \
	CARGO_INCREMENTAL=0 \
	RUSTFLAGS="$$( \
		FLAGS='-C debuginfo=0 -C link-arg=-Wl,--gc-sections -C link-arg=-Wl,--as-needed'; \
		if command -v ld.lld >/dev/null 2>&1 || command -v lld >/dev/null 2>&1; then \
			FLAGS="$$FLAGS -C link-arg=-fuse-ld=lld -C link-arg=-Wl,--icf=all"; \
		fi; \
		printf '%s' "$$FLAGS" \
	)" \
	cargo build --manifest-path src/wasmtime/Cargo.toml \
		--release --no-default-features \
		-F run -F winch -F disable-logging

	@if command -v strip >/dev/null 2>&1; then \
		strip --strip-unneeded target/release/wasmtime || true; \
	fi

	@if [ "$${UPX:-0}" = "1" ] && command -v upx >/dev/null 2>&1; then \
		upx -q --best --lzma target/release/wasmtime || true; \
	fi

	@ls -lh target/release/wasmtime || true
