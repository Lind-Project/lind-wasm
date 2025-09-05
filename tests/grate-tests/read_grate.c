#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

// Dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    return -1;
  }

  int (*fn)(uint64_t, uint64_t, uint64_t,
                uint64_t, uint64_t, uint64_t,
                uint64_t) = (int (*)(uint64_t, uint64_t, uint64_t,
                uint64_t, uint64_t, uint64_t,
                uint64_t))(uintptr_t)fn_ptr_uint;

  return fn(cageid, arg1, arg1cage, arg2, arg2cage, arg3, arg3cage);
}

int read_grate(uint64_t, uint64_t, uint64_t,
                uint64_t, uint64_t, uint64_t,
                uint64_t);

int read_grate(uint64_t cageid, uint64_t arg1, uint64_t arg1cage, uint64_t arg2,
               uint64_t arg2cage, uint64_t arg3, uint64_t arg3cage) {
  int thiscage = getpid();

  int fd = (int)arg1;
  size_t count = (size_t)arg3;

  char *buf = "Hello";
  int ret = 5;

  if (arg2 != 0) {
    copy_data_between_cages(thiscage, arg2cage, (uint64_t)buf, thiscage, arg2,
                            arg2cage, ret,
                            0 // Use copytype 0 so read exactly count
                              // bytes instead of stopping at '\0'
    );
  }

  return ret;
}

// Main function will always be same in all grates
int main(int argc, char *argv[]) {
  // Should be at least two inputs (at least one grate file and one cage file)
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_file>\n",
            argv[0]);
    exit(EXIT_FAILURE);
  }

  int grateid = getpid();

  pid_t pid = fork();
  if (pid < 0) {
    perror("fork failed");
    exit(EXIT_FAILURE);
  } else if (pid == 0) {
    int cageid = getpid();
    uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&read_grate;
    printf("[Grate|read] Registering read handler for cage %d in "
           "grate %d with fn ptr addr: %llu\n",
           cageid, grateid, fn_ptr_addr);
    int ret = register_handler(cageid, 0, 1, grateid, fn_ptr_addr);

    if (execv(argv[1], &argv[1]) == -1) {
      perror("execv failed");
      exit(EXIT_FAILURE);
    }
  }

  int status;
  while (wait(&status) > 0) {
    printf("[Grate|read] terminated, status: %d\n", status);
  }

  return 0;
}
