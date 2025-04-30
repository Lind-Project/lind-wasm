/* Copyright (C) 1996-2024 Free Software Foundation, Inc.
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

#include <pwd.h>


#define LOOKUP_TYPE	struct passwd
#define FUNCTION_NAME	getpwuid
#define DATABASE_NAME	passwd
#define ADD_PARAMS	uid_t uid
#define ADD_VARIABLES	uid
#define BUFLEN		NSS_BUFLEN_PASSWD

// TODO: normal getpwuid routine is not working correctly for some reason
// currently hardcoded the value but we should fix this in the future
// #include "../nss/getXXbyYY.c"

struct passwd *
getpwuid (uid_t uid)
{
   if(uid != 1000) return NULL;

   struct passwd *res = malloc(sizeof(struct passwd));
   res->pw_name = "lind";
   res->pw_passwd = "";
   res->pw_uid = 1000;
   res->pw_gid = 1000;
   res->pw_gecos = "lind";
   res->pw_dir = "/home";
   res->pw_shell = "/bin/sh";
   return res;
}
