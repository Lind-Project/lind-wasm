# function to set the stack pointer
# used by pthread to set thread stack pointer
# takes one argument (i32), and set __stack_pointer to the value
# technically this could be done directly in wasmtime, but I havn't dived into
# the way to manually set this yet. Using a helper wasm asm seems fine currently.
	.text

	.export_name	set_stack_pointer, set_stack_pointer

	.globaltype	__stack_pointer, i32

	.hidden	set_stack_pointer
	.globl	set_stack_pointer
	.type	set_stack_pointer,@function

set_stack_pointer:
	.functype	set_stack_pointer (i32) -> ()

	local.get   0  # start_arg
	global.set  __stack_pointer

	end_function
	