#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/types.h>
#include <unistd.h>

int main(void) {
  const char *test_msg = "hi\n";
  const size_t test_msg_len = 3;
  char read_buf[4096] = {0};
  int ret, fd[2];

  ret = pipe2(fd, 0);
  assert(ret == 0);

  ret = write(fd[1], test_msg, test_msg_len);
  assert(ret == (int)test_msg_len);

  ret = read(fd[0], read_buf, test_msg_len);
  assert(ret == (int)test_msg_len);

  assert(memcmp(read_buf, test_msg, test_msg_len) == 0);

  ret = close(fd[0]);
  assert(ret == 0);

  ret = close(fd[1]);
  assert(ret == 0);

  return 0;
}

