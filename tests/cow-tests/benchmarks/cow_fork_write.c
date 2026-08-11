// DESCRIPTION: fork() then child writes a fraction of inherited memory. cow-design.md §26 cases C and D. Uses cow_bench_touch (a volatile per-page write loop) rather than memset for the child's write -- see cow_bench.h -- so absolute numbers include a non-vectorized write cost on top of fork's own cost. Compare ForkWrite* between an eager run and a cow run of THIS SAME binary, not against an idealized memset benchmark.
#include "../../benchmarks/bench.h"
#include "cow_bench.h"
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>

static void run_write_bench(char *label, int mib, size_t write_bytes) {
    int loops = cow_bench_loops(mib);
    size_t bytes = (size_t)mib * 1024 * 1024;
    unsigned char *buf = malloc(bytes);
    cow_bench_touch(buf, bytes);

    long long start = gettimens();
    for (int i = 0; i < loops; i++) {
        pid_t pid = fork();
        if (pid == 0) {
            cow_bench_touch(buf, write_bytes);
            _exit(0);
        }
        int status;
        waitpid(pid, &status, 0);
    }
    long long end = gettimens();
    emit_result(label, mib, (end - start) / loops, loops);

    free(buf);
}

int main(void) {
    for (int s = 0; s < COW_BENCH_SIZE_COUNT; s++) {
        int mib = cow_bench_sizes_mib[s];
        if (mib == 0) continue; /* nothing to write */
        size_t bytes = (size_t)mib * 1024 * 1024;
        run_write_bench("ForkWrite1pct", mib, bytes / 100);
        run_write_bench("ForkWrite100pct", mib, bytes);
    }
    return 0;
}
