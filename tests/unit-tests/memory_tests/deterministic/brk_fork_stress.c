#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
#include <sys/wait.h>
#include <assert.h>

#define NUM_CHILDREN 20
#define ALLOCS_PER_CHILD 200

static void child_work(int id) {
    void *ptrs[ALLOCS_PER_CHILD];

    /* Repeatedly grow heap via malloc (which calls brk) */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        size_t sz = 1024 + (i * 64);
        ptrs[i] = malloc(sz);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], (char)(id + i), sz);
    }

    /* Free every other to fragment */
    for (int i = 0; i < ALLOCS_PER_CHILD; i += 2) {
        free(ptrs[i]);
        ptrs[i] = NULL;
    }

    /* Reallocate into gaps */
    for (int i = 0; i < ALLOCS_PER_CHILD; i += 2) {
        ptrs[i] = malloc(2048);
        assert(ptrs[i] != NULL);
        memset(ptrs[i], 0xCC, 2048);
    }

    /* Large allocation to force brk growth */
    void *big = malloc(512 * 1024);
    assert(big != NULL);
    memset(big, 0xDD, 512 * 1024);
    free(big);

    /* Clean up */
    for (int i = 0; i < ALLOCS_PER_CHILD; i++) {
        free(ptrs[i]);
    }

    /* Verify sbrk still works */
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
