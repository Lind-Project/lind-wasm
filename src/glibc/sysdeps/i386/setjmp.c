#include <setjmp.h>

int __imported_wasi_lind_setjmp (unsigned int jmp_buf)
    __attribute__ ((__import_module__ ("lind"),
		    __import_name__ ("lind-setjmp")));

int
__sigsetjmp (jmp_buf env, int savemask)
{
  return __imported_wasi_lind_setjmp ((unsigned int) env);
}
