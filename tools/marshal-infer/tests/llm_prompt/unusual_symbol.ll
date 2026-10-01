; Hand-written LLVM IR (not produced via scripts/lind_compile): a C
; identifier cannot contain a path separator or a space, so this is the one
; fixture in this suite that cannot be expressed as ordinary C source --
; it exists specifically to exercise sanitizeFunctionNameForFilename
; against a function name containing characters that would otherwise
; escape --prompt-output if used unsanitized in a file path. No debug info
; either, doubling as a plain (non-callee) no-DWARF entry-function case.
define void @"weird/name with spaces"(i32 %n, ptr %x) {
entry:
  store i32 %n, ptr %x
  ret void
}
