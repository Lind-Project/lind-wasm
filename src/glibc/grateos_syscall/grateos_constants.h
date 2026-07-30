/*
 * grateos_constants.h
 *
 * Named constants for the GrateOS syscall layer.
 */

#ifndef _GRATEOS_CONSTANTS_H
#define _GRATEOS_CONSTANTS_H

/* Define NOTUSED for unused arguments */
#define NOTUSED 0xdeadbeefdeadbeefULL

/* Define flags for errno translation
 * See comments in grateos_syscall/grateos_syscall.c for details */
#define TRANSLATE_ERRNO_ON  1
#define TRANSLATE_ERRNO_OFF 0

/* Upper bound (exclusive) of valid errno values.
 * Return values in the range (-MAX_ERRNO, 0) are treated as -errno
 * by make_threei_call() when TRANSLATE_ERRNO_ON is active. */
#define MAX_ERRNO 256

#endif /* _GRATEOS_CONSTANTS_H */
