// A pointer traced from an argument is itself CALLED as an indirect
// function target (not merely passed as an argument to one -- contrast
// ../stride_vector_extent/indirect_callee.c, where the traced pointer is
// an ARGUMENT of the indirect call, not the target). Distinguishing the
// two matters: this is walkTaint's `cb->getCalledOperand() == V` branch,
// a different code path from "a tainted pointer passed to an indirect
// call".
typedef void (*fnptr)(void);

void caller_ptr_as_target(void *x) {
    ((fnptr)x)();
}
