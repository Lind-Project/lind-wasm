int
__vfork (void)
{
  return 0;
}
weak_alias (__vfork, vfork) strong_alias (__vfork, __libc_vfork)
