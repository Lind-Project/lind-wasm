//! MPK handling of the clone, exec, and exit syscalls.

use super::execute::{exec_mpk_internal, GuestPtrArray};
use super::lind_if::MpkRuntime;
use super::syscalls::{mpk_clone_syscall_entry, mpk_exit_syscall_entry};
use crate::lifecycle::SyscallRuntime;
use libc::c_char;
use typemap::datatype_conversion::sc_convert_ptr_to_host;

impl SyscallRuntime for MpkRuntime {
    fn handle_clone(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32 {
        mpk_clone_syscall_entry(
            cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        )
    }

    // arg1 = path, arg1_cageid = execing cage; arg2/arg3 = argv/envp with
    // their owning cages.
    fn handle_exec(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32 {
        let _ = (cageid, arg4, arg4_cageid, arg5, arg5_cageid, arg6, arg6_cageid);

        // The pointers (and the string pointers in argv/envp) are guest
        // addresses of the cage that owns them; in a gs-forked cage they are
        // shifted by the cage's logical offset.
        let path_ptr = sc_convert_ptr_to_host(arg1, arg1_cageid) as *const c_char;
        let argv = GuestPtrArray {
            ptr: sc_convert_ptr_to_host(arg2, arg2_cageid) as *const u64,
            cageid: arg2_cageid,
        };
        let envp = GuestPtrArray {
            ptr: sc_convert_ptr_to_host(arg3, arg3_cageid) as *const u64,
            cageid: arg3_cageid,
        };

        //TODO: handle argument gathering from different cages
        //cageid is the rawposix cageid
        match exec_mpk_internal(arg1_cageid, path_ptr, argv, envp) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("[lind-mpk] exec failed: {}", e);
                -1
            }
        }
    }

    fn handle_exit(
        &self,
        cageid: u64,
        arg1: u64,
        arg1_cageid: u64,
        arg2: u64,
        arg2_cageid: u64,
        arg3: u64,
        arg3_cageid: u64,
        arg4: u64,
        arg4_cageid: u64,
        arg5: u64,
        arg5_cageid: u64,
        arg6: u64,
        arg6_cageid: u64,
    ) -> i32 {
        mpk_exit_syscall_entry(
            cageid,
            arg1,
            arg1_cageid,
            arg2,
            arg2_cageid,
            arg3,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        )
    }
}
