#ifndef _LIND_DEBUG_H
#define _LIND_DEBUG_H

// lind soft panic
void lind_debug_panic(const char* msg);

#ifdef LIND_DEBUG
void lind_debug_printf(const char *fmt, ...);
#endif // LIND_DEBUG

#endif // _LIND_DEBUG_H
