/*
 * CONC-005c -- syscall flood isolation.
 *
 * CONC-005a and CONC-005b exhaust a countable resource (descriptors,
 * memory) and ask whether the count is per-cage. CONC-005c exhausts
 * something with no count at all: time on the shared syscall path. One
 * cage spins issuing syscalls as fast as it can from several threads, and
 * a second cage must keep making forward progress the whole time.
 *
 * -------------------------------------------------------------------
 * What is actually shared, and therefore what this can starve
 * -------------------------------------------------------------------
 * Cages are otherwise genuinely parallel -- every cage thread is a real OS
 * thread with its own wasmtime Store, and there is no global runtime lock
 * around rawposix. But every guest syscall, from every thread of every
 * cage, passes through 3i's dispatch, and with the default `hashmap`
 * handler-table backend (src/threei/Cargo.toml) that means taking one
 * process-global std::sync::Mutex per call
 * (src/threei/src/handler_table/hashmap_impl.rs). Each call also probes
 * the EXITING_TABLE DashSet shard for its cage and bumps the cage's
 * ArcSwap refcount, twice over (src/threei/src/threei.rs).
 *
 * So this test has a single, identified target. If it ever does fail, the
 * fix is the sharded `dashmap` backend that already exists alongside the
 * default one -- not a weaker assertion here.
 *
 * -------------------------------------------------------------------
 * Why getpid() is the flood, and a shared page is the scoreboard
 * -------------------------------------------------------------------
 * getpid() is the only implemented syscall that does no host work at all:
 * rawposix answers it from the cage table (src/rawposix/src/sys_calls.rs),
 * and this glibc does not cache it, so every call is a real dispatch. That
 * concentrates the flood on the shared path rather than on the host
 * kernel, where the OS scheduler would absorb it.
 *
 * Progress is recorded in a MAP_SHARED|MAP_ANONYMOUS page mapped BEFORE
 * the forks, so reading or bumping a counter costs zero syscalls. Using a
 * pipe instead would drag fdtables' shard locks into the very measurement
 * the test is trying to make.
 *
 * -------------------------------------------------------------------
 * Why the assertion is a floor, and why there is no timer
 * -------------------------------------------------------------------
 * The test asserts only that B completes TARGET operations while A floods.
 * It does NOT compare rates, ratios, or latencies: native Linux has no
 * global syscall mutex, so any threshold tight enough to be interesting
 * under lind is trivially satisfied natively, and any threshold tight
 * enough to be interesting on an idle machine is flaky on a loaded CI box.
 * A floor is the assertion that means the same thing in both places.
 *
 * There is deliberately no in-test watchdog. Total starvation or a hang is
 * caught by the harness's own 30s timeout, which reports it as a timeout;
 * building a second, clock-derived watchdog on top would only add a source
 * of nondeterminism to a test whose entire value is being deterministic.
 *
 * -------------------------------------------------------------------
 * Ordering hazards this test is built around
 * -------------------------------------------------------------------
 * (1) fork() must happen from the MAIN thread only: lind returns -1 for
 *     fork() from a non-main thread (lind-multi-process/src/lib.rs) while
 *     native glibc succeeds, which would diverge the harness's
 *     native-vs-lind stdout diff. The parent stays single-threaded, and A
 *     creates its threads only AFTER it has been forked -- pthread_create
 *     inside a cage is fine, it is fork() from a thread that is not.
 * (2) Both children are forked before either starts work, and held at a
 *     two-pipe rendezvous, so B cannot finish before A has begun flooding.
 *     A test where B raced ahead of the flood would pass vacuously.
 * (3) The parent closes its own ack[1] before reading acks, so a child
 *     that dies before acking yields immediate EOF (diagnosable) rather
 *     than a 30s harness timeout.
 * (4) Forked children use only raw syscalls and _exit() with a distinct
 *     code per failure. A's flooding threads must not assert either: they
 *     record a failure line and stop, in the style of conc_002's workers,
 *     because a pthread that aborts inside a forked cage is far harder to
 *     attribute than an exit code. A uses codes 30-38 and B uses 51-57.
 * (5) Only two cages are forked. Cage IDs are never recycled
 *     (src/cage/src/cage.rs) and the process-wide ceiling is 2046, so a
 *     test that forked per iteration would burn a shared, unreplenishable
 *     resource -- which is what fork_max_cages.c does, and why it is in
 *     skip_test_cases.txt.
 * (6) The flood is stopped by a flag in the shared page, not by a signal
 *     and not by killing A. A must exit cleanly for its own assertions to
 *     be meaningful.
 *
 * Determinism: exactly one line on stdout ("CONC-005c PASS\n"). No pids,
 * clocks, addresses, iteration counts, or rates are ever printed or
 * compared -- how many syscalls A lands is exactly what differs between
 * machines and between native and lind. Diagnostics go to fd 2, which the
 * harness surfaces only on a nonzero exit.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define DIRNAME "conc005c_dir"
#define BFILE   DIRNAME "/b.dat"

/* Flooding threads inside cage A. Kept small: the point is to keep the
 * shared dispatch path busy, not to oversubscribe a CI container's cores
 * and turn the test into a measurement of the host scheduler. */
#define NFLOOD 4

/* Operations B must complete. Each is a full file lifecycle, so this is a
 * few hundred syscalls' worth of real work -- enough that B could not
 * finish it in a scheduling fluke, small enough to stay far inside the
 * harness's 30s timeout even on a slow machine. */
#define TARGET 200

#define RECORD 16
#define NREC   4

/* fd-leak scan in the parent, same convention as conc_002/003/004/005a.
 * Disable with -DCONC005C_NO_FD_LEAK_SCAN if it ever proves to be an
 * artifact rather than a real leak. */
#ifndef CONC005C_NO_FD_LEAK_SCAN
#define DO_FD_LEAK_SCAN 1
#else
#define DO_FD_LEAK_SCAN 0
#endif
#define FD_SCAN 128

/* The cross-cage scoreboard. One page, MAP_SHARED|MAP_ANONYMOUS, mapped
 * before any fork so every cage sees the same memory. Every field is
 * volatile: these are written by one cage and read by another, with no
 * lock and no syscall in between, so the compiler must not cache them. */
struct shared {
    volatile long b_progress; /* operations B has completed */
    volatile long a_started;  /* A's flooding threads that have begun */
    volatile long stop;       /* parent -> A: wind down */
};

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

static void pre_clean(void)
{
    unlink(BFILE);
    rmdir(DIRNAME);
}

/* ------------------------------------------------------------------ */
/* EINTR-retrying wrappers (same rationale as conc_003/004).            */
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

static void expect_exit0(int status, const char *who)
{
    if (WEXITSTATUS(status) == 0)
        return;
    {
        char m[128];
        int n = snprintf(m, sizeof m, "conc_005c FAIL child=%s exit=%d\n", who,
                         WEXITSTATUS(status));
        write(2, m, (size_t)n);
    }
    assert(0 && "child reported a failure");
}

#if DO_FD_LEAK_SCAN
static void snapshot_fds(int *out)
{
    int i;
    for (i = 0; i < FD_SCAN; i++)
        out[i] = (fcntl(i, F_GETFD) >= 0) ? 1 : 0;
}
#endif

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
/* Cage A: the flooder.                                                */
/* ------------------------------------------------------------------ */
static struct shared *g_sh;      /* A's view of the scoreboard */
static volatile long g_thread_fail; /* nonzero => some flood thread failed */

/* Flooding threads must not assert (hazard 4): they record and return, and
 * A's main thread turns that into an exit code. */
static void *flood_fn(void *arg)
{
    pid_t self = getpid();
    (void)arg;

    /* Announce arrival before spinning, so the parent's release is not
     * racing thread creation. */
    __sync_fetch_and_add(&g_sh->a_started, 1);

    while (g_sh->stop == 0) {
        int i;
        for (i = 0; i < 64; i++) {
            /* The flood itself. getpid() stays inside rawposix, so this
             * loads the shared dispatch path and nothing else. */
            if (getpid() != self) {
                g_thread_fail = 1;
                return NULL;
            }
        }
    }
    return NULL;
}

static void child_a(barrier_t *b, struct shared *sh)
{
    pthread_t th[NFLOOD];
    int made = 0;
    int i;
    char one = 1;
    char buf;

    g_sh = sh;

    close(b->gate[1]);
    close(b->ack[0]);

    /* Hold at the starting line until the parent has both children ready,
     * so the flood cannot finish before B has even begun (hazard 2). */
    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(30);
    if (xread(b->gate[0], &buf, 1) != 1)
        _exit(31);

    for (i = 0; i < NFLOOD; i++) {
        if (pthread_create(&th[i], NULL, flood_fn, NULL) != 0)
            break;
        made++;
    }
    if (made == 0)
        _exit(32); /* could not flood at all -- the test would be vacuous */

    /* The main thread floods too, so A is never merely idle-waiting. */
    {
        pid_t self = getpid();
        while (sh->stop == 0) {
            if (getpid() != self)
                _exit(33);
        }
    }

    for (i = 0; i < made; i++) {
        if (pthread_join(th[i], NULL) != 0)
            _exit(34);
    }
    if (g_thread_fail)
        _exit(35);
    if (made != NFLOOD)
        _exit(36); /* fewer threads than asked for: report, do not hide it */

    _exit(0);
}

/* ------------------------------------------------------------------ */
/* Cage B: the victim. Must keep completing real work throughout.      */
/* ------------------------------------------------------------------ */
static void child_b(barrier_t *b, struct shared *sh)
{
    unsigned char rec[RECORD], got[RECORD];
    struct stat st;
    char one = 1;
    char buf;
    long op;

    close(b->gate[1]);
    close(b->ack[0]);

    if (xwrite(b->ack[1], &one, 1) != 1)
        _exit(51);
    if (xread(b->gate[0], &buf, 1) != 1)
        _exit(52);

    for (op = 0; op < TARGET; op++) {
        int fd, i;

        fd = open(BFILE, O_RDWR | O_CREAT | O_TRUNC, 0644);
        if (fd < 0)
            _exit(53);
        for (i = 0; i < NREC; i++) {
            make_record(rec, 5, (int)((op + i) & 0xff));
            if (xwrite(fd, rec, RECORD) != RECORD)
                _exit(54);
        }
        if (lseek(fd, 0, SEEK_SET) != 0)
            _exit(55);
        for (i = 0; i < NREC; i++) {
            make_record(rec, 5, (int)((op + i) & 0xff));
            if (xread(fd, got, RECORD) != RECORD)
                _exit(56);
            /* Correctness under contention, not just liveness: a flood that
             * corrupted another cage's I/O would show up here. */
            if (memcmp(rec, got, RECORD) != 0)
                _exit(57);
        }
        if (fstat(fd, &st) != 0 || st.st_size != (off_t)(RECORD * NREC))
            _exit(58);
        if (close(fd) != 0)
            _exit(59);

        /* Publish progress only after the whole operation succeeded. */
        sh->b_progress = op + 1;
    }

    _exit(0);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    barrier_t ba, bb;
    struct shared *sh;
    pid_t pa, pb;
    int status;
    char one = 1;
    char buf;
#if DO_FD_LEAK_SCAN
    int before[FD_SCAN], after[FD_SCAN];
#endif

    pre_clean();
    assert(mkdir(DIRNAME, 0755) == 0);

#if DO_FD_LEAK_SCAN
    snapshot_fds(before);
#endif

    /* The scoreboard, mapped before any fork so all three cages share it. */
    sh = (struct shared *)mmap(NULL, 4096, PROT_READ | PROT_WRITE,
                               MAP_SHARED | MAP_ANONYMOUS, -1, 0);
    assert(sh != MAP_FAILED);
    sh->b_progress = 0;
    sh->a_started = 0;
    sh->stop = 0;

    assert(barrier_init(&ba) == 0);
    assert(barrier_init(&bb) == 0);

    fflush(stdout); /* repo convention; nothing buffered here */
    pa = fork();
    assert(pa >= 0);
    if (pa == 0)
        child_a(&ba, sh);

    fflush(stdout); /* repo convention; nothing buffered here */
    pb = fork();
    assert(pb >= 0);
    if (pb == 0)
        child_b(&bb, sh);

    /* Hazard (3): shed the parent's own copies so a child that dies before
     * acking becomes EOF rather than an indefinite block. */
    close(ba.gate[0]);
    close(ba.ack[1]);
    close(bb.gate[0]);
    close(bb.ack[1]);

    /* Both children are at the starting line. */
    assert(xread(ba.ack[0], &buf, 1) == 1);
    assert(xread(bb.ack[0], &buf, 1) == 1);

    /* Release A first and wait for its threads to be spinning, so B runs
     * entirely inside the flood rather than alongside its ramp-up. */
    assert(xwrite(ba.gate[1], &one, 1) == 1);
    while (sh->a_started < NFLOOD) {
        /* Spin without syscalls: any wait primitive here would itself
         * queue on the very dispatch path under test. If A never starts,
         * the harness timeout catches it -- see the header on watchdogs. */
    }

    assert(xwrite(bb.gate[1], &one, 1) == 1);

    /* THE ASSERTION: B runs to completion while A floods. A cage starved
     * by another cage's syscall volume would never get here, and the
     * harness's 30s timeout would report it. */
    assert(xwaitpid(pb, &status) == pb);
    assert(WIFEXITED(status));
    expect_exit0(status, "B");
    assert(sh->b_progress == TARGET);

    /* Wind the flood down and confirm A itself was healthy throughout --
     * a flooder that had died early would have made the test vacuous. */
    sh->stop = 1;
    assert(xwaitpid(pa, &status) == pa);
    assert(WIFEXITED(status));
    expect_exit0(status, "A");

    close(ba.gate[1]);
    close(ba.ack[0]);
    close(bb.gate[1]);
    close(bb.ack[0]);

    assert(munmap(sh, 4096) == 0);
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
                                 "conc_005c FAIL fd-leak fd=%d before=%d after=%d\n", i,
                                 before[i], after[i]);
                write(2, m, (size_t)n);
                failures++;
            }
        }
        assert(failures == 0);
    }
#endif

    write(1, "CONC-005c PASS\n", 15);
    return 0;
}
