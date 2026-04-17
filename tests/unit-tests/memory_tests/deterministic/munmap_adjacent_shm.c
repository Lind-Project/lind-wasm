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

    // Step 3: Check if target address is available before using MAP_FIXED
    // On native Linux, shm-2*PAGE might already be occupied by stack/heap/libs
    char *anon_target = shm - 2 * PAGE_SIZE;
    printf("[4] Probing target address %p (shm - 2*PAGE)\n", (void *)anon_target);

    // First try without MAP_FIXED to see if we can get near the target
    char *probe = (char *)mmap(anon_target, 2 * PAGE_SIZE, PROT_NONE,
                               MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (probe == MAP_FAILED) {
        perror("probe mmap failed");
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    printf("    probe returned %p\n", (void *)probe);

    if (probe != anon_target) {
        // Target address was not available - skip test on native Linux
        printf("SKIP: target address %p not available (got %p instead)\n",
               (void *)anon_target, (void *)probe);
        printf("      This is expected on native Linux where address space is occupied\n");
        munmap(probe, 2 * PAGE_SIZE);
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 77;  // Skip exit code
    }
    // Probe succeeded at target - unmap and re-map with proper permissions
    munmap(probe, 2 * PAGE_SIZE);

    printf("[5] Mapping anon at %p with MAP_FIXED\n", (void *)anon_target);
    char *anon = (char *)mmap(anon_target, 2 * PAGE_SIZE, PROT_READ | PROT_WRITE,
                              MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);
    if (anon == MAP_FAILED) {
        perror("mmap MAP_FIXED failed");
        printf("    errno = %d\n", errno);
        shmdt(shm);
        shmctl(shmid, IPC_RMID, NULL);
        return 1;
    }
    printf("    anon mapped at %p\n", (void *)anon);

    // Fill anon with different marker
    printf("[6] Filling anon with 0xCD pattern\n");
    memset(anon, 0xCD, 2 * PAGE_SIZE);

    // Print layout summary
    printf("\n    Memory layout:\n");
    printf("    [%p - %p): anon (2 pages, 0xCD)\n",
           (void *)anon, (void *)(anon + 2 * PAGE_SIZE));
    printf("    [%p - %p): shm  (1 page,  0xAB)\n",
           (void *)shm, (void *)(shm + PAGE_SIZE));

    // Step 7: Trigger the bug - munmap with unaligned length
    size_t unaligned_len = 2 * PAGE_SIZE + 1;
    size_t rounded_len = ((unaligned_len + PAGE_SIZE - 1) / PAGE_SIZE) * PAGE_SIZE;
    printf("\n[7] Triggering munmap(anon=%p, len=%zu)\n", (void *)anon, unaligned_len);
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

    // Step 8: Verify shm is still intact
    printf("\n[8] Verifying shm contents are intact...\n");
    int failures = 0;
    for (int i = 0; i < PAGE_SIZE; i++) {
        if ((unsigned char)shm[i] != 0xAB) {
            if (failures < 10) {
                printf("    CORRUPTION: shm[%d] = 0x%02x, expected 0xAB\n",
                       i, (unsigned char)shm[i]);
            }
            failures++;
        }
    }

    // Cleanup
    printf("\n[9] Cleanup: detaching and removing shm\n");
    shmdt(shm);
    shmctl(shmid, IPC_RMID, NULL);

    if (failures > 0) {
        printf("\nFAIL: %d bytes corrupted in shm page\n", failures);
        printf("      munmap with unaligned length clobbered adjacent shm\n");
        return 1;
    }

    printf("\nPASS: shm page intact after unaligned munmap of adjacent anon\n");
    return 0;
}
