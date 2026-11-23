/*
 * lind-perf benchmark grate: interposes on the benchmark syscalls 2001 and 2002 with a handler
 * that returns immediately, so the measured cost is the grate round trip itself.
 * (2003 and 2004 are answered before 3i and cannot be interposed.)
 *
 * Usage: lind-boot-perf --perf grate_syscall.cwasm <libc|fdtables>_syscall.cwasm
 */
#include "lind_perf_bench.h"

#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

// Return type of the grate entry point and handlers. Must match what lind-wasm expects:
// since "Threei 64bit Return Values" (lind-wasm #1416, grates repo #236), lind-3i calls
// pass_fptr_to_wt as returning i64.
typedef int64_t grate_ret_t;

// Dispatcher: 3i re-enters the grate here with the registered handler address.
grate_ret_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    return -1;
  }
  grate_ret_t (*fn)(uint64_t) = (grate_ret_t (*)(uint64_t))(uintptr_t)fn_ptr_uint;
  return fn(cageid);
}

// No-op handler for both benchmark syscalls.
grate_ret_t noop_grate(uint64_t cageid) { return 0; }

int main(int argc, char *argv[]) {
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_file> [args...]\n", argv[0]);
    exit(EXIT_FAILURE);
  }

  int grateid = getpid();

  pid_t pid = fork();
  if (pid < 0) {
    perror("fork failed");
    exit(EXIT_FAILURE);
  } else if (pid == 0) {
    int cageid = getpid();
    uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&noop_grate;

    // <targetcage, targetcallnum, this_grate_id, fn_ptr_u64>
    if (register_handler(cageid, LIBC_SYSCALL_NUM, grateid, fn_ptr_addr) != 0 ||
        register_handler(cageid, FDTABLES_SYSCALL_NUM, grateid, fn_ptr_addr) != 0) {
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
