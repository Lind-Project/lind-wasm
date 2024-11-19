#define memchr __memchr_noneon
#undef libc_hidden_builtin_def
#define libc_hidden_builtin_def(name)

#include <sysdeps/arm/armv6t2/memchr.S>

#if IS_IN (libc) && !defined (__ARM_NEON__)
strong_alias (__memchr_noneon, __GI_memchr)
#endif
