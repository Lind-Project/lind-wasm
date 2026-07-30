#ifndef _GRATEOS_DEBUG_H
#define _GRATEOS_DEBUG_H

// grateos soft panic
void grateos_debug_panic (const char* msg);

#ifdef GRATEOS_DEBUG
// grateos_debug raw WASM imports
unsigned int __grateos_debug_num(unsigned int num);
const char* __grateos_debug_str(const char *str);

// grateos_debug force import
void __grateos_debug_import(void);

// formatted debug print helper
void grateos_debug_printf(const char *fmt, ...);
#endif // GRATEOS_DEBUG

#endif // _GRATEOS_DEBUG_H
