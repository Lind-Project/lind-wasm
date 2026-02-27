#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>

// Dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    fprintf(stderr, "[Grate|interpose-mmap] Invalid function ptr\n");
    assert(0);
  }

  printf("[Grate|interpose-mmap] Handling function ptr: %llu from cage: %llu\n",
         fn_ptr_uint, cageid);

  int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;

  return fn(cageid);
}

int mmap_grate(uint64_t cageid, 
    uint64_t arg1, uint64_t arg1cage, 
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage, 
    uint64_t arg4, uint64_t arg4cage, 
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
  printf("[Grate|interpose-mmap] In mmap_grate %d handler for cage: %llu\n",
         getpid(), cageid);
  int self_grate_id = getpid();
  int ret = make_threei_call(
    9, // syscallnum for mmap
    0,    // callname is not used in the trampoline, set to 0
    self_grate_id,    // self_grate_id is not used in the trampoline, set to 0
    self_grate_id,    // target_cageid is not used in the trampoline, set to 0
    arg1, arg1cage, 
    arg2, arg2cage,
    arg3, arg3cage, 
    arg4, arg4cage, 
    arg5, arg5cage,
    arg6, arg6cage,
    0 // we will handle the errno in this grate instead of translating it to -1 in the trampoline
  );
  return ret;
}

// Main function will always be same in all grates
int main(int argc, char *argv[]) {
  // Should be at least two inputs (at least one grate file and one cage file)
  if (argc < 2) {
    fprintf(stderr, "Usage: %s <cage_file> <grate_file> <cage_file> [...]\n",
            argv[0]);
    assert(0);
  }

  int grateid = getpid();

  // Because we assume that all cages are unaware of the existence of grate,
  // cages will not handle the logic of `exec`ing grate, so we need to handle
  // these two situations separately in grate. grate needs to fork in two
  // situations:
  // - the first is to fork and use its own cage;
  // - the second is when there is still at least one grate in the subsequent
  // command line input. In the second case, we fork & exec the new grate and
  // let the new grate handle the subsequent process.
  for (int i = 1; i < (argc < 3 ? argc : 3); i++) {
    pid_t pid = fork();
    if (pid < 0) {
      perror("fork failed");
      assert(0);
    } else if (pid == 0) {
      // According to input format, the odd-numbered positions will always be
      // grate, and even-numbered positions will always be cage.
      if (i % 2 != 0) {
        // Next one is cage, only set the register_handler when next one is cage
        int cageid = getpid();
        // Set the mmap (syscallnum=9) of this cage to call this grate
        // function mmap_grate 
        // Syntax of register_handler:
        // <targetcage, targetcallnum, this_grate_id, fn_ptr_u64)>
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&mmap_grate;
        printf("[Grate|interpose-mmap] Registering mmap handler for cage %d in "
               "grate %d with fn ptr addr: %llu\n",
               cageid, grateid, fn_ptr_addr);
        int ret = register_handler(cageid, 9, grateid, fn_ptr_addr);
        if (ret != 0) {
            fprintf(stderr, "[Grate|interpose-mmap] Failed to register handler for cage %d in "
                    "grate %d with fn ptr addr: %llu, ret: %d\n",
                    cageid, grateid, fn_ptr_addr, ret);
            assert(0);
          }
      }

      if (execv(argv[i], &argv[i]) == -1) {
        perror("execv failed");
        assert(0);
      }
    }
  }

  int status;
  int failed = 0;
  while (wait(&status) > 0) {
    if (status != 0) {
      fprintf(stderr, "[Grate|interpose-mmap] FAIL: child exited with status %d\n", status);
      assert(0);
    }
  }

  printf("[Grate|interpose-mmap] PASS\n");
  return 0;
}
