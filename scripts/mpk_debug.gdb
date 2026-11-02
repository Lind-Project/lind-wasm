# set follow-fork-mode child
set environment LD_DEBUG=files
set environment LD_SHOW_AUXV=1
set environment LIND_MPK_DEBUG=1
set environment LIND_VMMAP_DEBUG=1
# handle SIGUSR2 nostop print

b make_syscall
command
   continue
end

b lind_mpk/execute.rs:776
command
   continue
end

define load_libc
    set $libc_text_off = $arg0 + 0x02e670
    printf "Loading symbols for libc.so at %p\n", $libc_text_off
    add-symbol-file lindfs/lib/elf/libc.so.6 $libc_text_off
end

define llibc
    load_libc 0x00007ffff7800000
end


define load_syms
    p/x $arg0
    p/x $arg1
    set $fork_exec_text_off = $arg1 + 0x001120

   load_libc $arg0
    printf "Loading symbols for fork_exec.so at %p\n", $fork_exec_text_off

    add-symbol-file lindfs/$arg2 $fork_exec_text_off
    printf "Loaded symbols for libc.so and %s\n", $arg2
end



define run_lind
    run $arg0
    load_syms 0x00007ffff7800000 0x00007ffff7a7d000 $arg0
    # load_syms 0x00007ffff7800000 0x00007ffff7c81000 "$arg0"
    # b *0x00007ffff7b0ffdf
end

#0x00007ffff7a00000
#0x00007ffff7c81000
#b *0x00007ffff7b0ffdf