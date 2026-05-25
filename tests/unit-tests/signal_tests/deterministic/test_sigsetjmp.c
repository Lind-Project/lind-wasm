/*
 * sigsetjmp / siglongjmp tests for lind-wasm EH mode.
 *
 * In EH mode, siglongjmp throws __c_longjmp (same tag as longjmp).  The
 * exception must propagate through any pure-wasm call chain back to the
 * try_table inserted by the SjLj pass at the sigsetjmp call site.
 *
 * Tests
 * -----
 * 1. Basic sigsetjmp/siglongjmp — round-trip with mask save/restore
 * 2. siglongjmp(buf, 0) must deliver 1 (ISO C)
 * 3. Signal handler calls siglongjmp — delivered via kill+pause pure-wasm path
 * 4. Signal handler calls siglongjmp — delivered via sigsuspend pure-wasm path
 * 5. Nested sigsetjmp — siglongjmp targets inner, outer not triggered
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
/* Test 2: siglongjmp(buf, 0) must deliver 1                          */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_buf2;

static void test_zero_val(void)
{
    printf("\n[2] siglongjmp(buf, 0) delivers 1\n");
    int v = sigsetjmp(g_buf2, 0);
    if (v == 0) {
        siglongjmp(g_buf2, 0);
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("siglongjmp(buf,0) delivers 1", v, 1);
}

/* ------------------------------------------------------------------ */
/* Test 3: siglongjmp from signal handler via kill + pause            */
/*                                                                     */
/* kill(getpid(), SIGUSR1) immediately queues the signal; pause()     */
/* delivers it via lind-take-next-signal + signal_callback (pure      */
/* wasm, no Rust boundary).  siglongjmp throws __c_longjmp which      */
/* unwinds through signal_callback / pause() to the sigsetjmp site.  */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_buf3;
static volatile int g_ran3 = 0;

static void handler3(int sig)
{
    (void)sig;
    g_ran3 = 1;
    siglongjmp(g_buf3, 99);
}

static void test_signal_pause(void)
{
    printf("\n[3] siglongjmp from signal handler via kill+pause\n");
    g_ran3 = 0;

    struct sigaction sa = {0};
    sa.sa_handler = handler3;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR1, &sa, NULL);

    int v = sigsetjmp(g_buf3, 1);
    if (v == 0) {
        kill(getpid(), SIGUSR1);
        pause();
        printf("  FAIL: pause() returned without siglongjmp (handler ran=%d)\n",
               g_ran3);
        failed++;
        signal(SIGUSR1, SIG_DFL);
        return;
    }
    EXPECT_EQ("signal handler ran", g_ran3, 1);
    EXPECT_EQ("siglongjmp from handler delivers 99", v, 99);
    signal(SIGUSR1, SIG_DFL);
}

/* ------------------------------------------------------------------ */
/* Test 4: siglongjmp from signal handler via sigsuspend              */
/*                                                                     */
/* Block SIGUSR2, queue it via kill(), then sigsuspend with empty     */
/* mask.  The EH sigsuspend sets the mask without triggering the      */
/* epoch, then calls pause() for pure-wasm delivery.                  */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_buf4;
static volatile int g_ran4 = 0;

static void handler4(int sig)
{
    (void)sig;
    g_ran4 = 1;
    siglongjmp(g_buf4, 77);
}

static void test_signal_sigsuspend(void)
{
    printf("\n[4] siglongjmp from signal handler via sigsuspend\n");
    g_ran4 = 0;

    struct sigaction sa = {0};
    sa.sa_handler = handler4;
    sigemptyset(&sa.sa_mask);
    sigaction(SIGUSR2, &sa, NULL);

    /* Block SIGUSR2 so it queues without firing yet. */
    sigset_t block, empty;
    sigemptyset(&block);
    sigaddset(&block, SIGUSR2);
    sigprocmask(SIG_BLOCK, &block, NULL);

    int v = sigsetjmp(g_buf4, 1);
    if (v == 0) {
        kill(getpid(), SIGUSR2);   /* queued but blocked */
        sigemptyset(&empty);
        sigsuspend(&empty);        /* unblock all → delivers SIGUSR2 */
        printf("  FAIL: sigsuspend returned without siglongjmp (handler ran=%d)\n",
               g_ran4);
        failed++;
        sigprocmask(SIG_UNBLOCK, &block, NULL);
        signal(SIGUSR2, SIG_DFL);
        return;
    }
    EXPECT_EQ("signal handler ran", g_ran4, 1);
    EXPECT_EQ("siglongjmp from sigsuspend delivers 77", v, 77);
    sigprocmask(SIG_UNBLOCK, &block, NULL);
    signal(SIGUSR2, SIG_DFL);
}

/* ------------------------------------------------------------------ */
/* Test 5: nested sigsetjmp — siglongjmp targets inner only           */
/* ------------------------------------------------------------------ */
static sigjmp_buf g_outer5, g_inner5;

static void test_nested(void)
{
    printf("\n[5] Nested sigsetjmp — siglongjmp targets inner only\n");

    int outer = sigsetjmp(g_outer5, 0);
    if (outer != 0) {
        printf("  FAIL: outer frame spuriously triggered (v=%d)\n", outer);
        failed++;
        return;
    }

    int inner = sigsetjmp(g_inner5, 0);
    if (inner == 0) {
        siglongjmp(g_inner5, 55);
        printf("  FAIL: should not reach\n");
        failed++;
        return;
    }
    EXPECT_EQ("inner frame caught, outer not", inner, 55);
}

/* ------------------------------------------------------------------ */
int main(void)
{
    printf("=== sigsetjmp / siglongjmp tests ===\n");

    test_basic();
    test_zero_val();
    test_signal_pause();
    test_signal_sigsuspend();
    test_nested();

    printf("\n=== Results: %d passed, %d failed ===\n", passed, failed);
    return failed > 0 ? 1 : 0;
}
