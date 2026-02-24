#include <stdio.h>
// #include <unistd.h>
#include <string.h>

int __lind_debug_num(int num) __attribute__((
    __import_module__("lind"),
    __import_name__("lind-debug-num")
));

typedef void (*func_type)(char*);

void lib_function(char *str)
{
    printf("from library function: %s\n", str);
    // write(1, "from library function: ", 23);
    // write(1, str, strlen(str));
    // write(1, "\n", 1);
}

void make_call(func_type func, char* arg)
{
    func(arg);
}

char *data = "this is the data from library";

int var = 233;
int library_data = 111;

func_type myfunc(func_type main_func)
{
    __lind_debug_num(2333);
    printf("in lib, var=%d\n", var);
    // write(1, tmp, strlen(tmp));
    func_type lib_func = lib_function;
    make_call(lib_function, data);
    make_call(main_func, data);
    return lib_function;
}
