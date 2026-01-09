#ifdef _LIND_DEBUG_H

// lind soft panic
void lind_debug_panic (const char* msg);

// lind_debug raw WASM imports
unsigned int lind_debug_num(unsigned int num);
const char* lind_debug_str(const char *str);

// lind_debug force import
void __lind_debug_import(void);

#endif // _LIND_DEBUG_H
