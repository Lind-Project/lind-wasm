#ifndef COW_BENCH_H
#define COW_BENCH_H
#include <stddef.h>

#define COW_BENCH_PAGE 4096u

/* MiB sizes swept by the cow-tests benchmarks. 0 isolates the fixed
 * per-fork floor from the cost of copying/sharing touched memory
 * (cow-design.md §0.5 / §26 case F); 512 is included because at smaller
 * sizes the fixed floor dominates and hides the linear-in-size copy cost
 * these benchmarks exist to show -- see tests/cow-tests/baseline/ for the
 * measured breakdown. */
static const int cow_bench_sizes_mib[] = {0, 4, 32, 128, 512};
#define COW_BENCH_SIZE_COUNT ((int)(sizeof(cow_bench_sizes_mib) / sizeof(cow_bench_sizes_mib[0])))

static inline int cow_bench_loops(int mib) {
    if (mib <= 0) return 30;
    if (mib <= 4) return 25;
    if (mib <= 32) return 15;
    if (mib <= 128) return 8;
    return 5;
}

/*
 * Dirty one byte per host page across [buf, buf+len) through a volatile
 * pointer.
 *
 * This is a deliberate workaround for a real measurement pitfall hit
 * while calibrating these benchmarks: a plain memset(buf, val, sz) whose
 * destination was never read afterward was silently removed by dead
 * store elimination, making every size report the same ~15ms floor
 * regardless of how much memory was supposedly touched (0 MiB and 512
 * MiB measured within noise of each other). A volatile store cannot be
 * eliminated by a conforming compiler, so this guarantees the pages are
 * genuinely made resident/dirty -- which is what determines fork's real
 * copy cost -- while staying cheap (one store per page, not per byte),
 * since page residency, not byte content, is what fork's copy path
 * actually operates on.
 */
static inline void cow_bench_touch(unsigned char *buf, size_t len) {
    volatile unsigned char *vbuf = (volatile unsigned char *)buf;
    for (size_t off = 0; off < len; off += COW_BENCH_PAGE) {
        vbuf[off] = (unsigned char)(off >> 12);
    }
    if (len > 0) {
        vbuf[len - 1] = 0xFF; /* also dirty the last (possibly partial) page */
    }
}

#endif /* COW_BENCH_H */
