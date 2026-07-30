#include "addr_translation.h"
#include "grateos_debug.h"

#ifdef GRATEOS_DEBUG
#include <stdarg.h>
#include <stdio.h>
#endif

void __grateos_debug_panic(uint64_t msg) __attribute__((
    __import_module__("grateos"),
    __import_name__("debug-panic")
));

// soft panic the system with a message
// depends on configuration, may halt or just log
void grateos_debug_panic (const char* msg)
{
    __grateos_debug_panic(TRANSLATE_GUEST_POINTER_TO_HOST(msg));
}

#ifdef GRATEOS_DEBUG

// These functions return the input value to ensure the operand
// remains on the WASM stack for potential debugging.

// Imported debug function to log or trace unsigned integer.
extern unsigned int __grateos_debug_num(unsigned int num) __attribute__((
    __import_module__("debug"),
    __import_name__("grateos_debug_num")
));

// Imported debug function to log or trace string.
extern const char* __grateos_debug_str(const char *str) __attribute__((
    __import_module__("debug"),
    __import_name__("grateos_debug_str")
));

// Force calls to import debug functions. Execution is not required;
// their presence here prevents the linker from stripping the imports.
void __grateos_debug_import(void)
{
    __grateos_debug_num(0);
    __grateos_debug_str("GRATEOS DEBUG INIT");
}

void grateos_debug_printf(const char *fmt, ...)
{
    char buf[1024];
    va_list ap;

    va_start(ap, fmt);
    vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);

    __grateos_debug_str(buf);
}

#endif // GRATEOS_DEBUG
