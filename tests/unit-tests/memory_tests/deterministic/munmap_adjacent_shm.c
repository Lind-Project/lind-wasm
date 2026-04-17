// Regression test for PR #1075: munmap with unaligned length must not clobber adjacent shm pages.
//
// Strategy: shmat first (allocator picks address), then MAP_FIXED to place anon
// exactly 2 pages before shm. This creates a deterministic layout without
// relying on allocator placement assumptions.
//
// Layout: [anon 2P][shm 1P]
// munmap(anon, 2*PAGE+1) rounds to 3*PAGE, covering [anon, shm+P).
// Buggy path: single PROT_NONE over full range wipes shm. Fixed path: shm skipped.
#include <sys/ipc.h>
#include <sys/shm.h>
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <stdint.h>
#include <errno.h>

#define PAGE_SIZE 4096

int main(void) {
    printf("=== munmap_adjacent_shm test ===\n");

    // Step 1: Create and attach shared memory segment
    key_t key = 4242;
    printf("[1] Creating shm segment with key=%d, size=%d\n", key, PAGE_SIZE);
    int shmid = shmget(key, PAGE_SIZE, 0666 | IPC_CREAT);
    if (shmid == -1) {
        perror("shmget failed");
        return 1;
    }
    printf("    shmid = %d\n", shmid);

    // Step 2: Attach shm at allocator-chosen address
    printf("[2] Attaching shm (NULL hint, let allocator pick)\n");
    char *shm = (char *)shmat(shmid, NULL, 0);
    if (shm == (char *)-1) {
        perror("shmat failed");
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    printf("    shm attached at %p\n", (void *)shm);

    // Verify page alignment
    if (((uintptr_t)shm % PAGE_SIZE) != 0) {
        printf("FAIL: shm not page-aligned\n");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }

    // Fill shm with marker pattern
    printf("[3] Filling shm with 0xAB pattern\n");
    memset(shm, 0xAB, PAGE_SIZE);

    // Step 3: Use MAP_FIXED to place anon exactly 2 pages before shm
    // In lind-wasm, the region before shm should be free (allocator works top-down)
    // On native Linux, this might fail or clobber existing mappings
    char *anon_target = shm - 2 * PAGE_SIZE;
    printf("[4] Mapping anon at %p (shm - 2*PAGE) with MAP_FIXED\n", (void *)anon_target);

    char *anon = (char *)mmap(anon_target, 2 * PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (anon == MAP_FAILED) {
        printf("SKIP: MAP_FIXED at %p failed (errno=%d)\n", (void *)anon_target, errno);
        printf("      Target address not mappable - expected on some systems\n");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 77;  // Skip exit code
    }
    printf("    anon mapped at %p\n", (void *)anon);

    // Fill anon with different marker
    printf("[5] Filling anon with 0xCD pattern\n");
    memset(anon, 0xCD, 2 * PAGE_SIZE);

    // Print layout summary
    printf("\n    Memory layout:\n");
    printf("    [%p - %p): anon (2 pages, 0xCD)\n",
           (void *)anon, (void *)(anon + 2 * PAGE_SIZE));
    printf("    [%p - %p): shm  (1 page,  0xAB)\n",
           (void *)shm, (void *)(shm + PAGE_SIZE));

    // Step 6: Trigger the bug - munmap with unaligned length
    size_t unaligned_len = 2 * PAGE_SIZE + 1;
    size_t rounded_len = ((unaligned_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE;
    printf("\n[6] Triggering munmap(anon=%p, len=%zu)\n", (void *)anon, unaligned_len);
    printf("    len rounds up to %zu bytes (%zu pages)\n", rounded_len, rounded_len / PAGE_SIZE);
    printf("    Range covered: [%p - %p)\n",
           (void *)anon, (void *)(anon + rounded_len));
    printf("    WARNING: This range includes shm at %p!\n", (void *)shm);

    int rc = munmap(anon, unaligned_len);
    if (rc != 0) {
        perror("munmap failed");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    printf("    munmap returned %d (success)\n", rc);

    // Step 7: Verify shm SEGMENT still exists (not just local mapping)
    // munmap may have detached local mapping, but segment should persist
    printf("\n[7] Verifying shm segment still exists...\n");
    struct shmid_ds shm_stat;
    if (shmctl(shmid, IPC_STAT, &shm_stat) == -1) {
        perror("shmctl IPC_STAT failed");
        printf("FAIL: shm segment was destroyed by munmap\n");
        return 1;
    }
    printf("    shm segment exists (nattch=%lu)\n", (unsigned long)shm_stat.shm_nattch);

    // Step 8: Re-attach and verify data integrity
    printf("\n[8] Re-attaching to shm to verify data...\n");
    char *shm2 = (char *)shmat(shmid, NULL, 0);
    if (shm2 == (char *)-1) {
        perror("shmat re-attach failed");
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    printf("    re-attached at %p\n", (void *)shm2);

    int failures = 0;
    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm2[i] != 0xAB) {
            if (failures < 10) {
                printf("    CORRUPTION: shm[%d] = 0x%02x, expected 0xAB\n",
                       i, (unsigned char)shm2[i]);
            }
            failures++;
        }
    }

    // Cleanup
    printf("\n[9] Cleanup: detaching and removing shm\n");
    shmdt(shm2);
    shmctl(shmid, IPC_RMID, NULL);

    if (failures > 0) {
        printf("\nFAIL: %d bytes corrupted in shm segment\n", failures);
        printf("      munmap with unaligned length clobbered shm data\n");
        return 1;
    }

    printf("\nPASS: shm segment and data intact after unaligned munmap\n");
    return 0;
}
