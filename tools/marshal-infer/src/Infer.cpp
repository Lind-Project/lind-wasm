// Infer.cpp — see Infer.h. Library-side, best-effort marshalling inference over
// LLVM IR. We keep it deliberately heuristic ("infer the common case, flag the
// rest"): every uncertain decision becomes a warning rather than a silent guess.
#include "Infer.h"
#include "Annotations.h"

#include "llvm/IR/Argument.h"
#include "llvm/IR/DataLayout.h"
#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/GlobalVariable.h"
#include "llvm/IR/InstrTypes.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/Intrinsics.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/IR/Module.h"
#include "llvm/IR/Operator.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/ADT/SmallPtrSet.h"
#include "llvm/BinaryFormat/Dwarf.h"

#include <algorithm>

#include <string>

using namespace llvm;

namespace marshal {
namespace {

// ---- small helpers ---------------------------------------------------------

// Strip pointer-preserving casts (bitcast/addrspacecast) to find the underlying
// pointer value.
const Value *stripPtr(const Value *v) { return v->stripPointerCasts(); }

// Strip integer width casts (zext/sext/trunc) to find the underlying integer.
const Value *stripIntCasts(const Value *v) {
  while (auto *ci = dyn_cast<CastInst>(v)) {
    if (!ci->getType()->isIntegerTy())
      break;
    v = ci->getOperand(0);
  }
  return v;
}

// Map an IR Argument back to its source (DWARF) parameter index, accounting for
// a leading sret hidden pointer.
int dwarfIndexOf(const Value *v, unsigned sretOffset) {
  if (auto *a = dyn_cast<Argument>(v)) {
    int idx = (int)a->getArgNo() - (int)sretOffset;
    return idx >= 0 ? idx : -1;
  }
  return -1;
}

// Find the pointer an integer value derived from (via ptrtoint + arithmetic).
// Any arithmetic en route means the eventual pointer is reached with an offset.
const Value *underlyingPtrOfInt(const Value *iv, bool &hadOffset,
                                SmallPtrSetImpl<const Value *> &seen) {
  if (!iv || !seen.insert(iv).second) return nullptr;
  iv = stripIntCasts(iv);
  if (auto *bo = dyn_cast<BinaryOperator>(iv)) {
    hadOffset = true; // add/sub/or on a pointer-derived integer = an offset
    if (const Value *p = underlyingPtrOfInt(bo->getOperand(0), hadOffset, seen))
      return p;
    return underlyingPtrOfInt(bo->getOperand(1), hadOffset, seen);
  }
  if (auto *p2i = dyn_cast<PtrToIntInst>(iv))
    return p2i->getPointerOperand();
  return nullptr;
}

// Trace a returned pointer back to the source argument it derives from —
// following casts, GEPs, phi/select, and int<->ptr arithmetic. Sets `hadOffset`
// when the return is arg+offset (a pointer INTO the arg) vs the arg itself.
int traceReturnPtr(const Value *v, unsigned sretOffset, bool &hadOffset,
                   SmallPtrSetImpl<const Value *> &seen) {
  if (!v || !seen.insert(v).second) return -1;
  v = v->stripPointerCasts();
  if (int idx = dwarfIndexOf(v, sretOffset); idx >= 0) return idx;
  if (auto *gep = dyn_cast<GetElementPtrInst>(v)) {
    if (!gep->hasAllZeroIndices()) hadOffset = true;
    return traceReturnPtr(gep->getPointerOperand(), sretOffset, hadOffset, seen);
  }
  if (auto *i2p = dyn_cast<IntToPtrInst>(v)) {
    SmallPtrSet<const Value *, 16> iseen;
    if (const Value *p = underlyingPtrOfInt(i2p->getOperand(0), hadOffset, iseen))
      return traceReturnPtr(p, sretOffset, hadOffset, seen);
  }
  if (auto *phi = dyn_cast<PHINode>(v))
    for (const Value *iv : phi->incoming_values())
      if (int idx = traceReturnPtr(iv, sretOffset, hadOffset, seen); idx >= 0)
        return idx;
  if (auto *sel = dyn_cast<SelectInst>(v)) {
    if (int idx = traceReturnPtr(sel->getTrueValue(), sretOffset, hadOffset, seen);
        idx >= 0)
      return idx;
    return traceReturnPtr(sel->getFalseValue(), sretOffset, hadOffset, seen);
  }
  return -1;
}

// --- known opaque, library-owned types: handle, never deep-copied (C2/C4) ---
// Handle/allocator/searcher lookups now consult the (data-driven) annotation
// set — built-in defaults extended by any --annotations file. See Annotations.h.
std::string canonicalHandleClass(const std::string &typeName) {
  auto &ht = annotations().handleTypes;
  if (auto it = ht.find(typeName); it != ht.end()) return it->second;
  return typeName.empty() ? std::string("void") : typeName;
}
// A struct/union pointee is an opaque handle if either (a) it's in the
// curated handle-types table (FILE*/DIR*-style, name-driven), or (b) it's an
// INCOMPLETE type (sizeBytes==0 — forward-declared, no visible definition
// anywhere in the compilation unit, e.g. zlib's `struct internal_state;`
// behind z_stream's `state` field). (b) needs no annotation at all: there is
// provably no way to deep-copy a type whose layout doesn't exist in the TU,
// so treating it as opaque isn't a heuristic — it's the only sound option, a
// stronger signal than the curated table. Strictly gated on isComposite():
// a zero size for a NON-composite pointee (e.g. a function-pointer pointee,
// kind=Unknown, typeName="<func>") means something different — "we don't
// know this type at all" — and must NOT be swept into this handle path; that
// would silently blit a raw function pointer across cages, unsound unless
// the value is proven NULL at the call site (a still-unimplemented,
// separate, runtime-guarded capability).
bool isKnownOpaqueStruct(const TreeNode *pointee) {
  if (!pointee) return false;
  if (annotations().handleTypes.count(pointee->typeName)) return true;
  return pointee->isComposite() && pointee->sizeBytes == 0;
}

struct AllocInfo {
  bool isAlloc = false;      // plain allocation -> ptr_alloc
  bool forceLocal = false;   // realloc-style -> whole function local
  SmallVector<int, 2> sizeArgs;
};
AllocInfo allocatorInfo(StringRef raw) {
  StringRef n = raw;
  if (!n.consume_front("__libc_")) n.consume_front("__");
  AllocInfo a;
  auto &al = annotations().allocators;
  if (auto it = al.find(n.str()); it != al.end()) {
    if (it->second.forceLocal) a.forceLocal = true;
    else { a.isAlloc = true;
           for (int s : it->second.sizeArgs) a.sizeArgs.push_back(s); }
  }
  return a;
}

// Classic search functions whose result is a pointer INTO arg0, computed in a
// sibling TU (e.g. strchr -> __strchrnul) so the IR trace can't see it.
int searcherReturnsIntoArg0(StringRef raw) {
  StringRef n = raw;
  n.consume_front("__");
  return annotations().returnIntoArg0.count(n.str()) ? 0 : -1;
}

// Is arg `argIdx` a NULL-terminated array of cstr pointers (argv/envp)? The array
// walk is interprocedural (exec -> syscall), so this is data-listed.
bool isPtrArrayArg(StringRef raw, size_t argIdx) {
  StringRef n = raw;
  n.consume_front("__");
  auto &m = annotations().ptrArrayArgs;
  auto it = m.find(n.str());
  if (it == m.end()) return false;
  for (int i : it->second)
    if (i >= 0 && (size_t)i == argIdx) return true;
  return false;
}

// GENERAL detector for the "out pointer-to-pointer into another arg" case: does
// the function store into *P (P a char**/void** arg) a pointer value that derives
// from a *different* pointer argument? If so, *P is an offset into that arg (the
// endptr idiom), translatable soundly. Returns the target arg's DWARF index, or
// -1. This is the primary path — it works for any in-body write in any library;
// the strtoX name table below is only the fallback for glibc functions that
// write *endptr in a sibling TU (invisible here).
int detectOutPtrIntoArg(const Argument *P, unsigned sretOffset) {
  int pIdx = dwarfIndexOf(P, sretOffset);
  SmallVector<const Value *, 8> work;
  SmallPtrSet<const Value *, 16> seen;
  work.push_back(P);
  seen.insert(P);
  while (!work.empty()) {
    const Value *D = work.pop_back_val();
    for (const User *U : D->users()) {
      if (auto *st = dyn_cast<StoreInst>(U)) {
        if (st->getPointerOperand() == D &&
            st->getValueOperand()->getType()->isPointerTy()) {
          bool off = false;
          SmallPtrSet<const Value *, 16> s2;
          int q = traceReturnPtr(st->getValueOperand(), sretOffset, off, s2);
          if (q >= 0 && q != pIdx) return q; // *P = (value derived from arg q)
        }
      } else if (auto *g = dyn_cast<GetElementPtrInst>(U)) {
        if (g->getPointerOperand() == D && seen.insert(g).second) work.push_back(g);
      } else if (isa<BitCastInst>(U)) {
        if (seen.insert(U).second) work.push_back(U);
      }
    }
  }
  return -1;
}

// Does value V derive (via casts/GEP/phi/select) from a `load` of *P (the slot)?
bool tracesToLoadOfSlot(const Value *V, const Value *P,
                        SmallPtrSetImpl<const Value *> &seen) {
  if (!V || !seen.insert(V).second) return false;
  V = V->stripPointerCasts();
  if (auto *ld = dyn_cast<LoadInst>(V))
    return ld->getPointerOperand()->stripPointerCasts() == P;
  if (auto *g = dyn_cast<GetElementPtrInst>(V))
    return tracesToLoadOfSlot(g->getPointerOperand(), P, seen);
  if (auto *phi = dyn_cast<PHINode>(V)) {
    for (const Value *iv : phi->incoming_values())
      if (tracesToLoadOfSlot(iv, P, seen)) return true;
  }
  if (auto *sel = dyn_cast<SelectInst>(V))
    return tracesToLoadOfSlot(sel->getTrueValue(), P, seen) ||
           tracesToLoadOfSlot(sel->getFalseValue(), P, seen);
  return false;
}

// CURSOR: the function stores into *P a pointer derived from a load of *P — i.e.
// *P is advanced within its own pointee buffer (strsep stringp, mbsrtowcs *src).
bool detectCursor(const Argument *P) {
  for (const User *U : P->users()) {
    auto *st = dyn_cast<StoreInst>(U);
    if (!st || st->getPointerOperand() != P) continue;
    if (!st->getValueOperand()->getType()->isPointerTy()) continue;
    SmallPtrSet<const Value *, 16> seen;
    if (tracesToLoadOfSlot(st->getValueOperand(), P, seen)) return true;
  }
  return false;
}

// No store anywhere targets slot P, i.e. *P is only read, never written. A
// read-only inner pointer (qsort/scandir comparators: const struct dirent **).
bool slotIsReadOnly(const Argument *P) {
  for (const User *U : P->users())
    if (auto *st = dyn_cast<StoreInst>(U))
      if (st->getPointerOperand() == P) return false;
  return true;
}

// Does v resolve to the ADDRESS of a module global (a static buffer)? We follow
// address-arithmetic only (casts/GEP/PHI/select) — NOT loads: returning a value
// LOADED from a global (e.g. a heap pointer stored in a global) is not a static
// buffer, only returning &global (or &global.field) is.
bool ptrIsGlobalAddr(const Value *v, SmallPtrSetImpl<const Value *> &seen) {
  if (!v || !seen.insert(v).second) return false;
  v = v->stripPointerCasts();
  if (isa<GlobalVariable>(v)) return true;
  if (auto *g = dyn_cast<GEPOperator>(v))
    return ptrIsGlobalAddr(g->getPointerOperand(), seen);
  if (auto *phi = dyn_cast<PHINode>(v)) {
    for (const Value *iv : phi->incoming_values())
      if (ptrIsGlobalAddr(iv, seen)) return true;
    return false;
  }
  if (auto *sel = dyn_cast<SelectInst>(v))
    return ptrIsGlobalAddr(sel->getTrueValue(), seen) ||
           ptrIsGlobalAddr(sel->getFalseValue(), seen);
  return false;
}

// Every non-null return path yields a pointer into a module-global/static buffer
// (inet_ntoa/ctime/localtime return &static_buf). Conservative: if ANY return
// path is not a global address, this is not a clean static-buffer return.
bool returnDerivesFromGlobal(const Function &F) {
  bool any = false;
  for (const BasicBlock &bb : F)
    if (auto *ri = dyn_cast<ReturnInst>(bb.getTerminator()))
      if (Value *rv = ri->getReturnValue()) {
        SmallPtrSet<const Value *, 16> seen;
        if (!ptrIsGlobalAddr(rv, seen)) return false;
        any = true;
      }
  return any;
}

// Does the function return a pointer derived from a load of *P? (strsep returns
// the token, a pointer into P's cursor buffer.)
bool retDerivesFromCursor(const Function &F, const Value *P) {
  for (const BasicBlock &bb : F)
    if (auto *ri = dyn_cast<ReturnInst>(bb.getTerminator()))
      if (Value *rv = ri->getReturnValue()) {
        SmallPtrSet<const Value *, 16> seen;
        if (tracesToLoadOfSlot(rv, P, seen)) return true;
      }
  return false;
}

// The strtoX/wcstoX parse family: int strtol(const char *nptr, char **endptr,
// int base[, locale]). POSIX guarantees *endptr points INTO nptr (arg0). glibc
// writes *endptr in a sibling TU (____strtol_l_internal, only declared here), so
// detectOutPtrIntoArg can't see it — this name table is the interprocedural
// FALLBACK for that family (endptr=arg1, into=arg0).
bool isParseEndptrFn(StringRef raw) {
  StringRef n = raw.ltrim('_');   // __strtol / ____strtol_l_internal -> strip leading _
  n.consume_front("isoc23_");
  n.consume_back("_internal");    // __strtol_internal -> strtol
  n.consume_back("_l");           // strtol_l -> strtol
  if (!n.consume_front("strto") && !n.consume_front("wcsto")) return false;
  static const char *suf[] = {"l",  "ll",   "ul",  "ull",  "imax", "umax",
                              "q",  "uq",   "d",   "f",    "ld",   "f16",
                              "f32","f64",  "f128","f32x", "f64x"};
  for (const char *s : suf)
    if (n == s) return true;
  return false;
}

// Final decisiveness net (E2): any non-mappable node anywhere → force_local.
// Catches by-value struct/union/array args (which the pointer-only per-arg loop
// skips) and any other leftover. Handles are opaque — their uncopied pointee is
// not inspected.
bool treeHasUnmappable(const TreeNode *n) {
  if (n->isHandle || n->ptrIntoArg) return false;
  if (n->kind == NodeKind::Unknown) return true;
  if (n->kind == NodeKind::Pointer && n->sizeKind != SizeKind::Const &&
      n->sizeKind != SizeKind::FromArg &&
      n->sizeKind != SizeKind::FromArgPointee && n->sizeKind != SizeKind::Cstr &&
      n->sizeKind != SizeKind::PtrArray)
    return true;
  for (const auto &c : n->children)
    if (treeHasUnmappable(c.get())) return true;
  return false;
}

bool isSizeyTypeName(const std::string &t) {
  return t == "size_t" || t == "unsigned long" || t == "unsigned long long" ||
         t == "long" || t == "unsigned int" || t == "int" || t == "unsigned" ||
         t == "long unsigned int" || t == "long int";
}
// Prefer genuine size_t-shaped names over plain int when several integers exist.
int sizeyRank(const std::string &t) {
  if (t == "size_t" || t == "unsigned long" || t == "unsigned long long" ||
      t == "long unsigned int")
    return 3;
  if (t == "long" || t == "long int" || t == "unsigned int" || t == "unsigned")
    return 2;
  if (t == "int")
    return 1;
  return 0;
}

// Per-argument access summary, accumulated by walking derived pointers.
struct Access {
  bool read = false;
  bool written = false;
  bool escapes = false;     // pointer stored away / passed to unknown callee
  bool stringOp = false;    // flows into a C-string libcall
  bool unknownCallee = false;
  SmallVector<const Value *, 4> lengths; // length operands paired with this ptr
};

// Classify the role of operand `opIdx` in a call to a known lib function.
struct LibRole {
  bool known = false;
  bool reads = false;
  bool writes = false;
  bool isString = false;
  int lenOperand = -1; // operand index carrying the byte length, or -1
};

LibRole classifyLibArg(StringRef name, unsigned opIdx, const CallBase *cb) {
  LibRole r;
  auto set = [&](bool rd, bool wr, bool str, int len) {
    r.known = true; r.reads = rd; r.writes = wr; r.isString = str;
    r.lenOperand = len;
  };
  // dst=0 write, src=1 read; len at the noted operand.
  if (name == "memcpy" || name == "memmove" || name == "mempcpy") {
    if (opIdx == 0) set(false, true, false, 2);
    else if (opIdx == 1) set(true, false, false, 2);
  } else if (name == "memset") {
    if (opIdx == 0) set(false, true, false, 2);
  } else if (name == "memcmp") {
    if (opIdx <= 1) set(true, false, false, 2);
  } else if (name == "strcpy" || name == "stpcpy" || name == "strcat") {
    if (opIdx == 0) set(false, true, true, -1);
    else if (opIdx == 1) set(true, false, true, -1);
  } else if (name == "strncpy" || name == "stpncpy" || name == "strncat") {
    if (opIdx == 0) set(false, true, false, 2);
    else if (opIdx == 1) set(true, false, false, 2);
  } else if (name == "strlen") {
    if (opIdx == 0) set(true, false, true, -1);
  } else if (name == "strnlen") {
    if (opIdx == 0) set(true, false, false, 1);
  } else if (name == "strcmp" || name == "strchr" || name == "strrchr" ||
             name == "strstr" || name == "strspn" || name == "strcspn" ||
             name == "strpbrk" || name == "strdup" || name == "puts") {
    set(true, false, true, -1);
  } else if (name == "strncmp") {
    if (opIdx <= 1) set(true, false, false, 2);
  }
  (void)cb;
  return r;
}

// Walk all values derived (by GEP/cast/phi/select) from pointer argument `A`,
// recording how the pointee memory is read/written and any paired length.
Access analyzeAccess(const Argument *A) {
  Access acc;
  SmallVector<const Value *, 16> work;
  SmallPtrSet<const Value *, 32> seen;
  work.push_back(A);
  seen.insert(A);

  while (!work.empty()) {
    const Value *V = work.pop_back_val();
    for (const User *U : V->users()) {
      if (auto *ld = dyn_cast<LoadInst>(U)) {
        if (ld->getPointerOperand() == V) acc.read = true;
      } else if (auto *st = dyn_cast<StoreInst>(U)) {
        if (st->getPointerOperand() == V) acc.written = true;
        if (st->getValueOperand() == V) acc.escapes = true; // ptr stored away
      } else if (auto *gep = dyn_cast<GetElementPtrInst>(U)) {
        if (gep->getPointerOperand() == V && seen.insert(gep).second)
          work.push_back(gep);
      } else if (isa<BitCastInst>(U) || isa<AddrSpaceCastInst>(U) ||
                 isa<PHINode>(U) || isa<SelectInst>(U)) {
        if (seen.insert(U).second) work.push_back(U);
      } else if (auto *mi = dyn_cast<AnyMemIntrinsic>(U)) {
        // llvm.memcpy/memmove/memset
        if (auto *t = dyn_cast<AnyMemTransferInst>(mi)) {
          if (stripPtr(t->getRawDest()) == V) { acc.written = true; acc.lengths.push_back(t->getLength()); }
          if (stripPtr(t->getRawSource()) == V) { acc.read = true; acc.lengths.push_back(t->getLength()); }
        } else if (auto *s = dyn_cast<AnyMemSetInst>(mi)) {
          if (stripPtr(s->getRawDest()) == V) { acc.written = true; acc.lengths.push_back(s->getLength()); }
        }
      } else if (auto *cb = dyn_cast<CallBase>(U)) {
        const Function *callee = cb->getCalledFunction();
        bool handled = false;
        if (callee && callee->hasName()) {
          StringRef nm = callee->getName();
          for (unsigned oi = 0; oi < cb->arg_size(); ++oi) {
            if (cb->getArgOperand(oi) != V) continue;
            LibRole role = classifyLibArg(nm, oi, cb);
            if (role.known) {
              handled = true;
              acc.read |= role.reads;
              acc.written |= role.writes;
              acc.stringOp |= role.isString;
              if (role.lenOperand >= 0 &&
                  (unsigned)role.lenOperand < cb->arg_size())
                acc.lengths.push_back(cb->getArgOperand(role.lenOperand));
            }
          }
        }
        if (!handled) {
          // Unknown callee: be conservative — the pointee may be read and/or
          // written, and the pointer may escape.
          for (unsigned oi = 0; oi < cb->arg_size(); ++oi)
            if (cb->getArgOperand(oi) == V) {
              acc.escapes = true;
              acc.unknownCallee = true;
            }
        }
      }
    }
  }
  return acc;
}

// Annotate the fields of a struct/union pointee, best-effort. Returns whether the
// struct is fully *resolvable* (every field maps to a concrete action). We lack
// caller-side per-field access analysis, so: mark every field touched; size a
// pointer field from a size-like sibling FIELD only on a tight (buf,len) signal
// (exactly one pointer + one size_t-ish scalar, e.g. toy_buffer{data,len}); a
// function-pointer / truncated / unresolvable-nested field makes the struct
// unresolvable (caller decides shallow vs force_local).
bool annotateComposite(TreeNode *s, FunctionTrees &ft, size_t topArg) {
  // Detect the tight (buf,len) shape.
  int nPtr = 0, nSizeScalar = 0, sizeField = -1, sizeRank = 0;
  for (size_t g = 0; g < s->children.size(); ++g) {
    TreeNode *c = s->children[g].get();
    if (c->kind == NodeKind::Pointer) nPtr++;
    else if (c->kind == NodeKind::Scalar) {
      int rk = sizeyRank(c->typeName);
      if (rk >= 2) { nSizeScalar++; if (rk > sizeRank) { sizeRank = rk; sizeField = (int)g; } }
    }
  }
  bool tightPair = (nPtr == 1 && nSizeScalar == 1);

  bool resolvable = true;
  for (size_t f = 0; f < s->children.size(); ++f) {
    TreeNode *fld = s->children[f].get();
    fld->touched = true;

    if (fld->kind == NodeKind::Struct || fld->kind == NodeKind::Union) {
      if (!annotateComposite(fld, ft, topArg)) resolvable = false;
      continue;
    }
    if (fld->kind == NodeKind::Scalar || fld->kind == NodeKind::Array)
      continue; // plain data — fine
    if (fld->kind != NodeKind::Pointer) { // Unknown: fn ptr / depth-cut / cyclic
      resolvable = false;
      const char *why = fld->depthTruncated
                            ? "depth-truncated (deep/cyclic struct)"
                            : (fld->typeName == "<func>" ? "function pointer"
                                                         : "unresolved");
      ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
          fld->fieldName + "': " + why + " — not marshalable");
      continue;
    }

    // --- a pointer field ---
    TreeNode *pe = fld->children.empty() ? nullptr : fld->children[0].get();
    fld->dir = Dir::In; // nested buffers default to IN (best-effort)

    if (fld->pointeeOpaque || !pe) {            // void* field -> handle
      fld->isHandle = true; fld->handleClass = "void";
    } else if (isKnownOpaqueStruct(pe)) { // FILE*/DIR*/incomplete-composite field -> handle
      fld->isHandle = true; fld->handleClass = canonicalHandleClass(pe->typeName);
    } else if (pe->isComposite()) {
      if (pe->sizeBytes > 0 && !pe->depthTruncated &&
          annotateComposite(pe, ft, topArg)) {
        fld->sizeKind = SizeKind::Const; fld->constSize = pe->sizeBytes;
      } else {
        resolvable = false; // nested struct we can't fully resolve
        ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
            fld->fieldName + "': nested struct '" + pe->typeName +
            "' not resolvable — not marshalable");
      }
    } else if (pe->kind == NodeKind::Pointer) {       // a T** field
      TreeNode *elem = pe->children.empty() ? nullptr : pe->children[0].get();
      if (elem && elem->kind == NodeKind::Scalar && elem->sizeBytes == 1) {
        // char** field = NULL-terminated array of C-strings (alias lists:
        // group.gr_mem, hostent.h_aliases, *ent.X_aliases). Same shape as a
        // top-level argv: deep-copy each element string, NULL-terminate.
        fld->sizeKind = SizeKind::PtrArray;
        pe->sizeKind = SizeKind::Cstr;
        pe->dir = Dir::In;
        ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
            fld->fieldName + "': char** treated as NULL-terminated string array; "
            "verify elements are C-strings (not fixed-size binary)");
      } else {
        resolvable = false; // T** to non-string (linked list / opaque)
        ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
            fld->fieldName + "': pointer-to-pointer (non-string) — not marshalable");
      }
    } else if (pe->kind == NodeKind::Unknown) {       // ptr to fn/truncated
      resolvable = false;
      ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
          fld->fieldName + "': pointer to fn/unresolved — not marshalable");
    } else {                                    // ptr to scalar
      uint64_t elem = pe->sizeBytes ? pe->sizeBytes : 1;
      if (tightPair) { fld->sizeKind = SizeKind::FromArg; fld->sizeArgIndex = sizeField; }
      else if (elem == 1) fld->sizeKind = SizeKind::Cstr; // char* -> assume string
      else { fld->sizeKind = SizeKind::Const; fld->constSize = elem; } // singleton
    }
  }
  return resolvable;
}

Dir directionFrom(const Argument *A, const Access &acc) {
  bool read = acc.read, written = acc.written;
  // Refine with optimizer-proven attributes.
  if (A->onlyReadsMemory()) written = false;
  if (A->hasAttribute(Attribute::WriteOnly)) read = false;
  if (read && written) return Dir::InOut;
  if (read) return Dir::In;
  if (written) return Dir::Out;
  return Dir::Unknown; // never dereferenced here (handle candidate / unused)
}

// =============================================================================
// Variadic argument handling (design: local-notes/active/
// design-variadic-argument-handling.md). None of this is keyed on a function
// name — every detector below is a pure IR/dataflow shape test rooted at a
// "walk root": either the alloca that `llvm.va_start` writes into (for a
// variadic function), or a named parameter whose DWARF type is `va_list`
// (vprintf-shaped, not itself variadic). Sub-cases, matched in this order:
//   Z (va_copy present)      -> force_local, unclassifiable
//   D (escapes to a call)    -> force_local, type sequence not recoverable
//   A (never read, no escape)-> vacuous, nothing to marshal
//   C (sentinel-terminated loop over a pointer-shaped read) -> ptr_array
//   B (fixed, bounded reads) -> synthetic trailing args, fed through the
//                               existing per-argument machinery
//   otherwise                -> Z, force_local with a specific reason
// =============================================================================

bool isVaBookkeepingIntrinsic(Intrinsic::ID id) {
  switch (id) {
  case Intrinsic::vastart:
  case Intrinsic::vaend:
  case Intrinsic::lifetime_start:
  case Intrinsic::lifetime_end:
  case Intrinsic::dbg_declare:
  case Intrinsic::dbg_value:
  case Intrinsic::dbg_assign:
    return true;
  default:
    return false;
  }
}

// Find the alloca `llvm.va_start` writes the initial cursor into, if any.
const AllocaInst *findVaStartAlloca(const Function &F) {
  for (const BasicBlock &bb : F)
    for (const Instruction &I : bb)
      if (auto *II = dyn_cast<IntrinsicInst>(&I))
        if (II->getIntrinsicID() == Intrinsic::vastart)
          if (auto *AI = dyn_cast<AllocaInst>(
                  II->getArgOperand(0)->stripPointerCasts()))
            return AI;
  return nullptr;
}

// Does `llvm.va_copy` touch `root` (as either its dest or src operand)? A
// second, independently-walked va_list is exactly the Sub-case Z pattern the
// design calls out — never silently folded into A/B/C/D.
bool hasVaCopyTouching(const Value *root) {
  for (const User *U : root->users())
    if (auto *cb = dyn_cast<CallBase>(U))
      if (auto *II = dyn_cast<IntrinsicInst>(cb))
        if (II->getIntrinsicID() == Intrinsic::vacopy)
          return true;
  return false;
}

// Does "the current va_list value" rooted at `v` escape whole into a real call
// (i.e. get passed as an actual argument), without first being read through a
// recognized va_arg cursor-advance? Starting `v` at the alloca itself catches
// both "&ap forwarded to a callee expecting va_list*" and, by following loads
// transitively, "ap forwarded by value" (wasm32 va_list is a plain pointer, so
// a load of the alloca IS the va_list value). Starting `v` at a va_list-typed
// Argument directly catches the vprintf-shaped case with no alloca at all.
// va_start/va_end/lifetime/dbg are bookkeeping, not escapes; va_copy is
// deliberately NOT excluded here — callers check hasVaCopyTouching separately.
bool cursorEscapesToCall(const Value *v) {
  SmallPtrSet<const Value *, 16> seen;
  SmallVector<const Value *, 16> work{v};
  while (!work.empty()) {
    const Value *cur = work.pop_back_val();
    if (!seen.insert(cur).second) continue;
    for (const User *U : cur->users()) {
      if (auto *cb = dyn_cast<CallBase>(U)) {
        if (auto *II = dyn_cast<IntrinsicInst>(cb))
          if (isVaBookkeepingIntrinsic(II->getIntrinsicID()) ||
              II->getIntrinsicID() == Intrinsic::vacopy)
            continue;
        for (const Use &use : cb->args())
          if (use.get() == cur) return true;
        continue;
      }
      if (auto *ld = dyn_cast<LoadInst>(U)) {
        if (ld->getPointerOperand() == cur) work.push_back(ld);
        continue;
      }
      if (isa<BitCastInst>(U)) work.push_back(U);
    }
  }
  return false;
}

// One recovered variadic-tail access: the LoadInst that reads the actual value
// (not the cursor-advance machinery), and its concrete LLVM type.
struct VarargRead {
  const LoadInst *valueLoad;
  Type *type;
};

// A cursor-snapshot VALUE, generalized beyond "a bare load of root": either
// (a) a LoadInst reading root directly (the plain, unlooped shape), or (b) a
// PHINode that traces back to a load of root via tracesToLoadOfSlot (already
// built for the strsep cursor detector, reused here). (b) is what the
// optimizer produces for a LOOPED va_arg walk at -O1+: the cursor gets
// promoted out of the alloca into a phi merging the initial load with the
// advanced value from the previous iteration, instead of re-loading the
// alloca every iteration — confirmed by inspecting actual -O1 wasm32 codegen
// for a sentinel loop over va_arg. Both are additionally required to feed a
// GEP whose result is stored back into `root` (the advance step), so this
// stays a precise, conjunctive signal rather than matching arbitrary phis.
bool isCursorCandidate(const Value *cand, const Value *root) {
  if (auto *ld = dyn_cast<LoadInst>(cand))
    return ld->getPointerOperand() == root;
  if (isa<PHINode>(cand)) {
    SmallPtrSet<const Value *, 16> seen;
    return tracesToLoadOfSlot(cand, root, seen);
  }
  return false;
}

// Find every cursor-snapshot value for `root` that is genuinely advanced (fed
// through a GEP whose result is stored back into `root` — the advance step
// clang/LLVM emit for wasm32 va_arg, whether or not the optimizer has since
// promoted the read side into a phi), then collect every direct load of each
// confirmed snapshot, as the recovered value sequence. Returns false if `root`
// has no recognizable cursor usage at all (caller then distinguishes vacuous
// vs unclassified separately).
bool collectVarargReads(const Function &F, const Value *root,
                        SmallVectorImpl<VarargRead> &out) {
  SmallPtrSet<const Value *, 8> candidates;
  for (const User *U : root->users())
    if (auto *ld = dyn_cast<LoadInst>(U); ld && ld->getPointerOperand() == root)
      candidates.insert(ld);
  for (const BasicBlock &bb : F)
    for (const Instruction &I : bb)
      if (isa<PHINode>(&I) && isCursorCandidate(&I, root))
        candidates.insert(&I);

  SmallPtrSet<const Value *, 8> confirmed;
  for (const Value *cand : candidates) {
    for (const User *U2 : cand->users())
      if (auto *gep = dyn_cast<GetElementPtrInst>(U2))
        for (const User *U3 : gep->users())
          if (auto *st = dyn_cast<StoreInst>(U3))
            if (st->getPointerOperand() == root && st->getValueOperand() == gep)
              confirmed.insert(cand);
  }
  if (confirmed.empty()) return false;

  for (const BasicBlock &bb : F)
    for (const Instruction &I : bb)
      if (auto *ld = dyn_cast<LoadInst>(&I))
        if (confirmed.count(ld->getPointerOperand()))
          out.push_back({ld, ld->getType()});
  return true;
}

// Is `bb` part of a control-flow cycle (has a path back to itself)? Function
// CFGs here are small; a bounded forward-reachability search is sufficient —
// no need for full LoopInfo/DominatorTree machinery.
bool blockInCycle(const BasicBlock *bb) {
  SmallPtrSet<const BasicBlock *, 16> visited;
  SmallVector<const BasicBlock *, 16> work(succ_begin(bb), succ_end(bb));
  while (!work.empty()) {
    const BasicBlock *cur = work.pop_back_val();
    if (cur == bb) return true;
    if (!visited.insert(cur).second) continue;
    for (const BasicBlock *s : successors(cur)) work.push_back(s);
  }
  return false;
}

// Does `val` feed an `icmp eq/ne` against a null pointer / zero-integer
// constant whose result drives a conditional branch (a sentinel-terminated
// loop's exit test)?
bool feedsNullSentinelBranch(const Value *val) {
  for (const User *U : val->users()) {
    if (isa<BitCastInst>(U)) { if (feedsNullSentinelBranch(U)) return true; continue; }
    auto *cmp = dyn_cast<ICmpInst>(U);
    if (!cmp || !(cmp->getPredicate() == ICmpInst::ICMP_EQ ||
                  cmp->getPredicate() == ICmpInst::ICMP_NE))
      continue;
    const Value *other =
        (cmp->getOperand(0) == val) ? cmp->getOperand(1) : cmp->getOperand(0);
    bool isNullLike = isa<ConstantPointerNull>(other) ||
                      (isa<ConstantInt>(other) && cast<ConstantInt>(other)->isZero());
    if (!isNullLike) continue;
    for (const User *U2 : cmp->users())
      if (isa<BranchInst>(U2)) return true;
  }
  return false;
}

// Does a named parameter's RAW (unstripped) DWARF type resolve to va_list?
// (`buildTreeFromDIType`'s TreeNode::typeName has already stripped typedefs by
// the time we'd see it, so this walks the DISubprogram's own type array
// directly.) Returns the 1-based DWARF parameter index, or -1.
int findVaListParam(const DISubprogram *sp) {
  if (!sp) return -1;
  auto *subTy = sp->getType();
  if (!subTy) return -1;
  DITypeRefArray types = subTy->getTypeArray();
  for (unsigned i = 1; i < types.size(); ++i) {
    const DIType *ty = types[i];
    while (auto *dt = dyn_cast_or_null<DIDerivedType>(ty)) {
      if (dt->getTag() == dwarf::DW_TAG_typedef) {
        StringRef n = dt->getName();
        if (n == "va_list" || n == "__builtin_va_list" || n == "__gnuc_va_list")
          return (int)i - 1;
        ty = dt->getBaseType();
        continue;
      }
      break;
    }
  }
  return -1;
}

// A single StoreInst spilling `arg` directly into a stack slot, if the
// function makes one (only relevant for a va_list-typed PARAMETER root: it
// starts as a plain SSA value, not an alloca-backed cursor, unless the body
// itself needs to mutate it locally).
const AllocaInst *findSpillAlloca(const Argument *arg) {
  for (const User *U : arg->users())
    if (auto *st = dyn_cast<StoreInst>(U))
      if (st->getValueOperand() == arg)
        if (auto *AI = dyn_cast<AllocaInst>(st->getPointerOperand()))
          return AI;
  return nullptr;
}

// Build a synthetic TreeNode for a recovered variadic scalar/pointer read.
// There is no DWARF type here (that's the whole reason this is hard) — the
// recovered LLVM type is the entry-level NodeKind only, exactly as
// buildTreeFromDIType's DWARF-derived NodeKind is for a named argument; a
// recovered pointer still needs the existing per-argument use-def
// classification to go further, which the caller runs afterward if it can.
std::unique_ptr<TreeNode> synthNodeFromType(Type *ty) {
  auto node = std::make_unique<TreeNode>();
  if (ty->isPointerTy()) {
    node->kind = NodeKind::Pointer;
    node->typeName = "<variadic ptr>";
    node->sizeBytes = 4; // wasm32
    node->pointeeOpaque = true; // no DWARF pointee for a variadic slot, ever —
    // leave children empty (matches how buildTreeFromDIType represents a
    // named void* argument): treeHasUnmappable treats an Unknown-kind CHILD
    // as unmappable regardless of the parent's own resolved sizeKind, so an
    // opaque pointee must be represented by pointeeOpaque=true + no child,
    // never a synthesized Unknown child node.
  } else {
    node->kind = NodeKind::Scalar;
    node->typeName = "<variadic scalar>";
    node->sizeBytes = ty->getPrimitiveSizeInBits() / 8;
  }
  return node;
}

// Does `ptrVal` (a recovered opaque variadic pointer) get directly
// load/store-dereferenced within this function's own body? If so, that's real
// use-def evidence of pointer use (B' unlock condition #2) — recover the
// accessed width and a direction from it. Only a DIRECT dereference counts
// (not "forwarded to another call that might dereference it") — anything less
// direct is exactly the unproven case the conservative B' default exists for.
bool provenDirectDereference(const Value *ptrVal, uint64_t &widthOut, Dir &dirOut) {
  bool read = false, written = false;
  uint64_t width = 0;
  for (const User *U : ptrVal->users()) {
    if (auto *ld = dyn_cast<LoadInst>(U)) {
      if (ld->getPointerOperand() == ptrVal) {
        read = true;
        width = std::max(width, ld->getType()->getPrimitiveSizeInBits() / 8);
      }
    } else if (auto *st = dyn_cast<StoreInst>(U)) {
      if (st->getPointerOperand() == ptrVal) {
        written = true;
        width = std::max(width,
            st->getValueOperand()->getType()->getPrimitiveSizeInBits() / 8);
      }
    }
  }
  if (!read && !written) return false;
  widthOut = width ? width : 4;
  dirOut = (read && written) ? Dir::InOut : (written ? Dir::Out : Dir::In);
  return true;
}

// Classify one walk root (an alloca cursor, or a plain va_list-typed value)
// and mutate `ft` accordingly: append synthetic B-classified trailing args,
// or set forceLocal with a specific warning for D/Z, or do nothing for A.
// `rootForCollection` is what collectVarargReads walks (an alloca); `rootForEscape`
// is what cursorEscapesToCall/hasVaCopyTouching check (may be the same alloca,
// or a bare Argument for an unspilled va_list parameter).
void classifyVarargRoot(FunctionTrees &ft, const Function &F,
                        const Value *rootForEscape,
                        const AllocaInst *rootForCollection) {
  ft.isVariadic = true;

  if (hasVaCopyTouching(rootForEscape) ||
      (rootForCollection && hasVaCopyTouching(rootForCollection))) {
    ft.forceLocal = true;
    ft.warnings.push_back(
        "variadic: va_copy present (independently-walked va_list) — force_local");
    return;
  }
  if (cursorEscapesToCall(rootForEscape)) {
    ft.forceLocal = true;
    ft.warnings.push_back(
        "variadic: va_list escapes to another call before being read; "
        "type sequence not statically recoverable — force_local");
    return;
  }

  if (!rootForCollection) {
    // A va_list-typed PARAMETER with no spill-to-alloca and no escape: we have
    // no recognized way to see how it's used locally. Real confirmed examples
    // of this root type (vprintf/vfprintf/...) are all escape-shaped and
    // already handled above; anything else here is genuinely unclassified.
    ft.forceLocal = true;
    ft.warnings.push_back(
        "variadic: va_list parameter with unrecognized usage pattern — force_local");
    return;
  }

  SmallVector<VarargRead, 8> reads;
  bool sawCursor = collectVarargReads(F, rootForCollection, reads);
  if (!sawCursor) {
    // No recognized cursor-advance pattern, and (checked above) it doesn't
    // escape either. If the alloca has no other real uses beyond va_start/
    // va_end/lifetime/dbg, it is genuinely vacuous (Sub-case A). Any other
    // usage we don't recognize falls to Z, not a silent A.
    for (const User *U : rootForCollection->users()) {
      auto *cb = dyn_cast<CallBase>(U);
      bool bookkeeping = cb && dyn_cast<IntrinsicInst>(cb) &&
          isVaBookkeepingIntrinsic(cast<IntrinsicInst>(cb)->getIntrinsicID());
      if (!bookkeeping) {
        ft.forceLocal = true;
        ft.warnings.push_back(
            "variadic: va_arg walk shape not recognized (unmatched cursor usage) "
            "— force_local");
        return;
      }
    }
    ft.warnings.push_back("variadic: tail never read — no marshalling needed");
    return;
  }

  // Partition into a loop-shaped group (Sub-case C) and a fixed group
  // (Sub-case B), by whether each recovered read's block is in a CFG cycle.
  SmallVector<VarargRead, 8> loopGroup, fixedGroup;
  for (const VarargRead &r : reads)
    (blockInCycle(r.valueLoad->getParent()) ? loopGroup : fixedGroup)
        .push_back(r);

  if (!loopGroup.empty()) {
    if (!fixedGroup.empty()) {
      // A loop-shaped group PLUS additional fixed reads (execle's trailing
      // envp after the argv loop) needs byte-offset-aware composition this
      // pass doesn't implement yet — conservative Z rather than guessing.
      ft.forceLocal = true;
      ft.warnings.push_back(
          "variadic: sentinel loop plus additional fixed reads (composed shape) "
          "— force_local");
      return;
    }
    // Every loop-group read must be pointer-typed (that's the only shape the
    // schema has a NULL-terminated-array representation for) and every read
    // must prove its own sentinel-branch termination.
    bool allPtr = std::all_of(loopGroup.begin(), loopGroup.end(),
        [](const VarargRead &r) { return r.type->isPointerTy(); });
    bool allSentinel = std::all_of(loopGroup.begin(), loopGroup.end(),
        [](const VarargRead &r) { return feedsNullSentinelBranch(r.valueLoad); });
    if (!allPtr || !allSentinel) {
      ft.forceLocal = true;
      ft.warnings.push_back(
          "variadic: loop-shaped va_arg read is not a proven pointer sentinel "
          "loop — force_local");
      return;
    }
    auto node = std::make_unique<TreeNode>();
    node->kind = NodeKind::Pointer;
    node->typeName = "<variadic ptr_array>";
    node->sizeBytes = 4;
    node->dir = Dir::In;
    node->sizeKind = SizeKind::PtrArray;
    auto elem = std::make_unique<TreeNode>();
    elem->kind = NodeKind::Pointer;
    elem->typeName = "<variadic ptr_array element>";
    elem->sizeBytes = 4;
    elem->dir = Dir::In;
    elem->sizeKind = SizeKind::Cstr;
    auto elemPointee = std::make_unique<TreeNode>();
    elemPointee->kind = NodeKind::Scalar;
    elemPointee->typeName = "char";
    elemPointee->sizeBytes = 1;
    elem->children.push_back(std::move(elemPointee));
    node->children.push_back(std::move(elem));
    ft.params.push_back(std::move(node));
    ft.warnings.push_back(
        "variadic: sentinel-terminated loop over pointer reads — treated as a "
        "NULL-terminated array of C-string pointers (ptr_array)");
    return;
  }

  // Sub-case B: every recovered read is a fixed, unconditional (or at most
  // named-argument-gated) access — no loop involved at all.
  for (const VarargRead &r : fixedGroup) {
    auto node = synthNodeFromType(r.type);
    if (node->kind == NodeKind::Scalar) {
      ft.params.push_back(std::move(node));
      continue;
    }
    // A recovered POINTER read has no DWARF pointee — ever. Default: force
    // local (B'), UNLESS the function itself directly dereferences it (proven
    // use-def pointer use — one of the design's three B' unlock conditions).
    uint64_t width; Dir dir;
    if (provenDirectDereference(r.valueLoad, width, dir)) {
      node->dir = dir;
      node->sizeKind = SizeKind::Const;
      node->constSize = width;
      ft.params.push_back(std::move(node));
      ft.warnings.push_back(
          "variadic: opaque pointer slot proven dereferenced in-body ("
          + std::to_string(width) + " bytes) — marshalled");
    } else {
      // Left as sizeKind=NA/Pointer: the existing per-argument safety net
      // (treeHasUnmappable) already force_locals any unresolved pointer node,
      // so this is the natural, correct default without extra plumbing —
      // just make the reason explicit rather than relying on the generic
      // catch-all message.
      ft.params.push_back(std::move(node));
      ft.warnings.push_back(
          "variadic: opaque pointer slot, meaning gated by an earlier value "
          "with no proven in-body dereference — force_local (B', no unlock "
          "condition met)");
    }
  }
}

// Entry point: find every walk root on `F` (the va_start alloca if variadic,
// and/or any va_list-typed parameter) and classify each. No-op if neither
// root exists.
void inferVariadic(const Function &F, FunctionTrees &ft, unsigned sretOffset) {
  bool anyRoot = false;

  if (F.isVarArg()) {
    anyRoot = true;
    const AllocaInst *alloca = findVaStartAlloca(F);
    const Value *escapeRoot = alloca ? static_cast<const Value *>(alloca) : nullptr;
    if (escapeRoot) {
      classifyVarargRoot(ft, F, escapeRoot, alloca);
    } else {
      // isVarArg but no va_start call anywhere (never touches the tail at
      // all, or an optimizer removed the whole va_list machinery as dead
      // code) -- if there's truly no va_start, there's nothing that could
      // read the tail, matching Sub-case A.
      ft.isVariadic = true;
      ft.warnings.push_back(
          "variadic: no va_start found — tail never read, no marshalling needed");
    }
  }

  if (const DISubprogram *sp = F.getSubprogram()) {
    int vaParamIdx = findVaListParam(sp);
    if (vaParamIdx >= 0) {
      unsigned irNo = (unsigned)vaParamIdx + sretOffset;
      if (irNo < F.arg_size()) {
        anyRoot = true;
        const Argument *arg = F.getArg(irNo);
        const AllocaInst *spill = findSpillAlloca(arg);
        classifyVarargRoot(ft, F, arg, spill);
      }
    }
  }

  (void)anyRoot;
}

// =============================================================================
// WebAssembly ABI lowering layer.
//
// marshal-infer analyzes .bc produced by `-emit-llvm -c` — LLVM IR PRE-backend.
// For most C types, the ABI-lowered shape (how a value actually crosses a wasm
// function-call boundary) already matches what this IR shows: a scalar stays a
// scalar, a T* stays one pointer argument. Two families of C types do NOT
// match, and need this layer to reconcile the DWARF/IR view with reality:
//
//   1. AGGREGATES too wide to pass/return directly (an ordinary large struct
//      passed by value, or C99 _Complex, which is structurally a 2-element
//      record) — clang's FRONTEND lowers these during -emit-llvm itself, so
//      the .bc already shows the real shape as LLVM attributes: `byval` on an
//      argument, `sret` on a hidden return-pointer argument
//      (`hasByValAttr()`/`hasStructRetAttr()`, both IR-visible, no guessing
//      needed).
//   2. SCALARS wider than any native wasm value type (i32/i64/f32/f64) with no
//      dedicated hardware representation — fp128 (`long double` on this
//      target) is the only known instance. The wasm32 BACKEND legalizes these
//      by (a) returning via a hidden pointer exactly like sret, and (b)
//      splitting an argument into N raw same-width parts (2x i64 for fp128,
//      confirmed via wasm-objdump: two incoming values, stored directly,
//      never loaded through any pointer — there is no address involved at
//      all). This legalization runs strictly AFTER -emit-llvm, so there is no
//      IR attribute for it — detected via a hardcoded, target-constant rule
//      keyed on the LLVM type itself (`Type::isFP128Ty()`), not inferred.
//
// Both families funnel into representations the rest of this file already
// understands: a wrapped pointer node (byval) or extra raw scalar slots
// (fp128 args, TreeNode::abiSlots — see its comment in ParamTree.h) alongside
// the existing per-argument tree, and a synthetic leading OUT-pointer argument
// (any sret-shaped return, real or fp128-hardcoded) via
// FunctionTrees::retSretArg. Neither needs ft.params's own length/indexing to
// change — dwarfIndexOf/sretOffset elsewhere in this file, which assume one
// DWARF argument == one ft.params entry, are untouched by this layer.
// =============================================================================

// Case 2 (scalar splitting): an fp128 argument is pure raw-bits passthrough,
// never an address — dir/sizeKind stay meaningless, nothing to translate.
void lowerFp128Arg(TreeNode *node, unsigned p, FunctionTrees &ft) {
  node->abiSlots = 2;
  ft.warnings.push_back("arg" + std::to_string(p) +
      ": long double argument (fp128) — the wasm32 backend splits this into "
      "2 raw i64 slots (low half, then high half); represented as 2 "
      "plain-scalar args, no address translation needed");
}

// Case 1 (indirect aggregate passing): wrap the existing, DWARF-correct node
// as the pointee of a synthetic pointer layer — the exact shape a real T*
// argument already produces, so nested struct fields (an ordinary byval
// struct) still flow through annotateComposite and the existing global safety
// net. dir is FORCED to In regardless of what analyzeAccess would otherwise
// infer: byval guarantees the callee's copy is private, so any writes the
// callee makes to its own copy must never be read back into the caller.
void lowerByvalArg(FunctionTrees &ft, size_t p, const Argument *A,
                   const Function &F) {
  TreeNode *node = ft.params[p].get();
  uint64_t byvalSize = node->sizeBytes;
  if (Type *bt = A->getParamByValType())
    byvalSize = F.getParent()->getDataLayout().getTypeAllocSize(bt);
  auto pointee = std::move(ft.params[p]);
  if (pointee->isComposite() && pointee->sizeBytes > 0 && !pointee->depthTruncated)
    annotateComposite(pointee.get(), ft, p); // resolves nested fields; the
        // global safety net below force_locals the whole function if any
        // field comes back unresolved, exactly as for a named struct ptr.
  auto wrapper = std::make_unique<TreeNode>();
  wrapper->kind = NodeKind::Pointer;
  wrapper->typeName = pointee->typeName;
  wrapper->sizeBytes = 4; // wasm32 pointer
  wrapper->dir = Dir::In; // byval = callee's own copy; writes never reach the caller
  wrapper->sizeKind = SizeKind::Const;
  wrapper->constSize = byvalSize;
  wrapper->children.push_back(std::move(pointee));
  ft.params[p] = std::move(wrapper);
  ft.warnings.push_back("arg" + std::to_string(p) +
      ": byval-lowered aggregate (complex/large struct) — marshalled as "
      "a const-sized IN pointer, no copy-back");
}

// Case 1 (return) + case 2 (return): a hidden-pointer-shaped return, real
// (sret attribute) or hardcoded (fp128). Builds ft.retSretArg; the caller is
// responsible for forcing ft.retKind = RetKind::Void for the fp128 case (true
// sret already naturally computes Void, since F.getReturnType() really is
// void there).
void lowerAbiReturn(const Function &F, FunctionTrees &ft, bool trueSret,
                    bool fp128Ret) {
  const DataLayout &DL = F.getParent()->getDataLayout();
  Type *rt = F.getReturnType();
  uint64_t sretSize = (trueSret && F.getArg(0)->getParamStructRetType())
      ? DL.getTypeAllocSize(F.getArg(0)->getParamStructRetType())
      : DL.getTypeAllocSize(rt);
  auto sret = std::make_unique<TreeNode>();
  sret->kind = NodeKind::Pointer;
  sret->typeName = "<sret>";
  sret->sizeBytes = 4; // wasm32 pointer
  sret->dir = Dir::Out;
  sret->sizeKind = SizeKind::Const;
  sret->constSize = sretSize;
  ft.retSretArg = std::move(sret);
  ft.warnings.push_back(trueSret
      ? "return: sret-shaped (struct/complex-by-value) — copied out via a "
        "synthetic leading pointer argument, not a return value"
      : "return: long double (fp128) is sret-lowered by the wasm32 "
        "backend, invisible at this IR level — copied out via a "
        "synthetic leading pointer argument, not a return value");
}

} // namespace

void inferFunction(const Function &F, FunctionTrees &ft) {
  unsigned sretOffset =
      (F.arg_size() && F.getArg(0)->hasStructRetAttr()) ? 1 : 0;
  int cursorArg = -1; // a char** arg whose *p walks its own buffer (strsep)

  // ---- variadic tail (if any) ----
  // Runs before the named-argument loop and appends any recovered synthetic
  // trailing args to ft.params first. This is safe: scalar synthetic nodes are
  // skipped by the loop below (kind != Pointer); pointer synthetic nodes index
  // past F.arg_size(), which the loop's own bounds check already skips,
  // leaving whatever this step decided untouched.
  inferVariadic(F, ft, sretOffset);

  // ---- per-parameter analysis ----
  for (size_t p = 0; p < ft.params.size(); ++p) {
    TreeNode *node = ft.params[p].get();
    unsigned irNo = (unsigned)p + sretOffset;
    const Argument *Airn = (irNo < F.arg_size()) ? F.getArg(irNo) : nullptr;

    // WASM ABI lowering layer (see the block comment above lowerFp128Arg) --
    // both checks below fire on the LLVM Argument itself, before this node's
    // DWARF-derived NodeKind (which knows nothing about ABI-level lowering)
    // would otherwise cause it to be silently skipped or misclassified.
    if (Airn && Airn->getType()->isFP128Ty()) {
      lowerFp128Arg(node, (unsigned)p, ft);
      continue;
    }
    if (node->kind != NodeKind::Pointer && Airn && Airn->hasByValAttr()) {
      lowerByvalArg(ft, p, Airn, F);
      continue;
    }

    if (node->kind != NodeKind::Pointer)
      continue; // scalars need no marshalling
    if (irNo >= F.arg_size())
      continue;
    const Argument *A = Airn;

    Access acc = analyzeAccess(A);
    node->dir = directionFrom(A, acc);

    // --- resolve a byte length paired with this pointer (FROM_ARG / *lenptr) ---
    int sizeArg = -1;
    bool fromPointee = false;
    for (const Value *lv : acc.lengths) {
      const Value *s = stripIntCasts(lv);
      if (int idx = dwarfIndexOf(s, sretOffset); idx >= 0) { sizeArg = idx; break; }
      if (auto *ld = dyn_cast<LoadInst>(s)) {
        const Value *pp = stripPtr(ld->getPointerOperand());
        if (int idx = dwarfIndexOf(pp, sretOffset); idx >= 0) {
          sizeArg = idx; fromPointee = true; break;
        }
      }
    }
    const TreeNode *pointee =
        node->children.empty() ? nullptr : node->children[0].get();
    bool opaque = node->pointeeOpaque || !pointee;
    bool charPointee = pointee && pointee->kind == NodeKind::Scalar &&
                       pointee->sizeBytes == 1;
    bool sizeHeuristic = false; // set when sizeArg is a guess, not from dataflow

    // (buf, *lenptr) idiom: a byte/void buffer immediately followed by a pointer
    // to a size-like scalar is sized by *that pointer — compress2/uncompress's
    // (dest, uLongf *destLen). This is FROM_ARG_POINTEE; check it BEFORE the
    // plain-scalar heuristic so `dest` is sized by *destLen, not a later scalar
    // (and `source`, followed by a scalar `sourceLen`, still gets plain FROM_ARG).
    if (sizeArg < 0 && (charPointee || opaque)) {
      size_t q = p + 1;
      if (q < ft.params.size() && ft.params[q]->kind == NodeKind::Pointer &&
          !ft.params[q]->children.empty()) {
        const TreeNode *lenPointee = ft.params[q]->children[0].get();
        if (lenPointee->kind == NodeKind::Scalar &&
            sizeyRank(lenPointee->typeName) >= 2) {
          sizeArg = (int)q; fromPointee = true; sizeHeuristic = true;
        }
      }
    }

    // Heuristic fallback: in the (buf,len) idiom the length is the size-like
    // scalar that *follows* the buffer — e.g. adler32(seed, buf, len) is sized by
    // `len`, NOT the preceding `seed` (which may outrank it by type). So prefer
    // the nearest qualifying scalar after the pointer, then fall back to the
    // nearest before. A char* needs a strong (size_t-shaped, rank>=2) companion
    // so a plain int (e.g. strchr's char) doesn't count and it falls through to
    // CSTR; non-char buffers accept rank>=1.
    if (sizeArg < 0) {
      // Highest-rank size-like scalar in one half (after: q>p, before: q<p).
      auto bestSizey = [&](bool after, int minR) -> int {
        int best = -1, bestRank = 0;
        for (size_t q = 0; q < ft.params.size(); ++q) {
          if (q == p || ft.params[q]->kind != NodeKind::Scalar) continue;
          if (after ? (q < p) : (q > p)) continue;
          int rk = sizeyRank(ft.params[q]->typeName);
          if (rk >= minR && rk > bestRank) { bestRank = rk; best = (int)q; }
        }
        return best;
      };
      // A genuine size_t-shaped scalar (rank>=2) is the strong length signal;
      // prefer the one AFTER the buffer (the (buf,len) idiom: adler32(seed,buf,
      // len)->len), then before. Strong type dominates position, so memchr(s,
      // int c, size_t n) sizes by n (rank 3), not the nearer c (rank 1). Only
      // non-char buffers fall back to a weak (plain int) length.
      int best = bestSizey(/*after=*/true, 2);
      if (best < 0) best = bestSizey(/*after=*/false, 2);
      if (best < 0 && !charPointee) best = bestSizey(/*after=*/true, 1);
      if (best < 0 && !charPointee) best = bestSizey(/*after=*/false, 1);
      if (best >= 0) { sizeArg = best; sizeHeuristic = true; }
    }

    // --- pick a size kind ---

    if (opaque) {
      // void*: a buffer only if we can size it (paired length). Otherwise it is
      // an opaque object passed by reference — i.e. a handle. We bias to handle
      // because misclassifying a handle as a deep-copy buffer corrupts memory,
      // while the reverse only loses an optimization.
      if (sizeArg >= 0) {
        node->sizeKind = fromPointee ? SizeKind::FromArgPointee : SizeKind::FromArg;
        node->sizeArgIndex = sizeArg;
        if (sizeHeuristic)
          ft.warnings.push_back("arg" + std::to_string(p) +
              (fromPointee ? ": void* size taken from *arg" : ": void* size paired to arg") +
              std::to_string(sizeArg) + " heuristically");
      } else {
        node->isHandle = true;
        node->handleClass = "void";
        node->sizeKind = SizeKind::Unknown;
        node->note = "opaque void* with no length — handle candidate";
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": opaque void* treated as handle (verify; could be an unsized buffer)");
      }
    } else if (pointee->isComposite()) {
      if (isKnownOpaqueStruct(pointee)) {
        node->isHandle = true;          // FILE*/DIR*/incomplete-composite -> handle
        node->handleClass = canonicalHandleClass(pointee->typeName);
      } else if (pointee->sizeBytes > 0 && !pointee->depthTruncated &&
                 annotateComposite(node->children[0].get(), ft, p)) {
        node->sizeKind = SizeKind::Const;               // resolvable -> deep marshal
        node->constSize = pointee->sizeBytes;
      } else {
        ft.forceLocal = true;                           // cycle/fn-ptr/unsized inner
        ft.warnings.push_back("arg" + std::to_string(p) + ": struct '" +
            pointee->typeName + "' not fully resolvable — force_local");
      }
    } else if (pointee->kind == NodeKind::Unknown) {
      ft.forceLocal = true;                             // pointer to function/etc
      ft.warnings.push_back("arg" + std::to_string(p) +
          ": pointer to function/unresolved type — force_local");
    } else if (pointee->kind == NodeKind::Pointer &&
               isPtrArrayArg(ft.funcName, p)) {
      // argv/envp: a NULL-terminated array of cstr pointers (IN). Deep-copy each
      // element string and rebuild a shadow array of shadow pointers.
      node->dir = Dir::In;
      node->sizeKind = SizeKind::PtrArray;
      TreeNode *elem = node->children[0].get(); // the inner char* element
      elem->dir = Dir::In;
      elem->sizeKind = SizeKind::Cstr;
      ft.warnings.push_back("arg" + std::to_string(p) +
          ": NULL-terminated array of C-string pointers (argv/envp)");
    } else if (pointee->kind == NodeKind::Pointer) {
      // OUT pointer-to-pointer whose inner value is an offset into another arg
      // (endptr idiom). General dataflow first; name-table fallback for the glibc
      // parse family that writes *endptr in a sibling TU.
      int into = detectOutPtrIntoArg(A, sretOffset);
      bool byTable = false;
      if (into < 0 && isParseEndptrFn(ft.funcName) && p == 1) { into = 0; byTable = true; }
      bool isCursor = detectCursor(A);
      if (into >= 0 && isCursor) {
        // DUAL SOURCE: *p is written from BOTH a foreign arg AND a load of its own
        // slot (a runtime branch selects which). This is the stateful/reentrant
        // token idiom (strtok_r: s ?: *save_ptr): on continuation calls the buffer
        // lives only behind *p from a PREVIOUS call, which we never deep-copy this
        // call. Neither endptr nor cursor marshalling is sound -> run locally.
        ft.forceLocal = true;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": stateful cursor (*p sourced from arg" + std::to_string(into) +
            " or *p across calls) — force_local");
      } else if (into >= 0) {
        node->dir = Dir::Out;
        node->sizeKind = SizeKind::Const;
        node->constSize = pointee->sizeBytes ? pointee->sizeBytes : 4;
        TreeNode *inner = node->children[0].get();
        inner->ptrIntoArg = true;
        inner->intoArgIndex = into;
        ft.warnings.push_back(
            "arg" + std::to_string(p) + ": *p translated as offset into arg" +
            std::to_string(into) + (byTable ? " (parse-family fallback)"
                                            : " (out-ptr-into-arg)"));
      } else if (isCursor) {
        // CURSOR: *p walks its own deep-copied pointee buffer (strsep/mbsrtowcs).
        node->dir = Dir::InOut;
        node->sizeKind = SizeKind::Const;
        node->constSize = pointee->sizeBytes ? pointee->sizeBytes : 4;
        TreeNode *inner = node->children[0].get();
        inner->cursor = true;
        inner->sizeKind = SizeKind::Cstr;
        inner->dir = inner->pointeeConst ? Dir::In : Dir::InOut;
        cursorArg = (int)p;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": cursor — *p walks its own deep-copied buffer");
      } else if (pointee->pointeeConst && slotIsReadOnly(A) &&
                 !pointee->children.empty() &&
                 pointee->children[0]->isComposite() &&
                 !treeHasUnmappable(pointee->children[0].get())) {
        // READ-ONLY NESTED: `const struct T **p` that is read but never written
        // (qsort/scandir comparators: alphasort/versionsort take two
        // `const struct dirent **`). The inner pointer points at a single,
        // fully-mappable const struct. Deep-copy ONE element IN: copy the inner
        // pointer (4 bytes), then deep-copy its const struct pointee.
        node->dir = Dir::In;
        node->sizeKind = SizeKind::Const;
        node->constSize = pointee->sizeBytes ? pointee->sizeBytes : 4;
        TreeNode *innerPtr = node->children[0].get(); // == pointee
        innerPtr->dir = Dir::In;
        innerPtr->sizeKind = SizeKind::Const;
        innerPtr->constSize = innerPtr->children[0]->sizeBytes;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": const ** read-only — deep-copied as a single element "
            "(comparator-style); verify it is not a NULL-terminated array");
      } else {
        // genuine pointer-to-pointer (char**/void** to an independent or fresh
        // buffer): the inner pointer needs translation we can't perform, so a
        // flat const copy would be silently wrong.
        ft.forceLocal = true;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": pointer-to-pointer (inner pointer untranslatable) — force_local");
      }
    } else {
      // pointer to a scalar/array element.
      uint64_t elem = pointee->sizeBytes ? pointee->sizeBytes : 1;
      bool charLike = (elem == 1);
      if (sizeArg >= 0 && !sizeHeuristic) {
        node->sizeKind = fromPointee ? SizeKind::FromArgPointee : SizeKind::FromArg;
        node->sizeArgIndex = sizeArg;
      } else if (charLike && (acc.stringOp ||
                 /* char* with no length companion */ sizeArg < 0)) {
        node->sizeKind = SizeKind::Cstr;
        if (!acc.stringOp)
          ft.warnings.push_back("arg" + std::to_string(p) +
              ": char* assumed NUL-terminated (CSTR) — verify");
      } else if (charLike && sizeArg >= 0) {
        // byte buffer paired with a length arg (heuristically) — *lenptr or len.
        node->sizeKind = fromPointee ? SizeKind::FromArgPointee : SizeKind::FromArg;
        node->sizeArgIndex = sizeArg;
        ft.warnings.push_back("arg" + std::to_string(p) +
            (fromPointee ? ": byte buffer size taken from *arg"
                         : ": byte buffer size paired to arg") +
            std::to_string(sizeArg) + " heuristically");
      } else {
        // non-char scalar pointee with no proven length → one fixed object.
        node->sizeKind = SizeKind::Const;
        node->constSize = elem;
      }
    }

    // const-qualified pointee is a hard read-only signal: the callee cannot
    // legally write through it. This overrides an unobserved/conservative guess.
    if (node->pointeeConst && !node->isHandle)
      node->dir = Dir::In;

    // Direction defaults when not observable here (access in a callee, or via
    // pointer arithmetic we don't track).
    if (node->dir == Dir::Unknown && !node->isHandle) {
      if (node->sizeKind == SizeKind::Cstr) {
        // C strings are read-only inputs by overwhelming convention.
        node->dir = Dir::In;
      } else if (node->sizeKind != SizeKind::Unknown &&
                 node->sizeKind != SizeKind::NA) {
        // Sized buffer: copy both ways (correct, just not minimal).
        node->dir = Dir::InOut;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": direction unobserved (escapes to callee?) — defaulted to INOUT");
      } else {
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": pointer direction undetermined");
      }
    }

    // Any pointer we still couldn't size (and isn't a handle) is non-mappable →
    // the whole function runs locally (E2 / V4).
    if (!node->isHandle && node->sizeKind != SizeKind::Const &&
        node->sizeKind != SizeKind::FromArg &&
        node->sizeKind != SizeKind::FromArgPointee &&
        node->sizeKind != SizeKind::Cstr &&
        node->sizeKind != SizeKind::PtrArray) {
      ft.forceLocal = true;
    }
  }

  // Safety net: by-value struct/union args (skipped by the pointer-only loop) and
  // any other non-mappable leftover → run the call locally.
  for (const auto &pn : ft.params)
    if (treeHasUnmappable(pn.get())) {
      ft.forceLocal = true;
      ft.warnings.push_back(
          "non-mappable component (by-value struct / nested pointer) — force_local");
      break;
    }

  // ---- return kind ----
  Type *rt = F.getReturnType();

  // WASM ABI lowering layer (see the block comment above lowerFp128Arg) --
  // TRUE sret is IR-visible via hasStructRetAttr() on arg0 (sretOffset==1);
  // fp128 return is the hardcoded case, since rt genuinely says `fp128`, not
  // void, at this level (confirmed via wasm-objdump: the real compiled
  // function takes a hidden i32 sret pointer as arg0 despite the .bc showing
  // a plain `fp128 @f(...)`). Either way, lowerAbiReturn represents the
  // hidden pointer as a synthetic LEADING argument (matching its real
  // position as the first raw wasm-level call argument) via ft.retSretArg,
  // spliced in as args[0] only at JSON-emission time in main.cpp.
  bool trueSret = sretOffset == 1;
  bool fp128Ret = rt->isFP128Ty();
  if (trueSret || fp128Ret)
    lowerAbiReturn(F, ft, trueSret, fp128Ret);

  if (fp128Ret || rt->isVoidTy()) {
    ft.retKind = RetKind::Void; // fp128's real ABI-level return IS void here
  } else if (!rt->isPointerTy()) {
    ft.retKind = RetKind::Scalar;
  } else if (cursorArg >= 0 &&
             retDerivesFromCursor(F, F.getArg(cursorArg + sretOffset))) {
    // strsep returns the token — a pointer into the cursor arg's deep-copied buffer.
    ft.retKind = RetKind::PtrIntoCursor;
    ft.retAliasArg = cursorArg;
  } else {
    AllocInfo ai = allocatorInfo(ft.funcName);
    const TreeNode *retPointee =
        (ft.ret && !ft.ret->children.empty()) ? ft.ret->children[0].get() : nullptr;
    bool retKnownOpaque = retPointee && isKnownOpaqueStruct(retPointee);

    if (ai.forceLocal) {                          // realloc-style alloc+copy
      ft.forceLocal = true;
      ft.warnings.push_back("return: realloc-style allocation — force_local");
    } else if (ai.isAlloc) {                      // malloc/calloc/... -> caller-cage alloc
      ft.retKind = RetKind::PtrAlloc;
      for (int s : ai.sizeArgs) ft.retAllocSizeArgs.push_back(s);
    } else if (retKnownOpaque) {                  // fopen/opendir -> FILE*/DIR* handle
      ft.retKind = RetKind::Handle;
      ft.retHandleClass = canonicalHandleClass(retPointee->typeName);
    } else {
      // does the return derive from an argument's buffer (alias vs into)?
      int aliasArg = -1; bool hadOffset = false;
      for (const BasicBlock &bb : F)
        if (auto *ri = dyn_cast<ReturnInst>(bb.getTerminator()))
          if (Value *rv = ri->getReturnValue()) {
            SmallPtrSet<const Value *, 16> seen;
            if (int idx = traceReturnPtr(rv, sretOffset, hadOffset, seen); idx >= 0)
              aliasArg = idx;
          }
      if (aliasArg >= 0) {
        ft.retKind = hadOffset ? RetKind::PtrIntoArg : RetKind::PtrAliasArg;
        ft.retAliasArg = aliasArg;
      } else if (int si = searcherReturnsIntoArg0(ft.funcName); si >= 0) {
        ft.retKind = RetKind::PtrIntoArg; ft.retAliasArg = si;  // strchr/index/...
      } else if (returnDerivesFromGlobal(F) && retPointee &&
                 retPointee->kind == NodeKind::Scalar &&
                 retPointee->sizeBytes == 1) {
        // &static char buffer (inet_ntoa/ctime/asctime, fixed strings) -> copy out.
        ft.retKind = RetKind::PtrToStatic;
        ft.retStaticSize = 0;                     // 0 => NUL-terminated C-string
        ft.warnings.push_back("return: pointer into a static/global buffer — "
            "copy-out (cstr); valid until next call");
      } else if (returnDerivesFromGlobal(F) && retPointee &&
                 (retPointee->isComposite() ||
                  retPointee->kind == NodeKind::Scalar ||
                  retPointee->kind == NodeKind::Array) &&
                 retPointee->sizeBytes && !treeHasUnmappable(retPointee)) {
        // &static struct/scalar (localtime/gmtime -> struct tm*) -> copy out.
        ft.retKind = RetKind::PtrToStatic;
        ft.retStaticSize = retPointee->sizeBytes;
        ft.warnings.push_back("return: pointer into a static/global buffer — "
            "copy-out (" + std::to_string(retPointee->sizeBytes) +
            " bytes); valid until next call");
      } else {
        ft.forceLocal = true;                     // unclassified pointer return
        ft.warnings.push_back("return: pointer of undetermined provenance — force_local");
      }
    }
  }
}

} // namespace marshal
