#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/wait.h>
#include <assert.h>

#define NUM_CHILDREN 50
#define ALLOCS_PER_CHILD 500
#define LARGE_ALLOC_SIZE (4 * 1024 * 1024)

static void child_work(int id) {
    void *ptrs[ALLOCS_PER_CHILD];

    /* Phase 1: grow heap aggressively */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        size_t sz = 4096 + (i * 256);
        ptrs[i] = malloc(sz);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], (char)(id + i), sz);
    }

    /* Phase 2: fragment by freeing every 3rd */
    for (int i = 0; i < ALLOCS_PER_CHILD; i += 3) {
        free(ptrs[i]);
        ptrs[i] = NULL;
    }

    /* Phase 3: reallocate with different sizes into gaps */
    for (int i = 0; i < ALLOCS_PER_CHILD; i += 3) {
        size_t sz = 8192 + (i * 128);
        ptrs[i] = malloc(sz);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], 0xCC, sz);
    }

    /* Phase 4: large allocations to force brk to extend far */
    for (int round = 0; round < 5; round++) {
        void *big = malloc(LARGE_ALLOC_SIZE);
        assert(big != NULL);
        memset(big, 0xDD, LARGE_ALLOC_SIZE);
        free(big);
    }

    /* Phase 5: free everything then regrow */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        free(ptrs[i]);
    }

    /* Phase 6: regrow to stress brk after shrink */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        size_t sz = 4096 + (i * 512);
        ptrs[i] = malloc(sz);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], 0xEE, sz);
    }

    /* Phase 7: interleaved large and small */
    for (int i = 0; i < 50; i++) {
        void *small = malloc(64);
        void *big = malloc(1024 * 1024);
        assert(small != NULL);
        assert(big != NULL);
        memset(small, 0xAA, 64);
        memset(big, 0xBB, 1024 * 1024);
        free(small);
        free(big);
    }

    /* Clean up */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        free(ptrs[i]);
    }

    void *brk = sbrk(0);
    assert(brk != (void *)-1);
}

int main(void) {
    pid_t pids[NUM_CHILDREN];

    for (int i = 0; i < NUM_CHILDREN; i++) {
        pids[i] = fork();
        assert(pids[i] >= 0);

        if (pids[i] == 0) {
            child_work(i);
            exit(0);
        }
    }

    int failed = 0;
    for (int i = 0; i < NUM_CHILDREN; i++) {
        int status;
        waitpid(pids[i], &status, 0);
        if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            fprintf(stderr, "child %d failed (status=%d)\n", i, status);
            failed = 1;
        }
    }

    assert(!failed);
    printf("brk_fork_stress: all tests passed\n");
    return 0;
}
