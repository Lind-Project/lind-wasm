/*
 * CONC-002: cage fd-table / filesystem concurrency stress.
 *
 * NTHREADS worker threads hammer the cage's fd table and filesystem on
 * per-thread-private paths, while the MAIN thread, and only the main
 * thread, fork()s and reaps children, interleaved with that activity.
 * That races rawposix's copy_fdtable_for_cage() against live fd-table
 * churn, which is the bug surface this test targets.
 *
 * fork() is main-thread-only: lind returns -1 for fork() from a non-main
 * thread while native glibc succeeds, which would diverge the harness's
 * native-vs-lind diff. The forked child uses only raw syscalls and
 * _exit(): fork() in a multithreaded process copies only the calling
 * thread, so printf()/malloc() there can deadlock on a lock some other
 * thread held.
 *
 * Determinism: every byte written is a pure function of (thread, round).
 * No pids, clocks, addresses, fd numbers, or errno values are ever printed
 * or compared; only success/failure of each syscall. Output is exactly
 * one line.
 */
#define _GNU_SOURCE
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#define NTHREADS  8
#define ROUNDS    64
#define NFORKS    4
#define RECORD    16

#define DIRNAME   "conc002_dir"
#define SHARED    DIRNAME "/shared.dat"

#define RENAME_EVERY 8   /* rounds */
#define MKDIR_EVERY  16  /* rounds */
#define CHURN_MAX    20000

/* Sentinel byte pattern uses a (thread,round) pair no worker can produce. */
#define SENTINEL_T 15
#define SENTINEL_I 255

/* fd-leak scan: comparable across native/lind as long as both allocate the
 * lowest free fd, which fdtables' get_unused_virtual_fd does by
 * construction.  Disable with -DCONC002_NO_FD_LEAK_SCAN if this ever
 * proves to be an artifact rather than a real leak. */
#ifndef CONC002_NO_FD_LEAK_SCAN
#define DO_FD_LEAK_SCAN 1
#else
#define DO_FD_LEAK_SCAN 0
#endif
#define FD_SCAN 128

/* Deterministic per-(thread,round) byte pattern. */
static void make_record(unsigned char *b, int t, int i)
{
    unsigned s = (unsigned)(t + 1) * 2654435761u + (unsigned)i * 40503u;
    int k;
    for (k = 0; k < RECORD - 2; k++)
        b[k] = (unsigned char)((s >> ((k & 3) * 8)) + (unsigned)k);
    b[RECORD - 2] = (unsigned char)(0xA0 | (t & 0x0f));
    b[RECORD - 1] = (unsigned char)(i & 0xff);
}

/* ------------------------------------------------------------------ */
/* Per-worker state.  Every field is written by exactly one owner:     */
/* the worker itself while running, main only after pthread_join.      */
/* ------------------------------------------------------------------ */
typedef struct {
    int  tid;
    int  fail_line;    /* first failing __LINE__ in this worker; 0 == ok */
    int  fail_errno;
    long fail_detail;  /* round index, or an observed value */
    volatile long rounds_done; /* progress; read by main without a lock
                                 * while this worker is still running (see
                                 * wait_for_progress), so it must be
                                 * volatile even though the read is
                                 * otherwise benign (a stale value just
                                 * delays a fork by a few rounds). */
    long churn_done;
} worker_t;

static worker_t g_w[NTHREADS];
static volatile int g_forks_done;            /* written only by main */
static int  g_shared_fd = -1;
static char g_child_path[NFORKS][64];        /* built before any fork */
static pthread_barrier_t g_start;            /* NTHREADS+1 parties, waited once */

#define WFAIL(w, det) do {                                       \
        if ((w)->fail_line == 0) {                                \
            (w)->fail_errno  = errno;                              \
            (w)->fail_detail = (long)(det);                         \
            (w)->fail_line   = __LINE__; /* set last */              \
        }                                                             \
        goto done;                                                     \
    } while (0)
#define WCHECK(w, cond, det) do { if (!(cond)) WFAIL(w, det); } while (0)

/* Best-effort cleanup of leftovers from a previous crashed run. */
static void pre_clean(void)
{
    char p[64];
    int t, i, f;

    unlink(SHARED);
    for (t = 0; t < NTHREADS; t++) {
        snprintf(p, sizeof p, DIRNAME "/t%d.dat", t); unlink(p);
        snprintf(p, sizeof p, DIRNAME "/t%d.tmp", t); unlink(p);
        snprintf(p, sizeof p, DIRNAME "/t%d.ren", t); unlink(p);
        for (i = 0; i < ROUNDS; i += MKDIR_EVERY) {
            snprintf(p, sizeof p, DIRNAME "/t%d.d%d", t, i);
            rmdir(p);
        }
    }
    for (f = 0; f < NFORKS; f++) {
        snprintf(p, sizeof p, DIRNAME "/child%d.dat", f);
        unlink(p);
    }
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

/* -------------------------------------------------------------------- */
/* Worker: a deterministic mixture of open/dup/dup2/fcntl/lseek/read/    */
/* write/pread/pwrite/stat/fstat/lstat/access/close every round, plus    */
/* periodic rename/unlink and mkdir/rmdir cycles, plus a bounded fd-      */
/* table churn phase after the rounds to guarantee overlap with forks.    */
/* -------------------------------------------------------------------- */
static void *worker(void *arg)
{
    worker_t *w = (worker_t *)arg;
    int t = w->tid;
    unsigned char rec[RECORD], chk[RECORD];
    char priv[64], tmp[64], ren[64], sub[64];
    struct stat sf, sp, sl;
    int i;

    snprintf(priv, sizeof priv, DIRNAME "/t%d.dat", t);
    snprintf(tmp,  sizeof tmp,  DIRNAME "/t%d.tmp", t);
    snprintf(ren,  sizeof ren,  DIRNAME "/t%d.ren", t);

    pthread_barrier_wait(&g_start);

    for (i = 0; i < ROUNDS; i++) {
        int fd, fdup, tgt, fdd;
        off_t off, soff;

        make_record(rec, t, i);

        /* --- open / dup / dup2 / fcntl: fd-table churn ------------- */
        fd = open(priv, O_RDWR | O_CREAT, 0644);
        WCHECK(w, fd >= 0, i);

        fdup = dup(fd);
        WCHECK(w, fdup >= 0, i);

        /* dup2's target must be an fd this thread already owns, NEVER
         * a fixed number, since the fd space is shared across threads. */
        tgt = dup(fd);
        WCHECK(w, tgt >= 0, i);
        WCHECK(w, dup2(g_shared_fd, tgt) == tgt, i);

        fdd = fcntl(fd, F_DUPFD, 0);
        WCHECK(w, fdd >= 0, i);
        WCHECK(w, fcntl(fd, F_SETFD, FD_CLOEXEC) == 0, i);
        WCHECK(w, (fcntl(fd, F_GETFD) & FD_CLOEXEC) != 0, i);
        WCHECK(w, fcntl(fd, F_SETFD, 0) == 0, i);
        WCHECK(w, (fcntl(fd, F_GETFD) & FD_CLOEXEC) == 0, i);

        /* --- private file: lseek + write + lseek + read ------------- */
        off = (off_t)i * RECORD;
        WCHECK(w, lseek(fd, off, SEEK_SET) == off, i);
        WCHECK(w, write(fd, rec, RECORD) == RECORD, i);
        WCHECK(w, lseek(fdup, off, SEEK_SET) == off, i);
        WCHECK(w, read(fdup, chk, RECORD) == RECORD, i);
        WCHECK(w, memcmp(chk, rec, RECORD) == 0, i);

        /* --- shared inode: positional only --------------------------
         * All worker access to g_shared_fd is pread/pwrite, never
         * read/write/lseek, so there is no shared-file-offset race even
         * though every thread touches the same fd concurrently. */
        soff = (off_t)(1 + t * ROUNDS + i) * RECORD;
        WCHECK(w, pwrite(g_shared_fd, rec, RECORD, soff) == RECORD, i);
        WCHECK(w, pread(g_shared_fd, chk, RECORD, soff) == RECORD, i);
        WCHECK(w, memcmp(chk, rec, RECORD) == 0, i);

        /* --- stat family agreement ------------------------------------ */
        WCHECK(w, fstat(fd, &sf) == 0, i);
        WCHECK(w, stat(priv, &sp) == 0, i);
        WCHECK(w, lstat(priv, &sl) == 0, i);
        WCHECK(w, S_ISREG(sf.st_mode), i);
        WCHECK(w, sf.st_ino == sp.st_ino && sp.st_ino == sl.st_ino, i);
        WCHECK(w, sf.st_dev == sp.st_dev, i);
        WCHECK(w, sf.st_size == sp.st_size && sp.st_size == sl.st_size, i);
        WCHECK(w, sf.st_mode == sp.st_mode && sp.st_mode == sl.st_mode, i);
        WCHECK(w, sf.st_nlink == 1, i);
        WCHECK(w, sf.st_size == (off_t)(i + 1) * RECORD, (long)sf.st_size);
        WCHECK(w, access(priv, F_OK | R_OK | W_OK) == 0, i);

        WCHECK(w, close(fdd) == 0, i);
        WCHECK(w, close(tgt) == 0, i);
        WCHECK(w, close(fdup) == 0, i);
        WCHECK(w, close(fd) == 0, i);

        /* --- rename / unlink / ftruncate cycle ------------------------ */
        if (i % RENAME_EVERY == 0) {
            int tf = open(tmp, O_WRONLY | O_CREAT | O_TRUNC, 0644);
            WCHECK(w, tf >= 0, i);
            WCHECK(w, write(tf, rec, RECORD) == RECORD, i);
            WCHECK(w, ftruncate(tf, 7) == 0, i);
            WCHECK(w, fstat(tf, &sf) == 0, i);
            WCHECK(w, sf.st_size == 7, (long)sf.st_size);
            WCHECK(w, close(tf) == 0, i);
            WCHECK(w, rename(tmp, ren) == 0, i);
            WCHECK(w, access(tmp, F_OK) == -1, i);
            WCHECK(w, stat(ren, &sp) == 0, i);
            WCHECK(w, sp.st_size == 7, (long)sp.st_size);
            WCHECK(w, unlink(ren) == 0, i);
            WCHECK(w, access(ren, F_OK) == -1, i);
        }

        /* --- mkdir / rmdir cycle --------------------------------------- */
        if (i % MKDIR_EVERY == 0) {
            snprintf(sub, sizeof sub, DIRNAME "/t%d.d%d", t, i);
            WCHECK(w, mkdir(sub, 0755) == 0, i);
            WCHECK(w, access(sub, F_OK) == 0, i);
            WCHECK(w, stat(sub, &sp) == 0, i);
            WCHECK(w, S_ISDIR(sp.st_mode), i);
            WCHECK(w, rmdir(sub) == 0, i);
        }

        w->rounds_done = i + 1;
    }

    /* Bounded churn phase: guarantees fd-table mutation overlaps every
     * fork, without changing any state the final validation checks. */
    while (!g_forks_done && w->churn_done < CHURN_MAX) {
        int a = open(priv, O_RDONLY);
        if (a < 0)
            break;
        int b = dup(a);
        if (b >= 0)
            close(b);
        close(a);
        w->churn_done++;
    }

done:
    return NULL;
}

/* -------------------------------------------------------------------- *
 * Forked child: async-signal-safe only (raw syscalls, memcmp, _exit()).
 * No stdio, no malloc, no assert() (assert -> fprintf -> abort), no
 * pthread calls.  Distinct exit codes make the parent's failure report
 * actionable without any printed output.
 * -------------------------------------------------------------------- */
static void child_main(int f)
{
    unsigned char sent[RECORD], got[RECORD];
    struct stat st;
    int d, c;

    /* 1. inherited fd is live in the child's fresh cage */
    if (fstat(g_shared_fd, &st) != 0) _exit(11);
    if (!S_ISREG(st.st_mode))         _exit(12);

    /* 2. inherited fd sees the pre-fork sentinel (fd-table copy fidelity) */
    make_record(sent, SENTINEL_T, SENTINEL_I);
    if (pread(g_shared_fd, got, RECORD, 0) != RECORD) _exit(13);
    if (memcmp(got, sent, RECORD) != 0)               _exit(14);

    /* 3. the child can allocate/free fds of its own in the copied table */
    d = dup(g_shared_fd);
    if (d < 0)         _exit(15);
    if (close(d) != 0) _exit(16);

    /* 4. a fresh file in the child's cage: open/write/close/unlink */
    c = open(g_child_path[f], O_RDWR | O_CREAT | O_TRUNC, 0644);
    if (c < 0)                            _exit(17);
    if (write(c, sent, RECORD) != RECORD) _exit(18);
    if (close(c) != 0)                    _exit(19);
    if (unlink(g_child_path[f]) != 0)     _exit(20);

    /* 5. closing the inherited fd here must NOT affect the parent; the
     * parent re-verifies g_shared_fd after waitpid. */
    if (close(g_shared_fd) != 0)          _exit(21);
    if (fstat(g_shared_fd, &st) == 0)     _exit(22); /* must now be bad */

    _exit(0);
}

/* Bounded, never-asserting spin: a timing heuristic to place forks
 * mid-churn.  Asserting on it would be flaky under load, so on budget
 * exhaustion it just gives up and lets the fork happen anyway. */
static void wait_for_progress(long target)
{
    long budget;
    for (budget = 2000000; budget > 0; budget--) {
        long sum = 0;
        int t;
        for (t = 0; t < NTHREADS; t++)
            sum += g_w[t].rounds_done;
        if (sum >= target)
            return;
        sched_yield();
    }
}

int main(void)
{
    unsigned char rec[RECORD], buf[RECORD];
    struct stat sa, sb, sc;
#if DO_FD_LEAK_SCAN
    int before[FD_SCAN], after[FD_SCAN];
#endif
    pthread_t th[NTHREADS];
    int t, f, i, failures;

    pre_clean();
    assert(mkdir(DIRNAME, 0755) == 0);

    g_shared_fd = open(SHARED, O_RDWR | O_CREAT | O_TRUNC, 0644);
    assert(g_shared_fd >= 0);
    make_record(rec, SENTINEL_T, SENTINEL_I);
    assert(pwrite(g_shared_fd, rec, RECORD, 0) == RECORD);

    for (f = 0; f < NFORKS; f++)
        snprintf(g_child_path[f], sizeof g_child_path[f],
                 DIRNAME "/child%d.dat", f);  /* built before any fork */

#if DO_FD_LEAK_SCAN
    snapshot_fds(before);
#endif

    assert(pthread_barrier_init(&g_start, NULL, NTHREADS + 1) == 0);
    for (t = 0; t < NTHREADS; t++) {
        g_w[t].tid = t;
        assert(pthread_create(&th[t], NULL, worker, &g_w[t]) == 0);
    }
    {
        int ret = pthread_barrier_wait(&g_start);
        assert(ret == 0 || ret == PTHREAD_BARRIER_SERIAL_THREAD);
    }

    /* ---- forks, on the MAIN thread only, interleaved with worker work */
    for (f = 0; f < NFORKS; f++) {
        pid_t pid, got_pid;
        int status;

        wait_for_progress((long)(f + 1) * NTHREADS * ROUNDS / (NFORKS + 1));

        fflush(stdout);
        pid = fork();
        assert(pid >= 0); /* main thread => lind must succeed too */
        if (pid == 0)
            child_main(f); /* never returns */

        status = 0;
        got_pid = waitpid(pid, &status, 0);
        assert(got_pid == pid);
        assert(WIFEXITED(status));
        assert(WEXITSTATUS(status) == 0);

        /* fd-table-copy oracle: the child closed g_shared_fd in ITS cage;
         * the parent's copy must be untouched. */
        assert(fstat(g_shared_fd, &sa) == 0);
        assert(pread(g_shared_fd, buf, RECORD, 0) == RECORD);
        assert(memcmp(buf, rec, RECORD) == 0);
    }
    g_forks_done = 1;

    for (t = 0; t < NTHREADS; t++)
        assert(pthread_join(th[t], NULL) == 0);
    assert(pthread_barrier_destroy(&g_start) == 0);

    /* --- (a) worker failure aggregation, diagnosable ------------------ */
    failures = 0;
    for (t = 0; t < NTHREADS; t++) {
        if (g_w[t].fail_line) {
            char m[128];
            int n = snprintf(m, sizeof m,
                "conc_002 FAIL thread=%d line=%d errno=%d detail=%ld rounds=%ld\n",
                t, g_w[t].fail_line, g_w[t].fail_errno,
                g_w[t].fail_detail, g_w[t].rounds_done);
            write(2, m, (size_t)n);
            failures++;
        }
    }
    assert(failures == 0);

    /* --- (b) shared file: size + sentinel ----------------------------- */
    assert(fstat(g_shared_fd, &sa) == 0);
    assert(sa.st_size == (off_t)(1 + NTHREADS * ROUNDS) * RECORD);
    assert(pread(g_shared_fd, buf, RECORD, 0) == RECORD);
    make_record(rec, SENTINEL_T, SENTINEL_I);
    assert(memcmp(buf, rec, RECORD) == 0);

    /* --- (c) per-thread private file: size, content, cross-check ------ */
    for (t = 0; t < NTHREADS; t++) {
        char priv[64], tmp[64], ren[64];
        int fd;

        snprintf(priv, sizeof priv, DIRNAME "/t%d.dat", t);
        fd = open(priv, O_RDONLY);
        assert(fd >= 0);
        assert(fstat(fd, &sb) == 0);
        assert(sb.st_size == (off_t)ROUNDS * RECORD);
        for (i = 0; i < ROUNDS; i++) {
            unsigned char p[RECORD], s[RECORD];
            make_record(rec, t, i);
            assert(pread(fd, p, RECORD, (off_t)i * RECORD) == RECORD);
            assert(memcmp(p, rec, RECORD) == 0);
            assert(pread(g_shared_fd, s, RECORD,
                         (off_t)(1 + t * ROUNDS + i) * RECORD) == RECORD);
            assert(memcmp(s, p, RECORD) == 0); /* the two paths agree */
        }
        assert(close(fd) == 0);
        assert(unlink(priv) == 0);

        /* rename/mkdir cycles must have left nothing behind */
        snprintf(tmp, sizeof tmp, DIRNAME "/t%d.tmp", t);
        snprintf(ren, sizeof ren, DIRNAME "/t%d.ren", t);
        assert(access(tmp, F_OK) == -1);
        assert(access(ren, F_OK) == -1);
    }

    /* --- (d) stat/fstat/lstat field agreement on the shared inode ----- */
    assert(fstat(g_shared_fd, &sa) == 0);
    assert(stat (SHARED, &sb) == 0);
    assert(lstat(SHARED, &sc) == 0);
    assert(sa.st_ino   == sb.st_ino   && sb.st_ino   == sc.st_ino);
    assert(sa.st_dev   == sb.st_dev   && sb.st_dev   == sc.st_dev);
    assert(sa.st_size  == sb.st_size  && sb.st_size  == sc.st_size);
    assert(sa.st_mode  == sb.st_mode  && sb.st_mode  == sc.st_mode);
    assert(sa.st_nlink == sb.st_nlink && sb.st_nlink == sc.st_nlink);
    assert(sa.st_uid   == sb.st_uid   && sa.st_gid   == sb.st_gid);
    assert(S_ISREG(sa.st_mode));
    /* Deliberately NOT compared: st_atime (relatime/noatime is host
     * policy), st_blocks/st_blksize (allocation policy differs, e.g.
     * tmpfs vs ext4, sparse files), st_mtim/st_ctim (wall-clock, equal
     * in practice here since nothing writes between the two stats, but
     * left out on principle).  None of these is ever printed or compared
     * across the native/lind boundary. */

    /* --- (e) fd-closure verification: every fd opened must be closed -- */
#if DO_FD_LEAK_SCAN
    snapshot_fds(after);
    for (i = 0; i < FD_SCAN; i++) {
        if (before[i] != after[i]) {
            char m[96];
            int n = snprintf(m, sizeof m,
                "conc_002 FAIL fd-leak fd=%d before=%d after=%d\n",
                i, before[i], after[i]);
            write(2, m, (size_t)n);
            failures++;
        }
    }
    assert(failures == 0);
#endif

    /* --- (f) cleanup, which is also an oracle -------------------------- */
    assert(close(g_shared_fd) == 0);
    assert(unlink(SHARED) == 0);
    for (f = 0; f < NFORKS; f++)
        assert(access(g_child_path[f], F_OK) == -1); /* children cleaned up */
    assert(rmdir(DIRNAME) == 0); /* ENOTEMPTY here == something leaked */

    write(1, "CONC-002 PASS\n", 14);
    return 0;
}
