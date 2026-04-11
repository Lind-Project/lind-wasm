#include_next <bits/sigstack.h>

#ifndef _ISOMAC
/* lind-wasm: On native x86 Linux this is 0 because the dynamic linker
   fills _dl_minsigstacksize from AT_MINSIGSTKSZ at startup.  In WASM
   there is no kernel auxval, so default to MINSIGSTKSZ (2048) to avoid
   the assertion in sysconf_sigstksz().  */
# define CONSTANT_MINSIGSTKSZ 2048
#endif
