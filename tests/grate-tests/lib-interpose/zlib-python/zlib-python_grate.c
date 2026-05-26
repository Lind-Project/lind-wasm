// Grate for zlib-python interposition test.
// Intercepts deflateInit2_ and inflateInit2_ (the entry points for zlib
// compress/decompress) called by Python's built-in zlib module.
// Returns Z_MEM_ERROR (-4) to make Python raise zlib.error, proving
// that real libz symbols were intercepted.
//
// Run manually (the cage is the Python interpreter, not a compiled .c file):
//   lind-wasm --preload env=/lib/libz.so \
//             --preload env=/lib/libpython3.14.so \
//             grates/zlib-python_grate.cwasm \
//             /usr/local/bin/python /test-zlib.py
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

// Shared counter — set by handlers running in the grate context, read by main.
static volatile int intercepted_count = 0;

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|zlib-python] invalid fn ptr\n");
        assert(0);
    }
    int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage,
              arg5, arg5cage, arg6, arg6cage);
}

// Handler for deflateInit2_(strm, level, method, windowBits, memLevel,
//                           strategy, version, stream_size).
// Returns Z_MEM_ERROR (-4) to abort compression immediately.
int deflate_init2_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    intercepted_count++;
    printf("[Grate|zlib-python] deflateInit2_ intercepted! cage=%llu count=%d\n",
           (unsigned long long)cageid, intercepted_count);
    return -4; // Z_MEM_ERROR
}

// Handler for inflateInit2_(strm, windowBits, version, stream_size).
// Returns Z_MEM_ERROR (-4) to abort decompression immediately.
int inflate_init2_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    intercepted_count++;
    printf("[Grate|zlib-python] inflateInit2_ intercepted! cage=%llu count=%d\n",
           (unsigned long long)cageid, intercepted_count);
    return -4; // Z_MEM_ERROR
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_wasm_or_python> [args...]\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();

    pid_t pid = fork();
    if (pid < 0) { perror("fork failed"); assert(0); }

    if (pid == 0) {
        int cageid = getpid();

        uint64_t deflate_fn = (uint64_t)(uintptr_t)&deflate_init2_handler;
        uint64_t inflate_fn = (uint64_t)(uintptr_t)&inflate_init2_handler;

        printf("[Grate|zlib-python] registering deflateInit2_ handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "deflateInit2_",
                                       LIBCALL_BASE + 1, grateid, deflate_fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflateInit2_ failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|zlib-python] registering inflateInit2_ handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "inflateInit2_",
                                   LIBCALL_BASE + 2, grateid, inflate_fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register inflateInit2_ failed: %d\n", ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    // The child (Python) is expected to exit non-zero because our handlers
    // return Z_MEM_ERROR, causing Python to raise zlib.error. We treat any
    // exit as acceptable — the interception count is what matters.
    int status;
    while (wait(&status) > 0) {}

    if (intercepted_count == 0) {
        fprintf(stderr, "[Grate|zlib-python] FAIL: no zlib interceptions observed\n");
        return 1;
    }

    printf("[Grate|zlib-python] PASS: intercepted %d zlib call(s)\n", intercepted_count);
    return 0;
}
