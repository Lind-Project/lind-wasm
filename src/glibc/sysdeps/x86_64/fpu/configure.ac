GLIBC_PROVIDES dnl See aclocal.m4 in the top level source directory.

AC_CACHE_CHECK([whether compiler supports libmvec calls through fopenmp],
	       libc_cv_libmvec_pragma_omp_simd_support, [
dnl
cat > conftest.c <<EOF
__attribute__ ((__simd__)) float sinf (float);
void
foo (float *x)
{
  #pragma omp simd
  for (int i = 0; i < 256; i++)
    x[[i]] = sinf (x[[i]]);
}
EOF
libc_cv_libmvec_pragma_omp_simd_support=no
# Check regardless of the ABI used
if AC_TRY_COMMAND([${CC-cc} $CFLAGS -fno-inline -fopenmp -Wno-unknown-pragmas -S conftest.c -o conftest.s 1>&AS_MESSAGE_LOG_FD])
  then
  if AC_TRY_COMMAND([grep '_ZGVbN4v_sinf' conftest.s >/dev/null]) \
     || AC_TRY_COMMAND([grep '_ZGVcN8v_sinf' conftest.s >/dev/null]) \
     || AC_TRY_COMMAND([grep '_ZGVdN8v_sinf' conftest.s >/dev/null]) \
     || AC_TRY_COMMAND([grep '_ZGVeN16v_sinf' conftest.s >/dev/null])
  then
    libc_cv_libmvec_pragma_omp_simd_support=yes
  fi
fi
rm -f conftest*])
LIBC_CONFIG_VAR([build-mathvec-pragma-simd-omp],
		[$libc_cv_libmvec_pragma_omp_simd_support])
