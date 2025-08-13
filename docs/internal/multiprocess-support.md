---
id: Multi-process Support
---

# Multi-Process Support in Lind-Wasm

## Multi-processing via Asyncify

The way multi-process (specifically clone_syscall, exit_syscall and longjmp) works in lind-wasm heavily depends on Asyncify from Binaryen. So let’s first introduce how Asyncify works on WebAssembly.\
So Asyncify is a second-time compilation that adds some logic to the existing compiled file.\
The Asyncify works by having a few global variables that define the current execution status. One global variable is to describe the current status of stack unwind/rewind. If current_state is set to unwind, that means the current process is undergoing stack unwind, and if current_state is set to rewind, that means the current process is undergoing stack rewind, and if current_state is set to normal, that means the current process is working normally, just like no Asyncify has applied to it.\
\
For example, suppose there is a program looks like this:

```
int funcA()
{
	int a;
	int b;

	for... {
		...do some work...
	}

	funcB();

	...do some work...
}

int funcB()
{
	...do some work
      imported_wasm_functionC();
}

```

After applying Asyncify, it may become something like this:

```
int funcA()
{
	if(current_state == rewind) {
		restore_functionA_context();
	}
	if(current_state == normal) {
		int a;
		int b;

		for... {
			...do some work...
		}
	}
	if(last_unwind_return_is_here)
	{
		funcB();
		if(current_state == unwind) {
			save_functionA_context();
			return;
		}
	}

	if(current_state == normal) {
		...do some work...
	}
}

int funcB()
{
	if(current_state == rewind) {
		restore_functionB_context();
	}
	if(current_state == normal) {
		if(current_state == normal) {
			...do some work
		}
	}

	if(last_unwind_return_is_here) {
		imported_wasm_functionC();
		if(current_state == unwind) {
			save_functionB_context();
			return;
		}
	}
}

```

So Asyncify basically adds an if statement for all the normal user code and only executes the user code if current_state is normal. After a function has been executed, it will check if current_state is set to unwind. If that is the case, the function context will be saved and the function will return immediately. When rewind happens later, the function context will be restored at the beginning of the function.\
\
Besides these, Asyncify also has four functions that control the global current_state.\
**Asyncify_unwind_start**: Once called, set current_state to unwind and return\
**Asyncify_unwind_stop**: Once called, set current_state to normal and return\
**Asyncify_rewind_start**: Once called, set current_state to rewind and return\
**Asyncify_rewind_stop**: Once called, set current_state to normal and return\
**Asyncify_unwind_start** and **Asyncify_rewind_start** also takes an additional argument that specifies where to store/retrieve the unwind_data (i.e. function context).

Such transformation from Asyncify allows you to freely navigate the callstack of a process, but with the cost of largely increased binary size, and slightly decreased performance (from a bunch of extra if statements added by Asyncify).

### fork()
The fork syscall is built up on Asyncify. When fork is called, the whole wasm process would undergo unwind and rewind. But the unwind_data (function context) is copied once unwind is done. The unwind_data could basically be viewed as a snapshot of the callstack (with the unwind_data, we can restore the wasm process to the state when unwind_data is captured). With such a powerful mechanism, the implementation of the fork is pretty straightforward: once we capture the snapshot of the parent process callstack, we can let the child do the rewind with the unwind_data from parent, and the child will be able to return to the exact state when parent calls fork. Threading creation is very similar to this, except that the memory is shared between parent and child.


### exit() and exec()
Exit syscall is currently also built on Asyncify, by performing the unwind on the process, then instead of doing rewinding, the process can just return.

Exec syscall is built upon Exit syscall: instead of returning directly after unwind is finished, a new wasm instance is created with the supplied binary path.

### setjmp() and longjmp()
**Setjmp** and **longjmp** implementation is also very similar to fork: When setjmp is called, the process will undergo unwind and rewind, leaving an unwind_data (callstack snapshot). The unwind_data is saved somewhere. When later the process calls longjmp and specifies a restore to the previous state, the process first will unwind, after unwind is finished, its unwind_data will be replaced by the old unwind_data generated when setjmp is called. Then after rewind, the process can restore to its previous state. 

### wait()
Last we have our **wait_syscall** which is implemented purely in rawposix and does not use Asyncify at all. Wait_syscall works by maintaining a zombie relationship in the cage struct: when a cage exits, it will insert itself into the parent’s zombie list. Therefore, the parent can simply check its zombie list when doing the wait syscall, and retrieve the first zombie in the list (first in first out).
