use crate::lind_mpk::RuntimeInfo::{
    MPKRuntimeInfo, MPKSupervisorContext, MPKSupervisorCtxStack, MpkCageThreadInfo,
    LIND_MPK_MAX_CONTEXTS, CONTEXT_PAGE_SIZE, GUEST_TCB_SIZE,
    current_ctx_stack, current_context_stack,
};
use crate::lind_mpk::gs_fork;
use cage::get_cage;
use std::arch::naked_asm;
use std::mem::{offset_of, size_of};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{THREEI_CAGEID, UNUSED_ARG, UNUSED_ID};
use threei::threei_const::{REGISTER_HANDLER_SYSCALL, RUNTIME_TYPE_MPK};

const GS_CURRENT_CONTEXT_OFFSET: usize = offset_of!(MPKSupervisorCtxStack, current_context);
pub const GS_OS_TID_OFFSET: usize = offset_of!(MPKSupervisorCtxStack, os_tid);
const GS_CONTEXT_ARRAY_OFFSET: usize = offset_of!(MPKSupervisorCtxStack, contexts);
const GS_CONTEXT_FS_OFFSET: usize = GS_CONTEXT_ARRAY_OFFSET + offset_of!(MPKSupervisorContext, fs_base);
const GS_CONTEXT_PKRU_OFFSET: usize = GS_CONTEXT_ARRAY_OFFSET + offset_of!(MPKSupervisorContext, pkru_guest_fs_slot);
const GS_CONTEXT_SIZE: usize = size_of::<MPKSupervisorContext>();
const GS_CONTEXT_NEXT_SUP_RSP_OFFSET: usize = GS_CONTEXT_ARRAY_OFFSET + 2*GS_CONTEXT_SIZE;
const GS_CONTEXT_NEXT_SUP_FS_OFFSET: usize = GS_CONTEXT_ARRAY_OFFSET + 2*GS_CONTEXT_SIZE + offset_of!(MPKSupervisorContext, fs_base);

fn mpk_debug(message: impl AsRef<str>) {
    if std::env::var_os("LIND_MPK_DEBUG").is_some() {
        eprintln!("[lind-mpk] {}", message.as_ref());
    }
}

pub fn register_mpk_handler_for_cage(cageid: u64) -> anyhow::Result<()> {
    
    let result = threei::register_handler(
        UNUSED_ID,
        THREEI_CAGEID,
        cageid,
        REGISTER_HANDLER_SYSCALL,
        RUNTIME_TYPE_MPK,
        THREEI_CAGEID,
        mpk_register_handler as *const () as u64,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
        UNUSED_ARG,
        UNUSED_ID,
    );
    if result != 0 {
        anyhow::bail!(
            "registering MPK register_handler for cage {} failed: {}",
            cageid,
            result
        );
    }
    Ok(())
}

extern "C" fn mpk_register_handler(
    _self_cageid: u64,
    _dispatch_target_cageid: u64,
    target_cageid: u64,
    target_call_num: u64,
    runtime_id: u64,
    handle_func_cage: u64,
    handler_ptr: u64,
    arg3_cageid: u64,
    arg4: u64,
    arg4_cageid: u64,
    arg5: u64,
    arg5_cageid: u64,
    arg6: u64,
    arg6_cageid: u64,
) -> i64 {
    mpk_debug(format!(
        "register_handler: target_cage={}, target_call={}, grate_cage={}, runtime_id={}, handler={:#x}",
        target_cageid, target_call_num, handle_func_cage, runtime_id, handler_ptr
    ));
    let result = (|| -> anyhow::Result<i32> {
        let cage = get_cage(target_cageid)
            .ok_or_else(|| anyhow::anyhow!("cage {} not found", target_cageid))?;
        let cage_runtime_info = cage.runtime_info.read();
        let cage_mpk_info = cage_runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .ok_or_else(|| anyhow::anyhow!("cage {} is not using MPK", target_cageid))?;
        let mut cage_threads = cage_mpk_info.threads.write();
        
        let grate = get_cage(handle_func_cage)
            .ok_or_else(|| anyhow::anyhow!("cage {} not found", handle_func_cage))?;
        let grate_runtime_info = grate.runtime_info.read();
        let grate_mpk_info = grate_runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .ok_or_else(|| anyhow::anyhow!("cage {} is not using MPK", handle_func_cage))?;
        let mut grate_threads = grate_mpk_info.threads.write();

        mpk_debug(format!(
            "register_handler: target threads={}, existing grate threads={}",
            cage_threads.len(), grate_threads.len()
        ));

        // Give each target-cage OS thread a MpkCageThreadInfo owned by this GRATE
        // registration. The supervisor state remains attached to the same OS thread;
        // only the grate stack belongs to this registration. If this thread already
        // has an entry in the grate (e.g. a prior register_handler call for the same
        // cage/grate pair, or the thread was registered by another handler), reuse it
        // instead of clobbering its already-allocated grate stack.
        let mut added = 0usize;
        for (tid, thread) in cage_threads.iter() {
            if grate_threads.contains_key(tid) {
                continue;
            }
            let mut new_thread = MpkCageThreadInfo {
                thread_info: Arc::clone(&thread.thread_info),
                grate_cage_id: handle_func_cage,
                stack_addr: std::sync::atomic::AtomicUsize::new(0),
                stack_base: 0,
                stack_size: 0,
                fs_base: 0,
                dtv: std::ptr::null_mut(),
                dtv_size: 0,
            };
            new_thread.allocate_grate_stack()?;
            new_thread.initialize_grate_tls(grate_mpk_info)?;
            new_thread.thread_info.register_cage(handle_func_cage);
            grate_threads.insert(*tid, new_thread);
            added += 1;
        }
        mpk_debug(format!(
            "register_handler: adding {} grate thread entries to cage {}",
            added, handle_func_cage
        ));

        let registration_result = threei::register_handler(
            UNUSED_ID,
            THREEI_CAGEID,
            target_cageid,
            target_call_num,
            runtime_id,
            handle_func_cage,
            handler_ptr,
            arg3_cageid,
            arg4,
            arg4_cageid,
            arg5,
            arg5_cageid,
            arg6,
            arg6_cageid,
        );
        mpk_debug(format!(
            "register_handler: threei result={} target_cage={} grate_cage={}",
            registration_result, target_cageid, handle_func_cage
        ));
        Ok(registration_result)
    })();

    match result {
        Ok(ret) => ret as i64,
        Err(error) => {
            eprintln!("[lind-mpk] register_handler failed: {}", error);
            -(libc::EINVAL as i64)
        }
    }
}

///Note: This currently does not support circular invocation (reentry) on the grate stack.
/// There is no mechanism to maintain a safe stack position for entering a grate G if G is already on the call chain.
/// However, implementing this would only require keeping the latest stack position for the current thread for each grate
pub extern "C" fn grate_callback_trampoline(
    in_grate_fn_ptr_u64: u64,
    cageid: u64,
    arg1: u64,
    arg1cageid: u64,
    arg2: u64,
    arg2cageid: u64,
    arg3: u64,
    arg3cageid: u64,
    arg4: u64,
    arg4cageid: u64,
    arg5: u64,
    arg5cageid: u64,
    arg6: u64,
    arg6cageid: u64,
) -> i64 {
    let gs_data = current_context_stack();
    let current_context_index = unsafe { (*gs_data).current_context };
    assert!(
        current_context_index < LIND_MPK_MAX_CONTEXTS - 2,
        "inner_grate_callback_trampoline: supervisor context stack is full"
    );
    let switch_to_context_index = current_context_index + 1;
    let os_tid = unsafe { (*gs_data).os_tid as libc::pid_t };
    let (grate_stack, grate_fs_base, thread_gs_data, grate_pkru) = {
        let cage = get_cage(cageid).expect("inner_grate_callback_trampoline: cage not found");
        let runtime_info = cage.runtime_info.read();
        let mpk_info = runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .expect("inner_grate_callback_trampoline: cage is not using MPK");
        let threads = mpk_info.threads.read();
        let cage_thread = threads
            .get(&os_tid)
            .expect("inner_grate_callback_trampoline: thread is not registered in cage");
        (
            cage_thread.stack_addr.load(std::sync::atomic::Ordering::Relaxed),
            cage_thread.fs_base,
            cage_thread.thread_info.gs_data,
            mpk_info.pkru.load(std::sync::atomic::Ordering::Relaxed),
        )
    };
    assert!(!thread_gs_data.is_null(), "inner_grate_callback_trampoline: null GS context");
    assert_eq!(
        thread_gs_data,
        gs_data,
        "inner_grate_callback_trampoline: thread context does not match current_ctx_stack"
    );
    current_ctx_stack.set(thread_gs_data);

    //determine next stack. 
    //TODO: in case of reentry, a new frame on the grate stack needs to be used.
    let next_stack = grate_stack;
    assert!(next_stack != 0, "inner_grate_callback_trampoline: grate stack is not initialized");
    unsafe {
        (*gs_data).contexts[switch_to_context_index].rsp = (next_stack & !0xf) as u64;
        (*gs_data).contexts[switch_to_context_index].cage_id = cageid;
        (*gs_data).contexts[switch_to_context_index].fs_base = grate_fs_base;
        (*gs_data).contexts[switch_to_context_index].pkru_guest_fs_slot.pkru = grate_pkru as u64;
    }

    let return_value = gs_fork::call_with_cage_gs(cageid, || unsafe { inner_grate_callback_trampoline_asm(
        cageid,
        arg1,
        arg1cageid,
        arg2,
        arg2cageid,
        arg3,
        arg3cageid,
        arg4,
        arg4cageid,
        arg5,
        arg5cageid,
        arg6,
        arg6cageid,
        in_grate_fn_ptr_u64,
        thread_gs_data,
    ) });
    
    return_value
}

#[unsafe(naked)]
unsafe extern "sysv64" fn inner_grate_callback_trampoline_asm(
    cageid: u64,
    arg1: u64,
    arg1cageid: u64,
    arg2: u64,
    arg2cageid: u64,
    arg3: u64,
    arg3cageid: u64,
    arg4: u64,
    arg4cageid: u64,
    arg5: u64,
    arg5cageid: u64,
    arg6: u64,
    arg6cageid: u64,
    in_grate_fn_ptr_u64: u64,
    ctx_stack: *mut MPKSupervisorCtxStack,
) -> i64 {
    unsafe {
        naked_asm!(

        
    //here, our stack is 8-byte aligned, it needs to be 16-byte aligned before making a call!
    // Nine stack arguments: seven guest arguments, the call target, and the context stack.
    
    
    //get context stack pointer and grate stack
    //switch to threads' grate stack, old rsp in r11
    "movq %rsp, %r11; ",
    "pushq %r12; ",
    "movq 0x48(%r11), %r12; ",
    
    //spill current rsp to current_context.rsp
    "movq {gs_current_context_offset}(%r12), %r10 ;",
    "imulq ${gs_contexts_size_value}, %r10;" ,
    "movq %rsp, {gs_contexts_array_offset}(%r12, %r10);",
    "movq %rsp, {gs_context_next_sup_rsp_offset}(%r12, %r10);", //use current supervisor stack as grate's supervisor stack
    
    //spill current fs_base
    "rdfsbase %rax; ",
    "movq %rax, {gs_context_fs_offset}(%r12, %r10);",
    "movq %rax, {gs_context_next_sup_fs_offset}(%r12, %r10);", //use current supervisor fs_base as grate's supervisor fs_base
    
    
    //get grate fs-base
    "addq ${gs_contexts_size_value}, %r10; ", //r10 now holds the index of the grate context * context_size
    "movq {gs_context_fs_offset}(%r12, %r10), %rax; ",
    "wrfsbase %rax; ",
    
    //get grate stack
    "movq {gs_contexts_array_offset}(%r12, %r10), %rsp ;",


    "subq $0x58, %rsp; ", // room for 8 byte alignment (0x8) + 7 args (0x38) + 3 scratch regs (0x18) 
    "movq %rcx, 0x0(%rsp); ",
    "movq %rsi, 0x8(%rsp); ",
    "movq %rdi, 0x10(%rsp); ",
    
    "leaq 0x8(%r11), %rsi; ", //first arg on sup stack
    "leaq 0x18(%rsp), %rdi; ", //first arg on grate stack
    "movq $7, %rcx; ", //number of qwords to copy
    "rep movsq; ", //copy 7 arguments from sup stack to grate stack

    //after rep mov, rsi points to last arg, fptr    
    "movq (%rsi), %r11; ",
    "popq %rcx; ",
    "popq %rsi; ",
    "popq %rdi; ",
    
    //increment the current context index
    "incq {gs_current_context_offset}(%r12); ",
    
    //switch pkru, get as offset from gs:r10

    // Switch to the callee's PKRU (contexts[i+1].pkru, r10 still holds its
    // offset). wrpkru needs ecx = edx = 0, so park the argument registers in
    // r10 and r12 (r12 is recomputed after the call).
    "movl {gs_context_pkru_offset}(%r12, %r10), %eax; ",
    "movq %rcx, %r10; ",
    "movq %rdx, %r12; ",
    "xorl %ecx, %ecx; ",
    "xorl %edx, %edx; ",
    "wrpkru; ",
    "movq %r10, %rcx; ",
    "movq %r12, %rdx; ",
    //call fptr
    "call *%r11; ",

    // Back to the supervisor PKRU before touching supervisor memory.
    "movq %rax, %r10; ",
    "xorl %eax, %eax; ",
    "xorl %ecx, %ecx; ",
    "xorl %edx, %edx; ",
    "wrpkru; ",
    "movq %r10, %rax; ",

    // Do not trust the saved r12 here: in a clone child it still names the
    // parent's context stack. The grate returns on its own FS, whose next
    // page after the TCB is this thread's context stack.
    "rdfsbase %r12; ",
    "addq {guest_tcb_size}(%rip), %r12; ",
    "addq ${context_page_size} - 1, %r12; ",
    "andq $-{context_page_size}, %r12; ",

    //decrement the current context index
    "decq {gs_current_context_offset}(%r12); ",

    //switch back to supervisor stack
    "movq {gs_current_context_offset}(%r12), %r10 ;",
    "imulq ${gs_contexts_size_value}, %r10;" ,
    "movq {gs_contexts_array_offset}(%r12, %r10), %rsp ;",

    //switch back to supervisor fs_base
    "movq {gs_context_fs_offset}(%r12, %r10), %r11; ",
    "wrfsbase %r11; ",
    "popq %r12; ",
    
    "ret; "
        ,
        gs_current_context_offset = const GS_CURRENT_CONTEXT_OFFSET,
        gs_contexts_array_offset = const GS_CONTEXT_ARRAY_OFFSET,
        gs_context_fs_offset = const GS_CONTEXT_FS_OFFSET,
        gs_contexts_size_value = const GS_CONTEXT_SIZE,
        gs_context_next_sup_rsp_offset = const GS_CONTEXT_NEXT_SUP_RSP_OFFSET,
        gs_context_next_sup_fs_offset = const GS_CONTEXT_NEXT_SUP_FS_OFFSET,
        gs_context_pkru_offset = const GS_CONTEXT_PKRU_OFFSET,
        guest_tcb_size = sym GUEST_TCB_SIZE,
        context_page_size = const CONTEXT_PAGE_SIZE,
        options(att_syntax));
    }
    
    //End ASM
}

#[unsafe(naked)]
pub(super) unsafe extern "sysv64" fn _call_into_cage_asm(
    arg1: u64, //rdi
    arg2: u64, //rsi
    arg3: u64, //rdx
    arg4: u64, //rcx
    arg5: u64, //r8
    in_grate_fn_ptr_u64: u64, //r9
    ctx_stack: *mut MPKSupervisorCtxStack,
) {
    unsafe {
        naked_asm!(

        
    //here, our stack is 8-byte aligned, it needs to be 16-byte aligned before making a call!
    //r9 hols our call target (in_grate_fn_ptr_u64)
    
    
    
    //get context stack pointer and grate stack
    //spill current rsp to current_context.rsp
    "pushq %r12; ",
    "movq 0x10(%rsp), %r12; ",
    "movq {gs_current_context_offset}(%r12), %r10 ;",
    "imulq ${gs_contexts_size_value}, %r10;" ,
    "movq %rsp, {gs_contexts_array_offset}(%r12, %r10);",
    "movq %rsp, {gs_context_next_sup_rsp_offset}(%r12, %r10);", //use current supervisor stack as grate's supervisor stack
    
    //spill current fs_base
    "rdfsbase %rax; ",
    "movq %rax, {gs_context_fs_offset}(%r12, %r10);",
    "movq %rax, {gs_context_next_sup_fs_offset}(%r12, %r10);", //use current supervisor fs_base as grate's supervisor fs_base
    
    
    //get grate fs-base
    "addq ${gs_contexts_size_value}, %r10; ", //r10 now holds the index of the grate context * context_size
    "movq {gs_context_fs_offset}(%r12, %r10), %rax; ",
    "wrfsbase %rax; ",
    
    //get grate stack //assume 16 byte alignment
    "movq {gs_contexts_array_offset}(%r12, %r10), %rsp ;",

    //increment the current context index
    "incq {gs_current_context_offset}(%r12); ",
    
    //switch pkru, get as offset from gs:r10

    // Switch to the callee's PKRU (contexts[i+1].pkru, r10 still holds its
    // offset). wrpkru needs ecx = edx = 0, so park the argument registers in
    // r10 and r12 (r12 is recomputed after the call).
    "movl {gs_context_pkru_offset}(%r12, %r10), %eax; ",
    "movq %rcx, %r10; ",
    "movq %rdx, %r12; ",
    "xorl %ecx, %ecx; ",
    "xorl %edx, %edx; ",
    "wrpkru; ",
    "movq %r10, %rcx; ",
    "movq %r12, %rdx; ",
    //call fptr
    "call *%r9; ",

    // Back to the supervisor PKRU before touching supervisor memory.
    "movq %rax, %r10; ",
    "xorl %eax, %eax; ",
    "xorl %ecx, %ecx; ",
    "xorl %edx, %edx; ",
    "wrpkru; ",
    "movq %r10, %rax; ",

    // Recompute the context stack from the cage's FS, see
    // inner_grate_callback_trampoline_asm.
    "rdfsbase %r12; ",
    "addq {guest_tcb_size}(%rip), %r12; ",
    "addq ${context_page_size} - 1, %r12; ",
    "andq $-{context_page_size}, %r12; ",

    //decrement the current context index
    "decq {gs_current_context_offset}(%r12); ",

    //switch back to supervisor stack
    "movq {gs_current_context_offset}(%r12), %r10 ;",
    "imulq ${gs_contexts_size_value}, %r10;" ,
    "movq {gs_contexts_array_offset}(%r12, %r10), %rsp ;",

    //switch back to supervisor fs_base
    "movq {gs_context_fs_offset}(%r12, %r10), %r11; ",
    "wrfsbase %r11; ",
    "popq %r12; ",
    
    "ret; "
        ,
        gs_current_context_offset = const GS_CURRENT_CONTEXT_OFFSET,
        gs_contexts_array_offset = const GS_CONTEXT_ARRAY_OFFSET,
        gs_context_fs_offset = const GS_CONTEXT_FS_OFFSET,
        gs_contexts_size_value = const GS_CONTEXT_SIZE,
        gs_context_next_sup_rsp_offset = const GS_CONTEXT_NEXT_SUP_RSP_OFFSET,
        gs_context_next_sup_fs_offset = const GS_CONTEXT_NEXT_SUP_FS_OFFSET,
        gs_context_pkru_offset = const GS_CONTEXT_PKRU_OFFSET,
        guest_tcb_size = sym GUEST_TCB_SIZE,
        context_page_size = const CONTEXT_PAGE_SIZE,
        options(att_syntax));
    }
    
    //End ASM
}


/// Enters contexts[0] from host TLS; contexts[1] must already contain the supervisor context.
pub extern "sysv64" fn jmp_into_cage_prepared_gs_naked(
    entry_point: u64,
) -> ! {
    let context_stack = current_context_stack();
    unsafe { jmp_into_cage_asm(entry_point, context_stack) }
}

#[unsafe(naked)]
unsafe extern "sysv64" fn jmp_into_cage_asm(
    entry_point: u64,
    context_stack: *mut MPKSupervisorCtxStack,
) -> ! {
    unsafe {
        naked_asm!(
            "movq {gs_contexts_array_offset}(%rsi), %rsp; ",
            "movq {gs_context_fs_offset}(%rsi), %rax; ",
            "wrfsbase %rax; ",
            // Enter with the cage's PKRU from contexts[0].
            "movl {gs_context_pkru_offset}(%rsi), %eax; ",
            "xorl %ecx, %ecx; ",
            "xorl %edx, %edx; ",
            "wrpkru; ",
            "jmp *%rdi",
        gs_contexts_array_offset = const GS_CONTEXT_ARRAY_OFFSET,
        gs_context_fs_offset = const GS_CONTEXT_FS_OFFSET,
        gs_context_pkru_offset = const GS_CONTEXT_PKRU_OFFSET,
        options(att_syntax));

    }
}

#[cfg(test)]
mod context_stack_tests {
    use super::*;
    use std::arch::asm;

    /// The trampolines execute wrpkru, which needs PKU enabled by the OS.
    fn pku_available() -> bool {
        let ospke = unsafe { std::arch::x86_64::__cpuid_count(7, 0) }.ecx & (1 << 4) != 0;
        if !ospke {
            eprintln!("skipping: CPU/OS without protection keys (OSPKE)");
        }
        ospke
    }

    struct ContextStack {
        pointer: *mut MPKSupervisorCtxStack,
        mapping: usize,
        mapping_size: usize,
        previous_gs: usize,
        previous_stack: *mut MPKSupervisorCtxStack,
        _guest_stack: Vec<u128>,
    }

    impl ContextStack {
        fn new() -> Self {
            let previous_gs;
            unsafe {
                asm!("rdgsbase {}", out(reg) previous_gs, options(nostack, preserves_flags));
            }
            let previous_stack = current_ctx_stack.get();
            let (pointer, mapping, mapping_size) =
                crate::lind_mpk::execute::setup_gs_context(
                    1,
                    crate::lind_mpk::execute::current_tid(),
                )
                .unwrap();
            let mut guest_stack = vec![0u128; 4096];
            unsafe {
                (*pointer).current_context = 1;
                (*pointer).contexts[2].rsp =
                    guest_stack.as_mut_ptr().add(guest_stack.len()) as u64;
                (*pointer).contexts[2].fs_base = (*pointer).contexts[1].fs_base;
            }
            Self {
                pointer, mapping, mapping_size, previous_gs, previous_stack,
                _guest_stack: guest_stack,
            }
        }
    }

    impl Drop for ContextStack {
        fn drop(&mut self) {
            unsafe {
                asm!("wrgsbase {}", in(reg) self.previous_gs, options(nostack, preserves_flags));
                libc::munmap(self.mapping as *mut libc::c_void, self.mapping_size);
            }
            current_ctx_stack.set(self.previous_stack);
        }
    }

    #[unsafe(naked)]
    unsafe extern "sysv64" fn check_cage_entry(expected_rsp: u64, expected_fs: u64) -> u64 {
        naked_asm!(
            "pushq %r12",
            "pushq %r13",
            "pushq %r14",
            "pushq %r15",
            "subq $8, %rsp",
            "movq %rsp, %r12",
            "rdfsbase %r13",
            "movq %rdi, %r14",
            "movq %rsi, %r15",
            "leaq {entry}(%rip), %rdi",
            "call {jump}",
            "ud2",
            entry = sym cage_entry_probe,
            jump = sym jmp_into_cage_prepared_gs_naked,
            options(att_syntax),
        );
    }

    #[unsafe(naked)]
    unsafe extern "sysv64" fn cage_entry_probe() -> ! {
        naked_asm!(
            // Compare the entry registers, then bypass the non-returning wrapper
            // to restore the test caller's stack, TLS, and callee-saved registers.
            "movq %rsp, %rax",
            "xorq %r14, %rax",
            "rdfsbase %rdx",
            "xorq %r15, %rdx",
            "orq %rdx, %rax",
            "wrfsbase %r13",
            "movq %r12, %rsp",
            "addq $8, %rsp",
            "popq %r15",
            "popq %r14",
            "popq %r13",
            "popq %r12",
            "ret",
            options(att_syntax),
        );
    }

    #[test]
    fn cage_entry_uses_thread_local_context_stack_instead_of_gs() {
        if !pku_available() {
            return;
        }
        let stack = ContextStack::new();
        let mut gs_context = MPKSupervisorCtxStack::new(0, Default::default());
        unsafe {
            (*stack.pointer).current_context = 0;
            (*stack.pointer).contexts[0].rsp = (*stack.pointer).contexts[2].rsp;
            (*stack.pointer).contexts[0].fs_base = stack.pointer as usize;
            asm!("wrgsbase {}", in(reg) &mut gs_context, options(nostack, preserves_flags));
            assert_eq!(
                check_cage_entry(
                    (*stack.pointer).contexts[0].rsp,
                    (*stack.pointer).contexts[0].fs_base as u64,
                ),
                0,
            );
        }
        assert_eq!(current_context_stack(), stack.pointer);
        assert_eq!(unsafe { (*stack.pointer).current_context }, 0);
    }

    extern "sysv64" fn grate(
        cageid: u64,
        arg1: u64,
        arg1cageid: u64,
        arg2: u64,
        arg2cageid: u64,
        arg3: u64,
        arg3cageid: u64,
        arg4: u64,
        arg4cageid: u64,
        arg5: u64,
        arg5cageid: u64,
        arg6: u64,
        arg6cageid: u64,
    ) -> i64 {
        let args = [
            cageid, arg1, arg1cageid, arg2, arg2cageid, arg3, arg3cageid,
            arg4, arg4cageid, arg5, arg5cageid, arg6, arg6cageid,
        ];
        let depth = unsafe { (*current_context_stack()).current_context };
        if depth != 2 {
            return -1;
        }
        args.iter().enumerate().map(|(i, arg)| (i as u64 + 1) * arg).sum::<u64>() as i64
    }

    thread_local! {
        /// Context stack to return to; models a clone child when set.
        static CHILD_STACK: std::cell::Cell<*mut MPKSupervisorCtxStack> =
            const { std::cell::Cell::new(std::ptr::null_mut()) };
    }

    /// Guest FS the grate returns on. Real grates run on their own TLS; here a
    /// fake TCB is placed right before the (page-aligned) context stack.
    extern "sysv64" fn grate_return_fs() -> u64 {
        let mut target = CHILD_STACK.get();
        if target.is_null() {
            target = current_context_stack();
        } else {
            unsafe { std::ptr::copy_nonoverlapping(current_context_stack(), target, 1) };
            current_ctx_stack.set(target);
        }
        (target as usize - GUEST_TCB_SIZE.load(std::sync::atomic::Ordering::Relaxed)) as u64
    }

    #[unsafe(naked)]
    unsafe extern "sysv64" fn grate_stub() -> i64 {
        naked_asm!(
            // Re-push the seven stack arguments and call the host grate.
            "subq $0x38, %rsp",
            "movq $0, %rax",
            "2:",
            "movq 0x40(%rsp,%rax,8), %r10",
            "movq %r10, (%rsp,%rax,8)",
            "incq %rax",
            "cmpq $7, %rax",
            "jne 2b",
            "call {grate}",
            "movq %rax, (%rsp)",
            "call {return_fs}",
            "movq %rax, %rdx",
            "movq (%rsp), %rax",
            "addq $0x38, %rsp",
            "wrfsbase %rdx",
            "ret",
            grate = sym grate,
            return_fs = sym grate_return_fs,
            options(att_syntax),
        );
    }

    #[test]
    fn grate_trampoline_preserves_arguments_and_restores_context() {
        if !pku_available() {
            return;
        }
        let stack = ContextStack::new();
        assert_eq!(current_context_stack(), stack.pointer);
        let result = unsafe {
            inner_grate_callback_trampoline_asm(
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
                grate_stub as *const () as u64,
                stack.pointer,
            )
        };
        assert_eq!(result, (1..=13).map(|arg| arg * arg).sum::<i64>());
        assert_eq!(unsafe { (*stack.pointer).current_context }, 1);
        assert_eq!(current_context_stack(), stack.pointer);
    }

    thread_local! {
        static SIGNAL_ARGS: std::cell::Cell<[u64; 6]> = const { std::cell::Cell::new([0; 6]) };
        static SIGNAL_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    extern "sysv64" fn signal_handler(arg1: u64, arg2: u64, arg3: u64, arg4: u64, arg5: u64, arg6: u64) {
        SIGNAL_ARGS.set([arg1, arg2, arg3, arg4, arg5, arg6]);
        SIGNAL_DEPTH.set(unsafe { (*current_context_stack()).current_context });
    }

    #[unsafe(naked)]
    unsafe extern "sysv64" fn signal_stub() {
        naked_asm!(
            "subq $8, %rsp",
            "call {handler}",
            "call {return_fs}",
            "addq $8, %rsp",
            "wrfsbase %rax",
            "ret",
            handler = sym signal_handler,
            return_fs = sym grate_return_fs,
            options(att_syntax),
        );
    }

    #[test]
    fn signal_trampoline_uses_explicit_context_stack() {
        if !pku_available() {
            return;
        }
        let stack = ContextStack::new();
        unsafe {
            _call_into_cage_asm(1, 2, 3, 4, 5, signal_stub as *const () as u64, stack.pointer);
        }
        assert_eq!(SIGNAL_ARGS.get(), [1, 2, 3, 4, 5, signal_stub as *const () as u64]);
        assert_eq!(SIGNAL_DEPTH.get(), 2);
        assert_eq!(unsafe { (*stack.pointer).current_context }, 1);
        assert_eq!(current_context_stack(), stack.pointer);
    }

    #[test]
    fn grate_trampoline_returns_through_child_context_stack_from_fs() {
        if !pku_available() {
            return;
        }
        let parent = ContextStack::new();
        let child = ContextStack::new();
        current_ctx_stack.set(parent.pointer);
        CHILD_STACK.set(child.pointer);
        let result = unsafe {
            inner_grate_callback_trampoline_asm(
                1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13,
                grate_stub as *const () as u64,
                parent.pointer,
            )
        };
        CHILD_STACK.set(std::ptr::null_mut());
        assert_eq!(result, (1..=13).map(|arg| arg * arg).sum::<i64>());
        assert_eq!(current_context_stack(), child.pointer);
        assert_eq!(unsafe { (*child.pointer).current_context }, 1);
        assert_eq!(unsafe { (*parent.pointer).current_context }, 2);
    }
}


// #[unsafe(naked)]
// pub extern "sysv64" fn call_into_cage_prepared_gs_naked(
//     rdi: u64,
//     rsi: u64,
//     rcx: u64,
//     r8: u64,
//     r9: u64,
//     r10: u64,
// ) {
//     unsafe {
//         naked_asm!(
//             //at this point, we assume that contexts[0] is the user context to switch to and that contexts[1] is an already prepared supervisor context
//             ""


//             ,
//         gs_current_context_offset = const GS_CURRENT_CONTEXT_OFFSET,
//         gs_contexts_array_offset = const GS_CONTEXT_ARRAY_OFFSET,
//         gs_contexts_size_value = const GS_CONTEXT_SIZE,
//         options(att_syntax));

//     }
// }