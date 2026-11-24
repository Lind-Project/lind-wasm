/*
 * posix_spawn issues clone(CLONE_VFORK): the parent must stay suspended until
 * the child execs or exits. Lind's glibc drops CLONE_VM for posix_spawn, so
 * the parent cannot see the child's progress through shared memory; the only
 * guarantee is the suspension itself. The child runs its file actions before
 * exec, so the marker file it creates must exist once posix_spawn returns.
 *
 * The spawned image is the one forkexecv.c runs via
 * execv("automated_tests/hello", ...) (also tells the test harness to build it).
 */
#include <assert.h>
#include <errno.h>
#include <fcntl.h>
#include <spawn.h>
#include <stdio.h>
#include <sys/wait.h>
#include <unistd.h>

extern char **environ;

static const char *MARKER = "posix_spawn_vfork.marker";

/* Returns the exit status of the child. */
static int spawn_and_check(const char *path) {
    unlink(MARKER);

    posix_spawn_file_actions_t actions;
    assert(posix_spawn_file_actions_init(&actions) == 0);
    /* Keep the child busy before it creates the marker. */
    for (int i = 0; i < 256; i++)
        assert(posix_spawn_file_actions_adddup2(&actions, 1, 20) == 0);
    assert(posix_spawn_file_actions_addopen(&actions, 21, MARKER,
                                            O_WRONLY | O_CREAT | O_TRUNC, 0644) == 0);

    char *argv[] = {(char *)path, NULL};
    pid_t pid;
    int ret = posix_spawn(&pid, path, &actions, NULL, argv, environ);
    int marker_exists = access(MARKER, F_OK) == 0;
    posix_spawn_file_actions_destroy(&actions);

    int status = 127;
    if (ret == 0) {
        assert(waitpid(pid, &status, 0) == pid);
        assert(WIFEXITED(status));
        status = WEXITSTATUS(status);
    } else {
        /* Linux reports the failed exec through shared memory; Lind cannot
           and instead lets the child exit with 127. */
        assert(ret == ENOENT);
    }
    assert(marker_exists && "posix_spawn returned before the child ran its file actions");
    unlink(MARKER);
    return status;
}

int main(void) {
    /* Released by the child's exit after a failed exec. */
    int missing = spawn_and_check("automated_tests/no-such-binary");
    /* Released by the child's exec. */
    int hello = spawn_and_check("automated_tests/hello");
    printf("missing binary: exit %d\n", missing);
    printf("hello: exit %d\n", hello);
    return 0;
}
