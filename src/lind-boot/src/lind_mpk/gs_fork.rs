//! In-process ("GS") fork for MPK cages.
//!
//! The binary rewriter prefixes every guest memory access with `%gs`, except
//! RIP- and RSP-relative ones. A child created by fork runs on a new host
//! thread in the lind process instead of in a forked OS process:
//!
//! - The parent's cage memory is duplicated at `parent_base + X`, where
//!   `X = child_base - parent_base`. GS-relative accesses of the child land in
//!   the copy because the child host thread runs with GS base `X` and its
//!   `struct pthread::sandbox_offset` set to `X`.
//! - Every cage reservation is `MPK_MEMORY_SIZE` bytes of cage memory followed
//!   by a `GS_FORK_ARENA_SIZE` arena that is `PROT_NONE` until stacks are
//!   placed in it. RSP-relative accesses are not GS-prefixed, so the child gets
//!   a real stack: a memfd holding a copy of the parent's stack, mapped in a
//!   slot of the arena directly after the parent's cage memory. The same memfd
//!   is aliased at `parent_stack + X` and at `child_stack + X`, so GS-relative
//!   accesses through pointers into either stack reach the child's copy.
//!
//! Code is not relocated, so a child keeps executing out of the memory of the
//! cage that was started by exec (the "root" of the fork family). Its data
//! accesses, RIP-relative ones included, are GS-relative and reach its own
//! copy, so the child runs with only its own protection key: PKRU
//! does not apply to instruction fetches. Nested forks therefore use offsets relative to the root, take
//! their stacks from the root's arena and alias every ancestor's stack. The
//! root's memory stays mapped until every member of its family has exited.


use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use cage::{get_cage, lind_signal_init, Vmmap, VmmapOps};
use libc::{c_void, pid_t};
use sysdefs::constants::fs_const::{MAP_SHARED, PAGESHIFT, PROT_NONE, PROT_READ, PROT_WRITE};
use sysdefs::logging::lind_debug_panic;
use threei::threei_const;
use wasmtime_lind_multi_process::THREAD_START_ID;

use crate::lind_mpk::pkeys;
use crate::lind_mpk::execute::{
    current_tid, mmap_aligned_cage_memory, CAGE_STACK_GUARD, CAGE_STACK_SIZE, MPK_MEMORY_SIZE,
};
use crate::lind_mpk::syscalls::{
    current_pointer_guard, relocate_cloned_contexts,
    relocate_mangled_stack_pointer, MpkJumpBuffer, LIND_MANAGER, MPK_EPOCH,
};
use crate::lind_mpk::RuntimeInfo::{
    allocate_context_pages, current_context_stack, current_ctx_stack, guest_sandbox_offset,
    guest_tcb_size, parent_stack_offset, MPKRuntimeInfo, MPKSupervisorCtxStack,
    MpkCageThreadInfo, MpkThreadInfo, LIND_MPK_MAX_CONTEXTS,
};

const PAGE_SIZE: usize = 1 << PAGESHIFT;

/// Arena reserved directly after each cage's memory for forked children's stacks.
pub(super) const GS_FORK_ARENA_SIZE: usize = 1 << 30;
/// Virtual address range reserved per cage: cage memory plus the arena.
pub(super) const CAGE_RESERVATION_SIZE: usize = MPK_MEMORY_SIZE + GS_FORK_ARENA_SIZE;
/// One arena slot holds a stack of at most `CAGE_STACK_SIZE` bytes below a
/// `PROT_NONE` guard of `CAGE_STACK_GUARD` bytes.
const STACK_SLOT_SIZE: usize = CAGE_STACK_SIZE + CAGE_STACK_GUARD;
const STACK_SLOTS: usize = GS_FORK_ARENA_SIZE / STACK_SLOT_SIZE;
/// Bytes below the guest RSP that may still be live (x86-64 red zone).
const RED_ZONE: usize = 128;
const COPY_CHUNK_SIZE: usize = 64 * PAGE_SIZE;

fn gs_debug(message: impl AsRef<str>) {
    if std::env::var_os("LIND_MPK_DEBUG").is_some() {
        eprintln!("[lind-mpk] gs-fork: {}", message.as_ref());
    }
}

/// Start of arena slot `slot` of the cage reserved at `root_base`.
fn slot_addr(root_base: usize, slot: usize) -> usize {
    root_base + MPK_MEMORY_SIZE + slot * STACK_SLOT_SIZE
}

/// Lowest address of a `stack_size` stack placed at the top of `slot`.
fn slot_stack_lo(root_base: usize, slot: usize, stack_size: usize) -> usize {
    slot_addr(root_base, slot) + STACK_SLOT_SIZE - stack_size
}

// ── Fork-family bookkeeping ─────────────────────────────────────────────────

/// A cage created by GS fork.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ForkedCage {
    /// Cage memory base of the family root; all guest addresses are relative to it.
    root_base: usize,
    /// Cage memory base of this cage.
    region_base: usize,
    /// `region_base - root_base` (wrapping): GS base and `sandbox_offset`.
    offset: usize,
    /// Arena slot of the root that holds this cage's real stack.
    slot: usize,
    /// Real stack of this cage's (only) thread.
    stack_lo: usize,
    stack_size: usize,
    /// Real stacks of all ancestors; each is aliased at `+ offset`.
    ancestor_stacks: Vec<usize>,
}

#[derive(Debug)]
struct Family {
    slots: Vec<bool>,
    /// Live cages of the family, including the root while `root_alive`.
    members: usize,
    root_alive: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum MemoryOp {
    Unmap(usize, usize),
    /// Replace the range with a fresh `PROT_NONE` reservation.
    Reserve(usize, usize),
}

/// Arena slot holding the real stack of a thread created in a GS-forked cage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ThreadStack {
    cage_id: u64,
    root_base: usize,
    slot: usize,
    /// GS offset of the cage; the stack is aliased at `+ offset`.
    offset: usize,
}

#[derive(Debug, Default)]
struct Registry {
    families: HashMap<usize, Family>,
    cages: HashMap<u64, ForkedCage>,
    /// Thread stacks of GS-forked cages, keyed by their lowest address.
    thread_stacks: HashMap<usize, ThreadStack>,
}

impl Registry {
    fn family(&mut self, root_base: usize) -> &mut Family {
        self.families.entry(root_base).or_insert_with(|| Family {
            slots: vec![false; STACK_SLOTS],
            members: 1,
            root_alive: true,
        })
    }

    fn alloc_slot(&mut self, root_base: usize) -> Option<usize> {
        let family = self.family(root_base);
        let slot = family.slots.iter().position(|used| !used)?;
        family.slots[slot] = true;
        Some(slot)
    }

    fn free_slot(&mut self, root_base: usize, slot: usize) {
        if let Some(family) = self.families.get_mut(&root_base) {
            family.slots[slot] = false;
        }
    }

    fn add_child(&mut self, cage_id: u64, cage: ForkedCage) {
        self.family(cage.root_base).members += 1;
        self.cages.insert(cage_id, cage);
    }

    /// Memory operations needed when cage `cage_id` with memory at `base`
    /// exits or execs.
    fn release(&mut self, cage_id: u64, base: usize) -> Vec<MemoryOp> {
        let mut ops = Vec::new();
        if self.cages.get(&cage_id).is_some_and(|cage| cage.region_base == base) {
            // The aliases of the cage's thread stacks go with its reservation.
            let thread_stacks: Vec<usize> = self
                .thread_stacks
                .iter()
                .filter(|(_, stack)| stack.cage_id == cage_id)
                .map(|(lo, _)| *lo)
                .collect();
            for lo in thread_stacks {
                let stack = self.thread_stacks.remove(&lo).unwrap();
                self.free_slot(stack.root_base, stack.slot);
                ops.push(MemoryOp::Reserve(slot_addr(stack.root_base, stack.slot), STACK_SLOT_SIZE));
            }
            let cage = self.cages.remove(&cage_id).unwrap();
            ops.push(MemoryOp::Unmap(base, CAGE_RESERVATION_SIZE));
            let family = self
                .families
                .get_mut(&cage.root_base)
                .expect("gs-fork: forked cage without family");
            family.slots[cage.slot] = false;
            family.members -= 1;
            if family.members == 0 {
                self.families.remove(&cage.root_base);
                ops.push(MemoryOp::Unmap(cage.root_base, CAGE_RESERVATION_SIZE));
            } else {
                ops.push(MemoryOp::Reserve(slot_addr(cage.root_base, cage.slot), STACK_SLOT_SIZE));
            }
            return ops;
        }
        if let Some(family) = self.families.get_mut(&base).filter(|family| family.root_alive) {
            family.root_alive = false;
            family.members -= 1;
            if family.members == 0 {
                self.families.remove(&base);
                ops.push(MemoryOp::Unmap(base, CAGE_RESERVATION_SIZE));
            }
            return ops;
        }
        ops.push(MemoryOp::Unmap(base, CAGE_RESERVATION_SIZE));
        ops
    }

    /// Memory operations releasing the thread stack at `lo`, if there is one.
    fn release_thread_stack(&mut self, lo: usize) -> Vec<MemoryOp> {
        let Some(stack) = self.thread_stacks.remove(&lo) else {
            return Vec::new();
        };
        self.free_slot(stack.root_base, stack.slot);
        let addr = slot_addr(stack.root_base, stack.slot);
        vec![
            MemoryOp::Reserve(addr, STACK_SLOT_SIZE),
            MemoryOp::Reserve(addr.wrapping_add(stack.offset), STACK_SLOT_SIZE),
        ]
    }
}

fn apply_memory_ops(ops: Vec<MemoryOp>) {
    for op in ops {
        match op {
            MemoryOp::Unmap(addr, len) => {
                gs_debug(format!("unmapping {addr:#x}..{:#x}", addr + len));
                unsafe { libc::munmap(addr as *mut c_void, len) };
                if len == CAGE_RESERVATION_SIZE {
                    pkeys::release_reservation(addr);
                }
            }
            MemoryOp::Reserve(addr, len) => {
                unsafe {
                    libc::mmap(
                        addr as *mut c_void,
                        len,
                        libc::PROT_NONE,
                        libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE | libc::MAP_FIXED,
                        -1,
                        0,
                    );
                }
                pkeys::retag(addr, len, libc::PROT_NONE);
            }
        }
    }
}

fn registry() -> &'static Mutex<Registry> {
    static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}

/// Guest stack `(lo, size)` of each host thread running guest code.
fn guest_stacks() -> &'static Mutex<HashMap<pid_t, (usize, usize)>> {
    static STACKS: OnceLock<Mutex<HashMap<pid_t, (usize, usize)>>> = OnceLock::new();
    STACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Records the guest stack of host thread `tid`; fork duplicates it.
pub(super) fn record_guest_stack(tid: pid_t, lo: usize, size: usize) {
    guest_stacks().lock().unwrap().insert(tid, (lo, size));
}

/// Forgets the guest stack of exiting host thread `tid` and releases it if it
/// is the arena stack of a thread in a GS-forked cage.
pub(super) fn forget_guest_stack(tid: pid_t) {
    let Some((lo, _)) = guest_stacks().lock().unwrap().remove(&tid) else {
        return;
    };
    let ops = registry().lock().unwrap().release_thread_stack(lo);
    apply_memory_ops(ops);
}

/// Whether `cage_id` was created by GS fork (and has not exec'd since).
pub(super) fn is_gs_forked_cage(cage_id: u64) -> bool {
    registry().lock().unwrap().cages.contains_key(&cage_id)
}

/// Releases the reservation of a cage's memory at `base` when the cage exits
/// or execs. Memory still needed by a fork family is kept until its last
/// member is gone.
pub(super) fn release_cage_memory(cage_id: u64, base: usize) {
    let ops = registry().lock().unwrap().release(cage_id, base);
    apply_memory_ops(ops);
}

/// Sandbox offset (GS base) of `cage_id`: `region_base - root_base` for a
/// GS-forked cage, 0 otherwise.
pub(super) fn cage_gs_offset(cage_id: u64) -> usize {
    registry().lock().unwrap().cages.get(&cage_id).map_or(0, |cage| cage.offset)
}

/// GS base of the guest code the supervisor returns to on `stack`: the
/// sandbox offset of the cage of the most recent guest context.
pub(super) fn active_guest_gs_base(stack: &MPKSupervisorCtxStack) -> Option<usize> {
    let current = stack.current_context.min(LIND_MPK_MAX_CONTEXTS);
    (0..current)
        .rev()
        .map(|i| stack.contexts[i].cage_id)
        .find(|&cage_id| cage_id != 0)
        .map(cage_gs_offset)
}

/// Runs `f`, which calls into guest code of `cage_id` from the supervisor,
/// with that cage's GS base. Afterwards installs the GS base of the guest code
/// the supervisor returns to. It is looked up instead of saved, because a fork
/// inside `f` resumes the child on a copy of this frame.
pub(super) fn call_with_cage_gs<R>(cage_id: u64, f: impl FnOnce() -> R) -> R {
    let caller: usize;
    unsafe {
        std::arch::asm!("rdgsbase {}", out(reg) caller, options(nostack, preserves_flags));
    }
    set_gs_base(cage_gs_offset(cage_id));
    let result = f();
    let stack = current_ctx_stack.get();
    let resume = if stack.is_null() {
        None
    } else {
        active_guest_gs_base(unsafe { &*stack })
    };
    set_gs_base(resume.unwrap_or(caller));
    result
}

/// Guest state of a new thread in a GS-forked cage.
#[derive(Clone, Copy, Debug)]
pub(super) struct GsThread {
    /// Real address of the thread descriptor (FS base).
    pub fs_base: usize,
    /// Real stack in an arena slot of the family root.
    pub stack_lo: usize,
    pub stack_size: usize,
}

/// Prepares a thread of `cage_id` whose descriptor glibc placed at guest
/// address `tls`. For a GS-forked cage, sets the descriptor's
/// `sandbox_offset` and returns its real address together with a new stack.
/// RSP-relative accesses are not GS-prefixed, so the stack is real memory in
/// an arena slot, aliased at `+ X` for GS-relative accesses through pointers
/// to it; the guest-allocated stack is not used. Returns `None` for other cages.
pub(super) fn prepare_thread(cage_id: u64, tls: usize, stack_size: usize) -> anyhow::Result<Option<GsThread>> {
    let (root_base, region_base, offset, slot) = {
        let mut registry = registry().lock().unwrap();
        let Some(cage) = registry.cages.get(&cage_id) else {
            return Ok(None);
        };
        let (root_base, region_base, offset) = (cage.root_base, cage.region_base, cage.offset);
        let slot = registry
            .alloc_slot(root_base)
            .ok_or_else(|| anyhow::anyhow!("no free stack slot in the arena of {root_base:#x}"))?;
        (root_base, region_base, offset, slot)
    };
    let stack_size = stack_size.next_multiple_of(PAGE_SIZE).clamp(PAGE_SIZE, CAGE_STACK_SIZE);
    let stack_lo = slot_stack_lo(root_base, slot, stack_size);
    let result = (|| {
        let sandbox_offset_field = guest_sandbox_offset()?;
        let fs_base = tls.wrapping_add(offset);
        let field = fs_base.wrapping_add(sandbox_offset_field);
        if fs_base < region_base
            || field
                .checked_add(size_of::<usize>())
                .is_none_or(|end| end > region_base + MPK_MEMORY_SIZE)
            || sandbox_offset_field + size_of::<usize>() > guest_tcb_size()?
        {
            anyhow::bail!("thread pointer {tls:#x} is outside the memory of cage {cage_id}");
        }
        let pkey = pkeys::reservation_key(region_base);
        create_aliased_stack(stack_lo, stack_size, &[stack_lo], offset, region_base, pkey)?;
        unsafe { std::ptr::write_unaligned(field as *mut usize, offset) };
        Ok(fs_base)
    })();
    let mut registry = registry().lock().unwrap();
    registry
        .thread_stacks
        .insert(stack_lo, ThreadStack { cage_id, root_base, slot, offset });
    match result {
        Ok(fs_base) => {
            gs_debug(format!(
                "cage {cage_id}: thread FS {tls:#x} -> {fs_base:#x}, stack {stack_lo:#x}+{stack_size:#x} (slot {slot})"
            ));
            Ok(Some(GsThread { fs_base, stack_lo, stack_size }))
        }
        Err(error) => {
            let ops = registry.release_thread_stack(stack_lo);
            drop(registry);
            apply_memory_ops(ops);
            Err(error)
        }
    }
}

/// Runs `f` with the GS base of the calling host thread set to `value`, for
/// calling rewritten guest code from the supervisor. Pointers passed to such
/// code must be given as `real - value`.
pub(super) fn with_gs_base<R>(value: usize, f: impl FnOnce() -> R) -> R {
    let previous: usize;
    unsafe {
        std::arch::asm!("rdgsbase {}", out(reg) previous, options(nostack, preserves_flags));
    }
    set_gs_base(value);
    let result = f();
    set_gs_base(previous);
    result
}

/// Sets the GS base of the calling host thread.
pub(super) fn set_gs_base(value: usize) {
    // arch_prctl(ARCH_SET_GS) rejects non-canonical-looking values such as a
    // negative offset, so write the register directly (FSGSBASE is required
    // by the trampolines already).
    unsafe {
        std::arch::asm!("wrgsbase {}", in(reg) value, options(nostack, preserves_flags));
    }
}

// ── Memory duplication ──────────────────────────────────────────────────────

/// Reads `buf.len()` bytes at `addr` of this process without faulting.
/// Returns the number of bytes read (0 if the first page is inaccessible).
fn read_own_memory(addr: usize, buf: &mut [u8]) -> usize {
    let local = libc::iovec { iov_base: buf.as_mut_ptr() as *mut c_void, iov_len: buf.len() };
    let remote = libc::iovec { iov_base: addr as *mut c_void, iov_len: buf.len() };
    let ret = unsafe { libc::process_vm_readv(libc::getpid(), &local, 1, &remote, 1, 0) };
    if ret < 0 { 0 } else { ret as usize }
}

/// Copies the non-zero pages of `[src, src + len)` to `dst`, which must be
/// writable and zero-filled. Untouched pages stay unallocated in the copy and
/// unreadable source pages are skipped.
fn copy_nonzero_pages(src: usize, dst: usize, len: usize, buf: &mut [u8]) {
    let mut offset = 0;
    while offset < len {
        let want = buf.len().min(len - offset);
        let read = read_own_memory(src + offset, &mut buf[..want]) & !(PAGE_SIZE - 1);
        if read == 0 {
            offset += PAGE_SIZE;
            continue;
        }
        for page in (0..read).step_by(PAGE_SIZE) {
            let bytes = &buf[page..page + PAGE_SIZE];
            if bytes.iter().any(|&byte| byte != 0) {
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        bytes.as_ptr(),
                        (dst + offset + page) as *mut u8,
                        PAGE_SIZE,
                    );
                }
            }
        }
        offset += read;
    }
}

/// Duplicates every vmmap entry of the parent at `+ delta`. The destination
/// must be a fresh read/write reservation.
fn duplicate_cage_memory(vmmap: &Vmmap, delta: usize) -> anyhow::Result<()> {
    let mut buf = vec![0u8; COPY_CHUNK_SIZE];
    for (_, entry) in vmmap.double_ended_iter() {
        if entry.removed || entry.npages == 0 {
            continue;
        }
        let src = vmmap.page_num_to_sys(entry.page_num);
        let len = entry.npages << PAGESHIFT;
        let dst = src.wrapping_add(delta);
        if entry.prot == PROT_NONE {
            unsafe { libc::mprotect(dst as *mut c_void, len, libc::PROT_NONE) };
            continue;
        }
        if entry.flags & MAP_SHARED as i32 != 0 {
            // Shared mappings stay shared between parent and child.
            let ret = unsafe {
                libc::syscall(
                    libc::SYS_mremap,
                    src,
                    0usize,
                    len,
                    libc::MREMAP_MAYMOVE | libc::MREMAP_FIXED,
                    dst,
                )
            };
            if ret as usize == dst {
                // The alias keeps the parent's protection key.
                pkeys::retag(dst, len, entry.prot);
                continue;
            }
            gs_debug(format!(
                "aliasing shared mapping {src:#x} failed ({}); copying it",
                std::io::Error::last_os_error()
            ));
        }
        let readable = entry.prot & PROT_READ != 0;
        if !readable {
            unsafe { libc::mprotect(src as *mut c_void, len, entry.prot | PROT_READ) };
        }
        copy_nonzero_pages(src, dst, len, &mut buf);
        if !readable {
            unsafe { libc::mprotect(src as *mut c_void, len, entry.prot) };
        }
        if entry.prot != PROT_READ | PROT_WRITE
            && unsafe { libc::mprotect(dst as *mut c_void, len, entry.prot) } != 0
        {
            anyhow::bail!(
                "mprotect of copied range {dst:#x}+{len:#x} failed: {}",
                std::io::Error::last_os_error()
            );
        }
    }
    Ok(())
}

/// Maps `fd` read/write at `addr`, tagged with `pkey` when MPK is enabled.
fn map_shared_fixed(fd: libc::c_int, addr: usize, len: usize, pkey: Option<u32>) -> anyhow::Result<()> {
    let ret = unsafe {
        libc::mmap(
            addr as *mut c_void,
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_FIXED,
            fd,
            0,
        )
    };
    if ret == libc::MAP_FAILED || ret as usize != addr {
        anyhow::bail!("mapping child stack at {addr:#x} failed: {}", std::io::Error::last_os_error());
    }
    if let Some(key) = pkey {
        pkeys::tag(addr, len, libc::PROT_READ | libc::PROT_WRITE, key)
            .map_err(|error| anyhow::anyhow!("tagging child stack at {addr:#x} failed: {error}"))?;
    }
    Ok(())
}

/// Creates the child stack: a memfd mapped at `real_lo` holding the parent
/// stack's live part `[copy_lo, parent_hi)`, aliased at every `view + offset`.
/// Every alias must fall inside `[child_base, child_base + CAGE_RESERVATION_SIZE)`.
/// The stack and its aliases are tagged with the child's key `pkey`.
fn create_child_stack(
    parent_stack: (usize, usize),
    copy_lo: usize,
    real_lo: usize,
    views: &[usize],
    offset: usize,
    child_base: usize,
    pkey: Option<u32>,
) -> anyhow::Result<()> {
    let (parent_lo, stack_size) = parent_stack;
    create_aliased_stack(real_lo, stack_size, views, offset, child_base, pkey)?;
    unsafe {
        std::ptr::copy_nonoverlapping(
            copy_lo as *const u8,
            (real_lo + (copy_lo - parent_lo)) as *mut u8,
            parent_lo + stack_size - copy_lo,
        );
    }
    Ok(())
}

/// Maps an empty `stack_size` memfd stack at `real_lo` and aliases it at every
/// `view + offset`, which must fall inside the cage reservation at `child_base`.
/// The stack and every alias are tagged with `pkey`, the key of the
/// cage owning the stack, even though the stack itself lies in the root's arena.
fn create_aliased_stack(
    real_lo: usize,
    stack_size: usize,
    views: &[usize],
    offset: usize,
    child_base: usize,
    pkey: Option<u32>,
) -> anyhow::Result<()> {
    let fd = unsafe { libc::memfd_create(c"lind-gs-fork-stack".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        anyhow::bail!("memfd_create for child stack failed: {}", std::io::Error::last_os_error());
    }
    let result = (|| {
        if unsafe { libc::ftruncate(fd, stack_size as libc::off_t) } != 0 {
            anyhow::bail!("ftruncate of child stack failed: {}", std::io::Error::last_os_error());
        }
        map_shared_fixed(fd, real_lo, stack_size, pkey)?;
        let child_end = child_base + CAGE_RESERVATION_SIZE;
        for view in views {
            let alias = view.wrapping_add(offset);
            if alias < child_base || alias.checked_add(stack_size).is_none_or(|end| end > child_end) {
                anyhow::bail!(
                    "stack alias {alias:#x} for {view:#x} is outside the child reservation {child_base:#x}..{child_end:#x}"
                );
            }
            map_shared_fixed(fd, alias, stack_size, pkey)?;
        }
        Ok(())
    })();
    unsafe { libc::close(fd) };
    result
}

// ── Context-stack rewriting ─────────────────────────────────────────────────

/// Hands the active parent contexts of a copied context stack to the child:
/// renames the cage, moves stack pointers on the parent's guest stack onto the
/// child stack and other pointers into the child's memory copy. The saved
/// PKRU of each parent context becomes `child_pkru`.
fn retarget_parent_contexts(
    stack: &mut MPKSupervisorCtxStack,
    parent: u64,
    child: u64,
    parent_stack: (usize, usize),
    stack_delta: usize,
    parent_region: (usize, usize),
    region_delta: usize,
    child_pkru: u32,
) {
    let (stack_lo, stack_hi) = parent_stack;
    let (region_lo, region_hi) = parent_region;
    let current = stack.current_context;
    for (i, context) in stack.contexts.iter_mut().enumerate() {
        if context.cage_id != parent {
            continue;
        }
        context.cage_id = child;
        context.pkru_guest_fs_slot.pkru = child_pkru as u64;
        if i >= current {
            continue;
        }
        let rsp = context.rsp as usize;
        if (stack_lo..=stack_hi).contains(&rsp) {
            context.rsp = rsp.wrapping_add(stack_delta) as u64;
        } else if (region_lo..region_hi).contains(&rsp) {
            context.rsp = rsp.wrapping_add(region_delta) as u64;
        }
        if (region_lo..region_hi).contains(&context.fs_base) {
            context.fs_base = context.fs_base.wrapping_add(region_delta);
        }
    }
}

/// Most recent guest context of `cage_id` below the current supervisor context.
fn caller_context_index(stack: &MPKSupervisorCtxStack, cage_id: u64) -> Option<usize> {
    let current = stack.current_context.min(LIND_MPK_MAX_CONTEXTS);
    (0..current).rev().find(|&i| stack.contexts[i].cage_id == cage_id)
}

/// Guest stack `(lo, size)` of the forking thread, containing `rsp`.
fn parent_guest_stack(parent: u64, rsp: usize, vmmap: &Vmmap) -> anyhow::Result<(usize, usize)> {
    let contains = |(lo, size): (usize, usize)| rsp >= lo && rsp <= lo + size;
    if let Some(cage) = registry().lock().unwrap().cages.get(&parent) {
        let stack = (cage.stack_lo, cage.stack_size);
        return if contains(stack) {
            Ok(stack)
        } else {
            Err(anyhow::anyhow!("guest rsp {rsp:#x} is not on the stack of forked cage {parent}"))
        };
    }
    if let Some(stack) = guest_stacks().lock().unwrap().get(&current_tid()).copied() {
        if contains(stack) {
            return Ok(stack);
        }
    }
    let entry = vmmap
        .find_page(vmmap.sys_to_page_num(rsp))
        .ok_or_else(|| anyhow::anyhow!("guest rsp {rsp:#x} is not in the parent's memory"))?;
    Ok((vmmap.page_num_to_sys(entry.page_num), entry.npages << PAGESHIFT))
}

// ── Fork ────────────────────────────────────────────────────────────────────

/// Undoes the parent-side allocations of a failed fork.
struct ForkCleanup {
    child_region: Option<usize>,
    slot: Option<(usize, usize)>,
}

impl Drop for ForkCleanup {
    fn drop(&mut self) {
        if let Some((root_base, slot)) = self.slot.take() {
            unsafe {
                libc::mmap(
                    slot_addr(root_base, slot) as *mut c_void,
                    STACK_SLOT_SIZE,
                    libc::PROT_NONE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS | libc::MAP_NORESERVE | libc::MAP_FIXED,
                    -1,
                    0,
                );
            }
            let slot_lo = slot_addr(root_base, slot);
            pkeys::retag(slot_lo, STACK_SLOT_SIZE, libc::PROT_NONE);
            registry().lock().unwrap().free_slot(root_base, slot);
        }
        if let Some(base) = self.child_region.take() {
            unsafe { libc::munmap(base as *mut c_void, CAGE_RESERVATION_SIZE) };
            pkeys::release_reservation(base);
        }
    }
}

struct ChildLaunch {
    tid_sender: std::sync::mpsc::SyncSender<pid_t>,
    thread_info: Arc<MpkThreadInfo>,
    registered_cage_ids: HashSet<u64>,
    child_cageid: u64,
    child_base: usize,
    child_fs: usize,
    gs_offset: usize,
    stack_lo: usize,
    stack_size: usize,
    jump_buffer_addr: usize,
    old_supervisor_stack_base: usize,
    parent_pointer_guard: u64,
}

/// Fork for MPK cages without an OS fork. Called on the parent's host thread
/// from the clone entry; `jump_buffer` is where the child resumes (returning
/// 0 from the clone entry). Returns the child cage id or a negative errno.
pub(super) fn gs_fork(parent_cageid: u64, child_cageid: u64, jump_buffer: *mut MpkJumpBuffer) -> i32 {
    match gs_fork_inner(parent_cageid, child_cageid, jump_buffer) {
        Ok(child) => child as i32,
        Err(error) => {
            eprintln!("[lind-mpk] gs-fork of cage {parent_cageid} failed: {error:#}");
            -libc::EAGAIN
        }
    }
}

fn gs_fork_inner(
    parent_cageid: u64,
    child_cageid: u64,
    jump_buffer: *mut MpkJumpBuffer,
) -> anyhow::Result<u64> {
    let tcb_size = guest_tcb_size()?;
    let sandbox_offset_field = guest_sandbox_offset()?;
    let parent_stack_offset_field = parent_stack_offset()?;
    let parent_cage = get_cage(parent_cageid)
        .ok_or_else(|| anyhow::anyhow!("parent cage {parent_cageid} not found"))?;
    let child_cage = get_cage(child_cageid)
        .ok_or_else(|| anyhow::anyhow!("child cage {child_cageid} not found"))?;

    let (parent_base, dl_allocate_tls, parent_thread) = {
        let runtime_info = parent_cage.runtime_info.read();
        let mpk = runtime_info
            .as_any()
            .downcast_ref::<MPKRuntimeInfo>()
            .ok_or_else(|| anyhow::anyhow!("parent cage has no MPKRuntimeInfo"))?;
        if mpk.pid != 0 || mpk.memory_base.is_null() {
            anyhow::bail!("parent cage does not run in the lind process");
        }
        let thread = mpk
            .threads
            .read()
            .get(&current_tid())
            .map(|thread| Arc::clone(&thread.thread_info))
            .ok_or_else(|| anyhow::anyhow!("forking thread is not registered"))?;
        (mpk.memory_base as usize, mpk.dl_allocate_tls, thread)
    };
    if parent_thread.gs_data.is_null() || parent_thread.supervisor_stack_size == 0 {
        anyhow::bail!("forking thread has no supervisor context");
    }
    let context_stack = current_context_stack();
    let caller = unsafe {
        let index = caller_context_index(&*context_stack, parent_cageid)
            .ok_or_else(|| anyhow::anyhow!("no guest context of the parent on the context stack"))?;
        (*context_stack).contexts[index]
    };
    let guest_rsp = caller.rsp as usize;

    let parent_vmmap = parent_cage.vmmap.read();
    let parent_stack = parent_guest_stack(parent_cageid, guest_rsp, &parent_vmmap)?;
    let (parent_stack_lo, stack_size) = parent_stack;
    if stack_size == 0 || stack_size % PAGE_SIZE != 0 || stack_size > CAGE_STACK_SIZE {
        anyhow::bail!("unsupported guest stack size {stack_size:#x}");
    }
    let (root_base, mut ancestor_stacks) = match registry().lock().unwrap().cages.get(&parent_cageid) {
        Some(parent) => (parent.root_base, parent.ancestor_stacks.clone()),
        None => (parent_base, Vec::new()),
    };
    ancestor_stacks.push(parent_stack_lo);

    let mut cleanup = ForkCleanup { child_region: None, slot: None };
    let child_base = mmap_aligned_cage_memory()? as usize;
    cleanup.child_region = Some(child_base);
    let slot = registry()
        .lock()
        .unwrap()
        .alloc_slot(root_base)
        .ok_or_else(|| anyhow::anyhow!("no free stack slot in the arena of {root_base:#x}"))?;
    cleanup.slot = Some((root_base, slot));

    // The child runs with only its own key. It executes the root's code, but
    // PKRU does not apply to instruction fetches, and every data access
    // (RIP-relative ones included) is GS-relative and reaches the child's copy.
    let child_key = pkeys::reservation_key(child_base)
        .ok_or_else(|| anyhow::anyhow!("child cage memory has no protection key"))?;
    let child_pkey = Some(child_key);
    let child_pkru = pkeys::pkru_for_keys(&[child_key]);
    let gs_offset = child_base.wrapping_sub(root_base);
    let region_delta = child_base.wrapping_sub(parent_base);
    let stack_lo = slot_stack_lo(root_base, slot, stack_size);
    let stack_delta = stack_lo.wrapping_sub(parent_stack_lo);
    gs_debug(format!(
        "cage {parent_cageid} -> {child_cageid}: memory {parent_base:#x} -> {child_base:#x}, \
         gs offset {gs_offset:#x}, stack {parent_stack_lo:#x} -> {stack_lo:#x} (slot {slot})"
    ));

    // ── Memory and stack ──
    duplicate_cage_memory(&parent_vmmap, region_delta)?;
    let child_vmmap = {
        let mut vmmap = parent_vmmap.clone();
        vmmap.set_base_address(child_base);
        // The child's guest addresses are those of the root, so mmap/brk and
        // pointer arguments must be shifted onto the child's memory.
        vmmap.set_logical_offset(gs_offset);
        vmmap
    };
    drop(parent_vmmap);
    let copy_lo = (guest_rsp.saturating_sub(RED_ZONE) & !(PAGE_SIZE - 1)).max(parent_stack_lo);
    let views: Vec<usize> = ancestor_stacks.iter().copied().chain(std::iter::once(stack_lo)).collect();
    create_child_stack(parent_stack, copy_lo, stack_lo, &views, gs_offset, child_base, child_pkey)?;

    // ── Supervisor state of the child thread ──
    let (context_pages, context_pages_size, new_gs_data) = allocate_context_pages()?;
    let supervisor_stack_size = parent_thread.supervisor_stack_size;
    let new_supervisor_stack = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            supervisor_stack_size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if new_supervisor_stack == libc::MAP_FAILED {
        unsafe { libc::munmap(context_pages as *mut c_void, context_pages_size) };
        anyhow::bail!("mmap for child supervisor stack failed: {}", std::io::Error::last_os_error());
    }
    let thread_info = Arc::new(MpkThreadInfo {
        gs_data: new_gs_data,
        context_pages,
        context_pages_size,
        supervisor_stack_base: new_supervisor_stack as usize,
        supervisor_stack_size,
        cage_ids: Mutex::new(HashSet::new()),
        exit_stack: AtomicUsize::new(0),
        child_tid: AtomicU64::new(0),
    });
    unsafe {
        std::ptr::copy_nonoverlapping(context_stack, new_gs_data, 1);
        std::ptr::copy_nonoverlapping(
            (parent_thread.supervisor_stack_base + CAGE_STACK_GUARD) as *const u8,
            (new_supervisor_stack as usize + CAGE_STACK_GUARD) as *mut u8,
            supervisor_stack_size - CAGE_STACK_GUARD,
        );
        libc::mprotect(new_supervisor_stack, CAGE_STACK_GUARD, libc::PROT_NONE);
        retarget_parent_contexts(
            &mut *new_gs_data,
            parent_cageid,
            child_cageid,
            (parent_stack_lo, parent_stack_lo + stack_size),
            stack_delta,
            (parent_base, parent_base + MPK_MEMORY_SIZE),
            region_delta,
            child_pkru,
        );
    }
    let child_fs = caller.fs_base.wrapping_add(region_delta);
    let field = child_fs.wrapping_add(sandbox_offset_field);
    let parent_stack_field = child_fs.wrapping_add(parent_stack_offset_field);
    if child_fs < child_base
        || field.checked_add(size_of::<usize>()).is_none_or(|end| end > child_base + MPK_MEMORY_SIZE)
        || sandbox_offset_field + size_of::<usize>() > tcb_size
        || parent_stack_field
            .checked_add(size_of::<usize>())
            .is_none_or(|end| end > child_base + MPK_MEMORY_SIZE)
        || parent_stack_offset_field + size_of::<usize>() > tcb_size
    {
        anyhow::bail!("child thread pointer {child_fs:#x} is outside the child memory");
    }
    unsafe { std::ptr::write_unaligned(field as *mut usize, gs_offset) };
    let parent_stack_base = parent_stack_lo + stack_size;
    let child_stack_base = stack_lo + stack_size;
    unsafe {
        std::ptr::write_unaligned(
            parent_stack_field as *mut usize,
            child_stack_base.wrapping_sub(parent_stack_base),
        )
    };

    let mut registered_cage_ids = parent_thread.cage_ids.lock().unwrap().clone();
    registered_cage_ids.remove(&parent_cageid);
    registered_cage_ids.insert(child_cageid);

    // ── Child cage ──
    *child_cage.vmmap.write() = child_vmmap;
    child_cage.signalhandler.clear();
    for entry in parent_cage.signalhandler.iter() {
        child_cage.signalhandler.insert(*entry.key(), *entry.value());
    }
    child_cage
        .sigset
        .store(parent_cage.sigset.load(Ordering::Relaxed), Ordering::Relaxed);
    let child_info = MPKRuntimeInfo::new(
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        dl_allocate_tls,
        0,
        child_base as *mut c_void,
        MPK_MEMORY_SIZE,
        THREAD_START_ID + 1,
        None,
    );
    child_info.pkru.store(child_pkru, Ordering::Relaxed);
    *child_cage.runtime_info.write() = Box::new(child_info);
    crate::lind_mpk::signals::register_mpk_memory_region(child_cageid, child_base, MPK_MEMORY_SIZE);
    child_cage
        .runtime_type
        .store(threei_const::RUNTIME_TYPE_MPK, Ordering::Release);
    threei::set_cage_runtime(child_cageid, threei_const::RUNTIME_TYPE_MPK);
    registry().lock().unwrap().add_child(
        child_cageid,
        ForkedCage {
            root_base,
            region_base: child_base,
            offset: gs_offset,
            slot,
            stack_lo,
            stack_size,
            ancestor_stacks,
        },
    );
    cleanup.child_region = None;
    cleanup.slot = None;
    LIND_MANAGER
        .get()
        .expect("mpk_clone: LIND_MANAGER not set – call init_mpk first")
        .increment();

    // The child resumes in the code that issued the fork, which is a grate
    // when the fork was interposed.
    let resume_gs = active_guest_gs_base(unsafe { &*new_gs_data }).unwrap_or(gs_offset);

    let (tid_sender, tid_receiver) = std::sync::mpsc::sync_channel(1);
    let launch = ChildLaunch {
        tid_sender,
        thread_info,
        registered_cage_ids,
        child_cageid,
        child_base,
        child_fs,
        gs_offset: resume_gs,
        stack_lo,
        stack_size,
        jump_buffer_addr: jump_buffer as usize,
        old_supervisor_stack_base: parent_thread.supervisor_stack_base,
        parent_pointer_guard: current_pointer_guard(),
    };
    spawn_child_thread(launch)?;
    let child_tid = tid_receiver
        .recv()
        .map_err(|_| anyhow::anyhow!("child thread exited before reporting its tid"))?;
    gs_debug(format!("cage {child_cageid} runs on host thread {child_tid}"));
    Ok(child_cageid)
}

/// Starts the child's host thread and detaches it by dropping its join handle.
fn spawn_child_thread(launch: ChildLaunch) -> anyhow::Result<()> {
    let thread = std::thread::Builder::new()
        .spawn(move || {
            let launch = Box::into_raw(Box::new(launch));
            let _ = gs_fork_thread_start(launch.cast());
        })
        .map_err(|error| anyhow::anyhow!("failed to spawn child thread: {error}"))?;
    drop(thread);
    Ok(())
}

/// Where `gs_fork_thread_start` resumes the child.
#[repr(C)]
struct ChildResume {
    jump_buffer: *mut MpkJumpBuffer,
    gs_offset: usize,
}

/// Thread start routine of a GS-forked cage. All Rust work happens in
/// `gs_fork_prepare_child`, which has returned (and dropped its state) before
/// GS is switched to the sandbox offset and `longjmp` resumes the child at
/// the parent's clone entry. `gs_fork_prepare_child` records the stack below
/// this frame as the thread's exit stack. Never returns.
#[unsafe(naked)]
extern "C" fn gs_fork_thread_start(launch: *mut c_void) -> *mut c_void {
    std::arch::naked_asm!(
        "subq $8, %rsp",
        "movq %rsp, %rsi",
        "call {prepare}",
        "wrgsbase %rdx", //second value in return struct
        "movq %rax, %rdi",
        "movl $1, %esi",
        "call {longjmp}",
        "ud2",
        prepare = sym gs_fork_prepare_child,
        longjmp = sym longjmp,
        options(att_syntax),
    );
}

/// Installs the child's thread state on its new host thread and returns the
/// relocated jump buffer to resume at.
extern "C" fn gs_fork_prepare_child(launch: *mut c_void, exit_stack: usize) -> ChildResume {
    let launch = *unsafe { Box::from_raw(launch as *mut ChildLaunch) };
    let thread_info = launch.thread_info;
    thread_info.exit_stack.store(exit_stack, Ordering::Release);
    let tid = current_tid();
    let gs_data = thread_info.gs_data;
    unsafe { (*gs_data).os_tid = tid as u64 };
    current_ctx_stack.set(gs_data);
    lind_signal_init(launch.child_cageid, MPK_EPOCH.as_ptr(), THREAD_START_ID, true);
    record_guest_stack(tid, launch.stack_lo, launch.stack_size);
    let _ = launch.tid_sender.send(tid);

    let mut cage_fs_bases = HashMap::new();
    for cage_id in launch.registered_cage_ids {
        let Some(cage) = get_cage(cage_id) else {
            lind_debug_panic(&format!("gs-fork: cage {cage_id} not found"));
        };
        let mut cage_info = MpkCageThreadInfo {
            thread_info: Arc::clone(&thread_info),
            grate_cage_id: cage_id,
            stack_addr: AtomicUsize::new(0),
            stack_base: 0,
            stack_size: 0,
            fs_base: 0,
            dtv: std::ptr::null_mut(),
            dtv_size: 0,
        };
        thread_info.register_cage(cage_id);
        if let Err(error) = cage_info.allocate_grate_stack() {
            lind_debug_panic(&format!("gs-fork: stack allocation in cage {cage_id} failed: {error}"));
        }
        let runtime_info = cage.runtime_info.read();
        let Some(mpk_info) = runtime_info.as_any().downcast_ref::<MPKRuntimeInfo>() else {
            lind_debug_panic(&format!("gs-fork: cage {cage_id} has no MPKRuntimeInfo"));
        };
        let result = if cage_id == launch.child_cageid {
            cage_info.fs_base = launch.child_fs;
            thread_info.alias_context_stack(cage_id, launch.child_fs, launch.child_base, MPK_MEMORY_SIZE)
        } else {
            cage_info.initialize_grate_tls(mpk_info)
        };
        if let Err(error) = result {
            lind_debug_panic(&format!("gs-fork: TLS setup in cage {cage_id} failed: {error}"));
        }
        cage_fs_bases.insert(cage_id, cage_info.fs_base);
        mpk_info.threads.write().insert(tid, cage_info);
    }

    let new_stack_base = thread_info.supervisor_stack_base;
    let old_stack_base = launch.old_supervisor_stack_base;
    let copied_jump_buffer =
        (new_stack_base + (launch.jump_buffer_addr - old_stack_base)) as *mut MpkJumpBuffer;
    let gs_offset = launch.gs_offset;
    unsafe {
        let supervisor_fs_base: usize;
        std::arch::asm!("rdfsbase {}", out(reg) supervisor_fs_base, options(nostack, preserves_flags));
        relocate_cloned_contexts(
            &mut *gs_data,
            old_stack_base,
            new_stack_base,
            thread_info.supervisor_stack_size,
            supervisor_fs_base,
            &cage_fs_bases,
        );
        drop(thread_info);

        let child_pointer_guard = current_pointer_guard();
        let stack_delta = new_stack_base as isize - old_stack_base as isize;
        let buffer = &mut *copied_jump_buffer;
        buffer.rsp = relocate_mangled_stack_pointer(buffer.rsp, stack_delta, launch.parent_pointer_guard, child_pointer_guard);
        buffer.rbp = relocate_mangled_stack_pointer(buffer.rbp, stack_delta, launch.parent_pointer_guard, child_pointer_guard);
        buffer.rip = relocate_mangled_stack_pointer(buffer.rip, 0, launch.parent_pointer_guard, child_pointer_guard);
    }
    ChildResume { jump_buffer: copied_jump_buffer, gs_offset }
}

unsafe extern "C" {
    fn longjmp(env: *mut libc::c_long, value: libc::c_int) -> !;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lind_mpk::RuntimeInfo::{InterposeHandlers, MPKSupervisorContext};

    const ROOT: usize = 0x1000_0000_0000;

    fn forked(root_base: usize, region_base: usize, slot: usize) -> ForkedCage {
        ForkedCage {
            root_base,
            region_base,
            offset: region_base.wrapping_sub(root_base),
            slot,
            stack_lo: slot_stack_lo(root_base, slot, CAGE_STACK_SIZE),
            stack_size: CAGE_STACK_SIZE,
            ancestor_stacks: Vec::new(),
        }
    }

    #[test]
    fn slots_fit_into_the_arena() {
        assert_eq!(STACK_SLOTS, 127);
        let last_end = slot_addr(ROOT, STACK_SLOTS - 1) + STACK_SLOT_SIZE;
        assert!(last_end <= ROOT + CAGE_RESERVATION_SIZE);
        assert_eq!(slot_stack_lo(ROOT, 0, CAGE_STACK_SIZE), ROOT + MPK_MEMORY_SIZE + CAGE_STACK_GUARD);
    }

    #[test]
    fn slots_are_allocated_per_family_and_reused() {
        let mut registry = Registry::default();
        assert_eq!(registry.alloc_slot(ROOT), Some(0));
        assert_eq!(registry.alloc_slot(ROOT), Some(1));
        assert_eq!(registry.alloc_slot(2 * ROOT), Some(0));
        registry.free_slot(ROOT, 0);
        assert_eq!(registry.alloc_slot(ROOT), Some(0));
        for _ in 2..STACK_SLOTS {
            assert!(registry.alloc_slot(ROOT).is_some());
        }
        assert_eq!(registry.alloc_slot(ROOT), None);
    }

    #[test]
    fn unrelated_cage_memory_is_unmapped_directly() {
        let mut registry = Registry::default();
        assert_eq!(registry.release(1, ROOT), vec![MemoryOp::Unmap(ROOT, CAGE_RESERVATION_SIZE)]);
    }

    #[test]
    fn root_memory_outlives_its_children() {
        let child = ROOT + CAGE_RESERVATION_SIZE + (1 << 30);
        let mut registry = Registry::default();
        let slot = registry.alloc_slot(ROOT).unwrap();
        registry.add_child(2, forked(ROOT, child, slot));

        assert!(registry.release(1, ROOT).is_empty());
        assert_eq!(
            registry.release(2, child),
            vec![
                MemoryOp::Unmap(child, CAGE_RESERVATION_SIZE),
                MemoryOp::Unmap(ROOT, CAGE_RESERVATION_SIZE),
            ]
        );
        assert!(registry.families.is_empty() && registry.cages.is_empty());
    }

    #[test]
    fn exiting_child_returns_its_slot() {
        let child = ROOT + 2 * CAGE_RESERVATION_SIZE;
        let mut registry = Registry::default();
        let slot = registry.alloc_slot(ROOT).unwrap();
        registry.add_child(2, forked(ROOT, child, slot));

        assert_eq!(
            registry.release(2, child),
            vec![
                MemoryOp::Unmap(child, CAGE_RESERVATION_SIZE),
                MemoryOp::Reserve(slot_addr(ROOT, slot), STACK_SLOT_SIZE),
            ]
        );
        assert_eq!(registry.alloc_slot(ROOT), Some(slot));
        assert_eq!(registry.release(1, ROOT), vec![MemoryOp::Unmap(ROOT, CAGE_RESERVATION_SIZE)]);
    }

    #[test]
    fn thread_stacks_return_their_slots() {
        let child = ROOT + 2 * CAGE_RESERVATION_SIZE;
        let offset = child - ROOT;
        let mut registry = Registry::default();
        let slot = registry.alloc_slot(ROOT).unwrap();
        registry.add_child(2, forked(ROOT, child, slot));
        let add_thread = |registry: &mut Registry| {
            let thread_slot = registry.alloc_slot(ROOT).unwrap();
            let lo = slot_stack_lo(ROOT, thread_slot, CAGE_STACK_SIZE);
            registry.thread_stacks.insert(lo, ThreadStack { cage_id: 2, root_base: ROOT, slot: thread_slot, offset });
            (lo, thread_slot)
        };
        let (exiting, exiting_slot) = add_thread(&mut registry);
        let (_, remaining_slot) = add_thread(&mut registry);

        let addr = slot_addr(ROOT, exiting_slot);
        assert_eq!(
            registry.release_thread_stack(exiting),
            vec![
                MemoryOp::Reserve(addr, STACK_SLOT_SIZE),
                MemoryOp::Reserve(addr + offset, STACK_SLOT_SIZE),
            ]
        );
        assert!(registry.release_thread_stack(exiting).is_empty());
        // Threads still running when the cage exits lose their aliases with it.
        assert_eq!(
            registry.release(2, child),
            vec![
                MemoryOp::Reserve(slot_addr(ROOT, remaining_slot), STACK_SLOT_SIZE),
                MemoryOp::Unmap(child, CAGE_RESERVATION_SIZE),
                MemoryOp::Reserve(slot_addr(ROOT, slot), STACK_SLOT_SIZE),
            ]
        );
        assert!(registry.thread_stacks.is_empty());
        assert_eq!(registry.alloc_slot(ROOT), Some(0));
    }

    #[test]
    fn grate_calls_switch_gs_to_the_callee_and_back_to_the_active_guest() {
        const FORKED: u64 = u64::MAX - 7;
        const GRATE: u64 = u64::MAX - 8;
        let offset = 3 * CAGE_RESERVATION_SIZE;
        registry().lock().unwrap().cages.insert(FORKED, forked(ROOT, ROOT + offset, 0));

        let mut stack = MPKSupervisorCtxStack::new(0, InterposeHandlers::default());
        stack.contexts[0].cage_id = FORKED;
        stack.current_context = 1;
        assert_eq!(active_guest_gs_base(&stack), Some(offset));
        stack.contexts[2].cage_id = GRATE;
        stack.current_context = 3;
        assert_eq!(active_guest_gs_base(&stack), Some(0));
        stack.current_context = 0;
        assert_eq!(active_guest_gs_base(&stack), None);

        // The forked cage calls into the (unforked) grate and back.
        stack.current_context = 1;
        let previous = current_ctx_stack.replace(&mut stack);
        set_gs_base(offset);
        let read_gs = || {
            let gs: usize;
            unsafe { std::arch::asm!("rdgsbase {}", out(reg) gs, options(nostack, preserves_flags)) };
            gs
        };
        assert_eq!(call_with_cage_gs(GRATE, read_gs), 0);
        assert_eq!(read_gs(), offset);
        assert_eq!(call_with_cage_gs(FORKED, read_gs), offset);
        set_gs_base(0);
        current_ctx_stack.set(previous);
        registry().lock().unwrap().cages.remove(&FORKED);
    }

    #[test]
    fn exec_of_a_child_does_not_release_the_family_twice() {
        let child = ROOT + 2 * CAGE_RESERVATION_SIZE;
        let new_memory = ROOT + 4 * CAGE_RESERVATION_SIZE;
        let mut registry = Registry::default();
        let slot = registry.alloc_slot(ROOT).unwrap();
        registry.add_child(2, forked(ROOT, child, slot));

        assert_eq!(registry.release(2, child).len(), 2);
        // After exec the cage owns plain memory again.
        assert_eq!(registry.release(2, new_memory), vec![MemoryOp::Unmap(new_memory, CAGE_RESERVATION_SIZE)]);
        assert_eq!(registry.release(1, ROOT), vec![MemoryOp::Unmap(ROOT, CAGE_RESERVATION_SIZE)]);
    }

    #[test]
    fn parent_contexts_move_to_the_child() {
        let mut stack = MPKSupervisorCtxStack::new(0, InterposeHandlers::default());
        stack.current_context = 3;
        let region = (0x10_0000_0000, 0x11_0000_0000);
        let guest_stack = (0x10_8000_0000, 0x10_8080_0000);
        stack.contexts[0] = MPKSupervisorContext { rsp: 0x10_807f_f000, fs_base: 0x10_2000_0000, cage_id: 1, ..Default::default() };
        // A grate context and the supervisor stay untouched.
        stack.contexts[1] = MPKSupervisorContext { rsp: 0x20_0000_1000, fs_base: 0x20_0000_0000, cage_id: 7, ..Default::default() };
        stack.contexts[2] = MPKSupervisorContext { rsp: 0x10_3000_0000, fs_base: 0x10_2000_0000, cage_id: 1, ..Default::default() };
        stack.contexts[3] = MPKSupervisorContext { rsp: 0x7000, fs_base: 0x9000, cage_id: 0, ..Default::default() };
        stack.contexts[5] = MPKSupervisorContext { rsp: 0x10_807f_f000, fs_base: 0x10_2000_0000, cage_id: 1, ..Default::default() };

        retarget_parent_contexts(&mut stack, 1, 2, guest_stack, 0x100, region, 0x2_0000_0000, 0x5555_5551);

        assert_eq!(stack.contexts[0].cage_id, 2);
        assert_eq!(stack.contexts[0].rsp, 0x10_807f_f100);
        assert_eq!(stack.contexts[0].fs_base, 0x12_2000_0000);
        assert_eq!(stack.contexts[1].cage_id, 7);
        assert_eq!(stack.contexts[1].rsp, 0x20_0000_1000);
        assert_eq!(stack.contexts[2].rsp, 0x12_3000_0000);
        assert_eq!(stack.contexts[3].rsp, 0x7000);
        assert_eq!(stack.contexts[3].cage_id, 0);
        // Parent contexts return into the child with the child's PKRU.
        assert_eq!(unsafe { stack.contexts[0].pkru_guest_fs_slot.pkru }, 0x5555_5551);
        assert_eq!(unsafe { stack.contexts[2].pkru_guest_fs_slot.pkru }, 0x5555_5551);
        assert_eq!(unsafe { stack.contexts[1].pkru_guest_fs_slot.pkru }, 0);
        assert_eq!(unsafe { stack.contexts[3].pkru_guest_fs_slot.pkru }, 0);
        // Inactive entries are only renamed.
        assert_eq!(stack.contexts[5].cage_id, 2);
        assert_eq!(stack.contexts[5].rsp, 0x10_807f_f000);
        assert_eq!(caller_context_index(&stack, 2), Some(2));
        assert_eq!(caller_context_index(&stack, 1), None);
    }

    fn anonymous(len: usize, prot: libc::c_int) -> usize {
        let addr = unsafe {
            libc::mmap(std::ptr::null_mut(), len, prot, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS, -1, 0)
        };
        assert_ne!(addr, libc::MAP_FAILED);
        addr as usize
    }

    #[test]
    fn copy_skips_zero_and_unreadable_pages() {
        let len = 4 * PAGE_SIZE;
        let src = anonymous(len, libc::PROT_READ | libc::PROT_WRITE);
        let dst = anonymous(len, libc::PROT_READ | libc::PROT_WRITE);
        unsafe {
            *(src as *mut u8) = 1;
            *((src + 3 * PAGE_SIZE + 7) as *mut u8) = 3;
            *((src + 2 * PAGE_SIZE) as *mut u8) = 2;
            libc::mprotect((src + 2 * PAGE_SIZE) as *mut c_void, PAGE_SIZE, libc::PROT_NONE);
        }
        let mut buf = vec![0u8; 2 * PAGE_SIZE];
        copy_nonzero_pages(src, dst, len, &mut buf);
        unsafe {
            assert_eq!(*(dst as *const u8), 1);
            assert_eq!(*((dst + 2 * PAGE_SIZE) as *const u8), 0);
            assert_eq!(*((dst + 3 * PAGE_SIZE + 7) as *const u8), 3);
            let mut resident = [0u8; 4];
            libc::mincore(dst as *mut c_void, len, resident.as_mut_ptr());
            assert_eq!(resident[1] & 1, 0, "zero page was materialized");
            libc::munmap(src as *mut c_void, len);
            libc::munmap(dst as *mut c_void, len);
        }
    }

    #[test]
    fn child_stack_is_shared_between_all_views() {
        let stack_size = 4 * PAGE_SIZE;
        // Parent stack, child reservation (two views) and the real child slot.
        let parent = anonymous(stack_size, libc::PROT_READ | libc::PROT_WRITE);
        let child_base = anonymous(CAGE_RESERVATION_SIZE, libc::PROT_NONE);
        let real = anonymous(stack_size, libc::PROT_NONE);
        unsafe {
            *((parent + stack_size - 8) as *mut u64) = 0xfeed;
            *(parent as *mut u64) = 0xdead;
        }
        let offset = (child_base + PAGE_SIZE * 16).wrapping_sub(parent);
        let copy_lo = parent + stack_size - PAGE_SIZE;
        create_child_stack((parent, stack_size), copy_lo, real, &[parent], offset, child_base, None).unwrap();
        let alias = parent.wrapping_add(offset);
        unsafe {
            assert_eq!(*((real + stack_size - 8) as *const u64), 0xfeed);
            assert_eq!(*(real as *const u64), 0, "only the live part is copied");
            *((real + 64) as *mut u64) = 0xbeef;
            assert_eq!(*((alias + 64) as *const u64), 0xbeef);
            assert_eq!(*((alias + stack_size - 8) as *const u64), 0xfeed);
            *((parent + stack_size - 8) as *mut u64) = 0;
            assert_eq!(*((alias + stack_size - 8) as *const u64), 0xfeed);
        }
        assert!(create_child_stack((parent, stack_size), copy_lo, real, &[parent + CAGE_RESERVATION_SIZE], offset, child_base, None).is_err());
        unsafe {
            libc::munmap(parent as *mut c_void, stack_size);
            libc::munmap(child_base as *mut c_void, CAGE_RESERVATION_SIZE);
            libc::munmap(real as *mut c_void, stack_size);
        }
    }
}
