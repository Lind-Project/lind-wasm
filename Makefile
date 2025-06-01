# Base directories
GLIBC_BASE := /home/lind-wasm/src/glibc
WASMTIME_BASE := /home/lind-wasm/src/wasmtime
RUSTPOSIX_BASE := /home/lind-wasm/src/safeposix-rust
RAWPOSIX_BASE := /home/lind-wasm/src/RawPOSIX

# Compiler settings
CLANG ?= /home/lind-wasm/clang+llvm-18.1.8-x86_64-linux-gnu-ubuntu-18.04
CC := $(CLANG)/bin/clang

# Color definitions
RED := \033[31m
GREEN := \033[32m
RESET := \033[0m

# Export command (for reference)
EXPORT_CMD := export LD_LIBRARY_PATH=$(WASMTIME_BASE)/crates/rustposix:$$LD_LIBRARY_PATH

# Compilation flags
WASM_FLAGS := --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot
PTHREAD_FLAGS := -pthread $(WASM_FLAGS) -Wl,--import-memory,--export-memory,--max-memory=67108864
STACK_EXPORT := -Wl,--export="__stack_pointer"
DEBUG_FLAGS := -g -O0

# Commands
WASMTIME_RUN := $(WASMTIME_BASE)/target/debug/wasmtime run
WASMTIME_COMPILE := $(WASMTIME_BASE)/target/debug/wasmtime compile

# Preview mode flag
PMODE ?= 0

.PHONY: all help export compile_test run debug compile_wasmtime compile_rustposix compile_rawposix compile_src make_all

# Default target
all: help

# Help target
help:
	@echo "Available commands:"
	@echo "1. export"
	@echo "2. compile_test (cptest)"
	@echo "3. run"
	@echo "4. compile_wasmtime (cpwasm)"
	@echo "5. compile_rustposix (cpposix)"
	@echo "6. compile_src (cpsrc)"
	@echo "7. make_all"
	@echo "8. make"

# Export command (informational only)
export:
	@echo -e "$(GREEN)$(EXPORT_CMD)$(RESET)"
	@echo "please manually run the export command"

# Compile test files
compile_test cptest: check_source
	@echo -e "$(GREEN)Compiling $(SOURCE).c$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		$(CC) $(PTHREAD_FLAGS) $(STACK_EXPORT) $(SOURCE).c $(DEBUG_FLAGS) -o $(OUTPUT).wasm && \
		wasm-opt --asyncify --debuginfo $(OUTPUT).wasm -o $(OUTPUT).wasm && \
		$(WASMTIME_COMPILE) $(OUTPUT).wasm -o $(OUTPUT).cwasm, \
		@echo "Preview mode: would compile $(SOURCE).c")

# Run compiled files
run: check_source
	@if [ -f "$(SOURCE).cwasm" ]; then \
		echo -e "$(GREEN)Running $(SOURCE).cwasm$(RESET)"; \
		$(if $(filter 0,$(PMODE)), \
			$(WASMTIME_RUN) --allow-precompiled --wasi threads=y --wasi preview2=n $(SOURCE).cwasm $(ARGS), \
			@echo "Preview mode: would run $(SOURCE).cwasm"); \
	else \
		echo -e "$(GREEN)Running $(SOURCE).wasm$(RESET)"; \
		$(if $(filter 0,$(PMODE)), \
			$(WASMTIME_RUN) --wasi threads=y --wasi preview2=n $(SOURCE).wasm $(ARGS), \
			@echo "Preview mode: would run $(SOURCE).wasm"); \
	fi

# Debug target
debug: check_source
	@echo -e "$(GREEN)Debugging $(SOURCE)$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		gdb --args $(WASMTIME_RUN) -D debug-info -O opt-level=0 --wasi threads=y --wasi preview2=n $(SOURCE) $(ARGS), \
		@echo "Preview mode: would debug $(SOURCE)")

# Compile Wasmtime
compile_wasmtime cpwasm:
	@echo -e "$(GREEN)Compiling Wasmtime$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		cd $(WASMTIME_BASE) && cargo build, \
		@echo "Preview mode: would compile Wasmtime")

# Compile RustPOSIX
compile_rustposix cpposix:
	@echo -e "$(GREEN)Compiling RustPOSIX$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		cd $(RUSTPOSIX_BASE) && cargo build && \
		cp $(RUSTPOSIX_BASE)/target/debug/librustposix.so $(WASMTIME_BASE)/crates/rustposix, \
		@echo "Preview mode: would compile RustPOSIX")

# Compile RawPOSIX
compile_rawposix cpraw:
	@echo -e "$(GREEN)Compiling RawPOSIX$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		cd $(RAWPOSIX_BASE) && cargo build && \
		cp $(RAWPOSIX_BASE)/target/debug/librustposix.so $(WASMTIME_BASE)/crates/rustposix, \
		@echo "Preview mode: would compile RawPOSIX")

# Compile specific source
compile_src cpsrc:
	@if [ -z "$(SOURCE)" ]; then \
		echo -e "$(RED)error: source file name not provided$(RESET)"; \
		exit 1; \
	fi
	@cd $(GLIBC_BASE)/build && \
	ESCAPED_TARGET=$$(echo "$(SOURCE)" | sed 's/[.[*+?^$${}()|\\]/\\&/g') && \
	RESULTS=$$(cat check.log | grep -E "[ /]$$ESCAPED_TARGET" | grep -E '\-o[[:space:]]+[^[:space:]]+\.o[[:space:]]' | grep '\-\-target=wasm32\-unkown\-wasi') && \
	if [ -z "$$RESULTS" ]; then \
		echo -e "$(RED)error: compile command not found$(RESET)"; \
		exit 1; \
	fi && \
	cd $(GLIBC_BASE)/$(dir $(SOURCE)) && \
	echo "$$RESULTS" | while read -r line; do \
		if [ "$(OPT)" = "-O0" ]; then \
			line=$$(echo "$$line" | sed "s| -O2 | -O0 |"); \
		fi; \
		echo -e "$(GREEN)$$line$(RESET)"; \
		$(if $(filter 0,$(PMODE)), eval "$$line"); \
	done && \
	cd $(GLIBC_BASE) && ./gen_sysroot.sh > /dev/null

# Make all target
make_all:
	@echo -e "$(GREEN)Building everything$(RESET)"
	$(if $(filter 0,$(PMODE)), \
		unset LD_LIBRARY_PATH && \
		cd $(GLIBC_BASE) && rm -rf build && ./wasm-config.sh && \
		cd build && make -j8 --keep-going THREAD_MODEL=posix 2>&1 | tee check.log && \
		cd ../nptl && $(CC) --target=wasm32-unknown-wasi -v -Wno-int-conversion \
			pthread_create.c -c -std=gnu11 -fgnu89-inline -matomics -mbulk-memory \
			-O0 -g -Wall -Wwrite-strings -Wundef -fmerge-all-constants \
			-ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE \
			-Wstrict-prototypes -Wold-style-definition -fmath-errno -fPIE \
			-ftls-model=local-exec -I../include -I$(GLIBC_BASE)/build/nptl \
			-I$(GLIBC_BASE)/build -I../sysdeps/lind -I../lind_syscall \
			-I../sysdeps/unix/sysv/linux/i386/i686 -I../sysdeps/unix/sysv/linux/i386 \
			-I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86 \
			-I../sysdeps/x86/nptl -I../sysdeps/i386/nptl \
			-I../sysdeps/unix/sysv/linux/include -I../sysdeps/unix/sysv/linux \
			-I../sysdeps/nptl -I../sysdeps/pthread -I../sysdeps/gnu \
			-I../sysdeps/unix/inet -I../sysdeps/unix/sysv -I../sysdeps/unix/i386 \
			-I../sysdeps/unix -I../sysdeps/posix -I../sysdeps/i386/fpu \
			-I../sysdeps/x86/fpu -I../sysdeps/i386 -I../sysdeps/x86/include \
			-I../sysdeps/x86 -I../sysdeps/wordsize-32 -I../sysdeps/ieee754/float128 \
			-I../sysdeps/ieee754/ldbl-96/include -I../sysdeps/ieee754/ldbl-96 \
			-I../sysdeps/ieee754/dbl-64 -I../sysdeps/ieee754/flt-32 \
			-I../sysdeps/ieee754 -I../sysdeps/generic -I.. -I../libio -I. \
			-nostdinc -isystem $(CLANG)/lib/clang/18/include \
			-isystem /usr/i686-linux-gnu/include -D_LIBC_REENTRANT \
			-include $(GLIBC_BASE)/build/libc-modules.h -DMODULE_NAME=libc \
			-include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc \
			-o $(GLIBC_BASE)/build/nptl/pthread_create.o -MD -MP \
			-MF $(GLIBC_BASE)/build/nptl/pthread_create.o.dt \
			-MT $(GLIBC_BASE)/build/nptl/pthread_create.o && \
		cd ../ && $(CC) --target=wasm32-wasi-threads -matomics \
			-o $(GLIBC_BASE)/build/csu/wasi_thread_start.o \
			-c $(GLIBC_BASE)/csu/wasm32/wasi_thread_start.s && \
		./gen_sysroot.sh, \
		@echo "Preview mode: would build everything")

# Utility targets
check_source:
	@if [ -z "$(SOURCE)" ]; then \
		echo -e "$(RED)error: source file name not provided$(RESET)"; \
		exit 1; \
	fi

# Usage:
# make compile_test SOURCE=hello [OUTPUT=output] [PMODE=1]
# make run SOURCE=hello [ARGS="arg1 arg2"] [PMODE=1]
# make debug SOURCE=hello [ARGS="arg1 arg2"] [PMODE=1]
# make compile_src SOURCE=file.c [OPT=-O0] [PMODE=1]