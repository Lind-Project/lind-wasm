#ifndef _LIND_DEBUG_H
#define _LIND_DEBUG_H

#include <stdarg.h>

// lind soft panic
void lind_debug_panic (const char* msg);

#ifdef LIND_DEBUG
// lind_debug wrappers around imported debug functions
unsigned int lind_debug_num(unsigned int num);
const char* lind_debug_str(const char *str);

// printf-style lind debug helpers
void lind_debug_vprintf(const char *fmt, va_list ap);
void lind_debug_printf(const char *fmt, ...);

// lind_debug force import
void __lind_debug_import(void);

#endif // LIND_DEBUG

#endif // _LIND_DEBUG_H