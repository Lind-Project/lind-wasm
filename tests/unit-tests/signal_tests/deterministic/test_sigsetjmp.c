/*
 * sigsetjmp / siglongjmp tests for lind-wasm EH mode.
 *
 * In EH mode, siglongjmp throws __c_longjmp (same tag as longjmp).  The
 * exception must propagate through any pure-wasm call chain back to the
 * try_table inserted by the SjLj pass at the sigsetjmp call site.
 *
 * General behaviors (zero val normalization, nested frames, deep stack) are
 * shared with the longjmp path and are covered by setjmp_edge.c.  This file
 * tests only what is unique to the sigsetjmp/siglongjmp API:
 *
 * Tests
 * -----
 * 1. Basic sigsetjmp/siglongjmp — verifies the sigsetjmp macro expansion
 *    and siglongjmp round-trip work end-to-end in EH mode.
 * 2. Signal handler calls siglongjmp — delivered via kill+pause pure-wasm
 *    path; verifies siglongjmp unwinds through signal_callback/pause() with
 *    no Rust boundary back to the sigsetjmp call site.
 *
 * Note: the sigsuspend delivery path (block signal → kill → sigsuspend) is
 * covered by the pre-existing signal_longjmp.c test.
 */

#include <setjmp.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static int passed = 0;
static int failed = 0;

#define EXPECT_EQ(label, got, expected)                                   \
    do {                                                                   \
        if ((int)(got) == (int)(expected)) {                              \
            printf("  PASS: %s\n", (label));                              \
            passed++;                                                      \
        } else {                                                           \
            printf("  FAIL: %s — got %d, expected %d\n",                 \
                   (label), (int)(got), (int)(expected));                 \
            failed++;                                                      \
        }                                                                  \
    } while (0)

/* ------------------------------------------------------------------ */
/* Test 1: basic sigsetjmp / siglongjmp                               */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_buf1;

static void test_basic(void)
{
    printf("\n[1] Basic sigsetjmp / siglongjmp\n");
    int v = sigsetjmp(g_buf1, 1);
    if (v == 0) {
        siglongjmp(g_buf1, 42);
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("siglongjmp delivers 42", v, 42);
}

/* ------------------------------------------------------------------ */
/* Test 2: siglongjmp from signal handler via kill + pause            */
/*                                                                     */
/* kill(getpid(), SIGUSR1) immediately queues the signal; pause()     */
/* delivers it via lind-take-next-signal + signal_callback (pure      */
/* wasm, no Rust boundary).  siglongjmp throws __c_longjmp which      */
/* unwinds through signal_callback / pause() to the sigsetjmp site.  */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_buf2;
static volatile int g_ran = 0;

static void handler(int sig)
{
    (void)sig;
    g_ran = 1;
    siglongjmp(g_buf2, 99);
}

static void test_signal_pause(void)
{
    printf("\n[2] siglongjmp from signal handler via kill+pause\n");
    g_ran = 0;

    struct sigaction sa = {0};
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR1, &sa, NULL);

    int v = sigsetjmp(g_buf2, 1);
    if (v == 0) {
        kill(getpid(), SIGUSR1);
        pause();
        printf("  FAIL: pause() returned without siglongjmp (handler ran=%d)\n",
               g_ran);
        failed++;
        signal(SIGUSR1, SIG_DFL);
        return;
    }
    EXPECT_EQ("signal handler ran", g_ran, 1);
    EXPECT_EQ("siglongjmp from handler delivers 99", v, 99);
    signal(SIGUSR1, SIG_DFL);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    printf("=== sigsetjmp / siglongjmp tests ===\n");

    test_basic();
    test_signal_pause();

    printf("\n=== Results: %d passed, %d failed ===\n", passed, failed);
    return failed > 0 ? 1 : 0;
}
