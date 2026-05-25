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
 * Tests
 * -----
 * 1. Basic:        user setjmp, longjmp routed through __wasm_longjmp in libc.so
 * 2. Zero val:     longjmp(buf, 0) must deliver 1 (ISO C requirement)
 * 3. Nested:       longjmp to inner frame must not trigger outer frame
 * 4. Deep stack:   longjmp across a deep wasm call stack
 * 5. Re-use buf:   same jmp_buf used multiple times sequentially
 * 6. Signal path:  setjmp in user code; kill(getpid(), SIGUSR1) immediately
 *                  queues a signal; pause() delivers it via pure-wasm
 *                  signal_callback; handler calls longjmp(EH path); exception
 *                  propagates through signal_callback/pause() with no Rust
 *                  boundary and is caught by the setjmp try_table in user code.
 * 7. dlopen lib:   setjmp in user code; dlopen longjmp_lib.cwasm; call
 *                  lib_do_longjmp which calls longjmp from inside the shared
 *                  library; the EH exception crosses two module boundaries
 *                  (lib → libc.so → user code) and is caught at the setjmp
 *                  call site.  Verifies the host tag is shared across all
 *                  module instances in the Store.
 *
 * Compilation
 * -----------
 * This test MUST be compiled as a dynamic build (no -s flag) to exercise
 * the cross-module path.  The test runner uses the default dynamic mode.
 */

#include <dlfcn.h>
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

#define EXPECT_NE(label, got, unexpected)                                      \
    do {                                                                        \
        if ((int)(got) != (int)(unexpected)) {                                 \
            printf("  PASS: %s\n", (label));                                   \
            passed++;                                                           \
        } else {                                                                \
            printf("  FAIL: %s — got %d, did not expect %d\n",                \
                   (label), (int)(got), (int)(unexpected));                    \
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
/* Test 2: longjmp(buf, 0) must deliver 1 (POSIX/ISO C)               */
/* ------------------------------------------------------------------ */
static void test_zero_val(void)
{
    printf("\n[2] longjmp(buf, 0) delivers 1\n");
    int v = setjmp(g_buf);
    if (v == 0) {
        longjmp(g_buf, 0);
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("longjmp(buf,0) delivers 1", v, 1);
}

/* ------------------------------------------------------------------ */
/* Test 3: nested frames — longjmp to inner must not trigger outer    */
/* testSetjmp searches the per-thread table and rethrows if the buf   */
/* doesn't match; the rethrown exception is caught by the outer       */
/* frame's try_table only if the outer buf matches.  Here we longjmp  */
/* to the inner buf while the outer is live, inner should fire.       */
/* ------------------------------------------------------------------ */
static jmp_buf g_outer, g_inner;

static void test_nested(void)
{
    printf("\n[3] Nested frames — longjmp targets inner only\n");

    int outer = setjmp(g_outer);
    if (outer != 0) {
        printf("  FAIL: outer frame spuriously triggered (v=%d)\n", outer);
        failed++;
        return;
    }

    int inner = setjmp(g_inner);
    if (inner == 0) {
        longjmp(g_inner, 77);
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("inner frame caught, outer not", inner, 77);
}

/* ------------------------------------------------------------------ */
/* Test 4: longjmp across a deep wasm call stack                      */
/* ------------------------------------------------------------------ */
static jmp_buf g_deep;

static void deep3(void) { longjmp(g_deep, 3); }
static void deep2(void) { deep3(); }
static void deep1(void) { deep2(); }

static void test_deep_stack(void)
{
    printf("\n[4] Deep call stack\n");
    int v = setjmp(g_deep);
    if (v == 0) {
        deep1();
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("deep longjmp delivers 3", v, 3);
}

/* ------------------------------------------------------------------ */
/* Test 5: re-use the same jmp_buf multiple times                     */
/* ------------------------------------------------------------------ */
static jmp_buf g_reuse;
static int g_reuse_count;

static void test_reuse(void)
{
    printf("\n[5] Re-use jmp_buf\n");
    g_reuse_count = 0;
    int v = setjmp(g_reuse);
    if (v == 0 || v < 3) {
        if (v != 0) g_reuse_count++;
        longjmp(g_reuse, v + 1);
    }
    EXPECT_EQ("reuse: final val", v, 3);
    EXPECT_EQ("reuse: jump count", g_reuse_count, 2);
}

/* ------------------------------------------------------------------ */
/* Test 6: signal path — longjmp from signal handler                  */
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
/* without needing to block.  alarm()+pause() would race: pause()     */
/* returns EINTR before the timer fires.                              */
/* ------------------------------------------------------------------ */
static jmp_buf g_sigbuf;
static volatile int g_signal_handler_ran = 0;

static void signal_handler_longjmp(int sig)
{
    (void)sig;
    g_signal_handler_ran = 1;
    /* EH path: longjmp throws __c_longjmp through pause()'s pure-wasm   */
    /* call chain.  No Rust host boundary in the unwind path.            */
    longjmp(g_sigbuf, 99);
}

static void test_signal_crossmodule(void)
{
    printf("\n[6] Signal path cross-module longjmp\n");
    g_signal_handler_ran = 0;

    struct sigaction sa = {0};
    sa.sa_handler = signal_handler_longjmp;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR1, &sa, NULL);

    int v = setjmp(g_sigbuf);
    if (v == 0) {
        /* Queue signal synchronously so it is pending when pause() polls */
        kill(getpid(), SIGUSR1);
        pause(); /* delivers SIGUSR1 inline via lind-take-next-signal + signal_callback */
        /* If we reach here, pause returned EINTR without longjmp — test failure */
        printf("  FAIL: pause() returned without longjmp (signal handler ran=%d)\n",
               g_signal_handler_ran);
        failed++;
        return;
    }
    EXPECT_EQ("signal handler ran", g_signal_handler_ran, 1);
    EXPECT_EQ("signal cross-module longjmp delivers 99", v, 99);

    /* Restore default SIGUSR1 disposition. */
    signal(SIGUSR1, SIG_DFL);
}

/* ------------------------------------------------------------------ */
/* Test 7: longjmp from a dlopen'd shared library                     */
/*                                                                     */
/* longjmp_lib.cwasm exports lib_do_longjmp(jmp_buf *, int).          */
/* When called, it executes longjmp which the SjLj pass lowers to     */
/* __wasm_longjmp (in libc.so).  The throw crosses two module         */
/* boundaries (longjmp_lib.so → libc.so → user code) and must be     */
/* caught by the try_table at the setjmp site in this file.           */
/* This verifies that the host-provided __c_longjmp tag is shared     */
/* across all module instances loaded in the same Store.              */
/* ------------------------------------------------------------------ */
static jmp_buf g_dlopen_buf;

static void test_dlopen_longjmp(void)
{
    printf("\n[7] longjmp from dlopen'd shared library\n");

    void *h = dlopen("longjmp_lib.cwasm", RTLD_LAZY);
    if (h == NULL) {
        printf("  SKIP: dlopen(longjmp_lib.cwasm) failed: %s\n", dlerror());
        return;
    }

    void (*lib_do_longjmp)(jmp_buf *, int) =
        (void (*)(jmp_buf *, int)) dlsym(h, "lib_do_longjmp");
    if (lib_do_longjmp == NULL) {
        printf("  SKIP: dlsym(lib_do_longjmp) failed: %s\n", dlerror());
        dlclose(h);
        return;
    }

    int v = setjmp(g_dlopen_buf);
    if (v == 0) {
        lib_do_longjmp(&g_dlopen_buf, 55);
        printf("  FAIL: should not reach after lib_do_longjmp\n");
        failed++;
        dlclose(h);
        return;
    }
    EXPECT_EQ("dlopen lib longjmp delivers 55", v, 55);

    dlclose(h);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    printf("=== Cross-module longjmp/setjmp tests (dynamic build) ===\n");

    test_basic();
    test_zero_val();
    test_nested();
    test_deep_stack();
    test_reuse();
    test_signal_crossmodule();
    test_dlopen_longjmp();

    printf("\n=== Results: %d passed, %d failed ===\n", passed, failed);
    return failed > 0 ? 1 : 0;
}
