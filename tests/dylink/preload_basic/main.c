#include <stdio.h>
#include <unistd.h>
#include <string.h>

int __lind_debug_num(int num) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-debug-num")
));

typedef void (*func_type)(char*);

extern func_type myfunc(func_type);

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

int var = 123;
extern int library_data;

int main()
{
    printf("main module var=%d, library_data=%d\n", var, library_data);
    __lind_debug_num(233);
    func_type main_func = main_function;
    make_call(main_func, data);
    func_type lib_func = myfunc(main_function);
    make_call(lib_func, data);
    return 0;
}
