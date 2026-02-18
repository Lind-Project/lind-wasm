/* lll_elision_shim.c (issue #245): fallback elision stubs.  */
#include <stdint.h>

int __lll_lock_elision(int *futex, short *adapt_count, int trylock) {
  (void)futex;
  (void)adapt_count;
  (void)trylock;
  return 0;
}

void __lll_unlock_elision(int *futex, short *adapt_count, int trylock) {
  (void)futex;
  (void)adapt_count;
  (void)trylock;
}
