#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/mman.h>

int main() {
  size_t page_size = 4096;

  printf("=== Bug 1: Silent Wrap-Around ===\n");

  uintptr_t addr = 0xfffff000;
  size_t len = page_size * 2; // addr + len overflows 32-bit

  printf("Requesting mmap at 0x%lx with size 0x%zx\n", (unsigned long)addr, len);
  printf("Expected end addr: 0x%lx (overflows 32-bit!)\n", (unsigned long)(addr + len));

  void *res = mmap((void *)addr, len, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1, 0);

  if (res == MAP_FAILED) {
    printf("PASS: mmap correctly rejected — %s\n", strerror(errno));
  } else {
    printf("FAIL: mmap succeeded — overflow not caught!\n");
    char *base = (char *)res;
    for (size_t i = 0; i < 2; i++) {
      uintptr_t page_addr = (uintptr_t)(base + i * page_size);
      printf("  Page %zu landed at: 0x%lx %s\n", i, (unsigned long)page_addr,
             page_addr < page_size ? "<-- WRAPPED to low memory!" : "");
    }
    munmap(res, len);
  }

  return 0;
}