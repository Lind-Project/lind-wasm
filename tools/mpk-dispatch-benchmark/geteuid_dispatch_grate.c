#include <errno.h>
#include <lind_syscall.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

long long pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid,
                         uint64_t arg1, uint64_t arg1cage,
                         uint64_t arg2, uint64_t arg2cage,
                         uint64_t arg3, uint64_t arg3cage,
                         uint64_t arg4, uint64_t arg4cage,
                         uint64_t arg5, uint64_t arg5cage,
                         uint64_t arg6, uint64_t arg6cage) {
    if (!fn_ptr_uint) {
        fprintf(stderr, "geteuid grate: invalid function pointer\n");
        _exit(1);
    }
    int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
    return fn(cageid);
}

int geteuid_grate(uint64_t cageid) {
    return 10;
}

int main(int argc, char **argv) {
    if (argc < 2) {
        fprintf(stderr, "Usage: %s <cage> [cage arguments]\n", argv[0]);
        return 2;
    }
    int grateid = getpid();
    pid_t pid = fork();
    if (pid < 0) {
        perror("fork");
        return 1;
    }
    if (pid == 0) {
        int ret = register_handler(getpid(), 107, grateid,
                                   (uint64_t)(uintptr_t)&geteuid_grate);
        if (ret != 0) {
            fprintf(stderr, "register_handler failed: %d\n", ret);
            _exit(1);
        }
        execv(argv[1], &argv[1]);
        perror("execv");
        _exit(1);
    }
    int status;
    pid_t waited;
    for (;;) {
        /* Blocking Lind waitpid busy-yields and competes with the pinned cage.
           WNOHANG is implemented for the any-child (-1) path. */
        waited = waitpid(-1, &status, WNOHANG);
        if (waited > 0)
            break;
        if (waited < 0) {
            if (errno == EINTR)
                continue;
            break;
        }
        struct timespec delay = {.tv_nsec = 10000000};
        while (nanosleep(&delay, &delay) < 0) {
            if (errno != EINTR) {
                perror("nanosleep");
                return 1;
            }
        }
    }
    if (waited < 0) {
        perror("waitpid");
        return 1;
    }
    if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
        fprintf(stderr, "benchmark cage failed: wait status %d\n", status);
        return 1;
    }
    return 0;
}
