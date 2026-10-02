// Grate proving Gate 5's own explicit acceptance criterion (issue #22's
// OpenBLAS inference-to-runtime integration): "A harness self-test
// deliberately disables registration and proves that a numerical pass
// is classified as local-only, not interposed." Registers a V1 handler
// for toy_trace_add and a hand-written V2 adapter for toy_trace_sum when
// `mode` is "both"; registers NEITHER when `mode` is "none" -- in BOTH
// modes the cage's own calls still produce the numerically correct
// result (exectrace_stub.c's real implementation is correct either way),
// but only "both" mode leaves this grate's own [lind-trace] evidence in
// the combined output (see lind_marshal.h's own doc on
// -DLIND_MARSHAL_DEBUG; this grate is built with it by run_tests.sh
// specifically for this test). tools/marshal-gen/execution_oracle.py's
// own classify() is what turns that evidence into PASS_INTERPOSED vs.
// PASS_LOCAL_ONLY -- see run_tests.sh's own invocation of it for this
// test.
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdint.h>

#include "../lind_marshal.h"

extern int toy_trace_add(int a, int b);
extern int toy_trace_sum(int n, const int *x);

// --- V1: toy_trace_add (plain scalars, no pointer) ---
static struct lind_marshal_spec add_spec = {
    .nargs = 2,
    .args = {
        { .kind = LIND_ARG_SCALAR },  // a
        { .kind = LIND_ARG_SCALAR },  // b
    },
    .ret = { .kind = LIND_RET_SCALAR },
};

static uint64_t handler_toy_trace_add(uint64_t a, uint64_t b) {
    return LIND_RET_INT(toy_trace_add((int)a, (int)b));
}
LIND_DEFINE_MARSHAL_HANDLER(toy_trace_add, &add_spec, handler_toy_trace_add)

// --- V2: toy_trace_sum (one scalar, one fixed-size IN pointer) ---
// Hand-written to mirror gen_v2_adapter.py's own emit_v2_adapter output
// exactly (including the _lind_dbg_call/_lind_marshal_prepare_arg calls
// every generated V2 adapter also makes) -- this is what a REAL
// generated adapter's trace evidence looks like, not a simplified stand-in.
#define TOY_TRACE_SUM_N 5
static struct lind_arg_spec sum_argspecs[2] = {
    { .kind = LIND_ARG_SCALAR },  // n
    { .kind = LIND_ARG_PTR, .ptr_direction = LIND_PTR_IN,
      .size_kind = LIND_SIZE_CONST, .const_size = TOY_TRACE_SUM_N * sizeof(int) },  // x
};
static struct lind_return_spec sum_retspec = { .kind = LIND_RET_SCALAR };

__attribute__((export_name("__lind_v2_adapter_toy_trace_sum")))
int32_t trace_v2_adapter_toy_trace_sum(uint64_t source_cage, uint64_t grate_cage,
                                        int32_t raw0, uint32_t raw1) {
    _lind_dbg_call("toy_trace_sum");
    _lind_marshal_reset();
    _lind_marshal_source_cage = source_cage;
    _lind_marshal_grate_cage  = grate_cage;
    uint64_t raw_args[2] = { (uint64_t)raw0, (uint64_t)raw1 };
    struct _lind_shadow shadows[2];
    uint32_t nshadows = 0;
    uint64_t h0 = _lind_marshal_prepare_arg(0, &sum_argspecs[0], raw_args, 2,
                                             source_cage, grate_cage, shadows, &nshadows, "toy_trace_sum");
    uint64_t h1 = _lind_marshal_prepare_arg(1, &sum_argspecs[1], raw_args, 2,
                                             source_cage, grate_cage, shadows, &nshadows, "toy_trace_sum");
    int32_t real_ret = toy_trace_sum((int32_t)h0, (const int *)(uintptr_t)h1);
    uint64_t handler_ret = (uint64_t)real_ret;
    for (uint32_t s = 0; s < nshadows; s++)
        _lind_marshal_finish_shadow(s, sum_argspecs, shadows, nshadows, source_cage, grate_cage);
    uint64_t result = _lind_marshal_translate_return(&sum_retspec, raw_args, 2, shadows, nshadows,
                                                       handler_ret, source_cage, grate_cage);
    _lind_marshal_reset();
    return (int32_t)result;
}

// Every V2 module must export this exact zero-arg, i32-returning function
// (see gen_v2_adapter.py's own MANIFEST_VERSION_EXPORT doc) -- the host
// checks it against the leading version number in the sig descriptor
// register_lib_handler_v2 is given below, at CALL time (not just
// registration time); omitting it makes every call reject, not just
// registration.
__attribute__((export_name("__lind_v2_manifest_version")))
int __lind_v2_manifest_version(void) { return 1; }

// --compile-grate unconditionally requires a pass_fptr_to_wt export (the
// uniform V1 dispatch entry point the host calls for ANY register_lib_handler
// registration, passing the registered value through as fn_ptr_uint) --
// mirrors fail-closed/exprsize_grate.c's own identical copy exactly.
int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|exectrace] invalid fn ptr\n");
        __builtin_trap();
    }
    int64_t (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t) =
        (int64_t (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, arg2, arg2cage,
              arg3, arg3cage, arg4, arg4cage,
              arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
    if (argc < 3) { fprintf(stderr, "Usage: %s <mode:both|none> <app>\n", argv[0]); return 2; }
    const char *mode = argv[1];
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) { perror("fork"); return 1; }
    if (pid == 0) {
        int cageid = getpid();
        int ok = 0, want = 0;
        if (strcmp(mode, "both") == 0) {
            want = 2;
            int r1 = register_lib_handler(cageid, "env", "toy_trace_add", grateid,
                                           (uint64_t)(uintptr_t)&lind_mh_toy_trace_add);
            if (r1 == 0) ok++;
            else fprintf(stderr, "[Grate|exectrace] register toy_trace_add failed: %d\n", r1);
            int r2 = register_lib_handler_v2(cageid, "env", "toy_trace_sum", grateid,
                                              "__lind_v2_adapter_toy_trace_sum", "1:ii:i");
            if (r2 == 0) ok++;
            else fprintf(stderr, "[Grate|exectrace] register toy_trace_sum failed: %d\n", r2);
        }
        // mode "none": register nothing at all -- both toy functions
        // fall straight through to exectrace_stub.c's own real,
        // uninterposed implementation. Deliberately the self-test Gate
        // 5's own acceptance criterion calls for.
        fprintf(stderr, "[Grate|exectrace] registered %d/%d handlers\n", ok, want);
        if (ok != want) {
            fprintf(stderr, "[Grate|exectrace] FATAL: %d/%d handler registrations failed\n",
                    want - ok, want);
            return 1;
        }
        if (execv(argv[2], &argv[2]) == -1) { perror("execv"); return 1; }
    }
    int status;
    while (wait(&status) > 0) {}
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[Grate|exectrace] app exited %d\n", ce);
    return ce == 0 ? 0 : 1;
}
