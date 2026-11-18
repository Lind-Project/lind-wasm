/*
 * lind-perf benchmark grate that forwards: interposes on the benchmark syscalls 2001 and 2002
 * and passes each call straight on to RawPOSIX, the way a real grate does after its own work
 * (same pattern as tests/grate-tests/interposing-calls/interpose-mmap_grate.c).
 *
 * Compared with the plain benchmarks, this measures what a grate in the path costs:
 *   libc_syscall.cwasm                         cage -> RawPOSIX -> kernel
 *   grate_forward.cwasm libc_syscall.cwasm     cage -> grate -> RawPOSIX -> kernel
 *
 * Usage: lind-boot-perf --perf grate_forward.cwasm <libc|fdtables>_syscall.cwasm
 */
#include "lind_perf_bench.h"

#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

// Must match what lind-3i expects: pass_fptr_to_wt returns i64 (lind-wasm #1416).
typedef int64_t grate_ret_t;

typedef grate_ret_t (*handler_t)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                                 uint64_t);

// This grate's cage id, looked up once in main() before the cage starts.
static uint64_t self_grate_id;

// Dispatcher: 3i re-enters the grate here with the registered handler address.
grate_ret_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                            uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                            uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                            uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                            uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    return -1;
  }
  handler_t fn = (handler_t)(uintptr_t)fn_ptr_uint;
  return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4, arg4cage, arg5,
            arg5cage, arg6, arg6cage);
}

// Forward `num` to the next handler (RawPOSIX), on behalf of the calling cage. The arguments
// and their cage ids are passed through unchanged; the calling cage is arg1cage.
static inline grate_ret_t forward(unsigned int num, uint64_t arg1, uint64_t arg1cage,
                                  uint64_t arg2, uint64_t arg2cage, uint64_t arg3,
                                  uint64_t arg3cage, uint64_t arg4, uint64_t arg4cage,
                                  uint64_t arg5, uint64_t arg5cage, uint64_t arg6,
                                  uint64_t arg6cage) {
  return make_threei_call(num, 0, self_grate_id, arg1cage, arg1, arg1cage, arg2, arg2cage,
                          arg3, arg3cage, arg4, arg4cage, arg5, arg5cage, arg6, arg6cage,
                          0 /* raw result, no errno translation */);
}

grate_ret_t forward_libc(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
                         uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                         uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage, uint64_t arg6,
                         uint64_t arg6cage) {
  return forward(LIBC_SYSCALL_NUM, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
                 arg4cage, arg5, arg5cage, arg6, arg6cage);
}

grate_ret_t forward_fdtables(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
                             uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage,
                             uint64_t arg4, uint64_t arg4cage, uint64_t arg5,
                             uint64_t arg5cage, uint64_t arg6, uint64_t arg6cage) {
  return forward(FDTABLES_SYSCALL_NUM, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
                 arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_file> [args...]\n", argv[0]);
    exit(EXIT_FAILURE);
  }

  int grateid = getpid();
  self_grate_id = (uint64_t)grateid;

  pid_t pid = fork();
  if (pid < 0) {
    perror("fork failed");
    exit(EXIT_FAILURE);
  } else if (pid == 0) {
    int cageid = getpid();

    // <targetcage, targetcallnum, this_grate_id, fn_ptr_u64>
    if (register_handler(cageid, LIBC_SYSCALL_NUM, grateid,
                         (uint64_t)(uintptr_t)&forward_libc) != 0 ||
        register_handler(cageid, FDTABLES_SYSCALL_NUM, grateid,
                         (uint64_t)(uintptr_t)&forward_fdtables) != 0) {
      fprintf(stderr, "register_handler failed\n");
      exit(EXIT_FAILURE);
    }

    if (execv(argv[1], &argv[1]) == -1) {
      perror("execv failed");
      exit(EXIT_FAILURE);
    }
  }

  int status;
  int failed = 0;
  while (wait(&status) > 0) {
    if (status != 0) {
      failed = 1;
    }
  }
  return failed;
}
