#include <assert.h>
#include <pthread.h>
#include <sched.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

enum { THREADS = 3, YIELDS = 1500, CHILDREN = 3, SPINS = 5000000, REUSE_THREADS = 32 };

static void *yield_repeatedly(void *unused) {
    (void)unused;
    volatile unsigned long progress = 0;
    for (unsigned long i = 0; i < SPINS; ++i) {
        progress += i;
    }
    assert(progress != 0);
    for (int i = 0; i < YIELDS; ++i) {
        assert(sched_yield() == 0);
    }
    return NULL;
}

static void *exit_immediately(void *unused) {
    (void)unused;
    return NULL;
}

int main(void) {
    pthread_t threads[THREADS];
    for (int i = 0; i < THREADS; ++i) {
        assert(pthread_create(&threads[i], NULL, yield_repeatedly, NULL) == 0);
    }
    yield_repeatedly(NULL);
    for (int i = 0; i < THREADS; ++i) {
        assert(pthread_join(threads[i], NULL) == 0);
    }

    for (int i = 0; i < CHILDREN; ++i) {
        pid_t child = fork();
        assert(child >= 0);
        if (child == 0) {
            pthread_t child_thread;
            assert(pthread_create(&child_thread, NULL, yield_repeatedly, NULL) == 0);
            yield_repeatedly(NULL);
            assert(pthread_join(child_thread, NULL) == 0);

            pthread_t reused[REUSE_THREADS];
            for (int j = 0; j < REUSE_THREADS; ++j) {
                assert(pthread_create(&reused[j], NULL, exit_immediately, NULL) == 0);
            }
            for (int j = 0; j < REUSE_THREADS; ++j) {
                assert(pthread_join(reused[j], NULL) == 0);
            }
            _exit(0);
        }
        int status;
        assert(waitpid(child, &status, 0) == child);
        assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    }

    puts("context switches passed");
    return 0;
}
