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

// These functions returns the input value to ensure the operand
// remains on the WASM stack for potential debugging

// Imported debug function to log or trace unsigned integer
__attribute__((used))
extern unsigned int __lind_debug_num(unsigned int num) __attribute__((
    __import_module__("debug"),
    __import_name__("lind_debug_num")
));

// Imported debug function to log or trace string
__attribute__((used))
extern const char* __lind_debug_str(const char *str) __attribute__((
    __import_module__("debug"),
    __import_name__("lind_debug_str")
));

// Force calls to import debug functions. Execution not required, their
// presence here prevents the linker from stripping the imports
__attribute__((used))
void __lind_debug_import(void) {
    __lind_debug_num(0);
    __lind_debug_str("LIND DEGUG INIT");
}
