#include "tst-tls10.h"

__thread int mod15b_var __attribute__((tls_model("local-exec")));

int
in_dso (void)
{
  return mod15b_var;
}
