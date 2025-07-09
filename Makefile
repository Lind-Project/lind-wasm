
.PHONY: all
all:
	@echo "Run targets individually!"

.PHONY: sysroot
sysroot:
	./scripts/make_glibc_and_sysroot.sh

.PHONY: wasmtime
wasmtime:
	cargo build --manifest-path src/wasmtime/Cargo.toml --release

.PHONY: test
test:
	# NOTE: `grep` workaround required for lack of meaningful exit code in wasmtestreport.py
	LIND_WASM_BASE=. LIND_FS_ROOT=src/RawPOSIX/tmp \
	./scripts/wasmtestreport.py && \
	cat results.json && \
	! grep '"number_of_failures": [^0]' results.json
