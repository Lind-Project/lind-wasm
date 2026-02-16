#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

// program execs on itself and checks with argc, argv
int main(int argc, char *argv[]) {
	if (argc > 1 && strcmp(argv[1], "--execd") == 0) { //success
		return 0;
	}

	execl(argv[0], argv[0], "--execd", NULL); //calls execl on itself

	//if exec fails
	perror("exec failed");
	return 1;
}
