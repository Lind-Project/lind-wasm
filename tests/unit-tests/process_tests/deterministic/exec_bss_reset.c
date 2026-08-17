#include <stdio.h>
#include <string.h>
#include <unistd.h>

// exec must hand the new program image zeroed .bss.
//
// lind reuses the cage's existing 4 GiB linear memory across exec instead of
// mapping a second reservation, so the region still holds the previous image's
// pages when the new one is instantiated.  Nothing writes .bss at startup -- it
// is simply assumed to be zero -- so the runtime has to release those pages
// before handing the memory to the new program.
#define BUF_SIZE (4 * 1024 * 1024)

static unsigned char scratch[BUF_SIZE];

int main(int argc, char *argv[]) {
  if (argc > 1 && strcmp(argv[1], "--execd") == 0) {
    for (size_t i = 0; i < BUF_SIZE; i++) {
      if (scratch[i] != 0) {
        printf("FAIL: .bss byte %zu survived exec with value 0x%02x\n", i,
               scratch[i]);
        return 1;
      }
    }
    printf("PASS\n");
    return 0;
  }

  // dirty every page of .bss, then replace the program image
  memset(scratch, 0xa5, BUF_SIZE);

  execl(argv[0], argv[0], "--execd", NULL);

  // only reached if exec fails
  perror("exec failed");
  return 1;
}
