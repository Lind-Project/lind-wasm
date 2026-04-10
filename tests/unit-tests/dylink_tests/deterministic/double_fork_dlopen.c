#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>
#include <dlfcn.h>

typedef void (*hello_fn)(const char *);

int main(void) {
    /* Fork before any dlopen so that child and grandchild both have to
       perform dlopen themselves, verifying that a process freshly forked
       from a process that has never called dlopen can still load and use
       a shared library. */
    int pid = fork();

    if (pid < 0) {
        fprintf(stderr, "fork (first) failed\n");
        return 1;
    }

    if (pid == 0) {
        /* ---- child ---- */

        /* dlopen inside the child. */
        void *handle = dlopen("lib.cwasm", RTLD_LAZY);
        if (!handle) {
            fprintf(stderr, "child: dlopen failed: %s\n", dlerror());
            return 1;
        }

        dlerror();
        hello_fn fn = (hello_fn)dlsym(handle, "hello");
        char *err = dlerror();
        if (err) {
            fprintf(stderr, "child: dlsym failed: %s\n", err);
            dlclose(handle);
            return 1;
        }

        fn("child, before second fork");

        /* Fork again from within the child so the grandchild inherits the
           already-dlopened library and must be able to call it. */
        int pid2 = fork();

        if (pid2 < 0) {
            fprintf(stderr, "child: fork (second) failed\n");
            dlclose(handle);
            return 1;
        }

        if (pid2 == 0) {
            /* ---- grandchild ---- */

            /* Call via the function pointer inherited from the child. */
            fn("grandchild, via inherited pointer");

            /* Re-resolve the symbol independently to verify the grandchild's
               own GOT and symbol table are consistent. */
            dlerror();
            hello_fn fn2 = (hello_fn)dlsym(handle, "hello");
            err = dlerror();
            if (err) {
                fprintf(stderr, "grandchild: dlsym failed: %s\n", err);
                dlclose(handle);
                return 1;
            }

            fn2("grandchild, via own dlsym");

            dlclose(handle);
            return 0;
        } else {
            /* child waits for grandchild */
            int status;
            waitpid(pid2, &status, 0);
            if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
                fprintf(stderr, "child: grandchild reported an error\n");
                dlclose(handle);
                return 1;
            }

            fn("child, after grandchild joined");
            dlclose(handle);
            return 0;
        }
    } else {
        /* ---- parent ---- */

        int status;
        waitpid(pid, &status, 0);
        if (!WIFEXITED(status) || WEXITSTATUS(status) != 0) {
            fprintf(stderr, "parent: child reported an error\n");
            return 1;
        }

        /* Parent never called dlopen; confirm it still works after the child
           tree has exited. */
        void *handle = dlopen("lib.cwasm", RTLD_LAZY);
        if (!handle) {
            fprintf(stderr, "parent: dlopen failed: %s\n", dlerror());
            return 1;
        }

        dlerror();
        hello_fn fn = (hello_fn)dlsym(handle, "hello");
        char *err = dlerror();
        if (err) {
            fprintf(stderr, "parent: dlsym failed: %s\n", err);
            dlclose(handle);
            return 1;
        }

        fn("parent, after child tree exited");
        dlclose(handle);
        return 0;
    }
}
