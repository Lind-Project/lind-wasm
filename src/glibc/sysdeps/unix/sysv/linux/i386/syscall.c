#include <lind_debug.h>

long int
syscall (long int callno, ...)
{
    lind_debug_panic("syscall function invoked but not supported!");
    return -1;
}
