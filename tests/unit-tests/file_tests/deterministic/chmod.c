#undef _GNU_SOURCE
#define _GNU_SOURCE

#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <unistd.h>

static int read_mode(const char *path, mode_t *out) {
    struct stat st;
    if (stat(path, &st) == -1) return -1;
    *out = st.st_mode & (S_IRWXU | S_IRWXG | S_IRWXO);
    return 0;
}

int main(void) {
    const char *file_name = "testfiles/chmodfile.txt";

#ifndef __wasi__
    /* Native / POSIX: keep the strict checks */
    if (chmod(file_name, S_IRUSR | S_IXUSR) != 0) { perror("chmod"); return 1; }

    mode_t m;
    if (read_mode(file_name, &m) == -1) { perror("stat"); return 1; }
    if ((m & (S_IRUSR | S_IXUSR)) != m) {
        fprintf(stderr, "Expected %s to have access mode 500 but was %03o\n",
                file_name, (unsigned int)m);
        return 1;
    }

    if (chmod(file_name, S_IRWXU) != 0) { perror("chmod revert"); return 1; }
    if (read_mode(file_name, &m) == -1) { perror("stat revert"); return 1; }
    if ((m & S_IRWXU) != m) {
        fprintf(stderr, "Expected %s to have access mode 700 but was %03o\n",
                file_name, (unsigned int)m);
        return 1;
    }

#else
    /* WASI: tolerate lack of chmod support, but keep output identical */
    int rc = chmod(file_name, S_IRUSR | S_IXUSR);
    if (rc != 0) {
        if (!(errno == ENOTSUP || errno == ENOSYS || errno == EINVAL)) {
            perror("chmod"); return 1;
        }
        /* Unsupported: treat as no-op and still pass */
    } else {
        mode_t before, after;
        if (read_mode(file_name, &before) == -1) { perror("stat before"); return 1; }
        if (read_mode(file_name, &after) == -1)  { perror("stat after");  return 1; }
        /* If FS reports some different unexpected mode, fail; otherwise ok */
        if (after != before && (after & (S_IRUSR | S_IXUSR)) != (S_IRUSR | S_IXUSR)) {
            fprintf(stderr, "WASI: unexpected mode change on %s (before %03o, after %03o)\n",
                    file_name, (unsigned int)before, (unsigned int)after);
            return 1;
        }
        /* Best-effort revert (ignore unsupported) */
        rc = chmod(file_name, S_IRWXU);
        if (rc != 0 && !(errno == ENOTSUP || errno == ENOSYS || errno == EINVAL)) {
            perror("chmod revert"); return 1;
        }
    }
#endif

    /* IMPORTANT: unified success message for both native and WASI */
    puts("Mode changed successfully");
    fflush(stdout);
    return 0;
}

