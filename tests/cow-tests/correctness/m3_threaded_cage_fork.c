/*
 * Milestone 3.1 (cow-implementation-plan.md): same-cage multi-thread
 * safety for a post-fork COW split (cow-design.md §17: "SharedMemory's
 * RwLock only guards grow()... a materialize_unique remap on one thread
 * of cage B can race with a sibling thread of the SAME cage B mid-load/
 * store on the range being swapped").
 *
 * Only the calling thread survives into a fork()'d child (POSIX
 * semantics), so the hazard has to be constructed on whichever SIDE
 * diverges the shared range while ANOTHER thread of that same cage is
 * concurrently touching it. Here the PARENT is the multithreaded side:
 * after fork, a background thread continuously re-verifies a range the
 * main thread will never touch, while the main thread repeatedly
 * rewrites a DISJOINT range (triggering, once COW lands, a same-cage
 * remap of the parent's own mapping). The two ranges are disjoint on
 * purpose -- this is deliberately NOT an unsynchronized data race (which
 * would have undefined per-byte content by ordinary C semantics and
 * would be a meaningless test); it isolates whether the remap machinery
 * can transiently corrupt or unmap memory OUTSIDE the range actually
 * being split, which a same-cage sibling thread could observe mid-remap.
 */
#include <pthread.h>
#include <stdlib.h>
#include <sys/wait.h>
#include <unistd.h>
#include "../common/cow_test.h"

#define SIZE       (8UL * 1024 * 1024)
#define HALF       (SIZE / 2)
#define WRITE_ITERS 200

#define SEED_ORIG 0x77660000u

static unsigned char *g_buf;
static volatile int g_stop;
static volatile int g_reader_saw_corruption;

static void *reader_thread(void *arg) {
    (void)arg;
    while (!g_stop) {
        if (!cow_verify_range(g_buf, HALF, HALF, SEED_ORIG, "reader-thread")) {
            g_reader_saw_corruption = 1;
            return NULL;
        }
    }
    return NULL;
}

int main(void) {
    g_buf = malloc(SIZE);
    COW_CHECK(g_buf != NULL, "malloc failed");
    cow_fill(g_buf, SIZE, SEED_ORIG);

    pid_t pid = fork();
    COW_CHECK(pid >= 0, "fork failed");

    if (pid == 0) {
        /* child just confirms the inherited range is intact and exits;
         * the interesting hazard is on the parent side below. */
        if (!cow_verify(g_buf, SIZE, SEED_ORIG, "child")) _exit(1);
        _exit(0);
    }

    pthread_t reader;
    COW_CHECK(pthread_create(&reader, NULL, reader_thread, NULL) == 0, "pthread_create failed");

    for (int i = 0; i < WRITE_ITERS; i++) {
        cow_fill_range(g_buf, 0, HALF, SEED_ORIG + (uint32_t)i + 1);
        if (!cow_verify_range(g_buf, 0, HALF, SEED_ORIG + (uint32_t)i + 1, "writer-thread")) {
            g_stop = 1;
            pthread_join(reader, NULL);
            COW_FAIL("parent main thread's own writes were corrupted");
        }
    }
    g_stop = 1;
    COW_CHECK(pthread_join(reader, NULL) == 0, "pthread_join failed");
    COW_CHECK(!g_reader_saw_corruption, "reader thread observed corruption in its disjoint range");

    int status;
    COW_CHECK(waitpid(pid, &status, 0) == pid, "waitpid failed");
    COW_CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 0, "child failed");

    free(g_buf);
    return 0;
}
