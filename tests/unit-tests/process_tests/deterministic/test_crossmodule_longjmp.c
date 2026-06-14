/*
 * Cross-module longjmp/setjmp test for lind-wasm dynamic builds.
 *
 * Design rationale
 * ----------------
 * In the dynamic build (lind-clang without -s), __wasm_longjmp lives in
 * libc.so while the LLVM SjLj try_table catch blocks live in user code.
 * Both modules import __c_longjmp from the wasmtime host linker, so they
 * share the same runtime tag identity.  A throw in libc.so is caught by a
 * user-code try_table, and vice versa.
 *
 * General setjmp/longjmp edge cases (zero val, nested, deep stack, etc.)
 * are covered by setjmp_edge.c.  This file tests only behaviors that are
 * unique to the cross-module path.
 *
 * Tests
 * -----
 * 1. Basic:       user setjmp, longjmp routed through __wasm_longjmp in libc.so;
 *                 proves the __c_longjmp tag identity is shared across modules.
 * 2. Signal path: setjmp in user code; kill(getpid(), SIGUSR1) immediately
 *                 queues a signal; pause() delivers it via pure-wasm
 *                 signal_callback; handler calls longjmp (EH path); exception
 *                 propagates through signal_callback/pause() with no Rust
 *                 boundary and is caught by the setjmp try_table in user code.
 *
 * The dlopen cross-module test (longjmp from a dlopen'd library) is in
 * dylink_tests/deterministic/longjmp_dlopen.c, which is skipped in the
 * main test runner alongside other dlopen tests.
 *
 * Compilation
 * -----------
 * This test MUST be compiled as a dynamic build (no -s flag) to exercise
 * the cross-module path.  The test runner uses the default dynamic mode.
 */

#include <setjmp.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static int passed = 0;
static int failed = 0;

#define EXPECT_EQ(label, got, expected)                                        \
    do {                                                                        \
        if ((int)(got) == (int)(expected)) {                                   \
            printf("  PASS: %s\n", (label));                                   \
            passed++;                                                           \
        } else {                                                                \
            printf("  FAIL: %s — got %d, expected %d\n",                      \
                   (label), (int)(got), (int)(expected));                      \
            failed++;                                                           \
        }                                                                       \
    } while (0)

/* ------------------------------------------------------------------ */
/* Test 1: basic cross-module longjmp                                  */
/* longjmp(buf, 42) is lowered by the SjLj pass to                    */
/* __wasm_longjmp(buf, 42), which lives in libc.so.  The throw        */
/* crosses the module boundary and is caught by the try_table the     */
/* pass inserted at the setjmp call site in user code.                */
/* ------------------------------------------------------------------ */
static jmp_buf g_buf;

static void thrower_42(void) { longjmp(g_buf, 42); }

static void test_basic(void)
{
    printf("\n[1] Basic cross-module longjmp\n");
    int v = setjmp(g_buf);
    if (v == 0) {
        thrower_42();
        printf("  FAIL: should not reach after longjmp\n");
        failed++;
        return;
    }
    EXPECT_EQ("longjmp delivers 42", v, 42);
}

/* ------------------------------------------------------------------ */
/* Test 2: signal path — longjmp from signal handler                  */
/*                                                                     */
/* The EH path here:                                                   */
/*   user code: setjmp(buf) -> kill(getpid(), SIGUSR1) -> pause()     */
/*   kill() immediately queues SIGUSR1 in lind's signal queue         */
/*   pause() calls lind-take-next-signal (Rust, returns before throw) */
/*   pause() calls signal_callback(handler, signo)  [pure wasm]       */
/*   signal_callback calls our handler               [pure wasm]       */
/*   handler calls longjmp(buf, 99)                                    */
/*     -> __wasm_longjmp in libc.so  [cross-module]                   */
/*     -> throws __c_longjmp                                           */
/*   exception unwinds through signal_callback -> through pause()     */
/*   (no Rust boundary in the unwinding path)                         */
/*   caught by try_table at setjmp call site in user code             */
/*                                                                     */
/* Note: kill(getpid(), sig) queues the signal synchronously before   */
/* pause() is called, so lind-take-next-signal finds it immediately   */
/* without needing to block.                                           */
/* ------------------------------------------------------------------ */
static jmp_buf g_sigbuf;
static volatile int g_signal_handler_ran = 0;

static void signal_handler_longjmp(int sig)
{
    (void)sig;
    g_signal_handler_ran = 1;
    longjmp(g_sigbuf, 99);
}

static void test_signal_crossmodule(void)
{
    printf("\n[2] Signal path cross-module longjmp\n");
    g_signal_handler_ran = 0;

    struct sigaction sa = {0};
    sa.sa_handler = signal_handler_longjmp;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR1, &sa, NULL);

    int v = setjmp(g_sigbuf);
    if (v == 0) {
        kill(getpid(), SIGUSR1);
        pause();
        printf("  FAIL: pause() returned without longjmp (signal handler ran=%d)\n",
               g_signal_handler_ran);
        failed++;
        return;
    }
    EXPECT_EQ("signal handler ran", g_signal_handler_ran, 1);
    EXPECT_EQ("signal cross-module longjmp delivers 99", v, 99);

    signal(SIGUSR1, SIG_DFL);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    printf("=== Cross-module longjmp/setjmp tests (dynamic build) ===\n");

    test_basic();
    test_signal_crossmodule();

    printf("\n=== Results: %d passed, %d failed ===\n", passed, failed);
    return failed > 0 ? 1 : 0;
}
