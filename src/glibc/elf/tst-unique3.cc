#include <cstdio>
#include <dlfcn.h>
#include <libc-diag.h>
#include "tst-unique3.h"

/* clang warns that the instatiation of the variable is required, but no
   definition is available.  They are implemented on tst-unique3lib.so.  */
DIAG_PUSH_NEEDS_COMMENT_CLANG;
DIAG_IGNORE_NEEDS_COMMENT_CLANG (13, "-Wundefined-var-template");
int t = S<char>::i;
DIAG_POP_NEEDS_COMMENT_CLANG;

int
main (void)
{
  std::printf ("%d %d\n", S<char>::i, t);
  int result = S<char>::i++ != 1 || t != 1;
  result |= in_lib ();
  void *d = dlopen ("$ORIGIN/tst-unique3lib2.so", RTLD_LAZY);
  int (*fp) ();
  if (d == NULL || (fp = (int(*)()) dlsym (d, "in_lib2")) == NULL)
    {
      std::printf ("failed to get symbol in_lib2\n");
      return 1;
    }
  result |= fp ();
  dlclose (d);
  return result;
}
