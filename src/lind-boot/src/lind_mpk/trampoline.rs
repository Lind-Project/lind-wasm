use crate::lind_mpk::RuntimeInfo::{
    MPKCageCtxStack, MPKRuntimeInfo, MPKSupervisorContext, MPKSupervisorCtxStack, MpkCageThreadInfo,
    LIND_MPK_MAX_CONTEXTS,
};
use cage::get_cage;
use std::arch::{asm, naked_asm};
use std::mem::{offset_of, size_of};
use std::sync::Arc;
use sysdefs::constants::lind_platform_const::{THREEI_CAGEID, UNUSED_ARG, UNUSED_ID};
use threei::threei_const::{REGISTER_HANDLER_SYSCALL, RUNTIME_TYPE_MPK};

const GS_SUPER_CURRENT_CONTEXT: usize = offset_of!(MPKSupervisorCtxStack, current_context);
pub const GS_SUPER_OS_TID: usize = offset_of!(MPKSupervisorCtxStack, os_tid);
const GS_SUPER_CONTEXT_ARRAY: usize = offset_of!(MPKSupervisorCtxStack, contexts);
const GS_SUPER_CONTEXT_SIZE: usize = size_of::<MPKSupervisorContext>();
// `cage_data` is mapped one page after the GS data in setup_supervisor_stack.
const MPK_CONTEXT_PAGE_SIZE: usize = 4096;
const GS_CURRENT_CONTEXT: usize = MPK_CONTEXT_PAGE_SIZE + offset_of!(MPKCageCtxStack, current_context);
const GS_CONTEXTS_ARRAY: usize = GS_CURRENT_CONTEXT + offset_of!(MPKCageCtxStack, contexts);
const GS_CONTEXTS_SIZE: usize = size_of::<usize>();

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
                stack_addr: 0,
                stack_base: 0,
                stack_size: 0,
            };
            new_thread.allocate_grate_stack()?;
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
    let gs_base: usize;
    let current_context_index: usize;
    unsafe {
        asm!(
            "mov %gs:0, {current_context_index}",
            current_context_index = out(reg) current_context_index,
            options(nostack, preserves_flags, att_syntax)
        );
        asm!(
            "rdgsbase {gs_base}",
            gs_base = out(reg) gs_base,
            options(nostack, preserves_flags, att_syntax)
        );
    }
    assert!(
        current_context_index + 1 < LIND_MPK_MAX_CONTEXTS,
        "inner_grate_callback_trampoline: supervisor context stack is full"
    );

    let gs_data = gs_base as *mut MPKSupervisorCtxStack;
    let os_tid = unsafe { (*gs_data).os_tid as libc::pid_t };
    let (grate_stack, cage_data) = {
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
        (cage_thread.stack_addr, cage_thread.thread_info.cage_data)
    };
    assert!(!cage_data.is_null(), "inner_grate_callback_trampoline: null cage context");

    assert!((gs_base + 0x1000) as u64 == cage_data as u64, "inner_grate_callback_trampoline: cage data is not related to gs base");

    let cage_context_index = unsafe { (*cage_data).current_context };
    assert!(
        cage_context_index < LIND_MPK_MAX_CONTEXTS,
        "inner_grate_callback_trampoline: cage context index out of bounds: {}",
        cage_context_index
    );

    //determine next stack. 
    //TODO: in case of reentry, a new frame on the grate stack needs to be used.
    let next_stack = grate_stack;
    assert!(next_stack != 0, "inner_grate_callback_trampoline: grate stack is not initialized");
    unsafe {
        (*cage_data).contexts[cage_context_index].rsp = (next_stack & !0xf) as u64; 
        (*gs_data).contexts[current_context_index].cage_id = cageid;
    }

    let asm_entry: unsafe extern "sysv64" fn(
        u64, u64, u64, u64, u64, u64, u64, u64,
        u64, u64, u64, u64, u64, u64,
    ) -> i64 = unsafe {
        std::mem::transmute::<
            unsafe extern "sysv64" fn(
                u64, u64, u64, u64, u64, u64, u64, u64,
                u64, u64, u64, u64, u64, u64,
            ),
            unsafe extern "sysv64" fn(
                u64, u64, u64, u64, u64, u64, u64, u64,
                u64, u64, u64, u64, u64, u64,
            ) -> i64,
        >(inner_grate_callback_trampoline_asm)
    };
    let return_value = unsafe { asm_entry(
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
    ) };
    
    return_value
}

#[unsafe(naked)]
extern "sysv64" fn inner_grate_callback_trampoline_asm(
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
) {
    unsafe {
        naked_asm!(

        
    //here, our stack is 8-byte aligned, it needs to be 16-byte aligned before making a call!
    //we have 8 arguments on the stack, the last one is the address we jump to
    
    
    //get context stack pointer and grate stack
    //switch to threads' grate stack, old rsp in r11
    "movq %rsp, %r11; ",
    //write supervisor rsp to sup stack
    "movq %gs:{gs_super_current_context_offset}, %r10 ;",
    "imulq ${gs_super_context_size_value}, %r10;" ,
    "subq $8, %rsp;", //16 byte alignment
    "movq %rsp, %gs:{gs_super_context_array_offset}(%r10);",
    //get grate stack
    "movq %gs:{gs_current_context_offset}, %r10 ;",
    "imulq ${gs_contexts_size_value}, %r10;" ,
    "movq %gs:{gs_contexts_array_offset}(%r10), %rsp ;",


    "subq $0x58, %rsp; ", // room for 8 byte alignment (0x8) + 7 args (0x38) + 3 regs (0x18) 
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
    
    //switch pkru, get as offset from gs:r10
    //call fptr
    "call *%r11; ",
    //switch back pkru


    //switch back to supervisor stack
    "movq %gs:{gs_super_current_context_offset}, %r10 ;",
    "imulq ${gs_super_context_size_value}, %r10;" ,
    "movq %gs:{gs_super_context_array_offset}(%r10), %rsp ;",
    "addq $8, %rsp; ", //undo 16 byte alignment
    
    "ret; "
        ,
        gs_super_current_context_offset = const GS_SUPER_CURRENT_CONTEXT,
        gs_super_context_array_offset = const GS_SUPER_CONTEXT_ARRAY,
        gs_super_context_size_value = const GS_SUPER_CONTEXT_SIZE,
        gs_current_context_offset = const GS_CURRENT_CONTEXT,
        gs_contexts_array_offset = const GS_CONTEXTS_ARRAY,
        gs_contexts_size_value = const GS_CONTEXTS_SIZE,
        options(att_syntax));
    }
    
    //End ASM
}