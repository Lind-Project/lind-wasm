/*
 * CONC-005a -- per-cage fd exhaustion isolation.
 *
 * CONC-002/003/004 all stay well below the fd limit and ask whether
 * descriptors are tracked correctly. CONC-005a drives ONE cage all the way
 * into EMFILE and asks the orthogonal question: is the limit actually
 * per-cage? A cap that is really global, or a runtime that faults when any
 * cage saturates its table, would let one cage deny service to every other
 * one -- the isolation property the whole cage model rests on.
 *
 * The scenario is a strict happens-before chain, so every assertion holds
 * under any interleaving:
 *
 *   A exhausts -> A acks -> parent does its own file I/O -> parent forks B
 *   -> B allocates and does file I/O -> B exits 0 -> parent releases A
 *   -> A closes everything and re-opens -> A exits 0
 *
 * B is forked AFTER A is already exhausted, on purpose. That covers cage
 * creation itself (copy_fdtable_for_cage from a parent whose sibling is at
 * its cap) in addition to allocation inside B, and it means B's success
 * cannot be explained by B having grabbed its descriptors first.
 *
 * -------------------------------------------------------------------
 * Why the exhaustion loop is bounded instead of unbounded
 * -------------------------------------------------------------------
 * lind pins every cage at FD_PER_PROCESS_MAX (1024,
 * src/fdtables/src/commonconstants.rs), and prlimit64 reports a hardcoded
 * RLIMIT_NOFILE of 1024 regardless of the host. The NATIVE reference run
 * of this same source sees the host soft limit instead, which is commonly
 * 1048576 under Docker/systemd -- so "open until it fails" is ~1000 opens
 * on lind and ~1e6 opens natively, which would blow the 30s harness
 * timeout on the side that is only supposed to be the oracle.
 *
 * Lowering the limit first is not available either: lind's prlimit64
 * rejects every setrlimit with EPERM (src/rawposix/src/sys_calls.rs), so
 * the two runs could not be normalised that way.
 *
 * So the loop stops at FD_CAP, just above lind's 1024, and the
 * EMFILE-specific assertions run only if the limit was actually reached
 * (`hit_limit`). On lind that is always true and the exhaustion checks
 * always run; on a native host with a high limit the run degrades to the
 * isolation and cleanup checks. Both emit the identical single PASS line,
 * which is what the harness diffs.
 *
 * -------------------------------------------------------------------
 * What the exhausted-state checks pin down
 * -------------------------------------------------------------------
 * Every allocating call must report EMFILE, and must report it as EMFILE:
 *
 *   open, openat  fdtables' get_unused_virtual_fd has no free slot. Both
 *                 must also close the kernel fd they already obtained
 *                 before returning, as pipe()/dup() already do, or every
 *                 such failure leaks one HOST fd -- and the host fd space
 *                 is shared by all cages, so that leak is exactly the
 *                 cross-cage denial this test exists to rule out. It is
 *                 unobservable from inside a cage (it would take ~1e6
 *                 failures to surface), so the loop below deliberately
 *                 stops at the FIRST EMFILE rather than spinning on it;
 *                 that part of the fix is enforced by review, not here.
 *   dup           same allocation, separate call site.
 *   fcntl F_DUPFD (and F_DUPFD_CLOEXEC) go through
 *                 get_unused_virtual_fd_from_startfd. Both used to
 *                 translate its EMFILE into EBADF, which makes an
 *                 exhausted cage indistinguishable from one that was
 *                 handed a bad descriptor.
 *   pipe          must fail atomically: neither end may be installed.
 *
 * dup2() onto an already-open descriptor must still SUCCEED while
 * exhausted. It reuses an occupied slot rather than allocating one, so a
 * cap enforced on operations instead of on table slots shows up here and
 * nowhere else.
 *
 * -------------------------------------------------------------------
 * Ordering hazards this test is built around
 * -------------------------------------------------------------------
 * (1) fork() must happen from the MAIN thread only: lind returns -1 for
 *     fork() from a non-main thread (lind-multi-process/src/lib.rs) while
 *     native glibc succeeds, which would diverge the harness's
 *     native-vs-lind stdout diff. This test keeps the parent
 *     single-threaded throughout, so the hazard cannot arise.
 * (2) The rendezvous fds are created BEFORE A starts exhausting, so they
 *     survive into the exhausted state. Once A is at its cap it cannot
 *     open anything -- including a pipe -- so a rendezvous established
 *     after exhaustion would be unbuildable, and sleeping instead would be
 *     nondeterministic. write()/read() on already-open fds need no
 *     allocation, so A can still talk to the parent while saturated.
 * (3) The parent closes its own ack[1] before reading acks, so a child
 *     that dies before acking yields immediate EOF (diagnosable) rather
 *     than a 30s harness timeout.
 * (4) Forked children use only raw syscalls and _exit() with a distinct
 *     code per failure: fork() in a multithreaded process copies only the
 *     calling thread, so printf()/malloc() in the child can deadlock on a
 *     lock held by a thread that does not exist there. A uses codes 30-45
 *     and B uses 51-60, and the parent recovers and reports the code on
 *     fd 2 -- a child that fails simply stops acking, so without that the
 *     only visible symptom would be a missing ack.
 * (5) A records its descriptors in a static array rather than closing a
 *     numeric range on the way out. The range's upper bound differs
 *     between the two runs (see FD_CAP above), and a blind range close
 *     would also shut the rendezvous fds A still needs.
 *
 * Determinism: exactly one line on stdout ("CONC-005a PASS\n"). No pids,
 * clocks, addresses, fd numbers, fd counts, or errno values are ever
 * printed or compared -- in particular the number of descriptors A
 * managed to open is deliberately never reported, since that is precisely
 * what differs between native and lind. Diagnostics go to fd 2, which the
 * harness surfaces only on a nonzero exit.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define DIRNAME "conc005a_dir"
#define HOGF    DIRNAME "/hog.dat"
#define PARENTF DIRNAME "/parent.dat"
#define BFILE   DIRNAME "/bfile.dat"

/* Upper bound on A's open loop. Above lind's FD_PER_PROCESS_MAX (1024) so
 * the limit is genuinely reached there, and small enough to be free on a
 * native host with a high RLIMIT_NOFILE. See the header. */
#define FD_CAP 1200

/* How many descriptors B allocates to prove it is not being denied. Small
 * enough to be trivially satisfiable, large enough that a shared or
 * global cap saturated by A could not possibly serve it. */
#define B_FDS 32

#define RECORD 16
#define NREC   8

/* fd-leak scan in the parent: comparable across native/lind as long as
 * both allocate the lowest free fd, which fdtables' get_unused_virtual_fd
 * does by construction (same convention as conc_002/003/004). Disable
 * with -DCONC005A_NO_FD_LEAK_SCAN if this ever proves to be an artifact
 * rather than a real leak. */
#ifndef CONC005A_NO_FD_LEAK_SCAN
#define DO_FD_LEAK_SCAN 1
#else
#define DO_FD_LEAK_SCAN 0
#endif
#define FD_SCAN 128

/* ------------------------------------------------------------------ */
/* Deterministic per-(a,c) byte pattern (same shape as conc_002/003/004). */
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
    unlink(HOGF);
    unlink(PARENTF);
    unlink(BFILE);
    rmdir(DIRNAME);
}

/* ------------------------------------------------------------------ */
/* EINTR-retrying wrappers. Lind interrupts blocking syscalls by       */
/* sending SIGUSR2 to a cage's main thread, via a handler installed    */
/* with no SA_RESTART. That never happens to this test today only     */
/* because SIGCHLD's default disposition is Ignore -- a fact about     */
/* SIGCHLD, not a guarantee from lind -- so every blocking call here   */
/* is retried on EINTR regardless.                                     */
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

#if DO_FD_LEAK_SCAN
static void snapshot_fds(int *out)
{
    int i;
    for (i = 0; i < FD_SCAN; i++)
        out[i] = (fcntl(i, F_GETFD) >= 0) ? 1 : 0;
}
#endif

/* Wait for a child's ack byte.
 *
 * A child that fails _exit()s with a distinct code instead of acking,
 * which drops its ack end and turns this read into EOF. Recovering and
 * reporting that code is the whole point of the exit-code bands: without
 * it the only symptom is "the ack never arrived", which says nothing
 * about which of A's ten checks actually tripped. Diagnostics go to fd 2,
 * which the harness surfaces only on a nonzero exit, so this cannot
 * perturb the stdout diff. */
static void expect_exit0(int status, const char *who)
{
    if (WEXITSTATUS(status) == 0)
        return;
    {
        char m[128];
        int n = snprintf(m, sizeof m, "conc_005a FAIL child=%s exit=%d\n", who,
                         WEXITSTATUS(status));
        write(2, m, (size_t)n);
    }
    assert(0 && "child reported a failure");
}

static void expect_ack(int ackfd, pid_t child, const char *phase)
{
    char buf;
    int status = 0;
    char m[160];
    int n;

    if (xread(ackfd, &buf, 1) == 1)
        return;

    if (xwaitpid(child, &status) == child && WIFEXITED(status))
        n = snprintf(m, sizeof m, "conc_005a FAIL no-ack phase=%s child_exit=%d\n", phase,
                     WEXITSTATUS(status));
    else
        n = snprintf(m, sizeof m, "conc_005a FAIL no-ack phase=%s child_status=%d\n", phase,
                     status);
    write(2, m, (size_t)n);
    assert(0 && "child exited before acking");
}

/* ------------------------------------------------------------------ */
/* Two-pipe (gate/ack) rendezvous, same shape as conc_003/conc_004.    */
/* A acks twice (exhausted, then recovered) and is gated once in       */
/* between, so the ends are driven directly rather than through        */
/* conc_004's single-rendezvous helpers.                               */
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

/* ------------------------------------------------------------------ */
/* Child A: exhaust, hold, prove the failure modes, then recover.      */
/* ------------------------------------------------------------------ */

/* A's descriptors, in BSS: a forked child must not malloc (hazard 4), and
 * the exact set is needed for the cleanup phase (hazard 5). */
static int g_fds[FD_CAP];

static void child_a(barrier_t *b)
{
    int nfds = 0;
    int hit_limit = 0;
    char one = 1;
    char buf;

    /* Shed the ends this child does not own. Everything A needs from here
     * on is already open: past the cap it cannot obtain anything new. */
    close(b->gate[1]);
    close(b->ack[0]);

    /* --- Phase 1: exhaust ------------------------------------------- */
    while (nfds < FD_CAP) {
        int fd = open(HOGF, O_RDONLY);
        if (fd < 0) {
            if (errno != EMFILE)
                _exit(31); /* failed for some reason other than the cap */
            hit_limit = 1;
            break;
        }
        g_fds[nfds++] = fd;
    }
    if (nfds == 0)
        _exit(30); /* could not open the hog file even once */

    /* --- Phase 2: the exhausted state must be well-formed ------------ */
    if (hit_limit) {
        int probe = g_fds[nfds - 1]; /* a descriptor known to be valid */
        int other = g_fds[0];
        int p[2];

        /* (A1) open/openat: the allocating calls that got us here. */
        if (open(HOGF, O_RDONLY) != -1 || errno != EMFILE)
            _exit(32);
        if (openat(AT_FDCWD, HOGF, O_RDONLY) != -1 || errno != EMFILE)
            _exit(33);

        /* (A2) dup: same allocation, different call site. */
        if (dup(probe) != -1 || errno != EMFILE)
            _exit(34);

        /* (A3) F_DUPFD/F_DUPFD_CLOEXEC: these reported EBADF before, which
         * conflates "your table is full" with "your fd is invalid". */
        if (fcntl(probe, F_DUPFD, 0) != -1 || errno != EMFILE)
            _exit(35);
        if (fcntl(probe, F_DUPFD_CLOEXEC, 0) != -1 || errno != EMFILE)
            _exit(36);

        /* (A4) pipe must fail atomically -- no half-installed end. */
        p[0] = -1;
        p[1] = -1;
        if (pipe(p) != -1 || errno != EMFILE)
            _exit(37);
        if (p[0] != -1 || p[1] != -1)
            _exit(38);

        /* (A5) dup2 onto an OCCUPIED slot allocates nothing, so it must
         * still work. Done last: it overwrites `probe`'s entry. Both ends
         * refer to the same hog file, so the table stays consistent. */
        if (dup2(other, probe) != probe)
            _exit(39);
    }

    /* Report the exhausted state and hold it until the parent is done. */
    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(40);
    if (xread(b->gate[0], &buf, 1) != 1)
        _exit(41);

    /* --- Phase 3: cleanup restores the ability to allocate ---------- */
    {
        int i;
        for (i = 0; i < nfds; i++) {
            if (close(g_fds[i]) != 0)
                _exit(42);
        }
    }
    {
        int fd = open(HOGF, O_RDONLY);
        if (fd < 0)
            _exit(43); /* still exhausted after releasing everything */
        if (close(fd) != 0)
            _exit(44);
    }

    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(45);
    _exit(0);
}

/* ------------------------------------------------------------------ */
/* Child B: forked while A is saturated; must be unaffected.           */
/* ------------------------------------------------------------------ */
static void child_b(barrier_t *b)
{
    int fds[B_FDS];
    unsigned char rec[RECORD], got[RECORD];
    struct stat st;
    int i, fd;

    /* B inherited A's rendezvous. Holding gate[0] open would be harmless,
     * but holding ack[1] open would keep the parent's ack[0] from ever
     * reporting EOF if A died -- exactly the diagnosability hazard (3) is
     * there to avoid. Shed both. */
    close(b->gate[0]);
    close(b->gate[1]);
    close(b->ack[0]);
    close(b->ack[1]);

    /* (B1) Allocation is not denied: a cap saturated by A could not
     * possibly serve these. */
    for (i = 0; i < B_FDS; i++) {
        fds[i] = open(HOGF, O_RDONLY);
        if (fds[i] < 0)
            _exit(51);
    }
    for (i = 0; i < B_FDS; i++) {
        if (close(fds[i]) != 0)
            _exit(52);
    }

    /* (B2) File operations remain correct, not merely permitted. */
    fd = open(BFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (fd < 0)
        _exit(53);
    for (i = 0; i < NREC; i++) {
        make_record(rec, 1, i);
        if (xwrite(fd, rec, RECORD) != RECORD)
            _exit(54);
    }
    if (lseek(fd, 0, SEEK_SET) != 0)
        _exit(55);
    for (i = 0; i < NREC; i++) {
        make_record(rec, 1, i);
        if (xread(fd, got, RECORD) != RECORD)
            _exit(56);
        if (memcmp(rec, got, RECORD) != 0)
            _exit(57);
    }
    if (fstat(fd, &st) != 0)
        _exit(58);
    if (st.st_size != (off_t)(RECORD * NREC))
        _exit(59);
    if (close(fd) != 0)
        _exit(60);

    _exit(0);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    barrier_t b;
    pid_t pa, pb;
    int status;
    char one = 1;
#if DO_FD_LEAK_SCAN
    int before[FD_SCAN], after[FD_SCAN];
#endif

    pre_clean();
    assert(mkdir(DIRNAME, 0755) == 0);

    /* The file every cage piles descriptors onto. Created and closed here
     * so no cage starts out holding an extra reference to it. */
    {
        int fd = open(HOGF, O_WRONLY | O_CREAT | O_TRUNC, 0644);
        assert(fd >= 0);
        assert(xwrite(fd, "x", 1) == 1);
        assert(close(fd) == 0);
    }

#if DO_FD_LEAK_SCAN
    snapshot_fds(before);
#endif

    assert(barrier_init(&b) == 0);

    fflush(stdout); /* repo convention; nothing buffered here */
    pa = fork();
    assert(pa >= 0);
    if (pa == 0)
        child_a(&b);

    /* Hazard (3): shed the parent's own copies so a child that dies before
     * acking becomes EOF on ack[0] rather than an indefinite block. */
    close(b.gate[0]);
    close(b.ack[1]);

    /* (a) A has reached its own cap and is holding it. */
    expect_ack(b.ack[0], pa, "exhausted");

    /* (b) The parent -- a different cage -- is unaffected while A is
     * saturated. Cheap coverage that needs no extra cage. */
    {
        unsigned char rec[RECORD], got[RECORD];
        struct stat st;
        int fd = open(PARENTF, O_RDWR | O_CREAT | O_TRUNC, 0644);
        assert(fd >= 0);
        make_record(rec, 2, 0);
        assert(xwrite(fd, rec, RECORD) == RECORD);
        assert(lseek(fd, 0, SEEK_SET) == 0);
        assert(xread(fd, got, RECORD) == RECORD);
        assert(memcmp(rec, got, RECORD) == 0);
        assert(fstat(fd, &st) == 0);
        assert(st.st_size == (off_t)RECORD);
        assert(close(fd) == 0);
    }

    /* (c) A whole new cage can still be created while A is saturated, and
     * it gets a working fd table of its own. B is forked here, not
     * earlier, so its success cannot be explained by it having allocated
     * before A filled up. */
    fflush(stdout); /* repo convention; nothing buffered here */
    pb = fork();
    assert(pb >= 0);
    if (pb == 0)
        child_b(&b);

    /* (d) B completed every allocation and every file operation. This is
     * the core isolation claim. */
    assert(xwaitpid(pb, &status) == pb);
    assert(WIFEXITED(status));
    expect_exit0(status, "B");

    /* (e) Release A; it closes everything and must be able to allocate
     * again. Cleanup restores the cage, it does not merely stop failing. */
    assert(xwrite(b.gate[1], &one, 1) == 1);
    expect_ack(b.ack[0], pa, "recovered");
    assert(xwaitpid(pa, &status) == pa);
    assert(WIFEXITED(status));
    expect_exit0(status, "A");

    close(b.gate[1]);
    close(b.ack[0]);

    /* (f) Nothing leaked anywhere: the parent can still allocate, the
     * directory empties cleanly (ENOTEMPTY here would mean a file the
     * children created was never accounted for), and the parent's own fd
     * space is exactly as it started. */
    {
        int fd = open(HOGF, O_RDONLY);
        assert(fd >= 0);
        assert(close(fd) == 0);
    }
    assert(unlink(HOGF) == 0);
    assert(unlink(PARENTF) == 0);
    assert(unlink(BFILE) == 0);
    assert(rmdir(DIRNAME) == 0);

#if DO_FD_LEAK_SCAN
    snapshot_fds(after);
    {
        int i, failures = 0;
        for (i = 0; i < FD_SCAN; i++) {
            if (before[i] != after[i]) {
                char m[128];
                int n = snprintf(m, sizeof m,
                                 "conc_005a FAIL fd-leak fd=%d before=%d after=%d\n",
                                 i, before[i], after[i]);
                write(2, m, (size_t)n);
                failures++;
            }
        }
        assert(failures == 0);
    }
#endif

    write(1, "CONC-005a PASS\n", 15);
    return 0;
}
