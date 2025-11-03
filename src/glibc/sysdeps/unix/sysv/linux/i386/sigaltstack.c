#include <unistd.h>
#include <signal/signal.h>

int
__GI___sigaltstack (const stack_t *__ss, stack_t *__oss)
{
  return 0;
}
weak_alias (__GI___sigaltstack, __sigaltstack)
