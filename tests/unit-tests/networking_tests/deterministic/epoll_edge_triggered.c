/*
 * Advanced epoll tests: EPOLLET (edge-triggered), EPOLLONESHOT,
 * EPOLL_CTL_MOD, multiple FD monitoring.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <assert.h>
#include <unistd.h>
#include <errno.h>
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
    struct epoll_event out[4];
    int n = epoll_wait(epfd, out, 4, 100);
    assert(n == 1);
    assert(out[0].events & EPOLLIN);
    assert(out[0].data.fd == p[0]);
    printf("1a. ET: first epoll_wait fired (1 event)\n");

    /* Don't read. Second wait: should NOT fire (edge already delivered) */
    n = epoll_wait(epfd, out, 4, 50);
    assert(n == 0);
    printf("1b. ET: second epoll_wait without read → 0 events (correct)\n");

    /* Now read partially, then write more to re-arm the edge */
    char buf[16];
    assert(read(p[0], buf, 2) == 2); /* read 2 of 3 bytes */

    /* Still no new edge (no new write) */
    n = epoll_wait(epfd, out, 4, 50);
    assert(n == 0);
    printf("1c. ET: partial read, no new write → 0 events\n");

    /* Write more → new edge */
    assert(write(p[1], "d", 1) == 1);
    n = epoll_wait(epfd, out, 4, 100);
    assert(n == 1);
    printf("1d. ET: new write → edge fires again\n");

    /* Drain */
    while (read(p[0], buf, sizeof(buf)) > 0)
        ;

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

    n = epoll_wait(epfd, out, 4, 100);
    assert(n == 1);
    printf("2a. ONESHOT: first fire OK\n");

    /* Read the data */
    assert(read(p[0], buf, sizeof(buf)) == 1);

    /* Write more — should NOT fire (oneshot disabled it) */
    assert(write(p[1], "y", 1) == 1);
    n = epoll_wait(epfd, out, 4, 50);
    assert(n == 0);
    printf("2b. ONESHOT: second write → 0 events (disabled)\n");

    /* Re-arm with EPOLL_CTL_MOD */
    ev.events = EPOLLIN | EPOLLONESHOT;
    assert(epoll_ctl(epfd, EPOLL_CTL_MOD, p[0], &ev) == 0);

    n = epoll_wait(epfd, out, 4, 100);
    assert(n == 1);
    printf("2c. ONESHOT: re-armed via MOD → fires again\n");

    assert(read(p[0], buf, sizeof(buf)) == 1);
    close(p[0]);
    close(p[1]);
    close(epfd);

    /* --- 3) Multiple FDs in one epoll instance --- */
    int p1[2], p2[2], p3[2];
    assert(pipe(p1) == 0);
    assert(pipe(p2) == 0);
    assert(pipe(p3) == 0);

    epfd = epoll_create1(0);
    assert(epfd >= 0);

    ev.events = EPOLLIN;
    ev.data.fd = p1[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p1[0], &ev) == 0);
    ev.data.fd = p2[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p2[0], &ev) == 0);
    ev.data.fd = p3[0];
    assert(epoll_ctl(epfd, EPOLL_CTL_ADD, p3[0], &ev) == 0);

    /* Write to p1 and p3 only */
    assert(write(p1[1], "a", 1) == 1);
    assert(write(p3[1], "b", 1) == 1);

    n = epoll_wait(epfd, out, 4, 100);
    assert(n == 2);

    int saw_p1 = 0, saw_p3 = 0;
    for (int i = 0; i < n; i++) {
        if (out[i].data.fd == p1[0]) saw_p1 = 1;
        if (out[i].data.fd == p3[0]) saw_p3 = 1;
    }
    assert(saw_p1 && saw_p3);
    printf("3. Multiple FDs: got events for p1 and p3, not p2\n");

    /* --- 4) EPOLL_CTL_DEL --- */
    assert(epoll_ctl(epfd, EPOLL_CTL_DEL, p1[0], NULL) == 0);

    /* p1 still has data but should not be reported */
    n = epoll_wait(epfd, out, 4, 50);
    for (int i = 0; i < n; i++) {
        assert(out[i].data.fd != p1[0]);
    }
    printf("4. EPOLL_CTL_DEL: p1 removed, not reported\n");

    /* --- 5) Error: add same FD twice → EEXIST --- */
    ev.events = EPOLLIN;
    ev.data.fd = p2[0];
    errno = 0;
    int ret = epoll_ctl(epfd, EPOLL_CTL_ADD, p2[0], &ev);
    assert(ret == -1);
    assert(errno == EEXIST);
    printf("5. EPOLL_CTL_ADD duplicate → EEXIST\n");

    /* --- 6) Error: mod non-existent FD → ENOENT --- */
    ev.data.fd = p1[0]; /* was deleted */
    errno = 0;
    ret = epoll_ctl(epfd, EPOLL_CTL_MOD, p1[0], &ev);
    assert(ret == -1);
    assert(errno == ENOENT);
    printf("6. EPOLL_CTL_MOD deleted FD → ENOENT\n");

    close(p1[0]); close(p1[1]);
    close(p2[0]); close(p2[1]);
    close(p3[0]); close(p3[1]);
    close(epfd);

    printf("All advanced epoll tests passed\n");
    return 0;
}
