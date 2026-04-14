#include <lind_debug.h>

int main(int argc, char *argv[], char *envp[]) {
#ifdef LIND_DEBUG
    lind_debug_printf("LIND DEBUG TEST: value=%d\n", 42);
#endif
    return 0;
}