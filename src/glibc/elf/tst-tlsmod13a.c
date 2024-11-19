__thread int b[2] __attribute__ ((tls_model ("local-exec")));

extern int foo (void);

int
bar (void)
{
  return foo () + b[0];
}
