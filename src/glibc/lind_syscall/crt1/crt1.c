#include <stdlib.h>
#include <sysexits.h>
#include <ctype.h>
#include <addr_translation.h>
#include <lind_debug.h>

extern char** environ;
// environ is a global variable that holds the environment variables for a program.
// It is typically an array of strings, where each string represents an environment variable in the format KEY=VALUE.
// The environ variable is used to store and access environment variables passed to the program by the runtime or operating system.

static char *empty_environ[1] = { NULL };

signed int __imported_lind_args_sizes_get(signed int arg0, signed int arg1) __attribute__((
    __import_module__("lind"),
    __import_name__("args_sizes_get")
));

unsigned short __lind_args_sizes_get(
    unsigned long *retptr0,
    unsigned long *retptr1
){
    signed int ret = __imported_lind_args_sizes_get((signed int) retptr0, (signed int) retptr1);
    return (unsigned short) ret;
}

signed int __imported_lind_args_get(signed int arg0, signed int arg1) __attribute__((
    __import_module__("lind"),
    __import_name__("args_get")
));

unsigned short __lind_args_get(
    unsigned char * * argv,
    unsigned char * argv_buf
){
    signed int ret = __imported_lind_args_get((signed int) argv, (signed int) argv_buf);
    return (unsigned short) ret;
}

signed int __imported_lind_environ_get(signed int arg0, signed int arg1) __attribute__((
    __import_module__("lind"),
    __import_name__("environ_get")
));

unsigned short __lind_environ_get(
    unsigned char * * environ,
    unsigned char * environ_buf
){
    signed int ret = __imported_lind_environ_get((signed int) environ, (signed int) environ_buf);
    return (unsigned short) ret;
}

signed int __imported_lind_environ_sizes_get(signed int arg0, signed int arg1) __attribute__((
    __import_module__("lind"),
    __import_name__("environ_sizes_get")
));

unsigned short __lind_environ_sizes_get(
    unsigned long *retptr0,
    unsigned long *retptr1
){
    signed int ret = __imported_lind_environ_sizes_get((signed int) retptr0, (signed int) retptr1);
    return (unsigned short) ret;
}

void __libc_setup_tls();
void __wasi_init_tp();


void __lind_initialize_environ(void) {
    // Get the sizes of the arrays we'll have to create to copy in the environment.
    size_t environ_count;
    size_t environ_buf_size;
    __lind_environ_sizes_get(&environ_count, &environ_buf_size);

    if (environ_count == 0) {
        environ = empty_environ;
        return;
    }

    // Add 1 for the NULL pointer to mark the end, and check for overflow.
    size_t num_ptrs = environ_count + 1;
    if (num_ptrs == 0) {
        goto software;
    }

    // Allocate memory for storing the environment chars.
    char *environ_buf = malloc(environ_buf_size);
    if (environ_buf == NULL) {
        goto software;
    }

    // Allocate memory for the array of pointers. This uses `calloc` both to
    // handle overflow and to initialize the NULL pointer at the end.
    char **environ_ptrs = calloc(num_ptrs, sizeof(char *));
    if (environ_ptrs == NULL) {
        free(environ_buf);
        goto software;
    }

    // Fill the environment chars, and the environ array with pointers into those chars.
    __lind_environ_get((unsigned char **)environ_ptrs, (unsigned char *)environ_buf);

    environ = environ_ptrs;
    return;
oserr:
    _Exit(EX_OSERR);
software:
    _Exit(EX_SOFTWARE);
}

void __wasm_call_dtors() {

}

void __wasi_proc_exit(unsigned int exit_code) {

}

__attribute__((__weak__))
int main(int argc, char *argv[], char *envp[]);

// The user's `main` function, expecting arguments.
//
// Note that we make this a weak symbol so that it will have a
// `WASM_SYM_BINDING_WEAK` flag in libc.so, which tells the dynamic linker that
// it need not be defined (e.g. in reactor-style apps with no main function).
// See also the TODO comment on `__main_void` below.
__attribute__((__weak__))
int __main_argc_argv(int argc, char *argv[]) {
  return main(argc, argv, environ);
}

// If the user's `main` function expects arguments, the compiler will rename
// it to `__main_argc_argv`, and this version will get linked in, which
// initializes the argument data and calls `__main_argc_argv`.
//
// TODO: Ideally this function would be defined in a crt*.o file and linked in
// as necessary by the Clang driver.  However, moving it to crt1-command.c
// breaks `--no-gc-sections`, so we'll probably need to create a new file
// (e.g. crt0.o or crtend.o) and teach Clang to use it when needed.
__attribute__((__weak__, nodebug))
int __main_void(void) {
    // Get the sizes of the arrays we'll have to create to copy in the args.
    size_t argv_buf_size;
    size_t argc;
    __lind_args_sizes_get(&argc, &argv_buf_size);

    // Add 1 for the NULL pointer to mark the end, and check for overflow.
    size_t num_ptrs = argc + 1;
    if (num_ptrs == 0) {
        _Exit(EX_SOFTWARE);
    }

    // Allocate memory for storing the argument chars.
    char *argv_buf = malloc(argv_buf_size);
    if (argv_buf == NULL) {
        _Exit(EX_SOFTWARE);
    }

    // Allocate memory for the array of pointers. This uses `calloc` both to
    // handle overflow and to initialize the NULL pointer at the end.
    char **argv = calloc(num_ptrs, sizeof(char *));
    if (argv == NULL) {
        free(argv_buf);
        _Exit(EX_SOFTWARE);
    }

    // Fill the argument chars, and the argv array with pointers into those chars.
    __lind_args_get((unsigned char **)argv, (unsigned char *)argv_buf);

    // Call `__main_argc_argv` with the arguments!
    return __main_argc_argv(argc, argv);
}

// define an unused function pointer
// the reason for this is that function pointer are translated to function index in WebAssembly
// which starts from 1. But in native environment, function pointer are real address so they are
// assumed that it cannot be 1. Hence, some constant value like SIG_IGN will have conflict with function
// indexes in WebAssembly.
// The workaround we are using here is to define an unused function pointer to occupy the function index of 1
// so that normal function pointer could have their index starting from 2.
int __unused_function_pointer() {
    return 42;
}
// explicitly marked this as "used" so it will not be optimized by compiler
void *___dummy_reference __attribute__((used)) = __unused_function_pointer;

int _start() {
    __lind_init_addr_translation(); // iniatilize cageids before anything else executes
    __libc_setup_tls();
    __wasi_init_tp();
    __lind_initialize_environ();
    __ctype_init(); //lind-wasm: init ctypes for isalpha etc.
    #ifdef LIND_DEBUG
    	__lind_debug_import();
    #endif
// Lind-Wasm: conditional compilation to ensure exit() is called before program termination,
// and return __main_void() if NO_ASYNCIFY is defined. exit() depends on asyncify,
// calling it here will force execution of the program to rely on asyncify which currently cannot be enabled for using gdb
#ifdef NO_ASYNCIFY
    return __main_void();
#else
    exit(__main_void());
#endif
}
