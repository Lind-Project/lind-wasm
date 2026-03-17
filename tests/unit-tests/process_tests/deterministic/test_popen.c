#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

int main(int argc, char *argv[]) {
    printf("Testing popen with simple echo...\n");
    fflush(stdout);

    FILE *fp = popen("echo hello_from_popen", "r");
    if (fp == NULL) {
        perror("popen failed");
        return 1;
    }
    printf("popen succeeded\n");
    fflush(stdout);

    char buf[256];
    while (fgets(buf, sizeof(buf), fp) != NULL) {
        printf("Got: %s", buf);
        fflush(stdout);
    }
    int ret = pclose(fp);
    printf("pclose returned %d\n", ret);
    return 0;
}
