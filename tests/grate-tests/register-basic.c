#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define ASSERT(got, exp)                                                       \
  do {                                                                         \
    printf("[%s] Got: %d | Exp: %d\n", ((got) == (exp)) ? "PASS" : "FAIL",     \
           (got), (exp));                                                      \
    if ((got) != (exp)) {                                                      \
      exit(-1);                                                                \
    }                                                                          \
  } while (0);

static int geteuid_orig = -1;

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    return -1;
  }

  int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;

  return fn(cageid);
}

int geteuid_grate(uint64_t);

int geteuid_grate(uint64_t cageid) { return geteuid_orig + 1; }

int main(int argc, char *argv[]) {
  int grateid = getpid();

  geteuid_orig = geteuid();

  ASSERT(geteuid(), geteuid_orig);

  uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;
  int ret = register_handler(grateid, 107, 1, grateid, fn_ptr_addr);

  ASSERT(ret, 0);
  ASSERT(geteuid(), geteuid_orig + 1);

  return 0;
}
