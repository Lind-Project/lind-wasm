// Weak TLS stubs for glibc-internal locale pointers that setlocale.o (dragged in
// by locale-aware formatting functions) references but that aren't otherwise
// defined when cherry-picking objects from libc.a into a standalone grate.
#define LC(n) __attribute__((weak)) __thread void *_nl_current_LC_##n;
LC(CTYPE) LC(NUMERIC) LC(TIME) LC(COLLATE) LC(MONETARY) LC(MESSAGES)
LC(PAPER) LC(NAME) LC(ADDRESS) LC(TELEPHONE) LC(MEASUREMENT) LC(IDENTIFICATION)
