#include <pthread.h>
#include <unistd.h>

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    char buf[] = "[T0:ok]\n";
    buf[2] = '0' + id;
    write(2, buf, 8);
    return NULL;
}

int main(void) {
    int ids[4] = {1, 2, 3, 4};
    pthread_t threads[4];
    for (int i = 0; i < 4; i++)
        pthread_create(&threads[i], NULL, thread_fn, &ids[i]);
    for (int i = 0; i < 4; i++)
        pthread_join(threads[i], NULL);
    write(1, "done\n", 5);
    return 0;
}
