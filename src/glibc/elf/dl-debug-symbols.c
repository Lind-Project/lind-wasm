/* Define symbols used to communicate dynamic linker state to the
   debugger at runtime.
   Copyright (C) 2021-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   The GNU C Library is free software; you can redistribute it and/or
   modify it under the terms of the GNU Lesser General Public
   License as published by the Free Software Foundation; either
   version 2.1 of the License, or (at your option) any later version.

   The GNU C Library is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
   Lesser General Public License for more details.

   You should have received a copy of the GNU Lesser General Public
   License along with the GNU C Library; if not, see
   <https://www.gnu.org/licenses/>.  */

#include <rtld-sizes.h>
#include <sysdep.h>

/* Define 2 symbols, _r_debug_extended and _r_debug, which is an alias
   of _r_debug_extended, but with the size of struct r_debug.  */

struct r_debug {
    // Add appropriate members based on actual struct definition
};

struct r_debug_extended {
    // Add appropriate members based on actual struct definition
};

// Ensure alignment
__attribute__((aligned(R_DEBUG_EXTENDED_ALIGN)))
char _r_debug_extended[R_DEBUG_EXTENDED_SIZE];

// Alias _r_debug to _r_debug_extended
extern char _r_debug[R_DEBUG_SIZE] __attribute__((alias("_r_debug_extended")));

// Ensure visibility
__attribute__((visibility("hidden")))
extern char _r_debug_extended[R_DEBUG_EXTENDED_SIZE];

