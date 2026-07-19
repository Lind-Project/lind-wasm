# Makefile fragment for lind (wasm32): register the extra fenv storage
# object.  The fenv routines themselves (fclrexcpt, fraiseexcpt, ...)
# reuse the basenames already listed in math/Makefile's libm-support, so
# they are picked up automatically by the normal sysdeps override
# resolution and need no registration here.

ifeq ($(subdir),math)
libm-support += fenv_state
endif
