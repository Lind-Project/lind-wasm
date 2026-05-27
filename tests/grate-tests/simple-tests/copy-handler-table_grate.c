#include <assert.h>
#include <errno.h>
#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

#define GETEUID_SYSCALL_NUM 107
#define EXPECTED_EUID 123

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|copy-handler-table] Invalid function ptr\n");
        assert(0);
    }

    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

int geteuid_grate(uint64_t cageid) {
    printf("[Grate|copy-handler-table] geteuid handler invoked for cage %llu\n",
           cageid);
    return EXPECTED_EUID;
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_file>\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();

    pid_t pid = fork();
    if (pid < 0) {
        perror("fork failed");
        assert(0);
    } else if (pid == 0) {
        int cageid = getpid();
        uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;

        printf("[Grate|copy-handler-table] Registering geteuid handler for cage %d in grate %d\n",
               cageid, grateid);

        int ret = register_handler(cageid, GETEUID_SYSCALL_NUM, grateid, fn_ptr_addr);
        if (ret != 0) {
            fprintf(stderr,
                    "[Grate|copy-handler-table] FAIL: register_handler returned %d\n",
                    ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status = 0;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr,
                    "[Grate|copy-handler-table] FAIL: child exited with status %d\n",
                    status);
            assert(0);
        }
    }

    printf("[Grate|copy-handler-table] PASS\n");
    return 0;
}
