#include <dirent.h>
#include <stdio.h>
#include <stdlib.h>

int main(int argc, char *argv[]) {
    DIR *dir;
    struct dirent *entry;
    int count = 0;
    const int max_entries = 1000;

    (void)argc;
    (void)argv;

    dir = opendir(".");
    if (dir == NULL) {
        perror("opendir failed");
        return 1;
    }

    // printf("asd");
    // entry = readdir(dir);
    // printf("returned");
    // if (entry == NULL) {
    //     perror("readdir failed");
    //     return 1;
    // }
    // printf("%s\n", entry->d_name);
    while ((entry = readdir(dir)) != NULL) {
        printf("%s\n", entry->d_name);
        count++;

        if (count > max_entries) {
            fprintf(stderr, "FAIL: readdir did not terminate within %d entries\n", max_entries);
            closedir(dir);
            return 1;
        }
    }

    closedir(dir);
    return 0;
}
