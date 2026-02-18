/* fenv_shim.c (issue #245): dummy fenv for wasm32/lind.  */
#include <fenv.h>

int fetestexcept(int e) { (void)e; return 0; }
int feraiseexcept(int e) { (void)e; return 0; }
int fegetenv(fenv_t *envp) { (void)envp; return 0; }
int fesetenv(const fenv_t *envp) { (void)envp; return 0; }
int fesetround(int r) { (void)r; return 0; }
