#include <stdio.h>
#include <tgmath.h>
#include <libc-diag.h>


int
main (void)
{
  int retval = 0;

#define TEST(expr, res) \
  if (sizeof (expr) != res)						      \
    {									      \
      printf ("sizeof(%s) == %zu, expected %zu\n", #expr,		      \
	      sizeof (expr), (size_t) (res));				      \
      retval = 1;								      \
    }

  TEST (creal (1.0), sizeof (double));
  TEST (creal (1.0 + 1.0i), sizeof (double));
  TEST (creal (1.0l), sizeof (long double));
  TEST (creal (1.0l + 1.0li), sizeof (long double));
  TEST (creal (1.0f), sizeof (float));
  TEST (creal (1.0f + 1.0fi), sizeof (float));

  TEST (cimag (1.0), sizeof (double));
  TEST (cimag (1.0 + 1.0i), sizeof (double));
  TEST (cimag (1.0l), sizeof (long double));
  TEST (cimag (1.0l + 1.0li), sizeof (long double));
  TEST (cimag (1.0f), sizeof (float));
  TEST (cimag (1.0f + 1.0fi), sizeof (float));

  /* The type generic fabs expansion issues the floating point absolute with
     the wrong argument type (for instance cabs with floating point or fabs
     with a complex type) and clang warns that implicit conversion might
     incur in unexpected behavior.  */
  DIAG_PUSH_NEEDS_COMMENT_CLANG;
  DIAG_IGNORE_NEEDS_COMMENT_CLANG (13, "-Wabsolute-value");
  TEST (fabs (1.0), sizeof (double));
  TEST (fabs (1.0 + 1.0i), sizeof (double));
  TEST (fabs (1.0l), sizeof (long double));
  TEST (fabs (1.0l + 1.0li), sizeof (long double));
  TEST (fabs (1.0f), sizeof (float));
  TEST (fabs (1.0f + 1.0fi), sizeof (float));

  TEST (carg (1.0), sizeof (double));
  TEST (carg (1.0 + 1.0i), sizeof (double));
  TEST (carg (1.0l), sizeof (long double));
  TEST (carg (1.0l + 1.0li), sizeof (long double));
  TEST (carg (1.0f), sizeof (float));
  TEST (carg (1.0f + 1.0fi), sizeof (float));

  return retval;
}
