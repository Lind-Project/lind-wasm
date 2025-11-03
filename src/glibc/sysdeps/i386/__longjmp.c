#include <setjmp.h>

int __imported_wasi_lind_longjmp (unsigned int jmp_buf, unsigned int retval)
    __attribute__ ((__import_module__ ("lind"),
		    __import_name__ ("lind-longjmp")));

void
__longjmp (__jmp_buf env, int val)
{
  __imported_wasi_lind_longjmp ((unsigned int) env, (unsigned int) val);
}
