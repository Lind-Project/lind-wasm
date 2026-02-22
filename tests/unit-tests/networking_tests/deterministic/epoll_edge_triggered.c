/*
 * Advanced epoll tests: EPOLLET (edge-triggered), EPOLLONESHOT,
 * EPOLL_CTL_MOD, EPOLL_CTL_DEL, error cases.
 *
 * NOTE: maxevents is kept at 1 throughout to work around a known
 * RawPOSIX bug where kernel_events Vec has len=1 regardless of
 * maxevents, causing an index-out-of-bounds panic when >1 events
 * are returned simultaneously (net_calls.rs:946).
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
#include <fcntl.h>
#include <sys/epoll.h>

int main(void) {
    /* --- 1) Edge-triggered: only fires once per new data --- */
    int p[2];
    assert(pipe(p) == 0);

    int epfd = epoll_create1(0);
    assert(epfd >= 0);

    struct epoll_event ev = {0};
    ev.events = EPOLLIN | EPOLLET;
    ev.data.fd = p[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p[0], &ev) == 0);

    /* Write some data */
    assert(write(p[1], "abc", 3) == 3);

    /* First wait: should fire */
    struct epoll_event out[1];
    int n = epoll_wait(epfd, out, 1, 100);
    assert(n == 1);
    assert(out[0].events & EPOLLIN);
    assert(out[0].data.fd == p[0]);
    printf("1a. ET: first epoll_wait fired (1 event)\n");

    /* Don't read. Second wait: should NOT fire (edge already delivered) */
    n = epoll_wait(epfd, out, 1, 50);
    assert(n == 0);
    printf("1b. ET: second epoll_wait without read → 0 events (correct)\n");

    /* Now read partially, then check — still no new edge (no new write) */
    char buf[16];
    assert(read(p[0], buf, 2) == 2); /* read 2 of 3 bytes */

    n = epoll_wait(epfd, out, 1, 50);
    assert(n == 0);
    printf("1c. ET: partial read, no new write → 0 events\n");

    /* Write more → new edge */
    assert(write(p[1], "d", 1) == 1);
    n = epoll_wait(epfd, out, 1, 100);
    assert(n == 1);
    printf("1d. ET: new write → edge fires again\n");

    /* Drain remaining bytes (abc + d = 4, read 2 already, so 2 left) */
    assert(read(p[0], buf, sizeof(buf)) == 2);

    close(p[0]);
    close(p[1]);
    close(epfd);

    /* --- 2) EPOLLONESHOT: fires once, then needs re-arming --- */
    assert(pipe(p) == 0);

    epfd = epoll_create1(0);
    assert(epfd >= 0);

    ev.events = EPOLLIN | EPOLLONESHOT;
    ev.data.fd = p[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p[0], &ev) == 0);

    assert(write(p[1], "x", 1) == 1);

    n = epoll_wait(epfd, out, 1, 100);
    assert(n == 1);
    printf("2a. ONESHOT: first fire OK\n");

    /* Read the data */
    assert(read(p[0], buf, sizeof(buf)) == 1);

    /* Write more — should NOT fire (oneshot disabled it) */
    assert(write(p[1], "y", 1) == 1);
    n = epoll_wait(epfd, out, 1, 50);
    assert(n == 0);
    printf("2b. ONESHOT: second write → 0 events (disabled)\n");

    /* Re-arm with EPOLL_CTL_MOD */
    ev.events = EPOLLIN | EPOLLONESHOT;
    assert(epoll_ctl(epfd, EPOLL_CTL_MOD, p[0], &ev) == 0);

    n = epoll_wait(epfd, out, 1, 100);
    assert(n == 1);
    printf("2c. ONESHOT: re-armed via MOD → fires again\n");

    assert(read(p[0], buf, sizeof(buf)) == 1);
    close(p[0]);
    close(p[1]);
    close(epfd);

    /* --- 3) EPOLL_CTL_DEL: removed FD no longer reported --- */
    int pa[2], pb[2];
    assert(pipe(pa) == 0);
    assert(pipe(pb) == 0);

    epfd = epoll_create1(0);
    assert(epfd >= 0);

    ev.events = EPOLLIN;
    ev.data.fd = pa[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, pa[0], &ev) == 0);
    ev.data.fd = pb[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, pb[0], &ev) == 0);

    /* Delete pa before any data */
    assert(epoll_ctl(epfd, EPOLL_CTL_DEL, pa[0], NULL) == 0);

    /* Write to both — only pb should fire */
    assert(write(pa[1], "a", 1) == 1);
    assert(write(pb[1], "b", 1) == 1);

    n = epoll_wait(epfd, out, 1, 100);
    assert(n == 1);
    assert(out[0].data.fd == pb[0]);
    printf("3. EPOLL_CTL_DEL: deleted FD not reported, remaining FD works\n");

    close(pa[0]); close(pa[1]);
    close(pb[0]); close(pb[1]);
    close(epfd);

    /* --- 4) Error: add same FD twice → EEXIST --- */
    assert(pipe(p) == 0);
    epfd = epoll_create1(0);
    assert(epfd >= 0);

    ev.events = EPOLLIN;
    ev.data.fd = p[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p[0], &ev) == 0);

    errno = 0;
    int ret = epoll_ctl(epfd, EPOLL_CTL_ADD, p[0], &ev);
    assert(ret == -1);
    assert(errno == EEXIST);
    printf("4. EPOLL_CTL_ADD duplicate → EEXIST\n");

    close(p[0]); close(p[1]);
    close(epfd);

    /* --- 5) Error: mod non-existent FD → ENOENT --- */
    assert(pipe(p) == 0);
    epfd = epoll_create1(0);
    assert(epfd >= 0);

    ev.events = EPOLLIN;
    ev.data.fd = p[0];
    errno = 0;
    ret = epoll_ctl(epfd, EPOLL_CTL_MOD, p[0], &ev);
    assert(ret == -1);
    assert(errno == ENOENT);
    printf("5. EPOLL_CTL_MOD on unadded FD → ENOENT\n");

    close(p[0]); close(p[1]);
    close(epfd);

    /* --- 6) EPOLL_CLOEXEC via epoll_create1 --- */
    epfd = epoll_create1(EPOLL_CLOEXEC);
    assert(epfd >= 0);

    int fdflags = fcntl(epfd, F_GETFD);
    assert(fdflags & FD_CLOEXEC);
    printf("6. epoll_create1(EPOLL_CLOEXEC) sets FD_CLOEXEC\n");

    close(epfd);

    printf("All advanced epoll tests passed\n");
    return 0;
}
