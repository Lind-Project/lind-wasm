// Grate for zlib-python interposition test.
// Intercepts deflateInit2_, deflate, and deflateEnd so that Python's
// zlib.compress() returns a fixed 4-byte output b"LIND" regardless of input.
//
// Uses lind_marshal.h for fully automated argument marshalling — no manual
// copy_data_between_cages in any handler:
//
//   deflateInit2_  → all args SCALAR; handler returns Z_OK
//   deflate        → z_stream* described as a nested struct (LIND_LO_STRUCT):
//                    next_out field is PTR OUT sized by sibling avail_out;
//                    handler writes FIXED_OUTPUT into the shadow buffer and
//                    updates the shadow struct fields; the runtime copies the
//                    output bytes back to the source cage and fixes up next_out.
//   deflateEnd     → z_stream* treated as SCALAR; handler returns Z_OK
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
// deflate: z_stream struct layout for the nested-struct marshaller
//
// Field indices:
//   0: next_in  (offset  0) — PTR IN, size=avail_in [field 1]; touched=0 (unused)
//   1: avail_in (offset  4) — SCALAR;                          touched=0
//   2: total_in (offset  8) — SCALAR;                          touched=0
//   3: next_out (offset 12) — PTR OUT, size=avail_out [field 4]; touched=1
//   4: avail_out(offset 16) — SCALAR;                          touched=1
//   5: total_out(offset 20) — SCALAR;                          touched=1
// ---------------------------------------------------------------------------

static struct lind_arg_spec _scalar_fspec  = { .kind = LIND_ARG_SCALAR };
static struct lind_arg_spec _next_out_fspec = {
    .kind           = LIND_ARG_PTR,
    .ptr_direction  = LIND_PTR_OUT,
    .size_kind      = LIND_SIZE_FROM_ARG,
    .size_arg_index = 4,   // sibling field index 4 = avail_out
};

static struct lind_field _zstream_fields[6] = {
    { .offset = 0,  .spec = &_scalar_fspec,   .touched = 0 },  // next_in
    { .offset = 4,  .spec = &_scalar_fspec,   .touched = 0 },  // avail_in
    { .offset = 8,  .spec = &_scalar_fspec,   .touched = 0 },  // total_in
    { .offset = 12, .spec = &_next_out_fspec, .touched = 1 },  // next_out
    { .offset = 16, .spec = &_scalar_fspec,   .touched = 1 },  // avail_out
    { .offset = 20, .spec = &_scalar_fspec,   .touched = 1 },  // total_out
};

static struct lind_layout _zstream_layout = {
    .kind        = LIND_LO_STRUCT,
    .nfields     = 6,
    .fields      = _zstream_fields,
    .struct_size = 24,
};

// ---------------------------------------------------------------------------
// deflateInit2_: ignore all args, return Z_OK
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_init2_spec = {
    .nargs = 8,
    .args  = {
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_SCALAR },
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate_init2(uint64_t a0, uint64_t a1, uint64_t a2,
                                       uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a0; (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    printf("[Grate|zlib-python] deflateInit2_ intercepted — returning Z_OK\n");
    return 0;
}

LIND_DEFINE_MARSHAL_HANDLER(deflate_init2_, &deflate_init2_spec, handler_deflate_init2)

// ---------------------------------------------------------------------------
// deflate: fully automated via nested struct layout
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_spec = {
    .nargs = 2,
    .args = {
        {
            .kind          = LIND_ARG_PTR,
            .ptr_direction = LIND_PTR_INOUT,
            .size_kind     = LIND_SIZE_CONST,
            .const_size    = 24,             // sizeof(ZStreamWasm32)
            .layout        = &_zstream_layout,
        },
        { .kind = LIND_ARG_SCALAR },         // flush
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate(uint64_t strm_u64, uint64_t flush,
                                 uint64_t a2, uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)flush; (void)a2; (void)a3; (void)a4; (void)a5;

    ZStreamWasm32 *zst = (ZStreamWasm32 *)(uintptr_t)strm_u64;

    if (zst->avail_out < FIXED_OUTPUT_LEN) {
        fprintf(stderr, "[Grate|zlib-python] deflate: avail_out=%u < %d\n",
                zst->avail_out, FIXED_OUTPUT_LEN);
        return (uint64_t)(uint32_t)(-4);
    }

    // zst->next_out is a valid local shadow pointer (grate memory).
    // Write FIXED_OUTPUT directly — no cross-cage copy needed.
    memcpy((void *)(uintptr_t)zst->next_out, FIXED_OUTPUT, FIXED_OUTPUT_LEN);

    // Advance stream fields; post-call copies the data back and fixes up next_out.
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
// deflateEnd: ignore z_stream*, return Z_OK
// ---------------------------------------------------------------------------

static struct lind_marshal_spec deflate_end_spec = {
    .nargs = 1,
    .args  = { { .kind = LIND_ARG_SCALAR } },
    .ret   = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_deflate_end(uint64_t a0, uint64_t a1, uint64_t a2,
                                     uint64_t a3, uint64_t a4, uint64_t a5) {
    (void)a0; (void)a1; (void)a2; (void)a3; (void)a4; (void)a5;
    printf("[Grate|zlib-python] deflateEnd intercepted — returning Z_OK\n");
    return 0;
}

LIND_DEFINE_MARSHAL_HANDLER(deflateEnd, &deflate_end_spec, handler_deflate_end)

// ---------------------------------------------------------------------------
// Standard grate dispatcher
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
