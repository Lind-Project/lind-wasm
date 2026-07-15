/* Minimal local stubs for argp_parse/argp_help.
 *
 * libc.so's exported-symbol list (curated by scripts/extract_glibc_symbols.sh) omits
 * argp_parse/argp_help — they're compiled into libc.a but never made part of the
 * shared libc.so's public export set. libm-test-support.c (glibc's own test-suite
 * command-line-argument handling) references both, and wasm requires every import a
 * module references to resolve before it can even instantiate — regardless of
 * whether the function is actually reached at runtime.
 *
 * Our test invocations always pass zero extra CLI arguments (argc=1, just the
 * program name; see run_libm_tests.sh), so the real argp_parse's whole job — parsing
 * options — is a no-op in our case, and argp_help is only reached on the
 * mismatched-argument-count error path, which we never hit. These stubs are
 * link-local (not touching the shared libc.so everyone else depends on) and only
 * need to satisfy that narrow contract, not implement real option parsing.
 */

struct argp; /* opaque: never dereferenced here */

int argp_parse(const struct argp *argp, int argc, char **argv,
                unsigned flags, int *arg_index, void *input) {
  if (arg_index)
    *arg_index = argc; /* "all args consumed" -> libm-test-support.c's remaining==argc check passes */
  return 0;
}

void argp_help(const struct argp *argp, void *stream, unsigned flags, char *name) {
  /* not expected to be reached with zero CLI args */
}
