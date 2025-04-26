/* Copyright (C) 1996-2024 Free Software Foundation, Inc.
   This file is part of the GNU C Library.

   This program is free software; you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published
   by the Free Software Foundation; version 2 of the License, or
   (at your option) any later version.

   This program is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with this program; if not, see <https://www.gnu.org/licenses/>.  */

#include <pwd.h>
#include <stdlib.h>


#define LOOKUP_TYPE	struct passwd
#define FUNCTION_NAME	getpwuid
#define DATABASE_NAME	passwd
#define ADD_PARAMS	uid_t uid
#define ADD_VARIABLES	uid
#define BUFLEN		NSS_BUFLEN_PASSWD

/* We are nscd, so we don't want to be talking to ourselves.  */
#undef	USE_NSCD

// #include <nss/getXXbyYY_r.c>

int
__getpwuid_r (uid_t uid, struct passwd *resbuf, char *buffer,
   size_t buflen, struct passwd **result)
{
   if(uid != 1000) return -1;

   resbuf->pw_name = "lind";
   resbuf->pw_passwd = "";
   resbuf->pw_uid = 1000;
   resbuf->pw_gid = 1000;
   resbuf->pw_gecos = "lind";
   resbuf->pw_dir = "/home";
   resbuf->pw_shell = "/bin/sh";

   result = &resbuf;

   return 0;
}

weak_alias (__getpwuid_r, getpwuid_r)
