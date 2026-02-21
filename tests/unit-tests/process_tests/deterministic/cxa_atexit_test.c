/* Test that __cxa_atexit handlers run correctly on exit.
 *
 * glibc's __cxa_atexit registers void(*)(void*) functions, but internally
 * stored them as void(*)(void*, int) and called with an extra status arg.
 * In native C the extra arg is harmlessly ignored. In WASM, the indirect
 * call type check catches the signature mismatch and traps.
 *
 * This test registers a handler via __cxa_atexit and exits normally.
 * Without the fix: WASM trap "indirect call type mismatch" in __run_exit_handlers.
 * With the fix: handler runs, prints output, process exits cleanly.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

extern int __cxa_atexit(void (*func)(void *), void *arg, void *d);

static int handler_ran = 0;

static void cleanup(void *arg) {
    handler_ran = 1;
    const char *msg = (const char *)arg;
    printf("cxa_atexit handler called: %s\n", msg);
}

int main(void) {
    __cxa_atexit(cleanup, "test_arg", NULL);

    printf("main returning, handler should run during exit\n");
    return 0;
}
