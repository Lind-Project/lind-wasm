// Spike grate: proves the AUTOMATED-interposition call mechanism end to end.
//
// Instead of a hand-written per-function wrapper, this uses ONE generic dispatcher:
// the value registered with register_lib_handler is a `struct libctx*` carrying
// {spec, &real_fn}. The generic pass_fptr_to_wt reads it and calls
// lind_marshal_dispatch(real_fn, spec, ...), which marshals per spec and then calls
// the REAL libz function (resolved from the preloaded libz.so) through a uniform
// (uint64 x6 -> uint64) pointer. That uniform indirect call only works because the
// grate is compiled with --fpcast-emu (which thunks every table entry to the uniform ABI).
//
// This is the kernel of the generated-grate approach: one generic dispatcher +
// a per-symbol (spec, &fn) table = automatic interposition of a whole library.
//
// Compile:
//   lind-clang -s --compile-grate --fpcast-emu spike_grate.c -o spike_grate.wasm -- -I..
// Run (from lindfs/):
//   lind-wasm --preload env=/lib/libz.so grates/spike_grate.cwasm /spike_cage.cwasm
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

// The real libz function (resolved from preloaded libz.so).
extern unsigned long adler32(unsigned long adler, const unsigned char *buf, unsigned int len);

// Inferred spec for adler32(uLong adler, const Bytef *buf, uInt len):
//   arg0 scalar, arg1 ptr IN sized by arg2 (len), arg2 scalar; ret scalar.
// (size_arg_index fixed to 2 here — the libz.marshal.json currently mis-sizes it as 0;
//  the spike tests the call mechanism, not the inference bug.)
static struct lind_marshal_spec adler32_spec = {
    .nargs = 3,
    .args = {
        { .kind = LIND_ARG_SCALAR },
        { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
          .size_kind = LIND_SIZE_FROM_ARG, .size_arg_index = 2 },
        { .kind = LIND_ARG_SCALAR },
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

// Per-symbol context: what the generic dispatcher needs to serve one function.
struct libctx {
    const struct lind_marshal_spec *spec;
    void                           *real_fn;
};

static struct libctx adler32_ctx = { &adler32_spec, (void *)(uintptr_t)&adler32 };

// Generic grate dispatcher. The "fn ptr" registered with register_lib_handler is
// actually a struct libctx*; we read (spec, real_fn) and run the generic marshaller.
int pass_fptr_to_wt(uint64_t ctx_ptr_u64, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (ctx_ptr_u64 == 0) {
        fprintf(stderr, "[Grate|spike] null ctx\n");
        assert(0);
    }
    struct libctx *ctx = (struct libctx *)(uintptr_t)ctx_ptr_u64;

    uint64_t raw[6]   = { arg1, arg2, arg3, arg4, arg5, arg6 };
    uint64_t cages[6] = { arg1cage, arg2cage, arg3cage, arg4cage, arg5cage, arg6cage };
    uint64_t src_cage = 0;
    for (int i = 0; i < 6 && src_cage == 0; i++) src_cage = cages[i];

    fprintf(stderr, "[Grate|spike] dispatch: real_fn=%p spec=%p src_cage=%llu thiscage=%llu "
            "raw=[%llu,%llu,%llu] nargs=%u\n",
            ctx->real_fn, (void*)ctx->spec,
            (unsigned long long)src_cage, (unsigned long long)cageid,
            (unsigned long long)raw[0], (unsigned long long)raw[1], (unsigned long long)raw[2],
            ctx->spec->nargs);

    uint64_t r = lind_marshal_dispatch(ctx->real_fn, ctx->spec,
                                       src_cage, cageid, raw, ctx->spec->nargs);
    fprintf(stderr, "[Grate|spike] dispatch returned 0x%llx\n", (unsigned long long)r);
    return (int)r;
}

int main(int argc, char *argv[]) {
    if (argc < 2) { fprintf(stderr, "Usage: %s <cage>\n", argv[0]); assert(0); }

    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); assert(0); }

    if (pid == 0) {
        int cageid = getpid();
        printf("[Grate|spike] registering generic handler for adler32 (cage %d)\n", cageid);
        int r = register_lib_handler(cageid, "env", "adler32",
                    grateid, (uint64_t)(uintptr_t)&adler32_ctx);
        if (r != 0) { fprintf(stderr, "[Grate|spike] register failed: %d\n", r); assert(0); }
        if (execv(argv[1], &argv[1]) == -1) { perror("execv"); assert(0); }
    }

    int status;
    while (wait(&status) > 0) {}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    if (ce != 0) {
        fprintf(stderr, "[Grate|spike] FAIL: cage exited %d\n", ce);
        return 1;
    }
    printf("[Grate|spike] PASS\n");
    return 0;
}
