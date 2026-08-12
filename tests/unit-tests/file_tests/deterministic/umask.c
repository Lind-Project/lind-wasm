#include <fcntl.h>
#include <stdio.h>
#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

static int failures;

#define CHECK(message, condition)                                              \
    do {                                                                       \
        if (condition) {                                                       \
            printf("PASS: %s\n", message);                                    \
        } else {                                                               \
            printf("FAIL: %s\n", message);                                    \
            failures++;                                                        \
        }                                                                      \
    } while (0)

static mode_t fd_mode(int fd) {
    struct stat st;
    return fstat(fd, &st) == 0 ? st.st_mode & 0777 : (mode_t)-1;
}

int main(void) {
    const char *unmasked_path = "testfiles/umask_unmasked";
    const char *default_path = "testfiles/umask_default";
    const char *private_path = "testfiles/umask_private";
    const char *directory_path = "testfiles/umask_directory";
    const char *fork_path = "testfiles/umask_fork";

    unlink(unmasked_path);
    unlink(default_path);
    unlink(private_path);
    unlink(fork_path);
    rmdir(directory_path);

    CHECK("initial umask is 0022", umask(0000) == 0022);

    int fd = open(unmasked_path, O_CREAT | O_RDWR, 0666);
    CHECK("umask 0000 permits mode 0666", fd >= 0 && fd_mode(fd) == 0666);
    if (fd >= 0) close(fd);

    CHECK("umask returns previous value", umask(0022) == 0000);
    fd = open(default_path, O_CREAT | O_RDWR, 0666);
    CHECK("umask 0022 produces mode 0644", fd >= 0 && fd_mode(fd) == 0644);
    if (fd >= 0) close(fd);

    CHECK("changing umask returns 0022", umask(0077) == 0022);
    fd = open(private_path, O_CREAT | O_RDWR, 0666);
    CHECK("umask 0077 produces mode 0600", fd >= 0 && fd_mode(fd) == 0600);
    if (fd >= 0) close(fd);

    CHECK("changing umask returns 0077", umask(0027) == 0077);
    CHECK("umask 0027 produces directory mode 0750",
          mkdir(directory_path, 0777) == 0);
    struct stat st;
    CHECK("created directory has mode 0750",
          stat(directory_path, &st) == 0 && (st.st_mode & 0777) == 0750);

    CHECK("set mask for fork inheritance", umask(0077) == 0027);
    pid_t child = fork();
    if (child == 0) {
        int child_fd = open(fork_path, O_CREAT | O_RDWR, 0666);
        int child_ok = child_fd >= 0 && fd_mode(child_fd) == 0600;
        if (child_fd >= 0) close(child_fd);
        _exit(child_ok ? 0 : 1);
    }

    int status = 0;
    CHECK("fork child creates with inherited umask",
          child > 0 && waitpid(child, &status, 0) == child &&
              WIFEXITED(status) && WEXITSTATUS(status) == 0);
    CHECK("parent retains umask", umask(01777) == 0077);
    CHECK("masked high bits are discarded", umask(0022) == 0777);

    unlink(unmasked_path);
    unlink(default_path);
    unlink(private_path);
    unlink(fork_path);
    rmdir(directory_path);

    printf("Result: %s\n", failures == 0 ? "PASS" : "FAIL");
    return failures == 0 ? 0 : 1;
}
