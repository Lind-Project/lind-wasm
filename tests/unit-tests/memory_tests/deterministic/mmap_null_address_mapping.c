#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>

int main() {
  size_t page_size = 4096;

  printf("=== Bug 2: NULL Mapping ===\n");
  printf("Requesting MAP_FIXED mmap at 0x0 (NULL)\n");

  void *res = mmap((void *)0, page_size, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);

  if (res == MAP_FAILED) {
    printf("PASS: mmap correctly rejected NULL mapping — %s\n", strerror(errno));
  } else {
    printf("FAIL: NULL mapping succeeded at %p — null dereferences won't fault!\n", res);
    *((volatile char *)res) = 'X';
    printf("  Write to NULL succeeded (extremely dangerous)\n");
    munmap(res, page_size);
  }

  return 0;
}