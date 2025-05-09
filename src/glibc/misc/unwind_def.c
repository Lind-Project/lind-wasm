// lind-wasm: define these symbol with empty content

#include <unwind.h>

_Unwind_Reason_Code _Unwind_Backtrace (_Unwind_Trace_Fn fn, void * arg) {
    return 0;
}

_Unwind_Ptr _Unwind_GetIP (struct _Unwind_Context * ctx) {
    return 0;
}

_Unwind_Word _Unwind_GetGR (struct _Unwind_Context * ctx, int arg) {
    return 0;
}

_Unwind_Word _Unwind_GetCFA (struct _Unwind_Context * ctx) {
    return 0;
}
