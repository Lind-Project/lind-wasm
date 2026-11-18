#include <assert.h>
#include <pthread.h>
#include <sys/wait.h>
#include <unistd.h>

static __thread int thread_value;

static void *fork_from_worker(void *arg) {
    int stack_value = *(int *)arg;
    thread_value = 91;

    pid_t pid = fork();
    assert(pid >= 0);
    if (pid == 0) {
        _exit(stack_value == 73 && thread_value == 91 ? 0 : 1);
    }

    int status;
    assert(waitpid(pid, &status, 0) == pid);
    assert(WIFEXITED(status) && WEXITSTATUS(status) == 0);
    assert(thread_value == 91);
    return NULL;
}

int main(void) {
    pthread_t thread;
    int value = 73;
    assert(pthread_create(&thread, NULL, fork_from_worker, &value) == 0);
    assert(pthread_join(thread, NULL) == 0);
    return 0;
}
