/* Test that a function with a stack frame larger than 64 KiB can run.
 *
 * Static builds get wasm-ld's default 64 KiB shadow stack, placed directly
 * above .data/.bss, and nothing checks for overflow. A frame that doesn't fit
 * silently overwrites static data (including glibc's cached __lind_cageid),
 * so the remaining stack is asserted before big_frame() runs.
 *
 * The check is wasm-only: __stack_low is synthesized by wasm-ld, and the
 * native build only needs to run big_frame() to produce matching output.
 */
#include <assert.h>
#include <stddef.h>
#include <stdint.h>

#define BIG_LOCAL (80 * 1024) /* > 64 KiB */
#define HEADROOM  (4 * 1024)  /* big_frame()'s own bookkeeping */

#ifdef __wasm__
/* Synthesized by wasm-ld. */
extern unsigned char __stack_low;
#endif

__attribute__((noinline)) static void big_frame(void) {
    volatile char buf[BIG_LOCAL];
    for (size_t i = 0; i < sizeof(buf); i++)
        buf[i] = 'A';
}

int main(void) {
#ifdef __wasm__
    volatile char here; /* on the shadow stack: marks the current depth */
    assert((uintptr_t)&here - (uintptr_t)&__stack_low > BIG_LOCAL + HEADROOM);
#endif

    big_frame();
    return 0;
}
