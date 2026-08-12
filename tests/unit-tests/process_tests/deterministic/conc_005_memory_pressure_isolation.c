/*
 * CONC-005b: per-cage memory pressure isolation.
 *
 * The memory counterpart to CONC-005a. One cage allocates until its memory
 * quota refuses it, holds that state, and a second cage must go on
 * allocating and computing correctly throughout. Exhausting A's quota must
 * not make B's allocation fail, and releasing A must restore it rather
 * than leaving it wedged.
 *
 * A bounds itself first with setrlimit(RLIMIT_AS) rather than allocating
 * until it dies: lind reserves every cage's linear memory at the wasm32
 * ceiling (4 GiB) up front, so "allocate until it fails" would try to
 * commit multiple GiB on the host, taking out the CI container rather
 * than testing it. This turns the exhaustion point into a known quantity
 * (LIMIT_MB) and is the same call on both sides of the harness's diff:
 * native glibc lowers a real RLIMIT_AS, lind lowers the cage's rawposix
 * quota. Lowering is the only direction available to a guest: the
 * ceiling itself belongs to the runtime (--max-cage-memory).
 *
 * If setrlimit is refused, or the allocator never reaches the limit, A
 * records that and skips only the assertions that require a failed
 * allocation; the isolation and recovery checks still run and the PASS
 * line is identical either way (same shape as CONC-005a's fd cap).
 *
 * B does not merely allocate: it fills its buffer from the same
 * deterministic generator the other CONC tests use and checksums it, so a
 * runtime that satisfied B's allocation from memory already handed to A
 * shows up as a checksum mismatch rather than a silent pass.
 *
 * Determinism: exactly one line on stdout ("CONC-005b PASS\n"). No pids,
 * clocks, addresses, sizes, allocation counts, or errno values are ever
 * printed or compared; how far A gets before failing legitimately
 * differs between native and lind. Diagnostics go to fd 2, which the
 * harness surfaces only on a nonzero exit.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <unistd.h>

/* A's self-imposed address-space bound. Large enough to clear whatever the
 * runtime and libc have already mapped at startup (lind commits roughly
 * 8 MiB before main() runs), small enough that reaching it costs little. */
#define LIMIT_MB 48

/* A's allocation unit, and the ceiling on how many it will attempt. The
 * cap keeps a run bounded even where the limit cannot be applied at all:
 * without it, an unbounded loop is precisely the CI hazard described
 * above. */
#define CHUNK   (1024 * 1024)
#define MAX_BLK (LIMIT_MB * 4)

/* B's modest buffer, deliberately tiny next to A's footprint. */
#define B_BYTES (256 * 1024)

#define PAGE   4096
#define RECORD 16

/* Deterministic per-(a,c) byte pattern (same shape as conc_002/003/004). */
static void make_record(unsigned char *b, int a, int c)
{
    unsigned s = (unsigned)(a + 1) * 2654435761u + (unsigned)(c & 0xff) * 40503u;
    int k;
    for (k = 0; k < RECORD - 2; k++)
        b[k] = (unsigned char)((s >> ((k & 3) * 8)) + (unsigned)k);
    b[RECORD - 2] = (unsigned char)(0xA0 | (a & 0x0f));
    b[RECORD - 1] = (unsigned char)(c & 0xff);
}

/* Order-sensitive checksum: a buffer that is correct except for two
 * transposed records still fails. */
static unsigned long checksum(const unsigned char *p, size_t n)
{
    unsigned long h = 1469598103u;
    size_t i;
    for (i = 0; i < n; i++) {
        h ^= p[i];
        h *= 16777619u;
        h &= 0xffffffffUL;
    }
    return h;
}

/* EINTR-retrying wrappers (same rationale as conc_003's header). */
static ssize_t xread(int fd, void *buf, size_t n)
{
    ssize_t r;
    do {
        r = read(fd, buf, n);
    } while (r < 0 && errno == EINTR);
    return r;
}

static ssize_t xwrite(int fd, const void *buf, size_t n)
{
    ssize_t r;
    do {
        r = write(fd, buf, n);
    } while (r < 0 && errno == EINTR);
    return r;
}

static pid_t xwaitpid(pid_t pid, int *status)
{
    pid_t r;
    do {
        r = waitpid(pid, status, 0);
    } while (r < 0 && errno == EINTR);
    return r;
}

static void expect_exit0(int status, const char *who)
{
    if (WEXITSTATUS(status) == 0)
        return;
    {
        char m[128];
        int n = snprintf(m, sizeof m, "conc_005b FAIL child=%s exit=%d\n", who,
                         WEXITSTATUS(status));
        write(2, m, (size_t)n);
    }
    assert(0 && "child reported a failure");
}

/* See the identical helper in conc_005a: a failing child stops acking, so
 * recovering its exit code is the only way to learn which check tripped.
 * fd 2 is surfaced only on a nonzero exit and cannot perturb the diff. */
static void expect_ack(int ackfd, pid_t child, const char *phase)
{
    char buf;
    int status = 0;
    char m[160];
    int n;

    if (xread(ackfd, &buf, 1) == 1)
        return;

    if (xwaitpid(child, &status) == child && WIFEXITED(status))
        n = snprintf(m, sizeof m, "conc_005b FAIL no-ack phase=%s child_exit=%d\n", phase,
                     WEXITSTATUS(status));
    else
        n = snprintf(m, sizeof m, "conc_005b FAIL no-ack phase=%s child_status=%d\n", phase,
                     status);
    write(2, m, (size_t)n);
    assert(0 && "child exited before acking");
}

typedef struct {
    int gate[2];
    int ack[2];
} barrier_t;

static int barrier_init(barrier_t *b)
{
    if (pipe(b->gate) != 0)
        return -1;
    if (pipe(b->ack) != 0)
        return -1;
    return 0;
}

/* ------------------------------------------------------------------ */
/* Child A: bound itself, allocate to the bound, hold, then release.   */
/* ------------------------------------------------------------------ */

/* A's blocks live in BSS: a forked child must not rely on the allocator to
 * track the very allocations it is stress-testing. */
static char *g_blk[MAX_BLK];

static void child_a(barrier_t *b)
{
    struct rlimit rl;
    int bounded = 0;
    int hit_limit = 0;
    int nblk = 0;
    char one = 1;
    char buf;

    close(b->gate[1]);
    close(b->ack[0]);

    /* Bound this cage's address space so exhaustion is cheap and
     * deterministic. Best-effort: a refusal only costs us the
     * limit-specific assertions below, never the isolation ones. */
    if (getrlimit(RLIMIT_AS, &rl) == 0) {
        rlim_t want = (rlim_t)LIMIT_MB * 1024 * 1024;
        if (rl.rlim_cur == RLIM_INFINITY || rl.rlim_cur > want) {
            struct rlimit nl;
            nl.rlim_cur = want;
            nl.rlim_max = rl.rlim_max;
            if (setrlimit(RLIMIT_AS, &nl) == 0) {
                struct rlimit chk;
                /* Trust the readback, not the return code: a setrlimit that
                 * reports success without applying anything is exactly the
                 * failure mode this has to distinguish. */
                if (getrlimit(RLIMIT_AS, &chk) == 0 && chk.rlim_cur == want)
                    bounded = 1;
            }
        }
    }

    /* Allocate toward the bound, touching one byte per page so the pages
     * are genuinely committed rather than merely reserved. */
    while (nblk < MAX_BLK) {
        char *p = (char *)malloc(CHUNK);
        int i;
        if (p == NULL) {
            hit_limit = 1;
            break;
        }
        for (i = 0; i < CHUNK; i += PAGE)
            p[i] = (char)(nblk & 0x7f);
        g_blk[nblk++] = p;
    }

    if (bounded && !hit_limit)
        _exit(30); /* bounded to LIMIT_MB but MAX_BLK never reached it */

    if (hit_limit) {
        /* (A1) The refusal must be reported as such: a NULL from malloc,
         * with ENOMEM. A runtime that let the allocation "succeed" and
         * handed back memory it had not mapped would fault instead. */
        if (errno != ENOMEM)
            _exit(31);

        /* (A2) Being at the limit must not corrupt the allocator. Every
         * block handed out earlier still has to hold what was written to
         * it: a quota that over-committed would show up here as one
         * block's pages having been reused for another. */
        {
            int i;
            for (i = 0; i < nblk; i++) {
                if (g_blk[i][0] != (char)(i & 0x7f))
                    _exit(32);
                if (g_blk[i][CHUNK - PAGE] != (char)(i & 0x7f))
                    _exit(33);
            }
        }

        /* (A3) The refusal is a ceiling, not a one-off.
         *
         * It is NOT sound to require that the very next malloc also fails:
         * malloc does not ask the runtime for memory one user-allocation at
         * a time, so the request that got refused was an arena growth, and
         * the arena can still have room for several more CHUNK-sized
         * requests afterwards. What must hold is that only a BOUNDED number
         * of them succeed; an unbounded run would mean the limit is
         * bounding nothing.
         *
         * Draining them here also leaves the cage genuinely at its ceiling
         * when the parent goes on to observe it. */
        {
            for (;;) {
                char *p = (char *)malloc(CHUNK);
                if (p == NULL)
                    break;
                if (nblk >= MAX_BLK)
                    _exit(34); /* allocating past the bound without end */
                p[0] = (char)(nblk & 0x7f);
                p[CHUNK - PAGE] = (char)(nblk & 0x7f);
                g_blk[nblk++] = p;
            }
        }
    }

    if (nblk == 0)
        _exit(35); /* could not allocate at all */

    /* Report the exhausted state and hold it while the parent works. */
    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(36);
    if (xread(b->gate[0], &buf, 1) != 1)
        _exit(37);

    /* (A4) Releasing restores the cage: after freeing everything, an
     * allocation of the same size must succeed again. */
    {
        int i;
        for (i = 0; i < nblk; i++)
            free(g_blk[i]);
    }
    {
        char *p = (char *)malloc(CHUNK);
        if (p == NULL)
            _exit(38); /* still exhausted after releasing everything */
        p[0] = 1;
        p[CHUNK - PAGE] = 2;
        if (p[0] != 1 || p[CHUNK - PAGE] != 2)
            _exit(39);
        free(p);
    }

    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(40);
    _exit(0);
}

/* ------------------------------------------------------------------ */
/* Child B: forked while A is at its limit; must be unaffected.        */
/* ------------------------------------------------------------------ */
static void child_b(barrier_t *b)
{
    unsigned char *buf;
    unsigned char rec[RECORD];
    unsigned long got, want;
    size_t off;
    int i;

    /* B inherited A's rendezvous. Holding ack[1] open would stop the
     * parent's ack[0] from ever reporting EOF if A died. Shed all four. */
    close(b->gate[0]);
    close(b->gate[1]);
    close(b->ack[0]);
    close(b->ack[1]);

    /* (B1) A modest allocation succeeds while A is exhausted. */
    buf = (unsigned char *)malloc(B_BYTES);
    if (buf == NULL)
        _exit(51);

    /* (B2) ...and the memory is genuinely B's. Filling from the shared
     * generator and checksumming catches a buffer that overlaps memory
     * already handed to A, which a mis-accounted quota could produce and
     * a bare non-NULL check never would. */
    for (off = 0; off + RECORD <= B_BYTES; off += RECORD) {
        make_record(rec, 3, (int)(off / RECORD));
        memcpy(buf + off, rec, RECORD);
    }
    got = checksum(buf, B_BYTES - (B_BYTES % RECORD));

    /* Recompute independently rather than trusting the buffer twice. */
    want = 1469598103u;
    for (off = 0; off + RECORD <= B_BYTES; off += RECORD) {
        make_record(rec, 3, (int)(off / RECORD));
        for (i = 0; i < RECORD; i++) {
            want ^= rec[i];
            want *= 16777619u;
            want &= 0xffffffffUL;
        }
    }
    if (got != want)
        _exit(52);

    /* (B3) Growing and shrinking still works: B is not merely able to
     * hold what it already had. */
    {
        unsigned char *more = (unsigned char *)realloc(buf, B_BYTES * 2);
        if (more == NULL)
            _exit(53);
        buf = more;
        memset(buf + B_BYTES, 0x5A, B_BYTES);
        if (buf[B_BYTES] != 0x5A || buf[(B_BYTES * 2) - 1] != 0x5A)
            _exit(54);
        /* The original half must have survived the move. */
        if (checksum(buf, B_BYTES - (B_BYTES % RECORD)) != want)
            _exit(55);
    }

    free(buf);
    _exit(0);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    barrier_t b;
    pid_t pa, pb;
    int status;
    char one = 1;

    assert(barrier_init(&b) == 0);

    fflush(stdout);
    pa = fork(); /* main thread only: lind returns -1 otherwise */
    assert(pa >= 0);
    if (pa == 0)
        child_a(&b);

    /* Shed the parent's own copies so a child that dies before acking
     * becomes EOF on ack[0] rather than an indefinite block. */
    close(b.gate[0]);
    close(b.ack[1]);

    /* (a) A has reached its own memory bound and is holding it. */
    expect_ack(b.ack[0], pa, "exhausted");

    /* (b) The parent (a different cage) allocates and computes
     * correctly while A is saturated. Free coverage, no extra cage. */
    {
        unsigned char *p = (unsigned char *)malloc(B_BYTES);
        unsigned char rec[RECORD];
        size_t off;
        assert(p != NULL);
        for (off = 0; off + RECORD <= B_BYTES; off += RECORD) {
            make_record(rec, 4, (int)(off / RECORD));
            memcpy(p + off, rec, RECORD);
        }
        for (off = 0; off + RECORD <= B_BYTES; off += RECORD) {
            make_record(rec, 4, (int)(off / RECORD));
            assert(memcmp(p + off, rec, RECORD) == 0);
        }
        free(p);
    }

    /* (c) A whole new cage can still be created while A is at its limit,
     * and (d) it allocates and computes correctly. B is forked here, not
     * earlier, so its success cannot be explained by ordering. */
    fflush(stdout);
    pb = fork();
    assert(pb >= 0);
    if (pb == 0)
        child_b(&b);

    assert(xwaitpid(pb, &status) == pb);
    assert(WIFEXITED(status));
    expect_exit0(status, "B");

    /* (e) Release A; freeing must restore its ability to allocate. */
    assert(xwrite(b.gate[1], &one, 1) == 1);
    expect_ack(b.ack[0], pa, "recovered");
    assert(xwaitpid(pa, &status) == pa);
    assert(WIFEXITED(status));
    expect_exit0(status, "A");

    close(b.gate[1]);
    close(b.ack[0]);

    /* (f) The parent is still healthy after both children have gone. */
    {
        void *p = malloc(B_BYTES);
        assert(p != NULL);
        free(p);
    }

    write(1, "CONC-005b PASS\n", 15);
    return 0;
}
