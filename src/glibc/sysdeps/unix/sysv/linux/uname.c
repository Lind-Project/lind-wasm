#include <unistd.h>
#include <sysdep-cancel.h>

#include <sys/utsname.h>

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
#ifdef	ENAMETOOLONG
      else if (errno == ENAMETOOLONG)
	/* The name was truncated.  */
	__set_errno (save);
#endif
      else
	return -1;
    }
  strncpy (name->sysname, "Linux", sizeof (name->sysname));
  strncpy (name->release, "unknown", sizeof (name->release));
  strncpy (name->version, "unknown", sizeof (name->version));
  strncpy (name->machine, "x86_64", sizeof (name->machine));

  return 0;
}

weak_alias(__GI___uname, __uname)
weak_alias(__GI___uname, __GI_uname)
weak_alias(__GI___uname, uname)
