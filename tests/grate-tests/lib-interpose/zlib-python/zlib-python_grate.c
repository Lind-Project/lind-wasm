// Grate for zlib-python interposition test.
// Intercepts deflateInit2_, deflate, and deflateEnd so that Python's
// zlib.compress() returns a fixed 4-byte output b"LIND" regardless of input.
//
// Uses lind_marshal.h for automated argument marshalling:
//   deflateInit2_  → all args treated as scalars; handler returns Z_OK
//   deflate        → z_stream* auto-marshalled INOUT (struct copy-in/copy-out);
//                    one manual copy_data_between_cages to write FIXED_OUTPUT
//                    into the source cage's next_out buffer (nested pointer)
//   deflateEnd     → z_stream* treated as scalar; handler returns Z_OK
//
// Compile:
//   lind-clang -s --compile-grate zlib-python_grate.c \
//     -o zlib-python_grate.wasm -- -I..
//   cp zlib-python_grate.cwasm ../../lindfs/grates/
//
// Run:
//   cd lindfs/
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

#include "../lind_marshal.h"

// z_stream layout in wasm32 (all pointers are uint32_t, uLong = uint32_t).
typedef struct {
    uint32_t next_in;    // offset  0: const Bytef*
    uint32_t avail_in;   // offset  4: uInt
    uint32_t total_in;   // offset  8: uLong
    uint32_t next_out;   // offset 12: Bytef*
    uint32_t avail_out;  // offset 16: uInt
    uint32_t total_out;  // offset 20: uLong
} ZStreamWasm32;

static const uint8_t FIXED_OUTPUT[] = { 'L', 'I', 'N', 'D' };
#define FIXED_OUTPUT_LEN 4

static volatile int intercepted_deflate_count = 0;

// ---------------------------------------------------------------------------
// deflateInit2_: ignore all args, return Z_OK.
// All 8 args treated as scalars — no pointer dereference needed.
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_init2_spec = {
    .nargs = 8,
    .args = {
        { .kind = LIND_ARG_SCALAR }, // z_stream* (unused)
        { .kind = LIND_ARG_SCALAR }, // level
        { .kind = LIND_ARG_SCALAR }, // method
        { .kind = LIND_ARG_SCALAR }, // windowBits
        { .kind = LIND_ARG_SCALAR }, // memLevel
        { .kind = LIND_ARG_SCALAR }, // strategy
        { .kind = LIND_ARG_SCALAR }, // version* (unused)
        { .kind = LIND_ARG_SCALAR }, // stream_size
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate_init2(uint64_t a0, uint64_t a1, uint64_t a2,
                                       uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a0; (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    printf("[Grate|zlib-python] deflateInit2_ intercepted — returning Z_OK\n");
    return 0; // Z_OK
}

LIND_DEFINE_MARSHAL_HANDLER(deflate_init2_, &deflate_init2_spec, handler_deflate_init2)

// ---------------------------------------------------------------------------
// deflate: auto-marshal z_stream* INOUT (struct copy-in/copy-out).
// One manual cross-cage copy remains for writing FIXED_OUTPUT into next_out
// (a nested pointer inside the struct — Stage 1 does not chase these).
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_spec = {
    .nargs = 2,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_INOUT,
            .size_kind     = LIND_SIZE_CONST,
            .const_size    = sizeof(ZStreamWasm32),
        },
        { .kind = LIND_ARG_SCALAR }, // flush flag
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate(uint64_t strm_u64, uint64_t flush,
                                 uint64_t a2, uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)flush; (void)a2; (void)a3; (void)a4; (void)a5;

    ZStreamWasm32 *zst = LIND_AS_PTR(strm_u64);

    if (zst->avail_out < FIXED_OUTPUT_LEN) {
        fprintf(stderr, "[Grate|zlib-python] deflate: avail_out=%u < %d, aborting\n",
                zst->avail_out, FIXED_OUTPUT_LEN);
        return (uint64_t)(uint32_t)(-4); // Z_MEM_ERROR as unsigned
    }

    // Write FIXED_OUTPUT into the source cage's output buffer at next_out.
    // next_out is a WASM virtual address in the source cage — this is a nested
    // pointer that Stage 1 does not automatically chase, so one manual call remains.
    copy_data_between_cages(
        LIND_GRATE_CAGE(),  LIND_SOURCE_CAGE(),
        (uint64_t)(uintptr_t)FIXED_OUTPUT, LIND_GRATE_CAGE(),
        (uint64_t)zst->next_out,           LIND_SOURCE_CAGE(),
        FIXED_OUTPUT_LEN, 0);

    // Update the local z_stream shadow — auto-marshal copies it back on return.
    zst->next_out  += FIXED_OUTPUT_LEN;
    zst->avail_out -= FIXED_OUTPUT_LEN;
    zst->total_out += FIXED_OUTPUT_LEN;

    intercepted_deflate_count++;
    printf("[Grate|zlib-python] deflate intercepted — wrote %d fixed bytes, returning Z_STREAM_END\n",
           FIXED_OUTPUT_LEN);
    return 1; // Z_STREAM_END
}

LIND_DEFINE_MARSHAL_HANDLER(deflate, &deflate_spec, handler_deflate)

// ---------------------------------------------------------------------------
// deflateEnd: ignore z_stream*, return Z_OK.
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_end_spec = {
    .nargs = 1,
    .args = {
        { .kind = LIND_ARG_SCALAR }, // z_stream* (unused)
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate_end(uint64_t a0, uint64_t a1, uint64_t a2,
                                     uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a0; (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    printf("[Grate|zlib-python] deflateEnd intercepted — returning Z_OK\n");
    return 0; // Z_OK
}

LIND_DEFINE_MARSHAL_HANDLER(deflateEnd, &deflate_end_spec, handler_deflate_end)

// ---------------------------------------------------------------------------
// Standard grate dispatcher (required export)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <python> [args...]\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork failed"); assert(0); }

    if (pid == 0) {
        int cageid = getpid();

        printf("[Grate|zlib-python] registering deflateInit2_ handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "deflateInit2_",
            grateid, (uint64_t)(uintptr_t)&lind_mh_deflate_init2_);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflateInit2_ failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|zlib-python] registering deflate handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "deflate",
            grateid, (uint64_t)(uintptr_t)&lind_mh_deflate);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflate failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|zlib-python] registering deflateEnd handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "deflateEnd",
            grateid, (uint64_t)(uintptr_t)&lind_mh_deflateEnd);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflateEnd failed: %d\n", ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status;
    while (wait(&status) > 0) {}
    int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;

    if (intercepted_deflate_count == 0) {
        fprintf(stderr, "[Grate|zlib-python] FAIL: deflate was never intercepted\n");
        return 1;
    }
    if (child_exit != 0) {
        fprintf(stderr, "[Grate|zlib-python] FAIL: Python exited with %d\n", child_exit);
        return 1;
    }

    printf("[Grate|zlib-python] PASS: deflate intercepted %d time(s), Python exited 0\n",
           intercepted_deflate_count);
    return 0;
}
