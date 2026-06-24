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
  Unknown,       // could not size — residue
};
const char *sizeKindName(SizeKind s);

// What the return value is / how it must be translated.
enum class RetKind {
  Void,
  Scalar,
  PtrAliasArg,   // returns a pointer arg unchanged (== arg, no offset)
  PtrIntoArg,    // returns a pointer INTO an arg buffer (arg + offset): memchr/strchr
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
                           // sibling field index (struct context) holds the size
  uint64_t constSize = 0;  // Const: byte count

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

  // KSplit projection bit (meaningful for struct fields): marshal this field
  // only if set. Library-side best-effort = mark every field touched (we lack
  // the caller side needed for the precise both-sides intersection).
  bool touched = true;

  // True if the depth cap truncated this node (cycle / very deep nesting).
  bool depthTruncated = false;

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
