#define _LARGEFILE64_SOURCE
#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char *argv[]) {
    DIR *dir;
    struct dirent64 *entry;
    int count = 0;
    const int max_entries = 1000;

    (void)argc;
    (void)argv;

    dir = opendir(".");
    if (dir == NULL) {
        perror("opendir failed");
        return 1;
    }

    while ((entry = readdir64(dir)) != NULL) {
        printf("%s\n", entry->d_name);
        count++;

        if (count > max_entries) {
            fprintf(stderr, "FAIL: readdir64 did not terminate within %d entries\n", max_entries);
            closedir(dir);
            return 1;
        }
    }

    closedir(dir);
    return 0;
}
