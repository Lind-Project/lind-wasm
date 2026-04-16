#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <dlfcn.h>

typedef void (*hello_fn)(const char *);

struct thread_args {
    void    *handle;
    hello_fn hello_func;
};

static void *thread_routine(void *arg)
{
    struct thread_args *ta = (struct thread_args *)arg;

    /* Call the function pointer inherited from the main thread.
       The library's function-table slot must be valid in this thread's
       Wasm instance for the indirect call to succeed. */
    ta->hello_func("thread (via parent's function pointer)");

    /* Re-resolve the symbol inside the thread.
       This exercises the thread's own symbol table, which should carry
       the dlopen'd library as a result of the replay implemented in
       pthread_create_call. */
    dlerror();
    hello_fn fn2 = (hello_fn)dlsym(ta->handle, "hello");
    char *err = dlerror();
    if (err) {
        fprintf(stderr, "dlsym in thread failed: %s\n", err);
        return (void *)1L;
    }

    fn2("thread (via thread's own dlsym)");

    return (void *)0L;
}

int main(void)
{
    /* Open the shared library in the main thread before spawning. */
    void *handle = dlopen("lib.cwasm", RTLD_LAZY);
    if (!handle) {
        fprintf(stderr, "dlopen failed\n");
        return 1;
    }

    dlerror();

    hello_fn hello_func = (hello_fn)dlsym(handle, "hello");
    char *err = dlerror();
    if (err) {
        fprintf(stderr, "dlsym failed: %s\n", err);
        dlclose(handle);
        return 1;
    }

    /* Confirm the library works in the main thread before creating the thread. */
    hello_func("main thread, before create");

    struct thread_args ta = { .handle = handle, .hello_func = hello_func };

    pthread_t tid;
    if (pthread_create(&tid, NULL, thread_routine, &ta) != 0) {
        fprintf(stderr, "pthread_create failed\n");
        dlclose(handle);
        return 1;
    }

    void *ret;
    pthread_join(tid, &ret);
    if ((long)ret != 0) {
        fprintf(stderr, "thread reported an error\n");
        dlclose(handle);
        return 1;
    }

    /* Confirm the library still works in the main thread after join. */
    hello_func("main thread, after join");

    dlclose(handle);
    return 0;
}
