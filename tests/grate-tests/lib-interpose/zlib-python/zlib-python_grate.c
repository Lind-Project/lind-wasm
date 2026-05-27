// Grate for zlib-python interposition test.
// Intercepts deflateInit2_, deflate, and deflateEnd so that Python's
// zlib.compress() returns a fixed 4-byte output b"LIND" regardless of input.
//
// How it works:
//   deflateInit2_  → return Z_OK without allocating real zlib state
//   deflate        → read z_stream from cage 2, write b"LIND" into its output
//                    buffer, update total_out/avail_out/next_out, return Z_STREAM_END
//   deflateEnd     → return Z_OK (no real state to free)
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

// z_stream layout in wasm32 (all pointers are uint32_t, uLong = uint32_t).
// Only the first 6 fields are needed; the rest are left unread.
typedef struct {
    uint32_t next_in;    // offset  0: const Bytef*
    uint32_t avail_in;   // offset  4: uInt
    uint32_t total_in;   // offset  8: uLong
    uint32_t next_out;   // offset 12: Bytef*
    uint32_t avail_out;  // offset 16: uInt
    uint32_t total_out;  // offset 20: uLong
} ZStreamWasm32;

// Fixed 4-byte output that our deflate handler writes into the stream.
static const uint8_t FIXED_OUTPUT[] = { 'L', 'I', 'N', 'D' };
#define FIXED_OUTPUT_LEN 4

// Set by deflate_handler; checked by main() after the child exits.
static volatile int intercepted_deflate_count = 0;

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

// deflateInit2_: succeed without allocating real zlib state.
// Python only checks the return value; the state pointer stays NULL.
// Since we intercept all subsequent calls (deflate, deflateEnd), no
// dereference of the uninitialized state pointer ever reaches real zlib.
int deflate_init2_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    printf("[Grate|zlib-python] deflateInit2_ intercepted — returning Z_OK\n");
    return 0; // Z_OK
}

// deflate: write FIXED_OUTPUT into the stream's output buffer and signal completion.
//   arg1      = z_stream* (WASM virtual address in the child cage)
//   arg1cage  = child cage id
//   arg2      = flush flag (ignored)
int deflate_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {

    uint64_t thiscage = cageid; // grate cage id

    // Read the z_stream struct from the child cage's memory.
    // copy_data_between_cages handles WASM virtual → host translation for
    // cross-cage pointers via _ensure_host_addr on the Rust side.
    ZStreamWasm32 zst;
    copy_data_between_cages(thiscage, arg1cage,
        arg1, arg1cage,                       // src: z_stream* in child cage
        (uint64_t)(uintptr_t)&zst, thiscage,  // dest: local copy in grate
        sizeof(ZStreamWasm32), 0);            // RawMemcpy

    if (zst.avail_out < FIXED_OUTPUT_LEN) {
        fprintf(stderr, "[Grate|zlib-python] deflate: avail_out=%u < %d, aborting\n",
                zst.avail_out, FIXED_OUTPUT_LEN);
        return -4; // Z_MEM_ERROR
    }

    // Write FIXED_OUTPUT into the child cage's output buffer at next_out.
    // next_out is a WASM virtual address in the child cage; Rust translates it.
    copy_data_between_cages(thiscage, arg1cage,
        (uint64_t)(uintptr_t)FIXED_OUTPUT, thiscage, // src: grate's static array
        (uint64_t)zst.next_out, arg1cage,             // dest: stream output buffer
        FIXED_OUTPUT_LEN, 0);                         // RawMemcpy

    // Update the stream fields to reflect the written output.
    zst.next_out  += FIXED_OUTPUT_LEN;
    zst.avail_out -= FIXED_OUTPUT_LEN;
    zst.total_out += FIXED_OUTPUT_LEN;

    // Write the updated struct back to the child cage.
    copy_data_between_cages(thiscage, arg1cage,
        (uint64_t)(uintptr_t)&zst, thiscage,  // src: updated local copy
        arg1, arg1cage,                        // dest: z_stream* in child cage
        sizeof(ZStreamWasm32), 0);             // RawMemcpy

    intercepted_deflate_count++;
    printf("[Grate|zlib-python] deflate intercepted — wrote %d fixed bytes, returning Z_STREAM_END\n",
           FIXED_OUTPUT_LEN);
    return 1; // Z_STREAM_END
}

// deflateEnd: succeed without freeing (no real state was allocated).
int deflate_end_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    printf("[Grate|zlib-python] deflateEnd intercepted — returning Z_OK\n");
    return 0; // Z_OK
}

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
            grateid, (uint64_t)(uintptr_t)&deflate_init2_handler);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflateInit2_ failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|zlib-python] registering deflate handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "deflate",
            grateid, (uint64_t)(uintptr_t)&deflate_handler);
        if (ret != 0) {
            fprintf(stderr, "[Grate|zlib-python] register deflate failed: %d\n", ret);
            assert(0);
        }

        printf("[Grate|zlib-python] registering deflateEnd handler for cage %d\n", cageid);
        ret = register_lib_handler(cageid, "env", "deflateEnd",
            grateid, (uint64_t)(uintptr_t)&deflate_end_handler);
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
