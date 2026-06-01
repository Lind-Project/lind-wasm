#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

static int seen_dot = 0;
static int seen_dotdot = 0;
static int seen_alpha = 0;
static int seen_beta = 0;
static int seen_gamma = 0;

static void mark_seen(const char *name) {
    if (strcmp(name, ".") == 0) {
        seen_dot = 1;
    } else if (strcmp(name, "..") == 0) {
        seen_dotdot = 1;
    } else if (strcmp(name, "alpha") == 0) {
        seen_alpha = 1;
    } else if (strcmp(name, "beta") == 0) {
        seen_beta = 1;
    } else if (strcmp(name, "gamma") == 0) {
        seen_gamma = 1;
    }
}

static int create_empty_file(const char *path) {
    int fd = open(path, O_CREAT | O_TRUNC | O_WRONLY, 0644);
    if (fd < 0) {
        perror("open");
        return -1;
    }
    if (close(fd) < 0) {
        perror("close");
        return -1;
    }
    return 0;
}

int main(int argc, char *argv[], char *envp[]) {
    (void)argc;
    (void)argv;
    (void)envp;
    
    const char *base_dir = "testfiles";
    const char *test_dir = "testfiles/readdir_basic_dir";
    const char *alpha_path = "testfiles/readdir_basic_dir/alpha";
    const char *beta_path = "testfiles/readdir_basic_dir/beta";
    const char *gamma_path = "testfiles/readdir_basic_dir/gamma";

    DIR *dir = NULL;
    struct dirent *entry;
    int count = 0;
    const int max_entries = 1000;

    /* Ensure base directory exists. */
    if (mkdir(base_dir, 0755) < 0 && errno != EEXIST) {
        perror("mkdir base_dir");
        return 1;
    }

    /* Recreate test directory from scratch as much as possible. */
    unlink(alpha_path);
    unlink(beta_path);
    unlink(gamma_path);
    rmdir(test_dir);

    if (mkdir(test_dir, 0755) < 0) {
        perror("mkdir test_dir");
        return 1;
    }

    if (create_empty_file(alpha_path) < 0) {
        return 1;
    }
    if (create_empty_file(beta_path) < 0) {
        return 1;
    }
    if (create_empty_file(gamma_path) < 0) {
        return 1;
    }

    dir = opendir(test_dir);
    if (dir == NULL) {
        perror("opendir");
        return 1;
    }

    while ((entry = readdir(dir)) != NULL) {
        mark_seen(entry->d_name);
        count++;

        if (count > max_entries) {
            fprintf(stderr, "FAIL: readdir did not terminate within %d entries\n", max_entries);
            closedir(dir);
            return 1;
        }
    }

    if (closedir(dir) < 0) {
        perror("closedir");
        return 1;
    }

    if (!seen_dot || !seen_dotdot || !seen_alpha || !seen_beta || !seen_gamma) {
        fprintf(stderr,
                "FAIL: missing expected entries "
                "(.=%d ..=%d alpha=%d beta=%d gamma=%d)\n",
                seen_dot, seen_dotdot, seen_alpha, seen_beta, seen_gamma);
        return 1;
    }

    return 0;
}
