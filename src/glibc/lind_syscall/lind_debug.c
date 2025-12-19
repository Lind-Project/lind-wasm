#include "addr_translation.h"

void __lind_debug_panic(uint64_t msg) __attribute__((
    __import_module__("lind"),
    __import_name__("debug-panic")
));

// soft panic the system with a message
// depends on configuration, may halt or just log
void lind_debug_panic (const char* msg)
{
    __lind_debug_panic(TRANSLATE_GUEST_POINTER_TO_HOST(msg));
}
