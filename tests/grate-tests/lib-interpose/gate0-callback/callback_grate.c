// Gate 0 (cross-cage function-pointer callbacks) feasibility probe: grate B.
//
// Hand-rolled V2 grate for `library_call` -- deliberately bypasses
// lind_marshal.h's declarative arg-spec system entirely. A function-pointer
// argument isn't representable in that schema yet (that's Gate 1/2's
// "callback contract" work, out of scope here); for this probe, the HOST
// (GrateWorker::install_callback_proxies, in wasmtime-lind-3i, run before
// this adapter is ever called) already replaces the caller's raw table
// index with a proxy index valid in THIS module's own table, so the adapter
// below just calls through it like any ordinary local function pointer --
// no cross-cage awareness needed at the wasm level at all.
//
// The registration descriptor "1:i::0" marks parameter 0 (the sole i32
// parameter) as a callback: manifest version 1, params "i", results ""
// (void), callback_params "0".
//
// Compile:
//   lind-clang -s --compile-grate --fpcast-emu callback_grate.c \
//       -- -Wl,--export-table
// Run (from lindfs/):
//   lind_run --preload env=/lib/liblibrary_call_stub.so:interposed \
//       grates/callback_grate.cwasm /callback_cage.cwasm
#include <lind_syscall.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdint.h>

__attribute__((export_name("__lind_v2_adapter_library_call")))
void __lind_v2_adapter_library_call(uint64_t source_cage, uint64_t grate_cage,
                                     int32_t callback_tableidx) {
    (void)source_cage;
    (void)grate_cage;
    void (*callback)(int) = (void (*)(int))(uintptr_t)callback_tableidx;
    callback(42);
}

// V2 registration/resolution requires every V2 module to export this exact
// zero-arg, i32-returning function.
__attribute__((export_name("__lind_v2_manifest_version")))
int __lind_v2_manifest_version(void) {
    return 1;
}

// --compile-grate unconditionally requires a pass_fptr_to_wt export even
// when the grate registers no V1 handler. Unreachable here.
int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                         uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                         uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                         uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                         uint64_t arg6, uint64_t arg6cage) {
    (void)fn_ptr_uint;
    (void)cageid;
    (void)arg1;
    (void)arg1cage;
    (void)arg2;
    (void)arg2cage;
    (void)arg3;
    (void)arg3cage;
    (void)arg4;
    (void)arg4cage;
    (void)arg5;
    (void)arg5cage;
    (void)arg6;
    (void)arg6cage;
    fprintf(stderr, "[gate0-callback-grate] FAIL: pass_fptr_to_wt reached (should be unreachable)\n");
    __builtin_trap();
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <app> [args...]\n", argv[0]);
        __builtin_trap();
    }
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        __builtin_trap();
    }
    if (pid == 0) {
        int cageid = getpid();
        int r = register_lib_handler_v2(cageid, "env", "library_call", grateid,
                                         "__lind_v2_adapter_library_call", "1:i::0");
        if (r != 0) {
            fprintf(stderr, "[gate0-callback-grate] register library_call failed: %d\n", r);
            __builtin_trap();
        }
        fprintf(stderr, "[gate0-callback-grate] registered 1/1 handlers\n");
        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv");
            __builtin_trap();
        }
    }
    int status;
    while (wait(&status) > 0) {
    }
    int ce = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
    fprintf(stderr, "[gate0-callback-grate] app exited %d\n", ce);
    return ce == 0 ? 0 : 1;
}
