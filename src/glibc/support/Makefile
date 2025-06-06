# Makefile for support library, used only at build and test time
# Copyright (C) 2016-2024 Free Software Foundation, Inc.
# This file is part of the GNU C Library.

# The GNU C Library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.

# The GNU C Library is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
# Lesser General Public License for more details.

# You should have received a copy of the GNU Lesser General Public
# License along with the GNU C Library; if not, see
# <https://www.gnu.org/licenses/>.

subdir := support

include ../Makeconfig

extra-libs := libsupport
extra-libs-others = $(extra-libs)
extra-libs-noinstall := $(extra-libs)

libsupport-routines = \
  blob_repeat \
  check \
  check_addrinfo \
  check_dns_packet \
  check_hostent \
  check_netent \
  delayed_exit \
  dtotimespec \
  dtotimespec-time64 \
  ignore_stderr \
  next_to_fault \
  oom_error \
  resolv_response_context_duplicate \
  resolv_response_context_free \
  resolv_test \
  set_fortify_handler \
  support-open-dev-null-range \
  support-xfstat \
  support-xfstat-time64 \
  support-xstat \
  support-xstat-time64 \
  support_become_root \
  support_can_chroot \
  support_capture_subprocess \
  support_capture_subprocess_check \
  support_chroot \
  support_copy_file \
  support_copy_file_range \
  support_create_timer \
  support_descriptor_supports_holes \
  support_descriptors \
  support_enter_mount_namespace \
  support_enter_network_namespace \
  support_format_address_family \
  support_format_addrinfo \
  support_format_dns_packet \
  support_format_herrno \
  support_format_hostent \
  support_format_netent \
  support_isolate_in_subprocess \
  support_mutex_pi_monotonic \
  support_need_proc \
  support_openpty \
  support_path_support_time64 \
  support_paths \
  support_process_state \
  support_ptrace \
  support_quote_blob \
  support_quote_blob_wide \
  support_quote_string \
  support_record_failure \
  support_run_diff \
  support_select_modifies_timeout \
  support_select_normalizes_timeout \
  support_set_small_thread_stack_size \
  support_set_vma_name_supported \
  support_shared_allocate \
  support_small_stack_thread_attribute \
  support_socket_so_timestamp_time64 \
  support_stack_alloc \
  support_stat_nanoseconds \
  support_subprocess \
  support_test_compare_blob \
  support_test_compare_failure \
  support_test_compare_string \
  support_test_compare_string_wide \
  support_test_main \
  support_test_verify_impl \
  support_wait_for_thread_exit \
  support_write_file_string \
  temp_file \
  timespec \
  timespec-add \
  timespec-add-time64 \
  timespec-sub \
  timespec-sub-time64 \
  timespec-time64 \
  write_message \
  xaccept \
  xaccept4 \
  xasprintf \
  xbind \
  xcalloc \
  xchdir \
  xchmod \
  xchroot \
  xclock_gettime \
  xclock_gettime_time64 \
  xclock_settime \
  xclock_settime_time64 \
  xclone \
  xclose \
  xconnect \
  xcopy_file_range \
  xdlfcn \
  xdlmopen \
  xdup2 \
  xfchmod \
  xfclose \
  xfgets \
  xfopen \
  xfork \
  xfread \
  xfreopen \
  xftruncate \
  xgetline \
  xgetsockname \
  xlisten \
  xlseek \
  xlstat \
  xlstat-time64 \
  xmalloc \
  xmemstream \
  xmkdir \
  xmkdirp \
  xmkfifo \
  xmmap \
  xmprotect \
  xmunmap \
  xnewlocale \
  xopen \
  xpipe \
  xpoll \
  xposix_memalign \
  xposix_spawn \
  xposix_spawn_file_actions_addclose \
  xposix_spawn_file_actions_adddup2 \
  xpthread_attr_destroy \
  xpthread_attr_init \
  xpthread_attr_setaffinity_np \
  xpthread_attr_setdetachstate \
  xpthread_attr_setguardsize \
  xpthread_attr_setstack \
  xpthread_attr_setstacksize \
  xpthread_barrier_destroy \
  xpthread_barrier_init \
  xpthread_barrier_wait \
  xpthread_barrierattr_destroy \
  xpthread_barrierattr_init \
  xpthread_barrierattr_setpshared \
  xpthread_cancel \
  xpthread_check_return \
  xpthread_cond_signal \
  xpthread_cond_wait \
  xpthread_create \
  xpthread_detach \
  xpthread_join \
  xpthread_key_create \
  xpthread_key_delete \
  xpthread_kill \
  xpthread_mutex_consistent \
  xpthread_mutex_destroy \
  xpthread_mutex_init \
  xpthread_mutex_lock \
  xpthread_mutex_unlock \
  xpthread_mutexattr_destroy \
  xpthread_mutexattr_init \
  xpthread_mutexattr_setprotocol \
  xpthread_mutexattr_setpshared \
  xpthread_mutexattr_setrobust \
  xpthread_mutexattr_settype \
  xpthread_once \
  xpthread_rwlock_destroy \
  xpthread_rwlock_init \
  xpthread_rwlock_rdlock \
  xpthread_rwlock_unlock \
  xpthread_rwlock_wrlock \
  xpthread_rwlockattr_init \
  xpthread_rwlockattr_setkind_np \
  xpthread_sigmask \
  xpthread_spin_lock \
  xpthread_spin_unlock \
  xraise \
  xread \
  xreadlink \
  xrealloc \
  xrecvfrom \
  xsendto \
  xsetlocale \
  xsetsockopt \
  xsigaction \
  xsignal \
  xsigstack \
  xsocket \
  xstrdup \
  xstrndup \
  xsymlink \
  xsysconf \
  xsystem \
  xunlink \
  xuselocale \
  xwaitpid \
  xwrite \
  # libsupport-routines

libsupport-static-only-routines := $(libsupport-routines)
# Only build one variant of the library.
libsupport-inhibit-o := .os
ifeq ($(build-shared),yes)
libsupport-inhibit-o += .o
endif

CFLAGS-support_paths.c = \
		-DSRCDIR_PATH=\"`cd .. ; pwd`\" \
		-DOBJDIR_PATH=\"`cd $(objpfx)/..; pwd`\" \
		-DOBJDIR_ELF_LDSO_PATH=\"`cd $(objpfx)/..; pwd`/elf/$(rtld-installed-name)\" \
		-DINSTDIR_PATH=\"$(prefix)\" \
		-DLIBDIR_PATH=\"$(libdir)\" \
		-DBINDIR_PATH=\"$(bindir)\" \
		-DSBINDIR_PATH=\"$(sbindir)\" \
		-DSLIBDIR_PATH=\"$(slibdir)\" \
		-DROOTSBINDIR_PATH=\"$(rootsbindir)\" \
		-DCOMPLOCALEDIR_PATH=\"$(complocaledir)\" \
		-DSYSCONFDIR_PATH=\"$(sysconfdir)\"

# Build with exception handling and asynchronous unwind table support.
CFLAGS-.oS += -fexceptions -fasynchronous-unwind-tables

# In support_timespec_check_in_range we may be passed a very tight
# range for which we should produce a correct result for expected
# being within the observed range.  The code uses double internally
# in support_timespec_check_in_range and for that computation we use
# -fexcess-precision=standard.
CFLAGS-timespec.c += $(config-cflags-fexcess-precision-standard)
CFLAGS-timespec-time64.c += $(config-cflags-fexcess-precision-standard)

# Ensure that general support files use 64-bit time_t
CFLAGS-delayed_exit.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-shell-container.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_can_chroot.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_copy_file.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_copy_file_range.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_descriptor_supports_holes.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_descriptors.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_process_state.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_stat_nanoseconds.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_subprocess.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-support_test_main.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-test-container.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
CFLAGS-xmkdirp.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64
# This is required to get an mkstemp which can create large files on some
# 32-bit platforms.
CFLAGS-temp_file.c += -D_FILE_OFFSET_BITS=64 -D_TIME_BITS=64

ifeq (,$(CXX))
LINKS_DSO_PROGRAM = links-dso-program-c
else
LINKS_DSO_PROGRAM = links-dso-program
LDLIBS-links-dso-program = -lstdc++ -lgcc -lgcc_s $(libunwind)
endif

ifeq (yes,$(have-selinux))
LDLIBS-$(LINKS_DSO_PROGRAM) += -lselinux
endif


LDLIBS-test-container = $(libsupport)

others += test-container
others-noinstall += test-container

others += \
  echo-container \
  shell-container \
  true-container \
  # others
others-noinstall += \
  echo-container \
  shell-container \
  true-container \
  # others-noinstall

others += $(LINKS_DSO_PROGRAM)
others-noinstall += $(LINKS_DSO_PROGRAM)

others += test-run-command
others-static += test-run-command
others-noinstall += test-run-command
LDLIBS-test-run-command = $(libsupport)

$(objpfx)test-container : $(libsupport)
$(objpfx)shell-container : $(libsupport)
$(objpfx)echo-container : $(libsupport)
$(objpfx)true-container : $(libsupport)
$(objpfx)test-run-command : $(libsupport) $(common-objpfx)elf/static-stubs.o

tests = \
  README-testing \
  tst-support-namespace \
  tst-support-open-dev-null-range \
  tst-support-process_state \
  tst-support_blob_repeat \
  tst-support_capture_subprocess \
  tst-support_descriptors \
  tst-support_format_dns_packet \
  tst-support_quote_blob \
  tst-support_quote_blob_wide \
  tst-support_quote_string \
  tst-support_record_failure \
  tst-test_compare \
  tst-test_compare_blob \
  tst-test_compare_string \
  tst-test_compare_string_wide \
  tst-timespec \
  tst-xreadlink \
  tst-xsigstack \
  # tests

ifeq ($(run-built-tests),yes)
tests-special = \
  $(objpfx)tst-support_record_failure-2.out

$(objpfx)tst-support_record_failure-2.out: tst-support_record_failure-2.sh \
  $(objpfx)tst-support_record_failure
	$(SHELL) $< $(common-objpfx) '$(test-program-prefix-before-env)' \
	  '$(run-program-env)' '$(test-program-prefix-after-env)' \
	  > $@; \
	$(evaluate-test)
endif

tests-special += $(objpfx)tst-glibcpp.out

$(objpfx)tst-glibcpp.out: tst-glibcpp.py $(..)scripts/glibcpp.py
	PYTHONPATH=$(..)scripts $(PYTHON) tst-glibcpp.py > $@ 2>&1; \
	$(evaluate-test)

$(objpfx)tst-support_format_dns_packet: $(common-objpfx)resolv/libresolv.so

tst-support_capture_subprocess-ARGS = -- $(host-test-program-cmd)

include ../Rules
