# Define environment variables with default values
CLANG_PATH ?= /home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/bin/clang
LIBC_INCLUDE_PATH ?= /home/lind/lind-wasm/src/glibc/build
CLANG_INCLUDE_PATH ?= /home/lind/lind-wasm/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib/clang/16/include

# Compiler options
COMMON_FLAGS := --target=wasm32-unkown-wasi -v -Wno-int-conversion -std=gnu11 -fgnu89-inline \
    -matomics -mbulk-memory -O2 -g -Wall -Wwrite-strings -Wundef -fmerge-all-constants \
    -ftrapping-math -fno-stack-protector -fno-common -Wp,-U_FORTIFY_SOURCE -Wstrict-prototypes \
    -Wold-style-definition -fmath-errno -fPIE -ftls-model=local-exec -nostdinc -isystem $(CLANG_INCLUDE_PATH) \
    -isystem /usr/i686-linux-gnu/include -D_LIBC_REENTRANT -include $(LIBC_INCLUDE_PATH)/libc-modules.h \
    -DMODULE_NAME=libc -include ../include/libc-symbols.h -DPIC -DTOP_NAMESPACE=glibc

INCLUDE_PATHS := -I../include -I$(LIBC_INCLUDE_PATH)/nptl -I$(LIBC_INCLUDE_PATH) -I../sysdeps/lind \
    -I../lind_syscall -I../sysdeps/unix/sysv/linux/i386/i686 -I../sysdeps/unix/sysv/linux/i386 \
    -I../sysdeps/unix/sysv/linux/x86/include -I../sysdeps/unix/sysv/linux/x86 -I../sysdeps/x86/nptl \
    -I../sysdeps/i386/nptl -I../sysdeps/unix/sysv/linux/include -I../sysdeps/unix/sysv/linux \
    -I../sysdeps/nptl -I../sysdeps/pthread -I../sysdeps/gnu -I../sysdeps/unix/inet -I../sysdeps/unix/sysv \
    -I../sysdeps/unix/i386 -I../sysdeps/unix -I../sysdeps/posix -I../sysdeps/i386/fpu -I../sysdeps/x86/fpu \
    -I../sysdeps/i386 -I../sysdeps/x86/include -I../sysdeps/x86 -I../sysdeps/wordsize-32 -I../sysdeps/ieee754/float128 \
    -I../sysdeps/ieee754/ldbl-96/include -I../sysdeps/ieee754/ldbl-96 -I../sysdeps/ieee754/dbl-64 \
    -I../sysdeps/ieee754/flt-32 -I../sysdeps/ieee754 -I../sysdeps/generic -I.. -I../libio -I.

# Targets
all: crt1.o lind_syscall.o

crt1.o: crt1/crt1.c
	$(CLANG_PATH) $(COMMON_FLAGS) $(INCLUDE_PATHS) crt1/crt1.c -c -o crt1.o

lind_syscall.o: lind_syscall.c
	$(CLANG_PATH) $(COMMON_FLAGS) $(INCLUDE_PATHS) lind_syscall.c -c -o lind_syscall.o

clean:
	rm -f crt1.o lind_syscall.o
