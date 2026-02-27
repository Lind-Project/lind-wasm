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
    fprintf(stderr, "[Grate|interpose-register] Invalid function ptr=%llu\n", fn_ptr_uint);
    assert(0);
  }

  printf("[Grate|interpose-register] Handling function ptr: %llu from cage: %llu\n",
         fn_ptr_uint, cageid);

  int (*fn)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
          uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
          uint64_t, uint64_t, uint64_t) =
    (int (*)(uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t, uint64_t, uint64_t,
              uint64_t, uint64_t, uint64_t))(uintptr_t)fn_ptr_uint;


  return fn(cageid, arg1, arg1cage, arg2, arg2cage,
            arg3, arg3cage, arg4, arg4cage,
            arg5, arg5cage, arg6, arg6cage);
}

int geteuid_grate(uint64_t cageid, 
    uint64_t arg1, uint64_t arg1cage, 
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage, 
    uint64_t arg4, uint64_t arg4cage, 
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
  printf("[Grate|interpose-register] In register_grate %d handler for cage: %llu\n",
         getpid(), cageid);
  return 10;
}

// We want to register a handler for geteuid (syscall num 107) in child cage, but also 
// monitor the register_handler behaviors, and the blow handler function
// will redirect the register_handler call from cage to this grate, attach the function ptr
// as the arg and then this grate will call the register_handler syscall to register 
// the handler in the target cage.
int register_grate(uint64_t cageid, 
    uint64_t arg1, uint64_t arg1cage, 
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage, 
    uint64_t arg4, uint64_t arg4cage, 
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {
    printf("[Grate|interpose-register] In register_grate %d handler for cage: %llu\n",
            getpid(), cageid);
    int self_grate_id = getpid();
    uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;
    printf("[Grate|geteuid] Registering geteuid handler for cage %d in "
            "grate %d with fn ptr addr: %llu\n",
            cageid, self_grate_id, fn_ptr_addr);
    int ret = make_threei_call(
        1001, // syscallnum for register_handler
        0,    // callname is not used in the trampoline, set to 0
        self_grate_id,    // self_grate_id is used in the 3i
        self_grate_id,    // target_cageid is used in the 3i
        arg1, arg1cage, 
        arg2, arg2cage,
        fn_ptr_addr, arg3cage, 
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

  for (int i = 1; i < (argc < 3 ? argc : 3); i++) {
    pid_t pid = fork();
    if (pid < 0) {
      perror("fork failed");
      assert(0);
    } else if (pid == 0) {
        // Next one is cage, only set the register_handler when next one is cage
        int cageid = getpid();
        // Set the register_handler (syscallnum=1001) of this cage to call this grate
        // function register_grate 
        // Syntax of register_handler:
        // <targetcage, targetcallnum, this_grate_id, fn_ptr_u64)>
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&register_grate;
        printf("[Grate|interpose-register] Registering register_handler for cage %d in "
                "grate %d with fn ptr addr: %llu\n",
                cageid, grateid, fn_ptr_addr);
        int ret = register_handler(cageid, 1001, grateid, fn_ptr_addr);
        if (ret != 0) {
            fprintf(stderr, "[Grate|interpose-register] Failed to register handler for cage %d in "
                    "grate %d with fn ptr addr: %llu, ret: %d\n",
                    cageid, grateid, fn_ptr_addr, ret);
            assert(0);
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
      fprintf(stderr, "[Grate|interpose-register] FAIL: child exited with status %d\n", status);
      assert(0);
    }
  }

  printf("[Grate|interpose-register] PASS\n");
  return 0;
}
