// DESCRIPTION: fork() then child reads 100% of inherited memory without writing. cow-design.md §26 case B -- the case that distinguishes shared-backing COW (zero content copy) from the UFFD_MISSING alternative (a read-only scan would still copy everything, §28).
#include "../../benchmarks/bench.h"
#include "cow_bench.h"
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

int main(void) {
    for (int s = 0; s < COW_BENCH_SIZE_COUNT; s++) {
        int mib = cow_bench_sizes_mib[s];
        if (mib == 0) continue; /* nothing to read */
        int loops = cow_bench_loops(mib);
        size_t bytes = (size_t)mib * 1024 * 1024;
        unsigned char *buf = malloc(bytes);
        cow_bench_touch(buf, bytes);

        long long start = gettimens();
        for (int i = 0; i < loops; i++) {
            pid_t pid = fork();
            if (pid == 0) {
                /* the exit code reads back the sum so the read loop can't
                 * be optimized away as dead computation (same pitfall as
                 * cow_bench_touch -- see its comment). */
                volatile unsigned char sum = 0;
                for (size_t off = 0; off < bytes; off += COW_BENCH_PAGE) {
                    sum ^= buf[off];
                }
                _exit(sum & 0x7F);
            }
            int status;
            waitpid(pid, &status, 0);
        }
        long long end = gettimens();
        emit_result("ForkReadAll", mib, (end - start) / loops, loops);

        free(buf);
    }
    return 0;
}
