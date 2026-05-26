// Grate for libc-strlen interposition test.
// Intercepts strlen() and returns len*2 to prove both interposition and
// that the grate can read the cage's string buffer via copy_data_between_cages.
#include <lind_syscall.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <assert.h>
#include <stdint.h>

int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                    uint64_t arg1, uint64_t arg1cage,
                    uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage,
                    uint64_t arg4, uint64_t arg4cage,
                    uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
    if (fn_ptr_uint == 0) {
        fprintf(stderr, "[Grate|libc-strlen] invalid fn ptr\n");
        assert(0);
    }
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

// Handler for strlen(const char *s).
// arg1     = guest pointer to the string (in arg1cage's address space)
// arg1cage = cage that owns arg1
// Returns real_len * 2 to prove interposition and buffer access.
int strlen_handler(uint64_t cageid,
    uint64_t arg1, uint64_t arg1cage,
    uint64_t arg2, uint64_t arg2cage,
    uint64_t arg3, uint64_t arg3cage,
    uint64_t arg4, uint64_t arg4cage,
    uint64_t arg5, uint64_t arg5cage,
    uint64_t arg6, uint64_t arg6cage) {

    int thiscage = getpid();
    char buf[256] = {0};

    // Copy the string from the cage's address space into grate-local buffer.
    copy_data_between_cages(thiscage, arg1cage,
                            arg1, arg1cage,
                            (uint64_t)buf, thiscage,
                            255, 1); // copytype=1: null-terminated string

    int real_len = strlen(buf);
    printf("[Grate|libc-strlen] strlen_handler: cage=%llu str=\"%s\" "
           "real_len=%d returning %d\n",
           (unsigned long long)cageid, buf, real_len, real_len * 2);
    return real_len * 2;
}

int main(int argc, char *argv[]) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage_wasm>\n", argv[0]);
        assert(0);
    }

    int grateid = getpid();

    pid_t pid = fork();
    if (pid < 0) { perror("fork failed"); assert(0); }

    if (pid == 0) {
        int cageid = getpid();
        uint64_t fn = (uint64_t)(uintptr_t)&strlen_handler;

        printf("[Grate|libc-strlen] registering strlen handler for cage %d\n", cageid);
        int ret = register_lib_handler(cageid, "env", "strlen",
                                       LIBCALL_BASE + 1, grateid, fn);
        if (ret != 0) {
            fprintf(stderr, "[Grate|libc-strlen] register strlen failed: %d\n", ret);
            assert(0);
        }

        if (execv(argv[1], &argv[1]) == -1) {
            perror("execv failed");
            assert(0);
        }
    }

    int status;
    while (wait(&status) > 0) {
        if (status != 0) {
            fprintf(stderr, "[Grate|libc-strlen] FAIL: child exited with status %d\n", status);
            assert(0);
        }
    }

    printf("[Grate|libc-strlen] PASS\n");
    return 0;
}
