#include <unistd.h>
#include <sysdep-cancel.h>
#include <sys/utsname.h>

/* Hardcoded uname values for Lind-Wasm environment
   TODO: These should eventually be retrieved from the runtime environment */
#define UNAME_SYSNAME "Linux"
#define UNAME_RELEASE "unknown"
#define UNAME_VERSION "unknown"
#define UNAME_MACHINE "x86_64"

int
__GI___uname (struct utsname *name)
{
  // lind-wasm: copied from posix/uname.c
  int save;

  if (name == NULL)
    {
      __set_errno (EINVAL);
      return -1;
    }

  save = errno;
  if (__gethostname (name->nodename, sizeof (name->nodename)) < 0)
    {
      if (errno == ENOSYS)
	{
	  /* Hostname is meaningless for this machine.  */
	  name->nodename[0] = '\0';
	  __set_errno (save);
	}
#ifdef ENAMETOOLONG
      else if (errno == ENAMETOOLONG)
	/* The name was truncated.  */
	__set_errno (save);
#endif
      else
	return -1;
    }
  strncpy (name->sysname, UNAME_SYSNAME, sizeof (name->sysname));
  strncpy (name->release, UNAME_RELEASE, sizeof (name->release));
  strncpy (name->version, UNAME_VERSION, sizeof (name->version));
  strncpy (name->machine, UNAME_MACHINE, sizeof (name->machine));
}

weak_alias (__GI___uname, __uname) weak_alias (__GI___uname, __GI_uname)
    weak_alias (__GI___uname, uname)
