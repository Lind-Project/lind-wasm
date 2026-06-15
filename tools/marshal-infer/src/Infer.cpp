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
// Canonicalize so typedef spellings collapse to one class string.
std::string canonicalHandleClass(const std::string &typeName) {
  if (typeName == "_IO_FILE" || typeName == "FILE") return "FILE";
  if (typeName == "__dirstream" || typeName == "DIR") return "DIR";
  if (typeName == "__locale_struct") return "locale";
  if (typeName.empty()) return "void";
  return typeName;
}
bool isKnownOpaqueStruct(const std::string &typeName) {
  return typeName == "_IO_FILE" || typeName == "__dirstream" ||
         typeName == "__locale_struct";
}

// --- allocator classification (C3) ---
// allocsize attr is absent in our -O1 glibc bitcode, so use a curated table.
struct AllocInfo {
  bool isAlloc = false;      // plain allocation -> ptr_alloc
  bool forceLocal = false;   // realloc-style -> whole function local
  SmallVector<int, 2> sizeArgs;
};
AllocInfo allocatorInfo(StringRef raw) {
  StringRef n = raw;
  if (!n.consume_front("__libc_")) n.consume_front("__");
  AllocInfo a;
  if (n == "malloc" || n == "valloc" || n == "pvalloc") { a.isAlloc = true; a.sizeArgs = {0}; }
  else if (n == "aligned_alloc" || n == "memalign") { a.isAlloc = true; a.sizeArgs = {1}; }
  else if (n == "calloc") { a.isAlloc = true; a.sizeArgs = {0, 1}; }
  else if (n == "realloc" || n == "reallocarray" || n == "posix_memalign" ||
           n == "strdup" || n == "strndup")
    a.forceLocal = true;
  return a;
}

// Classic search functions whose result is a pointer INTO arg0. Their return is
// computed in a callee (e.g. strchr -> __strchrnul + select), so the intraproc
// IR trace can't see it — recognize them by name (V1).
int searcherReturnsIntoArg0(StringRef raw) {
  StringRef n = raw;
  n.consume_front("__");
  if (n == "strchr" || n == "strrchr" || n == "index" || n == "rindex" ||
      n == "strchrnul" || n == "memchr" || n == "memrchr" || n == "rawmemchr" ||
      n == "strstr" || n == "strcasestr" || n == "strpbrk")
    return 0;
  return -1;
}

// Final decisiveness net (E2): any non-mappable node anywhere → force_local.
// Catches by-value struct/union/array args (which the pointer-only per-arg loop
// skips) and any other leftover. Handles are opaque — their uncopied pointee is
// not inspected.
bool treeHasUnmappable(const TreeNode *n) {
  if (n->isHandle) return false;
  if (n->kind == NodeKind::Unknown) return true;
  if (n->kind == NodeKind::Pointer && n->sizeKind != SizeKind::Const &&
      n->sizeKind != SizeKind::FromArg &&
      n->sizeKind != SizeKind::FromArgPointee && n->sizeKind != SizeKind::Cstr)
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
    } else if (isKnownOpaqueStruct(pe->typeName)) { // FILE*/DIR* field -> handle
      fld->isHandle = true; fld->handleClass = canonicalHandleClass(pe->typeName);
    } else if (pe->isComposite()) {
      if (pe->sizeBytes > 0 && !pe->depthTruncated &&
          annotateComposite(pe, ft, topArg)) {
        fld->sizeKind = SizeKind::Const; fld->constSize = pe->sizeBytes;
      } else {
        resolvable = false; // nested struct we can't fully resolve
      }
    } else if (pe->kind == NodeKind::Unknown ||
               pe->kind == NodeKind::Pointer) { // ptr to fn/truncated/ptr
      resolvable = false;
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
      if (isKnownOpaqueStruct(pointee->typeName)) {
        node->isHandle = true;                          // FILE*/DIR* -> handle
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
    } else if (pointee->kind == NodeKind::Pointer) {
      // pointer-to-pointer (char**, void**): the inner pointer needs translation
      // we can't perform, so a flat const copy would be silently wrong.
      ft.forceLocal = true;
      ft.warnings.push_back("arg" + std::to_string(p) +
          ": pointer-to-pointer (inner pointer untranslatable) — force_local");
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
        node->sizeKind != SizeKind::Cstr) {
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
  if (rt->isVoidTy()) {
    ft.retKind = RetKind::Void;
  } else if (!rt->isPointerTy()) {
    ft.retKind = RetKind::Scalar;
  } else {
    AllocInfo ai = allocatorInfo(ft.funcName);
    const TreeNode *retPointee =
        (ft.ret && !ft.ret->children.empty()) ? ft.ret->children[0].get() : nullptr;
    bool retKnownOpaque = retPointee && isKnownOpaqueStruct(retPointee->typeName);

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
      } else {
        ft.forceLocal = true;                     // unclassified pointer return
        ft.warnings.push_back("return: pointer of undetermined provenance — force_local");
      }
    }
  }
}

} // namespace marshal
