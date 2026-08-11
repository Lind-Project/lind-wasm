// DESCRIPTION: fork() immediately followed by exec() in the child, parametrized by MiB of parent memory made resident before forking. cow-design.md §26 cases A and F -- MiB=0 isolates the fixed per-fork floor (e.g. the static-cage grate-worker arena, §0.5) from the cost of copying touched memory.
#include "../../benchmarks/bench.h"
#include "cow_bench.h"
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    if (argc > 1 && strcmp(argv[1], "--child") == 0) {
        /* post-exec re-entry: nothing to do, the point is that exec() happened */
        return 0;
    }

    for (int s = 0; s < COW_BENCH_SIZE_COUNT; s++) {
        int mib = cow_bench_sizes_mib[s];
        int loops = cow_bench_loops(mib);
        size_t bytes = (size_t)mib * 1024 * 1024;
        unsigned char *buf = bytes ? malloc(bytes) : NULL;
        if (bytes) cow_bench_touch(buf, bytes);

        long long start = gettimens();
        for (int i = 0; i < loops; i++) {
            pid_t pid = fork();
            if (pid == 0) {
                char *child_argv[] = {argv[0], "--child", NULL};
                execv(argv[0], child_argv);
                _exit(127); /* only reached if execv failed */
            }
            int status;
            waitpid(pid, &status, 0);
        }
        long long end = gettimens();
        emit_result("ForkExec", mib, (end - start) / loops, loops);

        if (buf) free(buf);
    }
    return 0;
}
