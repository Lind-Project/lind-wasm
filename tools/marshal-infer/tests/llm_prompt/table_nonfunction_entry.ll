; Hand-written LLVM IR (not produced via scripts/lind_compile): a function-
; pointer table with one entry that names something other than a function
; or a null -- here, the address of a plain data object, standing in for
; any constant this tool does not interpret as a callee (a raw address, an
; inttoptr, ...). Unlike a provable null (a legitimate "no kernel for this
; slot" placeholder), this is NOT safe to silently skip: it could be a real,
; unexamined callee reached through a cast this tool does not follow. C
; offers no way to put a non-function, non-null value into a function-
; pointer-typed table entry without a type error, so this fixture is
; hand-written.
@table_stray_data = global i32 0

@__const.caller_table_nonfunction.table = private unnamed_addr constant [2 x ptr] [ptr @table_kernel_ok, ptr @table_stray_data]

define void @table_kernel_ok(ptr %x) {
entry:
  %v = load double, ptr %x
  %r = fadd double %v, 1.000000e+00
  store double %r, ptr %x
  ret void
}

define void @caller_table_nonfunction(i32 %which, ptr %x) {
entry:
  %idx = and i32 %which, 1
  %slot = getelementptr inbounds [2 x ptr], ptr @__const.caller_table_nonfunction.table, i32 0, i32 %idx
  %fp = load ptr, ptr %slot
  call void %fp(ptr %x)
  ret void
}
