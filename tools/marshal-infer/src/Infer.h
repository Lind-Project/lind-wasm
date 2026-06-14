// Infer.h — the inference step. Given a function's LLVM IR body and its
// DWARF-derived parameter forest, annotate the forest with marshalling info:
// pointer direction (IN/OUT/INOUT), size kind (CONST/FROM_ARG/FROM_ARG_POINTEE/
// CSTR/UNKNOWN), opaque-handle flags, and the return kind — plus a per-function
// warnings (residue) list. This is the library-side, best-effort analog of
// KSplit's read/write + NesCheck + size analyses (see research/arg-marshalling/).
#pragma once

#include "ParamTree.h"

namespace llvm {
class Function;
}

namespace marshal {

// Annotate `ft` in place using the IR of `f`. `ft` must already hold the DWARF
// parameter trees (from buildFunctionTrees); this fills dir/sizeKind/handle/ret.
void inferFunction(const llvm::Function &f, FunctionTrees &ft);

} // namespace marshal
