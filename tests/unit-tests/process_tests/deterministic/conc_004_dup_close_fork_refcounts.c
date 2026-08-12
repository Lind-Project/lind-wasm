/*
 * CONC-004: refcount conservation under dup/close/fork.
 *
 * The narrowly-controlled counterpart to CONC-003: pins down ONE lifecycle,
 * open -> dup/dup2/F_DUPFD -> fork -> concurrent close -> final close,
 * swept across a matrix of shapes, under a strict ownership model where
 * every descriptor is closed exactly once by exactly one owner. So every
 * assertion below is a hard equality that must hold under any interleaving.
 *
 * Black-box mirror of the in-crate `conc_004_*` tests in
 * src/fdtables/src/lib.rs, proving the same invariant externally via pipe
 * EOF (see CONC-003's header for why pipe EOF is a valid refcount oracle).
 *
 * The three duplication calls are swept separately because they take
 * different paths inside lind: dup() gets a FRESH (fdkind, underfd) key
 * (refcounting delegated to the host kernel); dup2() and fcntl(F_DUPFD)
 * both reuse the source's underfd, a SHARED key. A result that differs
 * between the dup and dup2 rows of the same shape is itself the finding.
 *
 * fork() is main-thread-only (closer pthreads only close); every child is
 * forked before any close happens, held at a two-pipe gate/ack rendezvous
 * until released together; every "released now" check is poll()-bounded,
 * since cage_finalize() signals the parent's waitpid() before actually
 * releasing the cage's fd-table references; forked children use only raw
 * syscalls and _exit() with a distinct code per failure.
 *
 * Determinism: exactly one line on stdout ("CONC-004 PASS\n"). No pids,
 * clocks, addresses, fd numbers or errno values are ever printed or
 * compared. Diagnostics go to fd 2, which the harness surfaces only on a
 * nonzero exit.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define DIRNAME "conc004_dir"
#define TFILE   DIRNAME "/shared.dat"
#define PROBEF  DIRNAME "/probe.dat"

#define MAX_DUP    4
#define MAX_CHILD  3
#define MAX_THREAD 2

#define RECORD  16
#define NREC    8
#define POLL_MS 5000

/* Repeat the whole config table this many times. Bumping it locally
 * (-DCONC004_ROUNDS=N) widens the search without touching CI timing. */
#ifndef CONC004_ROUNDS
#define CONC004_ROUNDS 1
#endif

/* dup2() needs an explicit target fd number, and it must be one that is
 * certainly free. This test never has more than a few dozen fds open, so
 * a reserved high band is free by construction on both native and lind
 * (both allocate the lowest free number, so neither will wander up here
 * on its own). Kept below MAXFD/FD_PER_PROCESS_MAX (1024). */
#define DUP2_BASE 200

/* An fd number that is certainly not open, for the EBADF checks. */
#define BADFD 500

/* fd-leak scan; same convention as conc_002 (see its header). FD_SCAN must
 * cover the DUP2_BASE band here. */
#ifndef CONC004_NO_FD_LEAK_SCAN
#define DO_FD_LEAK_SCAN 1
#else
#define DO_FD_LEAK_SCAN 0
#endif
#define FD_SCAN 256

/* Deterministic per-(a,c) byte pattern (same shape as conc_002/003). */
static void make_record(unsigned char *b, int a, int c)
{
    unsigned s = (unsigned)(a + 1) * 2654435761u + (unsigned)(c & 0xff) * 40503u;
    int k;
    for (k = 0; k < RECORD - 2; k++)
        b[k] = (unsigned char)((s >> ((k & 3) * 8)) + (unsigned)k);
    b[RECORD - 2] = (unsigned char)(0xA0 | (a & 0x0f));
    b[RECORD - 1] = (unsigned char)(c & 0xff);
}

/* Best-effort cleanup of leftovers from a previous crashed run. */
static void pre_clean(void)
{
    unlink(TFILE);
    unlink(PROBEF);
    rmdir(DIRNAME);
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

/* Bounded wait for readability (data OR EOF/POLLHUP). Never blocks past
 * POLL_MS, so a leaked reference (which would otherwise hang the read
 * forever) becomes a clean, diagnosable assertion failure instead of a
 * 30s harness timeout. */
static int wait_readable(int fd, int ms)
{
    struct pollfd pfd;
    for (;;) {
        pfd.fd = fd;
        pfd.events = POLLIN;
        pfd.revents = 0;
        int r = poll(&pfd, 1, ms);
        if (r < 0) {
            if (errno == EINTR)
                continue;
            return -1;
        }
        return r; /* 0 == timeout, >0 == ready */
    }
}

static int set_nonblock(int fd, int on)
{
    int flags = fcntl(fd, F_GETFL);
    if (flags < 0)
        return -1;
    if (on)
        flags |= O_NONBLOCK;
    else
        flags &= ~O_NONBLOCK;
    return fcntl(fd, F_SETFL, flags);
}

/* Non-blocking read must fail with EAGAIN/EWOULDBLOCK: no data pending
 * and no EOF (i.e. at least one write-side reference is still live). */
static int expect_eagain(int fd)
{
    char buf[1];
    ssize_t r;
    do {
        r = read(fd, buf, 1);
    } while (r < 0 && errno == EINTR);
    if (r != -1)
        return 0;
    return errno == EAGAIN || errno == EWOULDBLOCK;
}

/* Bounded blocking read must report EOF (0 bytes): the last write-side
 * reference, wherever it lived, has been released. */
static int expect_eof(int fd)
{
    if (wait_readable(fd, POLL_MS) <= 0)
        return 0;
    char buf[1];
    ssize_t n = xread(fd, buf, 1);
    return n == 0;
}

#if DO_FD_LEAK_SCAN
static void snapshot_fds(int *out)
{
    int i;
    for (i = 0; i < FD_SCAN; i++)
        out[i] = (fcntl(i, F_GETFD) >= 0) ? 1 : 0;
}

/* Reports every difference to fd 2 and returns the count. Called after
 * every round, not just once at the end (conc_002/conc_003 do the latter):
 * CONC-004's exact ownership makes a per-round balance meaningful, which
 * localises a leak to a single config instead of the whole run. */
static int diff_fds(const int *before, const int *after, int cfgidx, const char *tag)
{
    int i, failures = 0;
    for (i = 0; i < FD_SCAN; i++) {
        if (before[i] != after[i]) {
            char m[128];
            int n = snprintf(m, sizeof m,
                             "conc_004 FAIL fd-leak %s cfg=%d fd=%d before=%d after=%d\n",
                             tag, cfgidx, i, before[i], after[i]);
            write(2, m, (size_t)n);
            failures++;
        }
    }
    return failures;
}
#endif

/* ------------------------------------------------------------------ */
/* Two-pipe (gate/ack) rendezvous barrier for N forked children.       */
/* ------------------------------------------------------------------ */
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

/* Called first thing in a freshly-forked child: sheds this child's copy
 * of the ends it doesn't need, signals readiness, and blocks for the
 * release. Async-signal-safe (raw syscalls only). */
static void barrier_child_wait(barrier_t *b)
{
    close(b->gate[1]);
    close(b->ack[0]);
    char one = 1;
    xwrite(b->ack[1], &one, 1);
    char buf;
    xread(b->gate[0], &buf, 1);
}

/* Called in the parent after forking every child. Closing the parent's
 * own ack[1] here (before reading acks) is what turns a child that dies
 * before acking into an immediate EOF on ack[0] instead of an
 * indefinite block. Returns -1 (a child died) or 0 (all N acked). */
static int barrier_parent_ready(barrier_t *b, int n)
{
    close(b->gate[0]);
    close(b->ack[1]);
    int got = 0;
    while (got < n) {
        char buf;
        ssize_t r = xread(b->ack[0], &buf, 1);
        if (r <= 0)
            return -1;
        got++;
    }
    return 0;
}

static int barrier_release(barrier_t *b, int n)
{
    int i;
    for (i = 0; i < n; i++) {
        char one = 1;
        if (xwrite(b->gate[1], &one, 1) != 1)
            return -1;
    }
    return 0;
}

static void barrier_parent_close(barrier_t *b)
{
    close(b->gate[1]);
    close(b->ack[0]);
}

/* ------------------------------------------------------------------ */
/* Config matrix.                                                      */
/* ------------------------------------------------------------------ */
enum { HOW_DUP = 0, HOW_DUP2 = 1, HOW_FCNTL = 2 };

struct cfg {
    unsigned char ndup;        /* duplicates besides the sentinel, 1..MAX_DUP */
    unsigned char nchild;      /* forked children, 0..MAX_CHILD */
    unsigned char nthread;     /* parent closer pthreads, 0..MAX_THREAD */
    unsigned char how;         /* HOW_DUP | HOW_DUP2 | HOW_FCNTL */
    unsigned char dup_after;   /* create the duplicates after forking */
    unsigned char child_close; /* child explicitly closes its assigned subset */
};

/* Curated rather than exhaustive: the full cross product is ~300 configs,
 * each forking real cages, against a 30s harness timeout. These 20 cover
 * every dimension, run identical shapes through all three duplication
 * calls (the dup-vs-dup2 comparison above), and include the corners:
 * no children at all, no parent threads at all, and duplication after
 * fork (where children inherit only the sentinel). */
static const struct cfg CFGS[] = {
    /* ndup, nchild, nthread, how,       dup_after, child_close */
    {     1,      1,       0, HOW_DUP,           0,           1 },
    {     1,      1,       0, HOW_DUP2,          0,           1 },
    {     1,      1,       0, HOW_FCNTL,         0,           1 },
    {     1,      1,       1, HOW_DUP2,          0,           0 },
    {     2,      1,       1, HOW_DUP,           0,           1 },
    {     2,      1,       1, HOW_DUP2,          0,           1 },
    {     2,      1,       1, HOW_FCNTL,         0,           1 },
    {     2,      2,       2, HOW_DUP,           0,           0 },
    {     2,      2,       2, HOW_DUP2,          0,           0 },
    {     2,      0,       2, HOW_DUP2,          0,           0 }, /* no children */
    {     4,      2,       2, HOW_DUP,           0,           1 },
    {     4,      2,       2, HOW_DUP2,          0,           1 },
    {     4,      2,       2, HOW_FCNTL,         0,           1 },
    {     4,      3,       2, HOW_DUP2,          0,           0 },
    {     4,      3,       2, HOW_FCNTL,         0,           1 },
    {     4,      3,       0, HOW_DUP2,          0,           1 }, /* no threads */
    {     2,      2,       1, HOW_DUP2,          1,           0 }, /* dup after fork */
    {     2,      2,       1, HOW_DUP,           1,           0 },
    {     4,      3,       2, HOW_DUP2,          1,           1 },
    {     4,      3,       2, HOW_FCNTL,         1,           1 },
};
#define NCFGS ((int)(sizeof CFGS / sizeof CFGS[0]))

/* ------------------------------------------------------------------ */
/* Round state shared with the closer threads.                         */
/*                                                                     */
/* Only the main thread writes these, and only before pthread_create /  */
/* after pthread_join, so the barrier below is the sole synchronisation */
/* they need.                                                          */
/* ------------------------------------------------------------------ */
static int g_dup[MAX_DUP];      /* the parent's duplicate descriptors */
static int g_ndup;
static int g_nthread;
static int g_file_round;        /* pwrite through the fd before closing it */
static int g_fail[MAX_THREAD];  /* per-thread failure code, asserted after join */
static int g_tid[MAX_THREAD];
static pthread_barrier_t g_start;

/* Closer thread: owns exactly the duplicates at indices
 * tid, tid+nthread, tid+2*nthread, ...: a partition of 0..ndup, so no
 * two threads ever touch the same descriptor. */
static void *closer_fn(void *arg)
{
    int tid = *(int *)arg;
    int i;

    pthread_barrier_wait(&g_start);

    for (i = tid; i < g_ndup; i += g_nthread) {
        if (g_file_round) {
            /* The descriptor must still be fully usable right up to the
             * instant it is closed, even with every other owner closing
             * its own descriptor concurrently. */
            unsigned char rec[RECORD];
            make_record(rec, 2, i);
            if (pwrite(g_dup[i], rec, RECORD, (off_t)(NREC + MAX_CHILD + i) * RECORD) != RECORD) {
                g_fail[tid] = 1;
                return NULL;
            }
        }
        if (close(g_dup[i]) != 0) {
            g_fail[tid] = 2;
            return NULL;
        }
    }
    return NULL;
}

/* ------------------------------------------------------------------ */
/* Duplicate creation, per the config's `how`.                         */
/*                                                                     */
/* All three are POSIX-equivalent; see the header for why they are      */
/* swept separately. Returns 0 on success.                             */
/* ------------------------------------------------------------------ */
static int make_dups(int sentinel, const struct cfg *c, int from, int to)
{
    int i;
    for (i = from; i < to; i++) {
        int fd;
        switch (c->how) {
        case HOW_DUP:
            fd = dup(sentinel);
            break;
        case HOW_DUP2:
            fd = dup2(sentinel, DUP2_BASE + i);
            if (fd >= 0 && fd != DUP2_BASE + i)
                return -1; /* dup2 must return exactly the requested number */
            break;
        default: /* HOW_FCNTL: POSIX-equivalent to dup(), different path
                  * inside lind (get_unused_virtual_fd_from_startfd). */
            fd = fcntl(sentinel, F_DUPFD, 0);
            break;
        }
        if (fd < 0)
            return -1;
        g_dup[i] = fd;
    }
    return 0;
}

/* Fork `n` children, each of which acks and then blocks on the gate.
 * Child c owns the inherited duplicates at indices c, c+n, c+2n, ...
 * (again a partition, so no two children close the same one), but they
 * are the CHILD's private copies, entirely disjoint from the parent's,
 * so this never races the parent's closer threads.
 *
 * `ninherited` is how many duplicates existed at fork time: with
 * dup_after set, the children inherit only the sentinel and own nothing,
 * which is the pure fork-then-cage-teardown case. */
static int fork_children(pid_t *kids, int n, int ninherited, const struct cfg *c,
                         barrier_t *b, int sentinel)
{
    int i;
    for (i = 0; i < n; i++) {
        fflush(stdout);
        kids[i] = fork(); /* main thread only: lind returns -1 otherwise */
        if (kids[i] < 0)
            return -1;
        if (kids[i] == 0) {
            int j;
            barrier_child_wait(b);

            if (g_file_round) {
                /* Through the inherited SENTINEL copy, which every child
                 * has regardless of dup_after. Disjoint offsets, so the
                 * parent can verify every child's record individually. */
                unsigned char rec[RECORD];
                make_record(rec, 1, i);
                if (pwrite(sentinel, rec, RECORD, (off_t)(NREC + i) * RECORD) != RECORD)
                    _exit(31);
            }

            if (c->child_close) {
                for (j = i; j < ninherited; j += n) {
                    if (close(g_dup[j]) != 0)
                        _exit(32);
                }
                /* Per-cage isolation: a descriptor this child just closed
                 * must be gone HERE, while the parent's own copy of the
                 * same number stays live (checked in the parent below). */
                for (j = i; j < ninherited; j += n) {
                    if (fcntl(g_dup[j], F_GETFD) != -1)
                        _exit(33);
                }
            }
            /* Whatever is left (the sentinel copy, the duplicates this
             * child does not own, and, when !child_close, all of them)
             * must be released by cage teardown at _exit. */
            _exit(0);
        }
    }
    return 0;
}

static int reap_children(const pid_t *kids, int n)
{
    int i;
    for (i = 0; i < n; i++) {
        int status = 0;
        if (xwaitpid(kids[i], &status) != kids[i])
            return -1;
        if (!WIFEXITED(status) || WEXITSTATUS(status) != 0)
            return -1;
    }
    return 0;
}

/* Release every owner at once and collect them all.
 *
 * Exact simultaneity is not required and is not attempted: the
 * invariants asserted afterwards are equalities that hold under any
 * interleaving. What matters is only that no close happens before every
 * fork is complete. */
static void run_owners(const struct cfg *c, barrier_t *b)
{
    pthread_t th[MAX_THREAD];
    int t;

    for (t = 0; t < c->nthread; t++) {
        g_fail[t] = 0;
        g_tid[t] = t;
    }

    if (c->nthread > 0) {
        assert(pthread_barrier_init(&g_start, NULL, c->nthread + 1) == 0);
        for (t = 0; t < c->nthread; t++)
            assert(pthread_create(&th[t], NULL, closer_fn, &g_tid[t]) == 0);
    }

    assert(barrier_release(b, c->nchild) == 0);

    if (c->nthread > 0) {
        int ret = pthread_barrier_wait(&g_start);
        assert(ret == 0 || ret == PTHREAD_BARRIER_SERIAL_THREAD);
        for (t = 0; t < c->nthread; t++)
            assert(pthread_join(th[t], NULL) == 0);
        assert(pthread_barrier_destroy(&g_start) == 0);
        for (t = 0; t < c->nthread; t++)
            assert(g_fail[t] == 0);
    } else {
        /* No closer threads: the main thread is the sole owner of every
         * duplicate. Still exactly-once, still concurrent with the
         * children's closes and cage teardowns. */
        int i;
        for (i = 0; i < g_ndup; i++) {
            if (g_file_round) {
                unsigned char rec[RECORD];
                make_record(rec, 2, i);
                assert(pwrite(g_dup[i], rec, RECORD,
                              (off_t)(NREC + MAX_CHILD + i) * RECORD) == RECORD);
            }
            assert(close(g_dup[i]) == 0);
        }
    }

    barrier_parent_close(b);
}

/* ------------------------------------------------------------------ */
/* Phase A: pipe. The sentinel is the parent's ORIGINAL write end; the  */
/* read end is the oracle.                                             */
/* ------------------------------------------------------------------ */
static void run_pipe_round(const struct cfg *c, int cfgidx)
{
    pid_t kids[MAX_CHILD];
    barrier_t b;
    int p[2];
    int ninherited;
    int i;

    assert(pipe(p) == 0);
    /* p[1] is the sentinel: the one write-side reference the parent keeps
     * alive through the entire round. */

    g_ndup = c->ndup;
    g_nthread = c->nthread;
    g_file_round = 0;
    for (i = 0; i < MAX_DUP; i++)
        g_dup[i] = -1;

    ninherited = c->dup_after ? 0 : c->ndup;
    assert(make_dups(p[1], c, 0, ninherited) == 0);

    assert(barrier_init(&b) == 0);
    assert(fork_children(kids, c->nchild, ninherited, c, &b, p[1]) == 0);
    assert(barrier_parent_ready(&b, c->nchild) == 0);

    /* Duplication AFTER fork: these references exist only in the parent,
     * so the children contribute pure fork+teardown pressure on the
     * sentinel's key while the parent's own duplicates come and go. */
    if (c->dup_after)
        assert(make_dups(p[1], c, 0, c->ndup) == 0);

    run_owners(c, &b);
    assert(reap_children(kids, c->nchild) == 0);

    /* A1: NO EOF. Every duplicate is closed and every child cage is gone,
     * but the parent's sentinel is still live, so not one write-side
     * reference may have been over-released. */
    assert(set_nonblock(p[0], 1) == 0);
    assert(expect_eagain(p[0]));
    assert(set_nonblock(p[0], 0) == 0);

    /* A2 setup: a probe file opened BEFORE the write below. If lind ever
     * over-releases the retained reference, the sentinel's host fd number
     * becomes free and open() could recycle it, landing A2's write here
     * instead of in the pipe. */
    int probe = open(PROBEF, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(probe >= 0);

    /* A2: the retained reference is not merely counted, it is usable. */
    unsigned char tok[RECORD], got[RECORD];
    make_record(tok, 0, cfgidx);
    assert(xwrite(p[1], tok, RECORD) == RECORD);
    assert(xread(p[0], got, RECORD) == RECORD);
    assert(memcmp(tok, got, RECORD) == 0);

    struct stat pst;
    assert(fstat(probe, &pst) == 0);
    assert(pst.st_size == 0); /* A2's write did NOT land in the probe file */

    /* A3: release the LAST write-side reference anywhere -> EOF, stable
     * across repeated reads. This is the "underlying resource is finally
     * released" check. */
    assert(close(p[1]) == 0);
    assert(expect_eof(p[0]));
    assert(expect_eof(p[0]));

    /* A4: the closed sentinel is now unusable in this cage too. */
    {
        char buf[1];
        errno = 0;
        assert(write(p[1], "x", 1) == -1 && errno == EBADF);
        errno = 0;
        assert(close(p[1]) == -1 && errno == EBADF);
        (void)buf;
    }

    assert(close(p[0]) == 0);
    assert(close(probe) == 0);
    assert(unlink(PROBEF) == 0);
}

/* ------------------------------------------------------------------ */
/* Phase B: regular file. Same ownership shape, plus data fidelity:     */
/* the retained reference must see every owner's write, and the data    */
/* must survive the final close (i.e. it reached the file, rather than  */
/* being visible only through a lingering in-memory reference).         */
/* ------------------------------------------------------------------ */
static void run_file_round(const struct cfg *c, int cfgidx)
{
    pid_t kids[MAX_CHILD];
    barrier_t b;
    int ninherited;
    int i;

    (void)cfgidx;

    int fd = open(TFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(fd >= 0); /* fd is the sentinel */

    for (i = 0; i < NREC; i++) {
        unsigned char rec[RECORD];
        make_record(rec, 0, i);
        assert(xwrite(fd, rec, RECORD) == RECORD);
    }

    g_ndup = c->ndup;
    g_nthread = c->nthread;
    g_file_round = 1;
    for (i = 0; i < MAX_DUP; i++)
        g_dup[i] = -1;

    ninherited = c->dup_after ? 0 : c->ndup;
    assert(make_dups(fd, c, 0, ninherited) == 0);

    assert(barrier_init(&b) == 0);
    assert(fork_children(kids, c->nchild, ninherited, c, &b, fd) == 0);
    assert(barrier_parent_ready(&b, c->nchild) == 0);

    if (c->dup_after)
        assert(make_dups(fd, c, 0, c->ndup) == 0);

    run_owners(c, &b);
    assert(reap_children(kids, c->nchild) == 0);

    /* The sentinel must still see the parent's original records, every
     * child's record, and every closer-thread record, all written at
     * disjoint offsets through references that are now gone. */
    for (i = 0; i < NREC; i++) {
        unsigned char want[RECORD], gotb[RECORD];
        make_record(want, 0, i);
        assert(pread(fd, gotb, RECORD, (off_t)i * RECORD) == RECORD);
        assert(memcmp(want, gotb, RECORD) == 0);
    }
    for (i = 0; i < c->nchild; i++) {
        unsigned char want[RECORD], gotb[RECORD];
        make_record(want, 1, i);
        assert(pread(fd, gotb, RECORD, (off_t)(NREC + i) * RECORD) == RECORD);
        assert(memcmp(want, gotb, RECORD) == 0);
    }
    for (i = 0; i < c->ndup; i++) {
        unsigned char want[RECORD], gotb[RECORD];
        make_record(want, 2, i);
        assert(pread(fd, gotb, RECORD, (off_t)(NREC + MAX_CHILD + i) * RECORD) == RECORD);
        assert(memcmp(want, gotb, RECORD) == 0);
    }

    /* Close the last reference, then re-open: the data must be on disk,
     * not merely visible through a lingering in-memory reference. */
    assert(close(fd) == 0);

    int fd2 = open(TFILE, O_RDONLY);
    assert(fd2 >= 0);
    for (i = 0; i < NREC; i++) {
        unsigned char want[RECORD], gotb[RECORD];
        make_record(want, 0, i);
        assert(pread(fd2, gotb, RECORD, (off_t)i * RECORD) == RECORD);
        assert(memcmp(want, gotb, RECORD) == 0);
    }
    for (i = 0; i < c->nchild; i++) {
        unsigned char want[RECORD], gotb[RECORD];
        make_record(want, 1, i);
        assert(pread(fd2, gotb, RECORD, (off_t)(NREC + i) * RECORD) == RECORD);
        assert(memcmp(want, gotb, RECORD) == 0);
    }
    assert(close(fd2) == 0);
    assert(unlink(TFILE) == 0);
}

/* ------------------------------------------------------------------ */
/* Phase C: dup/dup2 edge semantics that the refcount depends on.       */
/*                                                                     */
/* dup2(oldfd, oldfd) is only a no-op when oldfd is VALID; POSIX says   */
/* an invalid oldfd fails with EBADF and newfd is not closed. Getting   */
/* this wrong hands back an fd number that was never open, a           */
/* fabricated reference the refcount knows nothing about.               */
/* ------------------------------------------------------------------ */
static void run_dup_semantics(void)
{
    /* Precondition: BADFD really is closed. */
    errno = 0;
    assert(fcntl(BADFD, F_GETFD) == -1 && errno == EBADF);

    errno = 0;
    assert(dup2(BADFD, BADFD) == -1 && errno == EBADF);
    /* ... and it must not have been conjured into existence. */
    errno = 0;
    assert(fcntl(BADFD, F_GETFD) == -1 && errno == EBADF);

    errno = 0;
    assert(dup(BADFD) == -1 && errno == EBADF);
    errno = 0;
    assert(fcntl(BADFD, F_DUPFD, 0) == -1 && errno == EBADF);

    int fd = open(PROBEF, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(fd >= 0);

    /* Valid oldfd == newfd: a genuine no-op returning newfd, and newfd is
     * NOT closed. A refcount that decremented here would release the file
     * out from under the caller. */
    assert(dup2(fd, fd) == fd);
    assert(fcntl(fd, F_GETFD) >= 0);
    unsigned char rec[RECORD], gotb[RECORD];
    make_record(rec, 3, 0);
    assert(xwrite(fd, rec, RECORD) == RECORD);
    assert(pread(fd, gotb, RECORD, 0) == RECORD);
    assert(memcmp(rec, gotb, RECORD) == 0);

    /* dup2 onto an already-open target silently closes the target first;
     * the result must alias the source, not the old occupant. */
    int other = open(TFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(other >= 0);
    assert(dup2(fd, other) == other);
    assert(pread(other, gotb, RECORD, 0) == RECORD);
    assert(memcmp(rec, gotb, RECORD) == 0);
    /* The old occupant's file must be untouched and empty. */
    assert(close(other) == 0);
    int chk = open(TFILE, O_RDONLY);
    assert(chk >= 0);
    struct stat st;
    assert(fstat(chk, &st) == 0);
    assert(st.st_size == 0);
    assert(close(chk) == 0);
    assert(unlink(TFILE) == 0);

    assert(close(fd) == 0);
    assert(unlink(PROBEF) == 0);
}

int main(void)
{
#if DO_FD_LEAK_SCAN
    int base[FD_SCAN], before[FD_SCAN], after[FD_SCAN];
    int leaks = 0;
#endif
    int rep, i;

    pre_clean();
    assert(mkdir(DIRNAME, 0755) == 0);

    run_dup_semantics();

#if DO_FD_LEAK_SCAN
    snapshot_fds(base);
#endif

    for (rep = 0; rep < CONC004_ROUNDS; rep++) {
        for (i = 0; i < NCFGS; i++) {
#if DO_FD_LEAK_SCAN
            snapshot_fds(before);
#endif
            run_pipe_round(&CFGS[i], i);
#if DO_FD_LEAK_SCAN
            /* Exact ownership means the parent's open-fd set must be back
             * to precisely what it was before this config ran. */
            snapshot_fds(after);
            leaks += diff_fds(before, after, i, "pipe");
#endif
        }
    }

    /* The file phase is heavier (NREC+ records per round), so it runs a
     * representative slice rather than the whole table: one config per
     * duplication call, plus the dup-after-fork corner. */
    {
        static const int file_cfgs[] = { 10, 11, 12, 14, 18 };
        for (rep = 0; rep < CONC004_ROUNDS; rep++) {
            for (i = 0; i < (int)(sizeof file_cfgs / sizeof file_cfgs[0]); i++) {
                int ci = file_cfgs[i];
#if DO_FD_LEAK_SCAN
                snapshot_fds(before);
#endif
                run_file_round(&CFGS[ci], ci);
#if DO_FD_LEAK_SCAN
                snapshot_fds(after);
                leaks += diff_fds(before, after, ci, "file");
#endif
            }
        }
    }

    assert(rmdir(DIRNAME) == 0); /* ENOTEMPTY here == something leaked */

#if DO_FD_LEAK_SCAN
    snapshot_fds(after);
    leaks += diff_fds(base, after, -1, "total");
    assert(leaks == 0);
#endif

    write(1, "CONC-004 PASS\n", 14);
    return 0;
}
