/*
 * CONC-003 -- cage-table and fd-refcount operations.
 *
 * Black-box mirror of the in-crate tests in src/fdtables/src/lib.rs
 * (the `conc_003_*` block): the public C/POSIX interface cannot inspect
 * Lind's internal fd refcount, so this test proves the same invariant
 * externally, via pipe EOF.
 *
 * Because lind runs every cage in a single host process, a pipe's write
 * end is backed by exactly one host fd, and that host fd's real
 * libc::close() is driven solely by fdtables' (fdkind, underfd) refcount
 * (see kernel_close / register_close_handlers in rawposix). So:
 *   - a reader must NOT see EOF while any write-side reference to the
 *     pipe remains live in ANY cage;
 *   - a reader MUST see EOF once the very last write-side reference,
 *     wherever it lives, is closed.
 * Native/lind agreement on this is a differential test of the refcount.
 *
 * Every fork is from the main thread only (lind returns -1 for fork()
 * from a non-main thread; this file has no threads at all -- the
 * concurrency under test comes from several simultaneously-live forked
 * child cages instead). Forked children use only raw syscalls and
 * _exit() with a distinct code per failure, per repo convention.
 *
 * A two-pipe (gate/ack) rendezvous, not sleeps, makes children act
 * concurrently: the parent forks every child before closing any of its
 * own references (so each child inherits the full reference set), each
 * child acks readiness and then blocks on the gate, and the parent
 * releases all children at once with one write per child. The parent
 * closes its own ack-write-end before waiting for acks, which turns a
 * child that dies before acking into an immediate EOF on the ack pipe
 * (a diagnosable failure) instead of an indefinite block (a 30s harness
 * timeout). Every blocking wait is also EINTR-retried and every "EOF
 * now" check is poll()-bounded, because lind's cage_finalize() records
 * a zombie and signals the parent's waitpid() BEFORE it actually calls
 * remove_cage_from_fdtable() -- so waitpid() returning does not, by
 * itself, prove a forked child's references are gone yet.
 *
 * Determinism: exactly one line on stdout ("CONC-003 PASS\n"). No pids,
 * clocks, addresses, fd numbers, or errno values are ever printed or
 * compared. Diagnostics (fd-leak scan only) go to fd 2, which the
 * harness surfaces only on a nonzero exit.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define DIRNAME "conc003_dir"
#define TFILE   DIRNAME "/shared.dat"
#define PROBEF  DIRNAME "/probe.dat"

#define RECORD   16
#define NREC     8
#define ROUNDS   4
#define POLL_MS  5000

/* fd-leak scan: comparable across native/lind as long as both allocate
 * the lowest free fd, which fdtables' get_unused_virtual_fd does by
 * construction (same convention as conc_002). Disable with
 * -DCONC003_NO_FD_LEAK_SCAN if this ever proves to be an artifact rather
 * than a real leak. */
#ifndef CONC003_NO_FD_LEAK_SCAN
#define DO_FD_LEAK_SCAN 1
#else
#define DO_FD_LEAK_SCAN 0
#endif
#define FD_SCAN 128

/* ------------------------------------------------------------------ */
/* Deterministic per-(a,c) byte pattern (same shape as conc_002's).    */
/* ------------------------------------------------------------------ */
static void make_record(unsigned char *b, int a, int c)
{
    unsigned s = (unsigned)(a + 1) * 2654435761u + (unsigned)(c & 0xff) * 40503u;
    int k;
    for (k = 0; k < RECORD - 2; k++)
        b[k] = (unsigned char)((s >> ((k & 3) * 8)) + (unsigned)k);
    b[RECORD - 2] = (unsigned char)(0xA0 | (a & 0x0f));
    b[RECORD - 1] = (unsigned char)(c & 0xff);
}

/* Best-effort removal of leftovers from a previous crashed run. The
 * native run is unprivileged and the lind run is under sudo, so a
 * root-owned leftover from a crashed lind run can otherwise block the
 * next native run; this self-heals when possible and fails loudly (at
 * mkdir/open below) when it cannot. */
static void pre_clean(void)
{
    unlink(TFILE);
    unlink(PROBEF);
    rmdir(DIRNAME);
}

#if DO_FD_LEAK_SCAN
static void snapshot_fds(int *out)
{
    int i;
    for (i = 0; i < FD_SCAN; i++)
        out[i] = (fcntl(i, F_GETFD) >= 0) ? 1 : 0;
}
#endif

/* ------------------------------------------------------------------ */
/* EINTR-retrying wrappers. Lind interrupts blocking syscalls by     */
/* sending SIGUSR2 to a cage's main thread, via a handler installed  */
/* with no SA_RESTART. That never happens to this test today only    */
/* because SIGCHLD's default disposition is Ignore -- a fact about   */
/* SIGCHLD, not a guarantee from lind -- so every blocking call here */
/* is retried on EINTR regardless.                                   */
/* ------------------------------------------------------------------ */
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
/* Phase A: pipe, parent retains the last write-side reference.        */
/* ------------------------------------------------------------------ */
static void run_phase_a(int round)
{
    int p[2];
    assert(pipe(p) == 0);
    int w1 = dup(p[1]);
    assert(w1 >= 0);
    int w2 = dup(p[1]);
    assert(w2 >= 0);
    int w3 = dup(p[1]);
    assert(w3 >= 0);
    int r1 = dup(p[0]);
    assert(r1 >= 0);
    /* write-side references: p[1], w1, w2, w3 (4). read-side: p[0], r1 (2). */

    barrier_t b;
    assert(barrier_init(&b) == 0);

    pid_t kids[3];
    int i;
    for (i = 0; i < 3; i++) {
        fflush(stdout); /* repo convention; nothing buffered here */
        kids[i] = fork();
        assert(kids[i] >= 0); /* main thread => lind must succeed too */
        if (kids[i] == 0) {
            barrier_child_wait(&b);
            switch (i) {
            case 0: /* closes ALL 4 inherited write references */
                close(p[1]);
                close(w1);
                close(w2);
                close(w3);
                break;
            case 1: /* closes 2 of the 4 */
                close(w1);
                close(w2);
                break;
            default: /* closes none; cage-exit drops the rest */
                break;
            }
            _exit(0);
        }
    }

    assert(barrier_parent_ready(&b, 3) == 0);

    /* Parent drops its own extra write references, retains p[1]. */
    assert(close(w1) == 0);
    assert(close(w2) == 0);
    assert(close(w3) == 0);

    assert(barrier_release(&b, 3) == 0);
    barrier_parent_close(&b);

    for (i = 0; i < 3; i++) {
        int status = 0;
        assert(xwaitpid(kids[i], &status) == kids[i]);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    /* A1: p[0] must NOT see EOF -- the parent's p[1] is still live,
     * regardless of what all three children just did to their copies. */
    assert(set_nonblock(p[0], 1) == 0);
    assert(expect_eagain(p[0]));
    assert(set_nonblock(p[0], 0) == 0);

    /* A3 setup: a probe file opened BEFORE A2's write. If lind ever
     * over-releases the retained reference, the host fd number becomes
     * free and open() could recycle it, landing A2's write here instead
     * of in the pipe. */
    int probe = open(PROBEF, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(probe >= 0);

    /* A2: the retained fd is still fully usable after all three child
     * closes. make_record() always writes exactly RECORD bytes. */
    unsigned char tok[RECORD], got[RECORD];
    make_record(tok, round, 0xA2);
    assert(xwrite(p[1], tok, RECORD) == RECORD);
    assert(xread(p[0], got, RECORD) == RECORD);
    assert(memcmp(tok, got, RECORD) == 0);

    struct stat pst;
    assert(fstat(probe, &pst) == 0);
    assert(pst.st_size == 0); /* A2's write did NOT land in the probe file */

    /* A4: close the last write reference anywhere -> EOF, and EOF is
     * stable across repeated reads. */
    assert(close(p[1]) == 0);
    assert(expect_eof(p[0]));
    assert(expect_eof(p[0]));

    /* A5: closed ends are now unusable. */
    assert(close(p[0]) == 0);
    assert(close(r1) == 0);
    assert(close(probe) == 0);
    assert(unlink(PROBEF) == 0);

    {
        char buf[1];
        errno = 0;
        assert(read(p[0], buf, 1) == -1 && errno == EBADF);
        errno = 0;
        assert(close(p[0]) == -1 && errno == EBADF);
    }
}

/* ------------------------------------------------------------------ */
/* Phase B: pipe, a CHILD cage retains the last write-side reference.  */
/* ------------------------------------------------------------------ */
static void run_phase_b(void)
{
    int p[2];
    assert(pipe(p) == 0);

    barrier_t b;
    assert(barrier_init(&b) == 0);

    fflush(stdout);
    pid_t kid = fork();
    assert(kid >= 0);
    if (kid == 0) {
        close(p[0]); /* child keeps only the write end */
        barrier_child_wait(&b);
        _exit(0); /* never explicitly closes p[1]; cage-exit must drop it */
    }

    assert(barrier_parent_ready(&b, 1) == 0);

    /* Parent closes ALL of its own write references -- after this, only
     * the CHILD cage's copy of p[1] is a live write-side reference. */
    assert(close(p[1]) == 0);

    /* B1: no EOF while a CHILD cage (not the parent) holds the only
     * write reference -- proves the refcount is not scoped to "the
     * cage that currently has the read end open". */
    assert(set_nonblock(p[0], 1) == 0);
    assert(expect_eagain(p[0]));
    assert(set_nonblock(p[0], 0) == 0);

    assert(barrier_release(&b, 1) == 0);
    barrier_parent_close(&b);

    int status = 0;
    assert(xwaitpid(kid, &status) == kid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);

    /* B2: the child's cage-exit (not an explicit close) must still
     * release its write reference, producing EOF. */
    assert(expect_eof(p[0]));
    assert(close(p[0]) == 0);
}

/* ------------------------------------------------------------------ */
/* Phase C: regular file, same lifetime shape as A, plus data fidelity. */
/* ------------------------------------------------------------------ */
static void run_phase_c(void)
{
    int fd = open(TFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(fd >= 0);

    int i;
    for (i = 0; i < NREC; i++) {
        unsigned char rec[RECORD];
        make_record(rec, 0, i);
        assert(xwrite(fd, rec, RECORD) == RECORD);
    }

    int d1 = dup(fd);
    assert(d1 >= 0);
    int d2 = dup(fd);
    assert(d2 >= 0);
    int d3 = dup(fd);
    assert(d3 >= 0);

    barrier_t b;
    assert(barrier_init(&b) == 0);

    pid_t kids[3];
    for (i = 0; i < 3; i++) {
        fflush(stdout);
        kids[i] = fork();
        assert(kids[i] >= 0);
        if (kids[i] == 0) {
            barrier_child_wait(&b);

            unsigned char crec[RECORD];
            make_record(crec, 1, i); /* a=1 distinguishes child records */
            off_t off = (off_t)(NREC + i) * RECORD;
            if (pwrite(fd, crec, RECORD, off) != RECORD)
                _exit(31);

            switch (i) {
            case 0: /* closes ALL 4 inherited references */
                if (close(fd) != 0)
                    _exit(32);
                if (close(d1) != 0)
                    _exit(33);
                if (close(d2) != 0)
                    _exit(34);
                if (close(d3) != 0)
                    _exit(35);
                {
                    struct stat st;
                    if (fstat(fd, &st) == 0) /* per-cage isolation */
                        _exit(36);
                }
                break;
            case 1: /* closes 2 of the 4 */
                if (close(d1) != 0)
                    _exit(37);
                if (close(d2) != 0)
                    _exit(38);
                break;
            default: /* closes none; cage-exit drops the rest */
                break;
            }
            _exit(0);
        }
    }

    assert(barrier_parent_ready(&b, 3) == 0);
    assert(close(d1) == 0);
    assert(close(d2) == 0);
    assert(close(d3) == 0);
    assert(barrier_release(&b, 3) == 0);
    barrier_parent_close(&b);

    for (i = 0; i < 3; i++) {
        int status = 0;
        assert(xwaitpid(kids[i], &status) == kids[i]);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);
    }

    /* The retained fd must be fully usable: original records plus all
     * three children's disjoint-offset writes. */
    for (i = 0; i < NREC; i++) {
        unsigned char want[RECORD], got[RECORD];
        make_record(want, 0, i);
        assert(pread(fd, got, RECORD, (off_t)i * RECORD) == RECORD);
        assert(memcmp(want, got, RECORD) == 0);
    }
    for (i = 0; i < 3; i++) {
        unsigned char want[RECORD], got[RECORD];
        make_record(want, 1, i);
        assert(pread(fd, got, RECORD, (off_t)(NREC + i) * RECORD) == RECORD);
        assert(memcmp(want, got, RECORD) == 0);
    }

    struct stat st;
    assert(fstat(fd, &st) == 0);
    assert(st.st_size == (off_t)(NREC + 3) * RECORD);

    /* Close the last reference, then re-open: the data must be on disk,
     * not merely visible through a lingering in-memory reference. */
    assert(close(fd) == 0);

    int fd2 = open(TFILE, O_RDONLY);
    assert(fd2 >= 0);
    for (i = 0; i < NREC; i++) {
        unsigned char want[RECORD], got[RECORD];
        make_record(want, 0, i);
        assert(pread(fd2, got, RECORD, (off_t)i * RECORD) == RECORD);
        assert(memcmp(want, got, RECORD) == 0);
    }
    assert(close(fd2) == 0);
    assert(unlink(TFILE) == 0);
}

/* ------------------------------------------------------------------ */
/* Optional, default-off: does fork() share the file DESCRIPTION (the  */
/* offset), not just the fd number? POSIX says yes and lind shares the  */
/* literal host fd, but this is unverified against native glibc under   */
/* the harness, so it is off by default; the pread-based assertions in  */
/* Phase C above carry the real refcount oracle regardless of this.     */
/* ------------------------------------------------------------------ */
#ifdef CONC003_CHECK_SHARED_OFFSET
static void run_shared_offset_check(void)
{
    int fd = open(TFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(fd >= 0);
    unsigned char rec[RECORD];
    make_record(rec, 2, 0);
    assert(xwrite(fd, rec, RECORD) == RECORD); /* offset now RECORD */

    barrier_t b;
    assert(barrier_init(&b) == 0);
    fflush(stdout);
    pid_t kid = fork();
    assert(kid >= 0);
    if (kid == 0) {
        barrier_child_wait(&b);
        if (lseek(fd, RECORD, SEEK_CUR) != (off_t)(2 * RECORD))
            _exit(41);
        _exit(0);
    }
    assert(barrier_parent_ready(&b, 1) == 0);
    assert(barrier_release(&b, 1) == 0);
    barrier_parent_close(&b);

    int status = 0;
    assert(xwaitpid(kid, &status) == kid);
    assert(WIFEXITED(status));
    assert(WEXITSTATUS(status) == 0);

    /* The child's lseek on the SHARED description must be visible here. */
    off_t cur = lseek(fd, 0, SEEK_CUR);
    assert(cur == (off_t)(2 * RECORD));

    assert(close(fd) == 0);
    assert(unlink(TFILE) == 0);
}
#endif

int main(void)
{
#if DO_FD_LEAK_SCAN
    int before[FD_SCAN], after[FD_SCAN];
#endif
    int round;

    pre_clean();
    assert(mkdir(DIRNAME, 0755) == 0);

#if DO_FD_LEAK_SCAN
    snapshot_fds(before);
#endif

    for (round = 0; round < ROUNDS; round++)
        run_phase_a(round);

    for (round = 0; round < ROUNDS; round++)
        run_phase_b();

    run_phase_c();

#ifdef CONC003_CHECK_SHARED_OFFSET
    run_shared_offset_check();
#endif

    assert(rmdir(DIRNAME) == 0); /* ENOTEMPTY here == something leaked */

#if DO_FD_LEAK_SCAN
    snapshot_fds(after);
    {
        int i, failures = 0;
        for (i = 0; i < FD_SCAN; i++) {
            if (before[i] != after[i]) {
                char m[96];
                int n = snprintf(m, sizeof m,
                    "conc_003 FAIL fd-leak fd=%d before=%d after=%d\n",
                    i, before[i], after[i]);
                write(2, m, (size_t)n);
                failures++;
            }
        }
        assert(failures == 0);
    }
#endif

    write(1, "CONC-003 PASS\n", 14);
    return 0;
}
