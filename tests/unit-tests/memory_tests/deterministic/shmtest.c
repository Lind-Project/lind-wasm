#include <sys/ipc.h>
#include <sys/shm.h>
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <assert.h>

#ifndef PAGE_SIZE
#define PAGE_SIZE 4096
#endif

int main()
{
    struct shmid_ds buf1, buf2;

    key_t key1 = 2000;
    key_t key2 = 3000;
  
    int shmid1 = shmget(key1, 2048, 0666 | IPC_CREAT);
    int shmid2 = shmget(key2, 2048, 0666 | IPC_CREAT);

    void *shm1 = (char*) shmat(shmid1, NULL, 0);
    void *shm2 = (char*) shmat(shmid2, NULL, 0);
    void *shm3 = (char*) shmat(shmid1, NULL, 0);
    void *shm4 = (char*) shmat(shmid1, NULL, 0);
    void *shm5 = (char*) shmat(shmid2, NULL, 0);

    shmctl(shmid1, IPC_STAT, &buf1);
    shmctl(shmid2, IPC_STAT, &buf2);

    assert(buf1.shm_nattch == 3);
    assert(buf2.shm_nattch == 2);

    shmctl(shmid1, IPC_RMID, (struct shmid_ds *) NULL);
    shmctl(shmid2, IPC_RMID, (struct shmid_ds *) NULL);
    shmdt(shm1);
    shmdt(shm2);
    shmdt(shm3);
    shmdt(shm4);
    shmdt(shm5);

    /*
     * Test: munmap with a non-page-aligned length adjacent to an shm page
     * must not clobber the shm page.
     *
     * Reproduces issue #1062: munmap rounded len up to 2 pages and called
     * mmap(MAP_FIXED|PROT_NONE) blindly over the rounded range, wiping the
     * adjacent shm page. The fix consults vmmap and only PROT_NONEs pages
     * belonging to anonymous entries, leaving shm-backed entries untouched.
     *
     * Layout after setup:
     *   [base .. base+PAGE_SIZE)          anonymous R/W  (what we munmap)
     *   [base+PAGE_SIZE .. base+2*PAGE_SIZE)  shm          (must survive)
     */
    {
        key_t key3 = 4000;
        /* clean up any leftover segment from a previous run */
        int old3 = shmget(key3, PAGE_SIZE, 0666);
        if (old3 >= 0)
            shmctl(old3, IPC_RMID, NULL);

        int shmid3 = shmget(key3, PAGE_SIZE, IPC_CREAT | IPC_EXCL | 0666);
        assert(shmid3 >= 0);

        /* Reserve 2 contiguous pages */
        char *base = mmap(NULL, 2 * PAGE_SIZE, PROT_READ | PROT_WRITE,
                          MAP_ANONYMOUS | MAP_PRIVATE, -1, 0);
        assert(base != MAP_FAILED);

        /* Release the second page so shmat can claim it */
        assert(munmap(base + PAGE_SIZE, PAGE_SIZE) == 0);

        /*
         * Attach shm with hint = base + PAGE_SIZE.
         * find_map_space_with_hint finds free space near the hint; since
         * base+PAGE_SIZE is now free, the allocator should return it exactly.
         */
        void *shm = shmat(shmid3, base + PAGE_SIZE, 0);
        assert(shm != (void *)-1);
        assert(shm == (void *)(base + PAGE_SIZE));

        /* Write a sentinel into the shm page */
        int *shm_val = (int *)shm;
        *shm_val = 0xdeadbeef;

        /*
         * munmap the first page with a non-page-aligned length (PAGE_SIZE + 1).
         * Buggy code:  rounds up to 2 pages → PROT_NONEs P1 (shm) → fault.
         * Fixed code:  intersects with vmmap; only touches the anonymous entry
         *              [base..base+PAGE_SIZE), leaves the shm entry at P1 alone.
         */
        assert(munmap(base, PAGE_SIZE + 1) == 0);

        /* shm page must still be accessible and hold its value */
        assert(*shm_val == 0xdeadbeef);

        /* confirm the page is still writable */
        *shm_val = 0xcafebabe;
        assert(*shm_val == 0xcafebabe);

        assert(shmdt(shm) == 0);
        assert(shmctl(shmid3, IPC_RMID, NULL) == 0);
    }

    return 0;
}

