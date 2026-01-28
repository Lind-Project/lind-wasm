#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define LINDABORT	0xE0010001

#define ASSERT(got, exp)                                                       \
  do {                                                                         \
    printf("[%s] Got: %d | Exp: %d\n", ((got) == (exp)) ? "PASS" : "FAIL",     \
           (got), (exp));                                                      \
    if ((got) != (exp)) {                                                      \
      exit(-1);                                                                \
    }                                                                          \
  } while (0);

#define ASSERTN(got, exp)                                                      \
  do {                                                                         \
    printf("[%s] Got: %d | Exp: %d\n", ((got) != (exp)) ? "PASS" : "FAIL",     \
           (got), (exp));                                                      \
    if ((got) == (exp)) {                                                      \
      exit(-1);                                                                \
    }                                                                          \
  } while (0);

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    return -1;
  }

  int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
            uint64_t) =
      (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
               uint64_t))(uintptr_t)fn_ptr_uint;

  return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage);
}

int read_grate(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
               uint64_t);

int read_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
               uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage) {
  
  int thiscage = getpid();

  char buf[10];

  int ret = copy_data_between_cages(thiscage, arg2cage, arg2, arg2cage,
				(uint64_t)buf, thiscage, 4, 1);

  if (ret == LINDABORT) {
	 return ret; 
  }

  strcat(buf, "(C)");

  ret = copy_data_between_cages(thiscage, arg2cage, 
		  (uint64_t)buf, thiscage, arg2,
                          arg2cage, 7, 0);
 
  if (ret == LINDABORT) {
	 return ret; 
  }
  
  return 7;
}

int main(int argc, char *argv[]) {
  int grateid = getpid();

  pid_t pid = fork();

  if (pid == 0) {
    int cageid = getpid();

    uint64_t fn_ptr = (uint64_t)(uintptr_t)&read_grate;
    register_handler(cageid, 0, 1, grateid, fn_ptr);

    char _buf[10] = "Test";

    int ret = read(0, _buf, 4);
    ASSERTN(ret, LINDABORT);
    
    ASSERT(strcmp(_buf, "Test(C)"), 0);
  }

  int status;
  while (wait(&status) > 0) {
  }

  return 0;
}
