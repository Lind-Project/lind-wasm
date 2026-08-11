// DESCRIPTION: recursive fork A -> B -> C, each generation dirtying 5% of memory before forking again. cow-design.md §26 case E and cow-implementation-plan.md Milestone 2 -- measures whether fork cost/behavior degrades across a fork chain (it should not, under COW, since B's divergence doesn't require materializing A's full range again for C).
#include "../../benchmarks/bench.h"
#include "cow_bench.h"
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    for (int s = 0; s < COW_BENCH_SIZE_COUNT; s++) {
        int mib = cow_bench_sizes_mib[s];
        if (mib == 0) continue; /* nothing to fork over */
        int loops = cow_bench_loops(mib) / 2;
        if (loops < 3) loops = 3;
        size_t bytes = (size_t)mib * 1024 * 1024;
        size_t five_pct = bytes / 20;

        long long start = gettimens();
        for (int i = 0; i < loops; i++) {
            unsigned char *buf = malloc(bytes);
            cow_bench_touch(buf, bytes);

            pid_t pid_b = fork();
            if (pid_b == 0) {
                /* generation B: dirty the first 5% */
                cow_bench_touch(buf, five_pct);

                pid_t pid_c = fork();
                if (pid_c == 0) {
                    /* generation C: dirty the NEXT 5% */
                    cow_bench_touch(buf + five_pct, five_pct);
                    _exit(0);
                }
                int status_c;
                waitpid(pid_c, &status_c, 0);
                _exit(0);
            }
            int status_b;
            waitpid(pid_b, &status_b, 0);
            free(buf);
        }
        long long end = gettimens();
        emit_result("ForkRecursiveChain", mib, (end - start) / loops, loops);
    }
    return 0;
}
