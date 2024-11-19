#include <unistd.h>
#include <sys/syscall.h>
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdint.h> // For uint64_t definition

// Define NOTUSED for unused arguments
#define NOTUSED 0xdeadbeefdeadbeefULL

// Macro for making a system call with up to six arguments
// The callname parameter should be provided in the form "syscall|callname"
#define MAKE_SYSCALL(syscallnum, arg1, arg2, arg3, arg4, arg5, arg6) \
    syscall(syscallnum, (uint64_t)(arg1), (uint64_t)(arg2), (uint64_t)(arg3), \
                 (uint64_t)(arg4), (uint64_t)(arg5), (uint64_t)(arg6))

// // Generic syscall function supporting up to 6 arguments
// long make_syscall(int syscall_number, const char *callname, int num_args, ...) {
//     va_list args;
//     va_start(args, num_args);
//     uint64_t sys_args[6] = {0};  // Array to hold up to 6 syscall arguments
    
//     // Populate the sys_args array with the provided arguments
//     for (int i = 0; i < num_args && i < 6; i++) {
//         sys_args[i] = va_arg(args, uint64_t);
//         if (sys_args[i] == NOTUSED) {
//             sys_args[i] = 0; // We can handle differently if needed
//         }
//     }
//     va_end(args);

//     // log the syscall being made
//     printf("Making syscall: %s (number %d)\n", callname, syscall_number);

//     // Call the syscall function with unpacked arguments
//     long result = syscall(syscall_number, sys_args[0], sys_args[1], sys_args[2],
//                           sys_args[3], sys_args[4], sys_args[5]);

//     if (result < 0) {
//         errno = -result;
//         return -1;
//     }

//     return result;
// }

