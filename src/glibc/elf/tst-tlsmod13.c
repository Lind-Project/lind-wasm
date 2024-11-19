__thread int a[2] __attribute__ ((tls_model ("local-exec")));

int
foo (void)
{
  return a[0];
}
