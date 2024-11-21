#include <setjmp.h>
#include <stdint.h>

int _setjmp(jmp_buf env) {
  return __sigsetjmp (env, 1);
}
