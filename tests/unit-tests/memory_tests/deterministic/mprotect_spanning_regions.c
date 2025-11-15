// Test: mprotect spanning multiple separate memory regions
// Verifies correct handling when protection change spans multiple mappings
#include <sys/mman.h>
#include <stdio.h>
#include <string.h>
#include <stdlib.h>
#include <unistd.h>

#define PAGESIZE 4096
#define REGION_SIZE (10 * PAGESIZE)

int main(void) {
    // Allocate three separate memory regions
    unsigned char *p1 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p1 == MAP_FAILED) {
        perror("first mmap failed");
        return 1;
    }

    unsigned char *p2 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p2 == MAP_FAILED) {
        perror("second mmap failed");
        munmap(p1, REGION_SIZE);
        return 2;
    }

    unsigned char *p3 = mmap(NULL, REGION_SIZE, PROT_READ | PROT_WRITE,
                             MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p3 == MAP_FAILED) {
        perror("third mmap failed");
        munmap(p1, REGION_SIZE);
        munmap(p2, REGION_SIZE);
        return 3;
    }

    // Write test data to all three regions
    for (int i = 0; i < 10; i++) {
        p1[i * PAGESIZE] = 0x31 + i;
        p2[i * PAGESIZE] = 0x41 + i;
        p3[i * PAGESIZE] = 0x51 + i;
    }

    // Change protection on parts of each region
    // Last 5 pages of p1 to READ-only
    if (mprotect(p1 + (5 * PAGESIZE), 5 * PAGESIZE, PROT_READ) != 0) {
        perror("mprotect p1 failed");
        goto cleanup;
    }

    // All of p2 to READ-only
    if (mprotect(p2, REGION_SIZE, PROT_READ) != 0) {
        perror("mprotect p2 failed");
        goto cleanup;
    }

    // First 5 pages of p3 to READ-only
    if (mprotect(p3, 5 * PAGESIZE, PROT_READ) != 0) {
        perror("mprotect p3 failed");
        goto cleanup;
    }

    // Verify we can write to unprotected parts of p1 (first 5 pages)
    for (int i = 0; i < 5; i++) {
        p1[i * PAGESIZE] = 0x61 + i;
    }

    // Verify we can read from protected parts of p1 (last 5 pages)
    if (p1[5 * PAGESIZE] != 0x36 || p1[9 * PAGESIZE] != 0x3A) {
        fprintf(stderr, "read from p1 protected region failed\n");
        goto cleanup;
    }

    // Verify we can read from p2 (all READ-only)
    if (p2[0] != 0x41 || p2[9 * PAGESIZE] != 0x4A) {
        fprintf(stderr, "read from p2 failed\n");
        goto cleanup;
    }

    // Verify we can write to unprotected parts of p3 (last 5 pages)
    for (int i = 5; i < 10; i++) {
        p3[i * PAGESIZE] = 0x71 + i;
    }

    // Verify we can read from protected parts of p3 (first 5 pages)
    if (p3[0] != 0x51 || p3[4 * PAGESIZE] != 0x55) {
        fprintf(stderr, "read from p3 protected region failed\n");
        goto cleanup;
    }

    // Cleanup
    munmap(p1, REGION_SIZE);
    munmap(p2, REGION_SIZE);
    munmap(p3, REGION_SIZE);

    printf("mprotect_spanning_regions test: PASS\n");
    return 0;

cleanup:
    munmap(p1, REGION_SIZE);
    munmap(p2, REGION_SIZE);
    munmap(p3, REGION_SIZE);
    return 4;
}

