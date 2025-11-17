#include <stdio.h>
#include <unistd.h>
#include <string.h>

int __lind_debug_num(int num) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-debug-num")
));

int __lind_dlopen(char* filename) __attribute__((
    __import_module__("lind"),
    __import_name__("dlopen")
));

int __lind_dlsym(void* handle, char* symbol) __attribute__((
    __import_module__("lind"),
    __import_name__("dlsym")
));

typedef void (*func_type)(char*);

void main_function(char *str)
{
    printf("from main function: %s\n", str);
}

void make_call(func_type func, char* arg)
{
    printf("make_call: func index: %d\n", func);
    func(arg);
}

char *data = "string from main process!";

int main()
{
    void* handle = (void *) __lind_dlopen("lib.wasm");
    func_type (*myfunc)(func_type) = (func_type (*)(func_type)) __lind_dlsym(handle, "myfunc");
    func_type main_func = main_function;
    make_call(main_func, data);
    func_type lib_func = myfunc(main_function);
    make_call(lib_func, data);
    return 0;
}
