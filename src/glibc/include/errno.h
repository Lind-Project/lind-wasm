#ifndef _ERRNO_H
#include <stdlib/errno.h>
#if !defined _ISOMAC && !defined __ASSEMBLER__

# if IS_IN (rtld)
#  include <dl-sysdep.h>
#  ifndef RTLD_PRIVATE_ERRNO
#   error "dl-sysdep.h must define RTLD_PRIVATE_ERRNO!"
#  endif
# else
#  define RTLD_PRIVATE_ERRNO	0
# endif

# if RTLD_PRIVATE_ERRNO
/* The dynamic linker uses its own private errno variable.
   All access to errno inside the dynamic linker is serialized,
   so a single (hidden) global variable is all it needs.  */

#  undef  errno
#  define errno rtld_errno
extern int rtld_errno attribute_hidden;

# elif IS_IN_LIB && !IS_IN (rtld)

#  if IS_IN (libc)
#   undef  errno
#   define errno __libc_errno
extern __thread int errno attribute_tls_model_ie;
#  endif
   /* For libraries other than libc (e.g. libm), keep the
      errno (*__errno_location ()) definition already provided by
      stdlib/errno.h above: this target links each such library as a
      separate WASM shared object, and wasm-ld's dynamic-linking mode
      cannot resolve a direct TLS reference to libc's errno across
      module boundaries the way an ELF dynamic linker can, so a raw
      __thread extern here would silently bind to a private copy.  */

# endif	/* IS_IN_LIB */

# define __set_errno(val) (errno = (val))

extern int *__errno_location (void) __THROW __attribute_const__
#  if RTLD_PRIVATE_ERRNO
     attribute_hidden
#  endif
;
// libc_hidden_proto (__errno_location)

#endif /* !_ISOMAC && !__ASSEMBLER__ */
#endif /* !_ERRNO_H */
