#define _GNU_SOURCE
#include <assert.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

/*
 * Printf deadlock smoke test.
 *
 * Two threads synchronize at a barrier then fprintf to the same unbuffered
 * FILE (/dev/null), exercising the stdio FILE lock contention path.
 * An alarm fires after 2 seconds to catch deadlocks.
 */

typedef struct {
  pthread_mutex_t mu;
  pthread_cond_t  cv;
  int             count;
  int             phase;
} barrier_t;

static void barrier_init(barrier_t* b) {
  memset(b, 0, sizeof(*b));
  int rc = pthread_mutex_init(&b->mu, NULL); assert(rc == 0);
  rc = pthread_cond_init(&b->cv, NULL); assert(rc == 0);
}

static void barrier_wait_2(barrier_t* b) {
  int rc = pthread_mutex_lock(&b->mu); assert(rc == 0);
  int my_phase = b->phase;

  b->count++;
  if (b->count == 2) {
    b->count = 0;
    b->phase++;
    rc = pthread_cond_broadcast(&b->cv); assert(rc == 0);
    rc = pthread_mutex_unlock(&b->mu); assert(rc == 0);
    return;
  }

  while (my_phase == b->phase) {
    rc = pthread_cond_wait(&b->cv, &b->mu);
    assert(rc == 0);
  }
  rc = pthread_mutex_unlock(&b->mu); assert(rc == 0);
}

static void on_alarm(int sig) {
  (void)sig;
  const char msg[] = "FAIL: likely deadlock (alarm)\n";
  write(2, msg, sizeof(msg) - 1);
  _exit(124);
}

typedef struct {
  int tid;
  barrier_t* bar;
  FILE* f;
} args_t;

static void* worker(void* p) {
  args_t* a = (args_t*)p;

  barrier_wait_2(a->bar);

  int rc = fprintf(a->f, "tid=%d hello\n", a->tid);
  assert(rc > 0);

  rc = fflush(a->f);
  assert(rc == 0);

  return NULL;
}

int main(void) {
  signal(SIGALRM, on_alarm);
  alarm(2);

  FILE* f = fopen("/dev/null", "w");
  assert(f);

  int rc = setvbuf(f, NULL, _IONBF, 0);
  assert(rc == 0);

  barrier_t bar;
  barrier_init(&bar);

  pthread_t t0, t1;
  args_t a0 = {.tid = 0, .bar = &bar, .f = f};
  args_t a1 = {.tid = 1, .bar = &bar, .f = f};

  rc = pthread_create(&t0, NULL, worker, &a0); assert(rc == 0);
  rc = pthread_create(&t1, NULL, worker, &a1); assert(rc == 0);

  rc = pthread_join(t0, NULL); assert(rc == 0);
  rc = pthread_join(t1, NULL); assert(rc == 0);

  rc = fclose(f); assert(rc == 0);

  alarm(0);
  write(1, "done\n", 5);
  return 0;
}
