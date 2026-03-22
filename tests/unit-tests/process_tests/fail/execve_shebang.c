// requires /bin/sh to run correctly
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <string.h>

int main() {
    const char *script = "./test_script.sh";

    // Create a script with a shebang
    int fd = open(script, O_WRONLY | O_CREAT | O_TRUNC, 0755);
    if (fd < 0) {
        perror("open");
        exit(1);
    }

    const char *content =
        "#!/bin/sh\n"
        "echo \"Hello from shebang script!\"\n";

    write(fd, content, strlen(content));
    close(fd);

    // Prepare arguments for execve
    char *const args[] = { (char *)script, NULL };
    char *const env[] = { NULL };

    printf("Executing script via execve...\n");

    // Execute the script
    if (execve(script, args, env) == -1) {
        perror("execve");
        exit(1);
    }

    return 0;
}
