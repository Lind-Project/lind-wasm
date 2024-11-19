extern __thread int tlsvar __attribute__((tls_model("local-exec")));

void *
in_dso (void)
{
  return &tlsvar;
}
