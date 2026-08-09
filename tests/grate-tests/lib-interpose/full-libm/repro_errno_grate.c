// Minimal hand-written grate (NOT auto-generated) that interposes libm's
// cos() only, calling the real, statically-linked cos and passing its
// result straight through. Used to test cross-cage errno propagation
// (see wasmtime's linker.rs instance_dylink portal / threei's
// take_last_grate_errno) in isolation from the full auto-generated
// libm_full_grate.c, whose 900+ interposed symbols make it hard to isolate
// unrelated build issues.
//
// Compile:
//   scripts/lind_compile -s --compile-grate repro_errno_grate.c \
//     -I../ -lm -- -DLIND_MARSHAL_NO_LIBC_HEADERS=0
//
// Run (from repo root, after copying grate .cwasm to lindfs/grates/ and
// the cage .wasm to lindfs/):
//   LIND_LIBM_MODE=interposed scripts/lind_run grates/repro_errno_grate.cwasm repro_errno_propagation.wasm
#include <lind_syscall.h>
#include <errno.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../lind_marshal.h"

static struct lind_marshal_spec cos_spec = {
    .nargs = 1,
    .args = {
        {.kind = LIND_ARG_SCALAR},
    },
    .ret = {.kind = LIND_RET_SCALAR},
};

static uint64_t handler_cos(uint64_t a, uint64_t, uint64_t, uint64_t,
                             uint64_t, uint64_t) {
  double x;
  __builtin_memcpy(&x, &a, sizeof(x));
  double r = cos(x);
  /* Reading errno in the very first statement after the call unreliably
   * reads back 0 on this target (see repro_errno_propagation.c's header
   * comment for the bisected pattern) -- an intervening call first, then
   * reading errno directly as a later printf argument, is reliable. */
  printf("[Grate|repro_errno] cos(%g) = %g\n", x, r);
  printf("[Grate|repro_errno] errno = %d\n", errno);
  uint64_t bits;
  __builtin_memcpy(&bits, &r, sizeof(bits));
  return bits;
}

LIND_DEFINE_MARSHAL_HANDLER(cos, &cos_spec, handler_cos)

int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                         uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                         uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                         uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                         uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    fprintf(stderr, "[Grate|repro_errno] invalid fn ptr\n");
    assert(0);
  }
  uint64_t (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                 uint64_t) =
      (uint64_t(*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                   uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
                   uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;
  return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage, arg4,
            arg4cage, arg5, arg5cage, arg6, arg6cage);
}

int main(int argc, char *argv[]) {
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_wasm>\n", argv[0]);
    assert(0);
  }

  int grateid = getpid();
  pid_t pid = fork();
  if (pid < 0) {
    perror("fork");
    assert(0);
  }

  if (pid == 0) {
    int cageid = getpid();
    printf("[Grate|repro_errno] registering cos handler for cage %d\n",
           cageid);
    int ret = register_lib_handler(cageid, "env", "cos", grateid,
                                    (uint64_t)(uintptr_t)&lind_mh_cos);
    if (ret != 0) {
      fprintf(stderr, "[Grate|repro_errno] register cos failed: %d\n", ret);
      assert(0);
    }
    if (execv(argv[1], &argv[1]) == -1) {
      perror("execv");
      assert(0);
    }
  }

  int status;
  while (wait(&status) > 0) {
  }
  int child_exit = WIFEXITED(status) ? WEXITSTATUS(status) : -1;
  return child_exit;
}
