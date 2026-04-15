#include "addr_translation.h"
#include "lind_debug.h"

#ifdef LIND_DEBUG
#include <stdarg.h>
#include <stdio.h>
#endif

void __lind_debug_panic(uint64_t msg) __attribute__((
    __import_module__("lind"),
    __import_name__("debug-panic")
));

// soft panic the system with a message
// depends on configuration, may halt or just log
void lind_debug_panic(const char* msg)
{
    __lind_debug_panic(TRANSLATE_GUEST_POINTER_TO_HOST(msg));
}

#ifdef LIND_DEBUG

// Imported debug function to log a string directly to host-visible debug output.
extern const char* __lind_debug_str(const char *str) __attribute__((
    __import_module__("debug"),
    __import_name__("lind_debug_str")
));

void lind_debug_printf(const char *fmt, ...)
{
    char buf[1024];
    va_list ap;

    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);

    __lind_debug_str(buf);
}

#endif // LIND_DEBUG
