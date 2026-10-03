// ParamTree.h — DWARF-driven parameter tree (the structural skeleton of a
// function argument), the analog of KSplit's Tree.cpp / CALI's createSubnodes.
//
// For each function argument (and the return value) we unfold its DWARF type
// (DIType) into a tree: a pointer gets one pointee child; a struct/union gets
// one child per field, recursively. This is the foundation onto which later
// passes hang direction / size / touched / handle information. It maps onto our
// runtime's struct lind_layout / lind_field (tests/grate-tests/lib-interpose/
// lind_marshal.h).
//
// We drive everything from DWARF, NOT from LLVM IR types, because clang-18 emits
// opaque pointers (`ptr`) — the pointee shape only exists in debug info.
#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

namespace llvm {
class DIType;
class DISubprogram;
} // namespace llvm

namespace marshal {

// What a node fundamentally is, before any access/size analysis refines it.
enum class NodeKind {
  Scalar,  // integer / float / enum — passed by value
  Pointer, // a pointer; has exactly one child = the pointee
  Struct,  // composite record; children = fields
  Union,   // composite union; children = arms
  Array,   // fixed/!fixed array; child = element type
  Unknown, // void* / unresolved / cycle-cut — residue
};

const char *nodeKindName(NodeKind k);

// Data-flow direction for a pointer node (copy-in / copy-out semantics).
// NA for non-pointers; Unknown when not yet analyzed.
enum class Dir { NA, In, Out, InOut, Unknown };
const char *dirName(Dir d);

// How the byte-extent of a pointer's referenced region is determined.
// Mirrors lind_marshal.h's lind_size_kind (roughly).
enum class SizeKind {
  NA,            // not a pointer
  Const,         // const bytes (sizeof pointee struct/scalar)
  FromArg,       // value of another argument (buf,len idiom) -> sizeArgIndex
  FromArgPointee,// *(*lenptr) — length read through another pointer arg
  Cstr,          // NUL-terminated
  PtrArray,      // NULL-terminated array of pointers (argv/envp); pointee = element
  // Strided vector: bytes = (1 + (n-1)*stride) * constSize, where n and
  // stride each independently come from sizeOperand/strideOperand -- either
  // read from a caller argument at dispatch time (BLAS-style, e.g. `incx`;
  // neither knowable statically) or a compile-time constant baked into the
  // spec (an ordinary `x[i]` loop with no separate increment argument at
  // all). Mirrors lind_marshal.h's LIND_SIZE_STRIDE_VECTOR exactly; see its
  // doc comment there for the n<=0 / negative-stride edge cases (an
  // argument-sourced negative stride aborts the whole grate at dispatch
  // time -- a deliberate runtime-side choice, not something inference can
  // avoid by not emitting this kind for a function that might see a
  // negative stride at some call site; a constant-sourced stride is always
  // proven positive before inference ever emits it). A function PROVEN to
  // rebase its own base pointer forward before walking backward -- so the
  // touched region, relative to the ORIGINAL argument, is the ordinary
  // magnitude-based envelope -- represents that proof by wrapping the
  // stride operand in an explicit ABS node instead (see LlmPrompt.cpp's
  // own proof obligation for when this is and isn't valid); the plain
  // StrideVector operand pair below never does this on its own.
  StrideVector,
  Unknown,       // could not size — residue
};
const char *sizeKindName(SizeKind s);

// How a StrideVector extent operand's raw wasm value is obtained. A scalar
// may be passed BY VALUE (CBLAS: `int n`) or BY REFERENCE (classic Fortran
// BLAS: `int *N`, unpacked as `n = *N` at function entry) -- the runtime
// needs, per operand, whether the raw argument slot IS the number or points
// to it. A third source, Constant, has no argument behind it at all: a
// genuine compile-time-constant element count (most commonly a stride of 1
// for an ordinary `x[i]` loop with no separate increment parameter --
// proven directly from the loop's own IR, see unwrapConstantStride in
// Infer.cpp). Mirrors lind_marshal.h's lind_extent_source.
enum class ExtentSource { Value, PointeeI32, Constant };
const char *extentSourceName(ExtentSource s);

// One runtime extent operand (a StrideVector length or stride): either
// which top-level argument and how to read it (Value/PointeeI32 -- argIndex
// meaningful, argIndex<0 means "not found"), or a value fixed at analysis
// time with no argument behind it (Constant -- constValue meaningful,
// argIndex unused). Mirrors lind_marshal.h's lind_extent_operand.
struct ExtentOperand {
  int argIndex = -1;
  ExtentSource source = ExtentSource::Value;
  uint64_t constValue = 0; // meaningful only when source == Constant
  bool valid() const {
    return source == ExtentSource::Constant ? constValue > 0 : argIndex >= 0;
  }
};

// How a StrideVector decision was reached.
//   Proven     — the length AND the address induction variable's zero start
//                were both proven exactly by static analysis (an exact
//                ScalarEvolution trip-count proof, or
//                detectFactoredStrideTripCount's decomposition -- see
//                Infer.cpp); runtime memory safety follows from that proof.
//   Configured — a checked-in config file's "contracts" entry (Config.h)
//                asserted this argument's extent: a human verified it,
//                presumably because static analysis couldn't. Validated
//                against the target function's real signature before it is
//                ever applied (see validateContractAgainstSignature).
// There is no third, unproven-and-unasserted level: a pairing this tool
// cannot prove and no contract asserts fails closed (force_local) instead
// of being accepted at reduced confidence.
enum class Confidence { Proven, Configured };
const char *confidenceName(Confidence c);

// What the return value is / how it must be translated.
enum class RetKind {
  Void,
  Scalar,
  PtrAliasArg,   // returns a pointer arg unchanged (== arg, no offset)
  PtrIntoArg,    // returns a pointer INTO an arg buffer (arg + offset): memchr/strchr
  PtrIntoCursor, // returns a pointer into a cursor arg's deep-copied buffer (strsep)
  PtrToStatic,   // returns a pointer into a module-global/static buffer (inet_ntoa,
                 // ctime, localtime) — copy OUT to caller; valid until next call
  PtrAlloc,      // freshly-allocated buffer (malloc family) — caller-cage alloc
  Handle,        // opaque constructor result (FILE*/DIR*) -> handle token
  ForceLocal,    // can't classify / must run locally
};
const char *retKindName(RetKind r);

// One node of the parameter tree.
struct TreeNode {
  NodeKind kind = NodeKind::Unknown;

  // Source-level field name when this node is a struct member ("" for roots,
  // pointees, array elements).
  std::string fieldName;
  // The (stripped) DWARF type name, for diagnostics ("toy_buffer", "char", ...).
  std::string typeName;

  // Layout, in BYTES, on the wasm32 target.
  //   sizeBytes : sizeof this node's type (struct_size for records).
  //   offsetBytes: byte offset of this field within its parent record
  //                (0 for non-member nodes).
  uint64_t sizeBytes = 0;
  uint64_t offsetBytes = 0;

  // For Pointer nodes only: true if the pointee type was unresolved (void*),
  // which downstream becomes residue (FORCE_LOCAL + warning).
  bool pointeeOpaque = false;

  // For Pointer nodes: pointee is const-qualified (const char*, const void*) —
  // a strong read-only (IN-direction) signal.
  bool pointeeConst = false;

  // For Pointer nodes: inferred copy direction (set by the inference step, not
  // the DWARF builder). NA for non-pointers.
  Dir dir = Dir::NA;

  // For Pointer nodes: how the referenced region is sized.
  SizeKind sizeKind = SizeKind::NA;
  int sizeArgIndex = -1;   // FromArg/FromArgPointee: which arg (top-level) or
                           // sibling field index (struct context) holds the
                           // element count. Unused for StrideVector -- see
                           // sizeOperand below.
  // StrideVector only: the length (element count) and per-element stride
  // operands, each independently carrying how its raw value is obtained
  // (direct vs. loaded through a pointer argument -- see ExtentOperand).
  ExtentOperand sizeOperand;
  ExtentOperand strideOperand;
  uint64_t constSize = 0;  // Const: byte count. StrideVector: per-element byte size.
  // How this StrideVector decision was reached -- Proven unless a checked-in
  // config's contract asserted it (Configured). Emitted in the output JSON
  // as "confidence" only for StrideVector nodes; every other sizeKind's
  // decision has exactly one way to be reached, so the field stays at the
  // default and is not emitted. See Confidence's own comment.
  Confidence confidence = Confidence::Proven;

  // For Pointer nodes: this pointer is an opaque handle (translate via token
  // table, never deep-copy the pointee). E.g. FILE*, z_stream's state, toy_ctx.
  bool isHandle = false;
  std::string handleClass;  // canonical grouping key (the pointee type name)

  // For Pointer-to-struct nodes: const-sized flat blit, inner pointers left
  // untranslated (not chased). Advisory — the copy uses the existing const path.
  bool shallow = false;

  // For an inner pointer that is written as an offset INTO another argument's
  // buffer (e.g. strtol's *endptr -> into the input string). The runtime
  // translates the value as src_base(intoArgIndex) + (written - shadow_base).
  bool ptrIntoArg = false;
  int  intoArgIndex = -1;

  // For an inner pointer that is a CURSOR into its own (deep-copied) pointee
  // buffer: it is advanced within the buffer (strsep/mbsrtowcs *p). The runtime
  // deep-copies the pointee per `dir`, then translates the advanced pointer back
  // as source_base + (written - shadow_base).
  bool cursor = false;

  // KSplit projection bit (meaningful for struct fields): marshal this field
  // only if set. Library-side best-effort = mark every field touched (we lack
  // the caller side needed for the precise both-sides intersection).
  bool touched = true;

  // True if the depth cap truncated this node (cycle / very deep nesting).
  bool depthTruncated = false;

  // WASM ABI LOWERING: how many raw wasm-level call-site slots this ONE
  // logical (DWARF-level) parameter actually occupies. Almost always 1 --
  // every ordinary scalar/pointer argument is exactly one wasm value. Set to
  // N>1 for a value whose LLVM type has no native wasm value-type
  // representation and gets legalized by the wasm32 BACKEND (SelectionDAG
  // type-legalization, which runs after -emit-llvm has already produced the
  // bitcode this tool reads) into N raw same-width parts passed as N separate
  // positional arguments -- e.g. fp128 (`long double`) always splits into 2
  // raw i64 slots (confirmed via wasm-objdump: two incoming values, stored
  // directly, never loaded through any pointer -- there is no address
  // involved at all). This is a hardcoded, hardware-target-level fact about
  // the LLVM type itself (not something inferred per-function), distinct from
  // -- and detected completely differently than -- the FRONTEND-level
  // indirect-passing lowering (`hasByValAttr`) below, which clang's own
  // codegen already makes IR-visible; a >1-native-word-wide SCALAR type with
  // no dedicated wasm value type is expanded into parts, while an aggregate
  // (struct/_Complex) too wide to pass directly goes indirect via a pointer.
  // For abiSlots>1, `kind` stays whatever the DWARF-derived type says
  // (Scalar), `dir`/`sizeKind` are meaningless (multi-slot values are pure
  // raw-bits passthrough, never an address, so nothing needs translating) --
  // main.cpp expands this ONE TreeNode into N consecutive plain-scalar JSON
  // "args" entries at emission time, mirroring how FunctionTrees::retSretArg
  // is spliced in as a synthetic argument only at emission time: ft.params's
  // own length/indexing (and every dwarfIndexOf/sretOffset computation
  // elsewhere in Infer.cpp that assumes 1 DWARF argument == 1 vector entry)
  // never needs to change to accommodate it.
  uint32_t abiSlots = 1;

  // Free-form per-node note explaining a residue / heuristic decision.
  std::string note;

  std::vector<std::unique_ptr<TreeNode>> children;

  bool isPointer() const { return kind == NodeKind::Pointer; }
  bool isComposite() const {
    return kind == NodeKind::Struct || kind == NodeKind::Union;
  }
};

// A function's full parameter forest: one tree per parameter, plus the return.
struct FunctionTrees {
  std::string funcName;
  std::unique_ptr<TreeNode> ret;                       // may be null (void)
  std::vector<std::unique_ptr<TreeNode>> params;       // one per source parameter

  // Filled by the inference step (Infer.cpp).
  RetKind retKind = RetKind::Void;
  int retAliasArg = -1;                  // PtrAliasArg / PtrIntoArg: which arg
  std::vector<int> retAllocSizeArgs;     // PtrAlloc: 1 arg (malloc) or 2 (calloc)
  std::string retHandleClass;            // Handle return: canonical class
  uint64_t retStaticSize = 0;            // PtrToStatic: copy-out byte count
                                         // (0 => NUL-terminated C-string)

  // True if the function is variadic (F.isVarArg()) OR takes an explicit
  // va_list-typed parameter (vprintf-shaped). Recovered variadic-tail slots, if
  // any, are appended to `params` as synthetic trailing TreeNodes (built from a
  // recovered LLVM type rather than DWARF, since variadic args have no DWARF
  // type) and classified through the same machinery as named parameters.
  bool isVariadic = false;

  // Set when the return is sret-shaped: the wasm32 ABI writes the real result
  // through a hidden pointer instead of a genuine return value (retKind is
  // Void in this case — that IS the real ABI-level return). This happens for
  // (a) TRUE sret, IR-visible via hasStructRetAttr() on arg0 — ordinary large
  // struct-by-value and C99 _Complex returns; (b) long double (fp128) on this
  // target, which is NOT IR-visible (the wasm32 backend's split/indirection is
  // a legalization detail applied after the .bc this tool reads) and is
  // instead detected via a hardcoded Type::isFP128Ty() check. Either way this
  // node (kind=Pointer, dir=Out, sizeKind=Const) is spliced in as the FIRST
  // emitted argument at JSON-emission time (main.cpp) — matching its real
  // position as the first raw wasm-level call argument — rather than folded
  // into `params`/`ret`, so none of this file's existing DWARF-index
  // arithmetic (dwarfIndexOf/sretOffset) needs to change to accommodate it.
  std::unique_ptr<TreeNode> retSretArg;

  // Per-function verdict (E2). When true, the runtime runs the call locally and
  // ignores args/ret; the record is emitted in compact form (name+decision+warnings).
  bool forceLocal = false;

  std::vector<std::string> warnings;     // residue report
};

// Build the parameter forest for a function from its DISubprogram debug info.
// `maxDepth` bounds recursion (cycle/blowup guard, like KSplit's depth cap).
// Returns nullptr if the function has no usable debug info.
std::unique_ptr<FunctionTrees> buildFunctionTrees(const llvm::DISubprogram *sp,
                                                  unsigned maxDepth = 6);

// Build a single tree from a DIType root (exposed for testing / reuse).
std::unique_ptr<TreeNode> buildTreeFromDIType(const llvm::DIType *ty,
                                              unsigned maxDepth);

} // namespace marshal
