/* 4 threads, malloc only (no free) — isolates malloc vs free */
#include <pthread.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

void *thread_fn(void *arg) {
    int id = *(int *)arg;
    char buf[] = "[T0:start]\n";
    buf[2] = '0' + id;
    write(2, buf, 11);

    for (int i = 0; i < 32; i++) {
        void *p = malloc(64);
        if (!p) { write(2, "malloc fail\n", 12); return (void *)1; }
        memset(p, id, 64);
        /* intentionally leak — no free */
    }

    buf[4] = 'd'; buf[5] = 'o'; buf[6] = 'n'; buf[7] = 'e';
    buf[8] = ']'; buf[9] = '\n'; buf[10] = '\0';
    write(2, buf, 10);
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
