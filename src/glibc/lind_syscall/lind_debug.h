#ifndef _LIND_DEBUG_H
#define _LIND_DEBUG_H

// lind soft panic
void lind_debug_panic (const char* msg);

#ifdef LIND_DEBUG
// lind_debug raw WASM imports
unsigned int __lind_debug_num(unsigned int num);
const char* __lind_debug_str(const char *str);

// lind_debug force import
void __lind_debug_import(void);

// formatted debug print helper
void lind_debug_printf(const char *fmt, ...);
#endif // LIND_DEBUG

#endif // _LIND_DEBUG_H
