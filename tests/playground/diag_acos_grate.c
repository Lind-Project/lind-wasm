// Diagnostic: hand-written grate that interposes acos() only, and checks
// fetestexcept(FE_INVALID) *locally, inside the grate's own handler*,
// immediately after calling the real acos(). This isolates whether acos's
// internal __feraiseexcept call sets the grate's own flag at all, vs.
// whether the flag is set correctly but fails to survive until a later,
// separately-dispatched fetestexcept() call from the caller.
//
// Compile:
//   scripts/lind_compile -s --compile-grate tests/playground/diag_acos_grate.c \
//     -I tests/grate-tests/lib-interpose -lm
// Run:
//   LIND_LIBM_MODE=interposed scripts/lind_run \
//     grates/diag_acos_grate.cwasm libm_interpose_repro_acos.wasm
#include <lind_syscall.h>
#include <fenv.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

#include "../grate-tests/lib-interpose/lind_marshal.h"

static struct lind_marshal_spec acos_spec = {
    .nargs = 1,
    .args = {
        {.kind = LIND_ARG_SCALAR},
    },
    .ret = {.kind = LIND_RET_SCALAR},
};

static uint64_t handler_acos(uint64_t a, uint64_t, uint64_t, uint64_t,
                              uint64_t, uint64_t) {
  double x;
  __builtin_memcpy(&x, &a, sizeof(x));
  feclearexcept(FE_ALL_EXCEPT);
  double r = acos(x);
  printf("[Grate|diag] acos(%g) = %g\n", x, r);
  printf("[Grate|diag] local fetestexcept(FE_INVALID) right after acos: %s\n",
         fetestexcept(FE_INVALID) != 0 ? "SET" : "NOT SET");
  uint64_t bits;
  __builtin_memcpy(&bits, &r, sizeof(bits));
  return bits;
}

LIND_DEFINE_MARSHAL_HANDLER(acos, &acos_spec, handler_acos)

int64_t pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                         uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                         uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                         uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                         uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    fprintf(stderr, "[Grate|diag] invalid fn ptr\n");
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
    printf("[Grate|diag] registering acos handler for cage %d\n", cageid);
    int ret = register_lib_handler(cageid, "env", "acos", grateid,
                                    (uint64_t)(uintptr_t)&lind_mh_acos);
    if (ret != 0) {
      fprintf(stderr, "[Grate|diag] register acos failed: %d\n", ret);
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
