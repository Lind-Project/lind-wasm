#include <errno.h>
#include <lind_syscall.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

// Dispatcher function
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, uint64_t arg1,
                    uint64_t arg1cage, uint64_t arg2, uint64_t arg2cage,
                    uint64_t arg3, uint64_t arg3cage, uint64_t arg4,
                    uint64_t arg4cage, uint64_t arg5, uint64_t arg5cage,
                    uint64_t arg6, uint64_t arg6cage) {
  if (fn_ptr_uint == 0) {
    fprintf(stderr, "[Grate|geteuid] Invalid function ptr\n");
    return -1;
  }

  int (*fn)(uint64_t) = (int (*)(uint64_t))(uintptr_t)fn_ptr_uint;
  return fn(cageid);
}

int geteuid_grate(uint64_t);

int geteuid_grate(uint64_t cageid) {
  return 10;
}

static void run_one_cage(int start_fd, int idx) {
  // 1) Wait for grate to give the start signal to ensure both cages are forked
  char token;
  ssize_t n = read(start_fd, &token, 1);
  if (n < 0) {
    perror("[Cage] read start token failed");
    _exit(1);
  }

  int cageid = getpid();
  int grateid = getppid();  

  uint64_t fn_ptr_addr = (uint64_t)(uintptr_t)&geteuid_grate;

  printf("[Cage %d] pid=%d registering handler to grate=%d fn_ptr=%llu\n",
         idx, cageid, grateid, (unsigned long long)fn_ptr_addr);

  int r = register_handler(cageid, 107, 1, grateid, fn_ptr_addr);
  if (r < 0) {
    fprintf(stderr, "[Cage %d] register_handler failed with %d\n", idx, r);
    _exit(1);
  }

  printf("[Cage %d] pid=%d calling geteuid()\n", idx, cageid);
  int ret;
  for (int i = 0; i < 10000; i++) 
    ret = geteuid();

  printf("[Cage %d] pid=%d geteuid ret=%d\n", idx, cageid, ret);

  _exit(0);
}

int main(int argc, char *argv[]) {
  setvbuf(stdout, NULL, _IONBF, 0);

  int grateid = getpid();
  printf("[Grate] pid=%d starting, will fork 2 cages\n", grateid);

  int p[2];
  if (pipe(p) != 0) {
    perror("pipe");
    return 1;
  }
  // p[0] read end, p[1] write end

  pid_t c1 = fork();
  if (c1 < 0) { perror("fork c1"); exit(1); }
  if (c1 == 0) {
    close(p[1]);         
    run_one_cage(p[0], 1);
  }

  pid_t c2 = fork();
  if (c2 < 0) { perror("fork c2"); exit(1); }
  if (c2 == 0) {
    close(p[1]);
    run_one_cage(p[0], 2);
  }

  // parent
  close(p[0]);            

  // 2) Signal cages to start after both have been forked
  if (write(p[1], "AA", 2) != 2) {
    perror("[Grate] write tokens");
  }
  close(p[1]);

  int status;
  pid_t w;
  while ((w = wait(&status)) > 0) {
    printf("[Grate] child pid=%d terminated, status=%d\n", (int)w, status);
  }
  return 0;
}
