# ======================================
# Variables
# ======================================

# Base directories
GLIBC_BASE := /home/lind-wasm/glibc
WASMTIME_BASE := /home/lind-wasm/wasmtime
RUSTPOSIX_BASE := /home/lind-wasm/safeposix-rust
RAWPOSIX_BASE := /home/lind-wasm/RawPOSIX

# Compiler
CC := /home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang-16

# Export command
EXPORT_CMD := export LD_LIBRARY_PATH=$(WASMTIME_BASE)/crates/rustposix:$$LD_LIBRARY_PATH

# Compile commands
COMPILE_TEST_CMD_FORK := $(CC) --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot -Wl,--export="__stack_pointer" [input] -g -O0 -o [output] && wasm-opt --asyncify --debuginfo [output] -o [output]
COMPILE_TEST_CMD_NOSHARED := $(CC) --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot -Wl,--export="__stack_pointer" [input] -g -O0 -o [output]
COMPILE_TEST_CMD := $(CC) -pthread --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864 [input] -g -O0 -o [output]
PRECOMPILE_WASM := $(WASMTIME_BASE)/target/debug/wasmtime compile [input] -o [output]

COMPILE_TEST_CMD_FORK_TEST := $(CC) -pthread --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot -Wl,--import-memory,--export-memory,--max-memory=67108864,--export="__stack_pointer" [input] -g -O0 -o [output] && wasm-opt --asyncify --debuginfo [output] -o [output]

# Run commands
RUN_CMD := $(WASMTIME_BASE)/target/debug/wasmtime run --wasi threads=y --wasi preview2=n [target]
RUN_CMD_PRECOMPILE := $(WASMTIME_BASE)/target/debug/wasmtime run --allow-precompiled --wasi threads=y --wasi preview2=n [target]

# Build commands
COMPILE_WASMTIME_CMD := cd $(WASMTIME_BASE) && cargo build
COMPILE_RUSTPOSIX_CMD := cd $(RUSTPOSIX_BASE) && cargo build && cp $(RUSTPOSIX_BASE)/target/debug/librustposix.so $(WASMTIME_BASE)/crates/rustposix
COMPILE_RAWPOSIX_CMD := cd $(RAWPOSIX_BASE) && cargo build && cp $(RAWPOSIX_BASE)/target/debug/librustposix.so $(WASMTIME_BASE)/crates/rustposix

# Pthread and Wasi Thread Start compilation
COMPILE_PTHREAD_CREATE := $(CC) --target=wasm32-unkown-wasi -v -Wno-int-conversion pthread_create.c -c -std=gnu11 -fgnu89-inline -matomics -mbulk-memory -O0 -g -Wall -Wwrite-strings -Wundef -fmerge-all-constants -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -Wstrict-prototypes -Wold-style-definition -fmath-errno -fPIE -ftls-model=local-exec -I../include -I/home/lind-wasm/glibc/build/nptl -I/home/lind-wasm/glibc/build -I../sysdeps/lind -I../lind_syscall -I../sysdeps/unix/sysv/linux/i386/i686 -I../sysdeps/unix/sysv/linux/i386 -I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86 -I../sysdeps/x86/nptl -I../sysdeps/i386/nptl -I../sysdeps/unix/sysv/linux/include -I../sysdeps/unix/sysv/linux -I../sysdeps/nptl -I../sysdeps/pthread -I../sysdeps/gnu -I../sysdeps/unix/inet -I../sysdeps/unix/sysv -I../sysdeps/unix/i386 -I../sysdeps/unix -I../sysdeps/posix -I../sysdeps/i386/fpu -I../sysdeps/x86/fpu -I../sysdeps/i386 -I../sysdeps/x86/include -I../sysdeps/x86 -I../sysdeps/wordsize-32 -I../sysdeps/ieee754/float128 -I../sysdeps/ieee754/ldbl-96/include -I../sysdeps/ieee754/ldbl-96 -I../sysdeps/ieee754/dbl-64 -I../sysdeps/ieee754/flt-32 -I../sysdeps/ieee754 -I../sysdeps/generic -I.. -I../libio -I. -nostdinc -isystem /home/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/include -isystem /usr/i686-linux-gnu/include -D_LIBC_REENTRANT -include /home/lind-wasm/glibc/build/libc-modules.h -DMODULE_NAME=libc -include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc -o /home/lind-wasm/glibc/build/nptl/pthread_create.o -MD -MP -MF /home/lind-wasm/glibc/build/nptl/pthread_create.o.dt -MT /home/lind-wasm/glibc/build/nptl/pthread_create.o
COMPILE_WASI_THREAD_START := $(CC) --target=wasm32-wasi-threads -matomics -o $(GLIBC_BASE)/build/csu/wasi_thread_start.o -c $(GLIBC_BASE)/csu/wasm32/wasi_thread_start.s

# Make command
MAKE_CMD := cd $(GLIBC_BASE) && rm -rf build && ./wasm-config.sh && cd build && make -j8 --keep-going 2>&1 THREAD_MODEL=posix | tee check.log && cd ../nptl && $(COMPILE_PTHREAD_CREATE) && cd ../ && $(COMPILE_WASI_THREAD_START) && ./gen_sysroot.sh

# Colors for output
RED := \033[31m
GREEN := \033[32m
RESET := \033[0m

# Preview mode flag
PMODE := 0

# ======================================
# Phony Targets
# ======================================
.PHONY: export compile_test run compile_wasmtime compile_rustposix compile_rawposix compile_src make_all make help

# ======================================
# Helper Functions
# ======================================

# Function to split path into directory and file
split_path = $(shell dirname $(1)) $(shell basename $(1))

# Function to escape special characters for grep
escape_special = $(shell echo "$(1)" | sed 's/[.[*+?^${}()|\\]/\\&/g')

# ======================================
# Targets
# ======================================

# Default target
all: help

# Handle export command
export:
	@echo -e "$(GREEN)$(EXPORT_CMD)$(RESET)"
	@echo "Please manually run the export command"

# Compile test
compile_test: 
ifndef SOURCE
	$(error "error: source file name not provided")
endif
	@echo -e "$(GREEN)Exporting LD_LIBRARY_PATH...$(RESET)"
	@$(EXPORT_CMD)
	@echo -e "$(GREEN)Compiling test...$(RESET)"
	@$(CC) --target=wasm32-unknown-wasi --sysroot $(GLIBC_BASE)/sysroot -Wl,--export="__stack_pointer" $(SOURCE).c -g -O0 -o $(SOURCE).wasm && wasm-opt --asyncify --debuginfo $(SOURCE).wasm -o $(SOURCE).wasm
	@$(WASMTIME_BASE)/target/debug/wasmtime compile $(SOURCE).wasm -o $(SOURCE).cwasm

# Run wasm
run:
ifndef TARGET
	$(error "error: target file name not provided")
endif
	@echo -e "$(GREEN)Exporting LD_LIBRARY_PATH...$(RESET)"
	@$(EXPORT_CMD)
ifeq ($(wildcard $(TARGET).cwasm),)
	@$(WASMTIME_BASE)/target/debug/wasmtime run --wasi threads=y --wasi preview2=n $(TARGET).wasm $(ARGS)
else
	@$(WASMTIME_BASE)/target/debug/wasmtime run --allow-precompiled --wasi threads=y --wasi preview2=n $(TARGET).cwasm $(ARGS)
endif

# Compile wasmtime
compile_wasmtime:
	@echo -e "$(GREEN)$(COMPILE_WASMTIME_CMD)$(RESET)"
	@$(COMPILE_WASMTIME_CMD)

# Compile rustposix
compile_rustposix:
	@echo -e "$(GREEN)$(COMPILE_RUSTPOSIX_CMD)$(RESET)"
	@$(COMPILE_RUSTPOSIX_CMD)

# Compile rawposix
compile_rawposix:
	@echo -e "$(GREEN)$(COMPILE_RAWPOSIX_CMD)$(RESET)"
	@$(COMPILE_RAWPOSIX_CMD)

# Make all
make_all:
	@echo -e "$(GREEN)$(MAKE_CMD)$(RESET)"
	@$(MAKE_CMD)

# Help
help:
	@echo "Available commands are:"
	@echo "  make export             - Export LD_LIBRARY_PATH"
	@echo "  make compile_test SOURCE=<name> [OUTPUT=<name>] - Compile test"
	@echo "  make run TARGET=<name> [ARGS=<args>] - Run wasm module"
	@echo "  make compile_wasmtime   - Compile wasmtime"
	@echo "  make compile_rustposix  - Compile rustposix"
	@echo "  make compile_rawposix   - Compile rawposix"
	@echo "  make make_all           - Build all components"
	@echo "  make help               - Show this help message"

# ======================================
# Command-Line Argument Handling
# ======================================

# Override variables based on command-line arguments
# Example usage:
# make compile_test SOURCE=filename
# make run TARGET=filename ARGS="additional arguments"

# Parse -p flag for preview mode
ifeq ($(lastword $(MAKECMDGOALS)),-p)
	PMODE := 1
	MAKECMDGOALS := $(filter-out -p,$(MAKECMDGOALS))
endif
