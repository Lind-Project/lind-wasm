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
    fprintf(stderr, "[Grate|geteuid] Invalid function ptr\n");
    return -1;
  }

  int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;

  return fn(cageid);
}

// Function ptr and signatures of this grate
int geteuid_grate(uint64_t);

int geteuid_grate(uint64_t cageid) {
  return 10;
}

// Main function will always be same in all grates
int main(int argc, char *argv[]) {
  // Should be at least two inputs (at least one grate file and one cage file)
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_file> <grate_file> <cage_file> [...]\n",
            argv[0]);
    exit(EXIT_FAILURE);
  }

  int grateid = getpid();

  for (int i = 1; i < argc; i++) {
    pid_t pid = fork();
    if (pid < 0) {
      perror("fork failed");
      exit(EXIT_FAILURE);
    } else if (pid == 0) {
      int cageid = getpid();
      // Set the geteuid (syscallnum=107) of this cage to call this grate
      // function geteuid_grate (func index=0) Syntax of register_handler:
      // <targetcage, targetcallnum, handlefunc_flag (deregister(0) or register
      // (non-zero), this_grate_id, fn_ptr_u64)>
      uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;
      printf("[Grate|geteuid] Registering geteuid handler for cage %d in "
              "grate %d with fn ptr addr: %llu\n",
              cageid, grateid, fn_ptr_addr);
      int ret = register_handler(cageid, 107, 1, grateid, fn_ptr_addr);

      if (execv(argv[i], &argv[i]) == -1) {
        perror("execv failed");
        exit(EXIT_FAILURE);
      }
    }
  }

  int status;
  while (wait(&status) > 0) {
    printf("[Grate|geteuid] terminated, status: %d\n", status);
  }

  return 0;
}
