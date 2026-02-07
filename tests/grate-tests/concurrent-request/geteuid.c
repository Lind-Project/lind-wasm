#include <stdio.h>
#include <unistd.h>

int main(int argc, char *argv[]) {
    int ret;
  for (int i = 0; i < 1000000; i++) {
    ret = geteuid();
    if (ret == -1) {
      printf("[Cage | geteuid] geteuid failed with ret=%d\n", ret);
    }
  }
    printf("[Cage | geteuid] geteuid ret = %d\n", ret);
    return 0;
}
