# set follow-fork-mode child
set environment LD_DEBUG=files
set environment LIND_MPK_DEBUG=1

b make_syscall
comm
cont
end

define run_lind
    run --preload env=/lib/libc.cwasm --preload env=/lib/libm.cwasm \
    $arg0 $arg1
    # b *0x00007ffff7b0ffdf
end

#0x00007ffff7a00000
#0x00007ffff7c81000
#b *0x00007ffff7b0ffdf