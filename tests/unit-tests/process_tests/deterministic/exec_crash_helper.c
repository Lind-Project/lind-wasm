/* Helper program for exec_crash_finalize test: immediately traps. */
#include <stdlib.h>

int main(void)
{
    abort();
    return 0; /* unreachable */
}
