#include <grateos_debug.h>

long int
syscall (long int callno, ...)
{
    grateos_debug_panic("syscall function invoked but not supported!");
    return -1;
}
