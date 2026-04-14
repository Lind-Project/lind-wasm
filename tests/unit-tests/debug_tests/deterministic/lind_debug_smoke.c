#include <stdarg.h>
#include <stddef.h>

void lind_debug_printf(const char *fmt, ...);

int main(void) {
    lind_debug_printf("LIND DEBUG SMOKE: hello %d\n", 42);
    return 0;
}
