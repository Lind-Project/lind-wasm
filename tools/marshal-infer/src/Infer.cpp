// Infer.cpp — see Infer.h. Library-side, best-effort marshalling inference over
// LLVM IR. We keep it deliberately heuristic ("infer the common case, flag the
// rest"): every uncertain decision becomes a warning rather than a silent guess.
#include "Infer.h"

#include "llvm/IR/Argument.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/InstrTypes.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/ADT/SmallVector.h"
#include "llvm/ADT/SmallPtrSet.h"

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

// Trace a pointer value back to the source argument it derives from, following
// casts, GEPs (pointer-into-arg), and phi/select. Returns the DWARF index or -1.
int tracePtrToArg(const Value *v, unsigned sretOffset,
                  SmallPtrSetImpl<const Value *> &seen) {
  if (!v || !seen.insert(v).second) return -1;
  v = v->stripPointerCasts();
  if (int idx = dwarfIndexOf(v, sretOffset); idx >= 0) return idx;
  if (auto *gep = dyn_cast<GetElementPtrInst>(v))
    return tracePtrToArg(gep->getPointerOperand(), sretOffset, seen);
  if (auto *phi = dyn_cast<PHINode>(v)) {
    for (const Value *iv : phi->incoming_values())
      if (int idx = tracePtrToArg(iv, sretOffset, seen); idx >= 0) return idx;
  }
  if (auto *sel = dyn_cast<SelectInst>(v)) {
    if (int idx = tracePtrToArg(sel->getTrueValue(), sretOffset, seen); idx >= 0)
      return idx;
    return tracePtrToArg(sel->getFalseValue(), sretOffset, seen);
  }
  return -1;
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

// Annotate the fields of a struct/union pointee, best-effort. We lack per-field
// access analysis on the caller side, so: mark every field touched; size pointer
// fields from a size-like SIBLING FIELD (the (buf,len)-in-struct idiom, e.g.
// toy_buffer{data,len}); default nested-pointer direction to IN (nested buffers
// are inputs by convention). Recurses into nested structs.
void annotateComposite(TreeNode *s, FunctionTrees &ft, size_t topArg) {
  for (size_t f = 0; f < s->children.size(); ++f) {
    TreeNode *fld = s->children[f].get();
    fld->touched = true;
    if (fld->kind == NodeKind::Struct || fld->kind == NodeKind::Union) {
      annotateComposite(fld, ft, topArg);
      continue;
    }
    if (fld->kind != NodeKind::Pointer)
      continue;

    // Find a size-like sibling scalar field (becomes FROM_ARG sibling index).
    int sib = -1, rank = 0;
    for (size_t g = 0; g < s->children.size(); ++g) {
      if (g == f) continue;
      TreeNode *c = s->children[g].get();
      if (c->kind == NodeKind::Scalar) {
        int rk = sizeyRank(c->typeName);
        if (rk > rank) { rank = rk; sib = (int)g; }
      }
    }
    TreeNode *pe = fld->children.empty() ? nullptr : fld->children[0].get();
    bool opaque = fld->pointeeOpaque || !pe;
    // nested buffers default to IN (best-effort); const makes it certain.
    fld->dir = Dir::In;

    if (opaque) {
      fld->isHandle = true; fld->handleClass = "void";
      fld->sizeKind = SizeKind::Unknown;
      ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
          fld->fieldName + "': opaque void* nested pointer — handle candidate");
    } else if (pe->isComposite()) {
      if (pe->sizeBytes > 0 && !pe->depthTruncated) {
        fld->sizeKind = SizeKind::Const; fld->constSize = pe->sizeBytes;
        annotateComposite(pe, ft, topArg);
      } else {
        fld->isHandle = true; fld->handleClass = pe->typeName;
        fld->sizeKind = SizeKind::Unknown;
        ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
            fld->fieldName + "': opaque/recursive struct — handle candidate");
      }
    } else {
      uint64_t elem = pe->sizeBytes ? pe->sizeBytes : 1;
      if (sib >= 0) {
        fld->sizeKind = SizeKind::FromArg; // sibling FIELD index
        fld->sizeArgIndex = sib;
      } else if (elem == 1) {
        fld->sizeKind = SizeKind::Cstr;
      } else {
        fld->sizeKind = SizeKind::Const; fld->constSize = elem;
      }
      ft.warnings.push_back("arg" + std::to_string(topArg) + " field '" +
          fld->fieldName + "': nested pointer direction assumed IN");
    }
  }
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

} // namespace

void inferFunction(const Function &F, FunctionTrees &ft) {
  unsigned sretOffset =
      (F.arg_size() && F.getArg(0)->hasStructRetAttr()) ? 1 : 0;

  // ---- per-parameter analysis ----
  for (size_t p = 0; p < ft.params.size(); ++p) {
    TreeNode *node = ft.params[p].get();
    if (node->kind != NodeKind::Pointer)
      continue; // scalars need no marshalling
    unsigned irNo = (unsigned)p + sretOffset;
    if (irNo >= F.arg_size())
      continue;
    const Argument *A = F.getArg(irNo);

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

    // Heuristic fallback: pick the most size_t-shaped scalar argument. For a
    // char* (likely a C string) require a *strong* size_t-shaped companion
    // (rank>=2); a plain int is more likely a flag/char (e.g. strchr's char),
    // so leave such pointers to fall through to CSTR.
    bool sizeHeuristic = false;
    if (sizeArg < 0) {
      int best = -1, bestRank = 0;
      for (size_t q = 0; q < ft.params.size(); ++q) {
        if (q == p || ft.params[q]->kind != NodeKind::Scalar) continue;
        int rk = sizeyRank(ft.params[q]->typeName);
        if (rk > bestRank) { bestRank = rk; best = (int)q; }
      }
      int minRank = charPointee ? 2 : 1;
      if (best >= 0 && bestRank >= minRank) { sizeArg = best; sizeHeuristic = true; }
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
              ": void* size paired to arg" + std::to_string(sizeArg) + " heuristically");
      } else {
        node->isHandle = true;
        node->handleClass = "void";
        node->sizeKind = SizeKind::Unknown;
        node->note = "opaque void* with no length — handle candidate";
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": opaque void* treated as handle (verify; could be an unsized buffer)");
      }
    } else if (pointee->isComposite()) {
      // pointer to struct/union: copy sizeof(pointee); fields handled by layout.
      if (pointee->sizeBytes > 0 && !pointee->depthTruncated) {
        node->sizeKind = SizeKind::Const;
        node->constSize = pointee->sizeBytes;
        annotateComposite(node->children[0].get(), ft, p);
      } else {
        node->isHandle = true;
        node->handleClass = pointee->typeName;
        node->sizeKind = SizeKind::Unknown;
        node->note = "opaque/cyclic struct — handle candidate";
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": opaque/recursive struct '" + pointee->typeName +
            "' treated as handle");
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
        // byte buffer paired with a length arg (heuristically).
        node->sizeKind = SizeKind::FromArg;
        node->sizeArgIndex = sizeArg;
        ft.warnings.push_back("arg" + std::to_string(p) +
            ": byte buffer size paired to arg" + std::to_string(sizeArg) +
            " heuristically");
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
  }

  // ---- return kind ----
  Type *rt = F.getReturnType();
  if (rt->isVoidTy()) {
    ft.retKind = RetKind::Void;
  } else if (!rt->isPointerTy()) {
    ft.retKind = RetKind::Scalar;
  } else {
    // pointer return: does it alias one of the pointer args?
    int aliasArg = -1;
    for (const BasicBlock &bb : F) {
      if (auto *ri = dyn_cast<ReturnInst>(bb.getTerminator())) {
        if (Value *rv = ri->getReturnValue()) {
          SmallPtrSet<const Value *, 16> seen;
          if (int idx = tracePtrToArg(rv, sretOffset, seen); idx >= 0)
            aliasArg = idx;
        }
      }
    }
    bool retOpaque = ft.ret && ft.ret->pointeeOpaque;
    if (aliasArg >= 0) {
      ft.retKind = RetKind::PtrAliasArg;
      ft.retAliasArg = aliasArg;
    } else if (F.returnDoesNotAlias()) {
      if (retOpaque) {
        ft.retKind = RetKind::Handle;     // constructor of an opaque object
        if (ft.ret) { ft.ret->isHandle = true; ft.ret->handleClass = "void"; }
      } else {
        ft.retKind = RetKind::ForceLocal; // fresh malloc'd buffer
        ft.warnings.push_back("return: freshly-allocated pointer (FORCE_LOCAL)");
      }
    } else {
      ft.retKind = RetKind::PtrUnknown;
      ft.warnings.push_back("return: pointer of undetermined provenance");
    }
  }
}

} // namespace marshal
