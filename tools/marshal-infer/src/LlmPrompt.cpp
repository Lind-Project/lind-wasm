// LlmPrompt.cpp — see LlmPrompt.h. Deliberately self-contained (does not
// reuse Infer.cpp's file-local helpers): this module performs EVIDENCE
// COLLECTION for a human/LLM reader, not the PROOF static inference
// performs, and the two have different soundness requirements -- evidence
// collection must be permissive about what it INCLUDES, but precise about
// what it claims is COMPLETE.

#include "LlmPrompt.h"

#include "llvm/ADT/StringExtras.h"
#include "llvm/IR/Constants.h"
#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/GlobalAlias.h"
#include "llvm/IR/GlobalVariable.h"
#include "llvm/IR/Instructions.h"
#include "llvm/IR/IntrinsicInst.h"
#include "llvm/IR/Module.h"
#include "llvm/Support/SHA256.h"
#include "llvm/Support/raw_ostream.h"

#include <algorithm>
#include <deque>
#include <map>
#include <regex>
#include <set>
#include <sstream>

using namespace llvm;

namespace marshal {

namespace {

// ---------------------------------------------------------------------
// Small, local helpers (deliberately not shared with Infer.cpp -- see
// this file's own header comment).
// ---------------------------------------------------------------------

const Value *stripCasts(const Value *V) {
  for (;;) {
    if (auto *ci = dyn_cast<CastInst>(V)) { V = ci->getOperand(0); continue; }
    break;
  }
  return V;
}

unsigned sretOffset(const Function &F) {
  return (F.arg_size() && F.getArg(0)->hasStructRetAttr()) ? 1 : 0;
}

// Returns the 0-based LLVM argument index `V` is a DIRECT alias of --
// itself (through casts), or a load through a pointer that is itself a
// direct alias (the Fortran-by-reference idiom, e.g. `load i32, ptr %N`
// where %N is a parameter) -- or -1 for anything else: a GEP, arithmetic,
// a PHI/select merge point, or a value with no traceable origin at all.
// Deliberately conservative: this decides whether a call ARGUMENT can be
// reported as a clean identity correspondence versus a transformed value
// whose provenance must be shown via its defining instruction instead
// (always visible regardless, since the whole function body is included).
int directParamIndex(const Value *V, const Function &F) {
  const Value *S = stripCasts(V);
  if (auto *A = dyn_cast<Argument>(S))
    if (A->getParent() == &F) return (int)A->getArgNo();
  if (auto *LD = dyn_cast<LoadInst>(S))
    return directParamIndex(LD->getPointerOperand(), F);
  return -1;
}

// DWARF parameter names, keyed by LLVM (not source-level) argument index --
// shifted by sretOffset when the function has a hidden leading sret
// pointer, since that argument is real at the LLVM level but absent from
// DWARF's own parameter list entirely (source code never named it). Empty
// when the function has no debug info at all -- a label, never a
// requirement (see LlmPrompt.h): every caller falls back to a synthetic
// "argN" identity, which is what the canonical identity always is anyway.
std::map<unsigned, std::string> collectParamNames(const Function &F) {
  std::map<unsigned, std::string> names;
  DISubprogram *sp = F.getSubprogram();
  if (!sp) return names;
  unsigned off = sretOffset(F);
  for (DINode *node : sp->getRetainedNodes()) {
    auto *lv = dyn_cast<DILocalVariable>(node);
    if (!lv || lv->getArg() == 0) continue; // getArg()==0 -> not a parameter
    unsigned llvmIdx = (lv->getArg() - 1) + off;
    if (!lv->getName().empty()) names[llvmIdx] = lv->getName().str();
  }
  return names;
}

std::string argLabel(unsigned idx, const std::map<unsigned, std::string> &names) {
  auto it = names.find(idx);
  std::string base = "arg" + std::to_string(idx);
  if (it == names.end()) return base;
  return base + " (" + it->second + ")";
}

std::string typeStr(Type *Ty) {
  std::string s;
  raw_string_ostream rso(s);
  Ty->print(rso);
  return s;
}

std::string argKind(Type *Ty) {
  if (Ty->isPointerTy()) return "pointer";
  if (Ty->isIntegerTy() || Ty->isFloatingPointTy()) return "scalar";
  return "unsupported";
}

// Metadata attachment kinds stripped from rendered IR: all of them are
// dangling references here regardless (Function::print never resolves or
// prints the module-level node table a per-function excerpt would need to
// look them up), and none are needed for pointer-region reasoning --
// calling-convention, parameter attributes (nocapture/readonly/sret/byval/
// noalias, all printed INLINE on the signature, never behind one of these),
// memory-effect summaries (the "; Function Attrs:" comment line, likewise
// inline), and control flow are all preserved untouched.
const char *const kStrippedMetadataKinds[] = {
    "dbg", "tbaa", "tbaa.struct", "llvm.loop", "llvm.access.group",
    "noalias", "alias.scope", "prof", "llvm.mem.parallel_loop_access",
};

std::regex buildMetadataStripRegex() {
  std::string alt;
  for (size_t i = 0; i < std::size(kStrippedMetadataKinds); ++i) {
    if (i) alt += "|";
    // '.' inside a kind name (tbaa.struct) is a literal dot, not "any char".
    for (const char *p = kStrippedMetadataKinds[i]; *p; ++p)
      alt += (*p == '.') ? "\\." : std::string(1, *p);
  }
  return std::regex(",\\s*!(" + alt + ")\\s+![0-9]+");
}

// Renders a function's own IR (signature + body only -- Function::print
// never includes module-level globals/other functions) as an EXCERPT, not
// self-contained IR: attribute-group references (`#N`, resolved only via a
// module-level table this excerpt does not include) and the metadata
// attachment kinds above are stripped rather than left dangling. Pure
// debug-bookkeeping calls (llvm.dbg.{value,declare,assign} -- the same
// classification Infer.cpp's isVaBookkeepingIntrinsic already uses) are
// dropped entirely, not just their attachment.
std::string renderFunctionIR(const Function &F) {
  std::string raw;
  {
    raw_string_ostream rso(raw);
    F.print(rso);
  }
  static const std::regex dbgCall(
      R"(^\s*(tail\s+)?call\s+void\s+@llvm\.dbg\.(value|declare|assign)\()");
  static const std::regex metadataAttach = buildMetadataStripRegex();
  static const std::regex attrGroupRef(R"(\s#[0-9]+\b)");

  std::string out;
  std::istringstream in(raw);
  std::string line;
  while (std::getline(in, line)) {
    if (std::regex_search(line, dbgCall)) continue;
    line = std::regex_replace(line, metadataAttach, "");
    line = std::regex_replace(line, attrGroupRef, "");
    out += line;
    out += "\n";
  }
  return out;
}

unsigned countInstructions(const Function &F) {
  unsigned n = 0;
  for (const BasicBlock &bb : F) n += (unsigned)bb.size();
  return n;
}

// Stable (basic-block ordinal, instruction ordinal) identity for `cb`
// within its own defining function -- see CallSiteId's own comment on why
// this exists instead of a pointer-derived identity.
CallSiteId computeCallSiteId(const CallBase *cb) {
  const Function *F = cb->getFunction();
  unsigned bbOrd = 0;
  for (const BasicBlock &bb : *F) {
    if (&bb == cb->getParent()) {
      unsigned instOrd = 0;
      for (const Instruction &I : bb) {
        if (&I == cb) return {bbOrd, instOrd};
        ++instOrd;
      }
    }
    ++bbOrd;
  }
  return {0, 0}; // unreachable: cb's own getParent()/getFunction() are consistent
}

std::optional<unsigned> pickOne(const std::vector<unsigned> &origins) {
  if (origins.empty()) return std::nullopt;
  return *std::min_element(origins.begin(), origins.end());
}

// One raw entry read from a compile-time function-pointer table, before
// cross-module resolution (which needs CalleeIndex, not available where
// the table itself is read -- see resolveFunctionPointerTable). Exactly
// one of `fn` and `isNull` describes what the entry actually is:
//   fn set        -- a function reference (possibly only a declaration in
//                    the defining module; resolved later exactly like an
//                    ordinary direct call's callee -- see resolveDeclaration).
//   isNull        -- a provable null-pointer constant: a legitimate "no
//                    kernel implements this combination" placeholder (seen
//                    in some OpenBLAS dispatch tables), safe to skip.
//   neither set   -- some other constant (a ConstantExpr this tool does not
//                    interpret, undef, poison, ...): NOT safe to skip, since
//                    unlike a provable null this could be a real, unexamined
//                    callee -- must be reported as an incompleteness note.
struct TableEntry {
  const Function *fn = nullptr;
  bool isNull = false;
};

// If `calledOperand` is a load of one entry of a module-level, immutable,
// compile-time-initialized array of function pointers -- the
// `int (*table[])(...) = {A, B, ...}; ...; table[idx](...)` dispatch idiom
// LLVM compiles a local table-of-kernels declaration into (hoisted to a
// private unnamed_addr constant global) -- returns every entry of that
// table, in its own declaration order, classified per TableEntry's own
// comment. Returns nullopt if `calledOperand` does not match this shape at
// all -- a genuinely indirect call whose target set cannot be enumerated
// this way.
std::optional<std::vector<TableEntry>> resolveFunctionPointerTable(const Value *calledOperand) {
  auto *load = dyn_cast<LoadInst>(calledOperand);
  if (!load) return std::nullopt;
  auto *gep = dyn_cast<GetElementPtrInst>(load->getPointerOperand());
  if (!gep) return std::nullopt;
  auto *gv = dyn_cast<GlobalVariable>(gep->getPointerOperand());
  if (!gv || !gv->isConstant() || !gv->hasDefinitiveInitializer()) return std::nullopt;
  auto *arr = dyn_cast<ConstantArray>(gv->getInitializer());
  if (!arr) return std::nullopt;
  std::vector<TableEntry> entries;
  for (unsigned i = 0; i < arr->getNumOperands(); ++i) {
    Constant *op = arr->getOperand(i)->stripPointerCasts();
    if (auto *fn = dyn_cast<Function>(op)) entries.push_back({fn, false});
    else if (isa<ConstantPointerNull>(op)) entries.push_back({nullptr, true});
    else entries.push_back({nullptr, false}); // unresolvable, not safe to skip
  }
  return entries;
}

// One call site where at least one operand traces back to a pointer
// parameter of the CURRENT function being walked, in the CURRENT
// function's own local argument indexing. `isTableDispatch` is set only
// when the call's own callee operand resolved via
// resolveFunctionPointerTable -- `tableEntries` then holds every entry
// (raw, not yet cross-module-resolved) instead of the single
// `resolveCallee` resolution an ordinary direct call gets.
struct TaintedCall {
  const CallBase *cb;
  std::map<unsigned, std::vector<unsigned>> argOrigins; // calleeArg -> local caller arg(s)
  bool isTableDispatch = false;
  std::vector<TableEntry> tableEntries;
};

struct TaintWalkResult {
  std::vector<TaintedCall> calls; // discovery order (deterministic for a fixed input)
  std::vector<EvidenceNote> notes;
};

// Forward taint propagation from every pointer parameter of `F`, following
// the SAME address-flow instruction classes analyzeAccess's own worklist
// follows (casts, GEPs, PHIs, selects, a load THROUGH a tainted pointer) --
// see Infer.cpp. Unlike analyzeAccess, this does not attempt to PROVE an
// extent: it only decides which call sites are relevant evidence and
// records every escape (a store of the tainted pointer, the pointer used
// AS an indirect call target, a tainted pointer PASSED to an indirect
// call) as an explicit Incomplete note rather than silently continuing --
// see EvidenceNoteKind's own comment on why completeness is tracked this
// way. `localArgToEntryArg` maps F's own argument indices back to the
// ORIGINAL entry function's argument indices (identity for the entry
// function itself; composed across call edges for anything deeper), used
// only to attribute `originatingArgument` on the notes this produces.
TaintWalkResult walkTaint(const Function &F,
                          const std::map<unsigned, std::vector<unsigned>> &localArgToEntryArg) {
  TaintWalkResult out;
  std::map<const CallBase *, size_t> callIndex;

  auto originFor = [&](unsigned localArgIdx) -> std::optional<unsigned> {
    auto it = localArgToEntryArg.find(localArgIdx);
    if (it == localArgToEntryArg.end()) return std::nullopt;
    return pickOne(it->second);
  };

  auto recordCallOperand = [&](const CallBase *cb, unsigned operandIdx,
                               unsigned originArgIdx) {
    size_t idx;
    auto it = callIndex.find(cb);
    if (it == callIndex.end()) {
      idx = out.calls.size();
      out.calls.push_back({cb, {}, false, {}});
      callIndex[cb] = idx;
    } else {
      idx = it->second;
    }
    auto &origins = out.calls[idx].argOrigins[operandIdx];
    if (std::find(origins.begin(), origins.end(), originArgIdx) == origins.end())
      origins.push_back(originArgIdx);
  };

  for (const Argument &A : F.args()) {
    if (!A.getType()->isPointerTy()) continue;
    unsigned argIdx = A.getArgNo();
    std::vector<const Value *> work{&A};
    std::set<const Value *> seen{&A};
    while (!work.empty()) {
      const Value *V = work.back();
      work.pop_back();
      for (const User *U : V->users()) {
        if (auto *ld = dyn_cast<LoadInst>(U)) {
          if (ld->getPointerOperand() != V) continue;
          // Only a load that itself yields a pointer continues the taint
          // walk (the pointer-to-pointer / Fortran-by-reference idiom).
          // Loading a scalar through a tainted pointer is an ordinary read;
          // treating the loaded scalar as still-tainted would misattribute
          // any later store of that scalar as a pointer escape.
          if (!ld->getType()->isPointerTy()) continue;
          if (seen.insert(ld).second) work.push_back(ld);
        } else if (auto *st = dyn_cast<StoreInst>(U)) {
          if (st->getValueOperand() != V) continue;
          out.notes.push_back({EvidenceNoteKind::Incomplete,
              "a pointer traced from arg" + std::to_string(argIdx) +
              " is stored to memory inside " + F.getName().str() +
              " -- not traced past the store",
              originFor(argIdx)});
        } else if (isa<BitCastInst>(U) || isa<AddrSpaceCastInst>(U) ||
                   isa<GetElementPtrInst>(U) || isa<PHINode>(U) ||
                   isa<SelectInst>(U)) {
          if (seen.insert(U).second) work.push_back(U);
        } else if (auto *cb = dyn_cast<CallBase>(U)) {
          if (cb->getCalledOperand() == V) {
            out.notes.push_back({EvidenceNoteKind::Incomplete,
                "a pointer traced from arg" + std::to_string(argIdx) +
                " is itself called as an indirect function target inside " +
                F.getName().str() + " -- callee unknown, not followed",
                originFor(argIdx)});
            continue;
          }
          const Function *calledFn = cb->getCalledFunction();
          // A pure computational intrinsic (fmuladd, sqrt, fabs, ...) has
          // no marshalling relevance and no body to ever resolve -- it is
          // already fully visible in the included function's own IR, so
          // recording it as a call needing resolution would only produce a
          // confusing "unresolved" note about ordinary arithmetic.
          if (calledFn && calledFn->isIntrinsic() && !isa<AnyMemIntrinsic>(cb))
            continue;
          if (isa<AnyMemIntrinsic>(cb)) {
            // A bulk memory intrinsic is COMPLETE evidence on its own: the
            // intrinsic call and its explicit byte-count operand are both
            // directly visible in the included function's own IR, exactly
            // like any other instruction -- not a gap the way an
            // unresolved call is (see this function's own doc comment).
            out.notes.push_back({EvidenceNoteKind::Informational,
                "a pointer traced from arg" + std::to_string(argIdx) +
                " reaches a bulk memory intrinsic (" +
                calledFn->getName().str() + ") inside " + F.getName().str() +
                " -- fully represented by the intrinsic call and its "
                "byte-count operand, visible in the IR evidence",
                originFor(argIdx)});
            continue;
          }
          if (!calledFn) {
            // A genuinely indirect call (the callee itself is unknown) that
            // a traced pointer is merely an ARGUMENT to -- distinct from
            // the "pointer IS the target" case above. Before giving up,
            // check whether the callee operand is a compile-time-constant
            // function-pointer table: if so, its entries are known, even
            // though cross-module resolution of each one (which needs
            // CalleeIndex) happens later where that index is available.
            if (auto table = resolveFunctionPointerTable(cb->getCalledOperand())) {
              for (unsigned oi = 0; oi < cb->arg_size(); ++oi)
                if (cb->getArgOperand(oi) == V)
                  recordCallOperand(cb, oi, argIdx);
              TaintedCall &rec = out.calls[callIndex.at(cb)];
              rec.isTableDispatch = true;
              rec.tableEntries = std::move(*table);
              continue;
            }
            out.notes.push_back({EvidenceNoteKind::Incomplete,
                "a pointer traced from arg" + std::to_string(argIdx) +
                " is passed to an indirect call inside " + F.getName().str() +
                " -- callee unknown, not followed",
                originFor(argIdx)});
            continue;
          }
          for (unsigned oi = 0; oi < cb->arg_size(); ++oi)
            if (cb->getArgOperand(oi) == V)
              recordCallOperand(cb, oi, argIdx);
        }
        // Anything else (icmp, ptrtoint, ret, ...) is not address flow and
        // is not followed -- it's still visible in the function's own IR,
        // which is always included in full (see renderFunctionIR).
      }
    }
  }

  // Finalize: for every call already found relevant above (via a pointer-
  // tainted operand), also record any OTHER operand that is a direct
  // identity alias of one of F's own parameters (see directParamIndex).
  // The pointer-taint walk above only ever SEEDS from pointer parameters,
  // so a plain scalar passthrough (n, incx, ...) reaching the SAME call is
  // otherwise never independently discovered.
  for (TaintedCall &tc : out.calls) {
    for (unsigned oi = 0; oi < tc.cb->arg_size(); ++oi) {
      if (tc.argOrigins.count(oi)) continue;
      int origin = directParamIndex(tc.cb->getArgOperand(oi), F);
      if (origin >= 0) tc.argOrigins[oi].push_back((unsigned)origin);
    }
  }
  return out;
}

// Resolves a directly-named function reference that may be only a
// declaration in its own defining module to the one resident definition
// across all loaded modules -- shared by resolveCallee (an ordinary direct
// call) and the function-pointer-table resolution below, so a table entry
// is resolved through EXACTLY the same resident-definition index a direct
// call's callee is, not a separate or looser check.
const Function *resolveDeclaration(const Function *raw, const CalleeIndex &calleeIndex,
                                   bool &ambiguous) {
  ambiguous = false;
  if (!raw->isDeclaration()) return raw;
  if (!raw->hasName()) return nullptr;
  auto it = calleeIndex.find(raw->getName());
  if (it == calleeIndex.end()) return nullptr;
  if (!it->second) { ambiguous = true; return nullptr; }
  return it->second;
}

const Function *resolveCallee(const CallBase *cb, const CalleeIndex &calleeIndex,
                              bool &ambiguous) {
  ambiguous = false;
  const Function *callee = cb->getCalledFunction();
  if (!callee) return nullptr; // indirect
  return resolveDeclaration(callee, calleeIndex, ambiguous);
}

const Function *findFunctionByName(StringRef name,
                                   const std::vector<std::unique_ptr<Module>> &mods) {
  for (const auto &mod : mods) {
    if (const Function *f = mod->getFunction(name))
      if (!f->isDeclaration()) return f;
    for (const GlobalAlias &ga : mod->aliases())
      if (ga.getName() == name)
        if (auto *f = dyn_cast_or_null<Function>(ga.getAliaseeObject()))
          if (!f->isDeclaration()) return f;
  }
  return nullptr;
}

std::string jsonEscape(StringRef s) {
  std::string out;
  for (char c : s) {
    switch (c) {
    case '"': out += "\\\""; break;
    case '\\': out += "\\\\"; break;
    case '\n': out += "\\n"; break;
    case '\t': out += "\\t"; break;
    default:
      if ((unsigned char)c < 0x20) {
        // Control characters other than \n/\t (already handled above) are
        // rejected by strict JSON parsers unescaped -- always possible in
        // stripped IR text (none observed in practice, handled anyway).
        char buf[8];
        snprintf(buf, sizeof(buf), "\\u%04x", (unsigned)c);
        out += buf;
      } else {
        out += c;
      }
    }
  }
  return out;
}

// Merges two entry-argument-origin sets (see PromptEvidence/
// localArgToEntryArg) into a sorted, deduplicated result.
std::vector<unsigned> mergeOrigins(const std::vector<unsigned> &a,
                                   const std::vector<unsigned> &b) {
  std::vector<unsigned> out = a;
  out.insert(out.end(), b.begin(), b.end());
  std::sort(out.begin(), out.end());
  out.erase(std::unique(out.begin(), out.end()), out.end());
  return out;
}

std::string sha256Hex(StringRef data) {
  SHA256 hasher;
  hasher.update(data);
  std::array<uint8_t, 32> digest = hasher.final();
  return toHex(ArrayRef<uint8_t>(digest.data(), digest.size()), /*LowerCase=*/true);
}

} // namespace

std::string sanitizeFunctionNameForFilename(StringRef functionName) {
  std::string out;
  out.reserve(functionName.size());
  for (char c : functionName) {
    bool safe = (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                (c >= '0' && c <= '9') || c == '_' || c == '-' || c == '.';
    out += safe ? c : '_';
  }
  if (out.empty()) out = "function";
  // A leading '.' could combine with a later path join to look like a
  // hidden/relative-traversal segment even after the character-level
  // sanitization above (e.g. a sanitized name of exactly "..") -- reject
  // that shape specifically rather than relying on callers to notice.
  if (out == "." || out == ".." || out[0] == '.') out = "fn_" + out;
  return out;
}

bool isEligibleForLlmInference(const PromptEvidence &evidence) {
  // Currently exactly the function-wide completeness flag -- see this
  // function's own header comment on why it stays a separate predicate
  // rather than callers reading sliceComplete directly.
  return evidence.sliceComplete;
}

bool buildLlmPrompt(StringRef functionName,
                    const std::vector<std::unique_ptr<Module>> &mods,
                    const CalleeIndex &calleeIndex, const LlmPromptLimits &limits,
                    LlmPromptResult &result, std::string &err) {
  const Function *entry = findFunctionByName(functionName, mods);
  if (!entry) {
    err = "function '" + functionName.str() + "' not found (or declaration-only) "
          "in any input module";
    return false;
  }

  PromptEvidence &evidence = result.evidence;
  evidence.entry = entry;
  evidence.limits = limits;

  // ---- relevant-callee discovery: bounded BFS over the call graph ----
  //
  // Each queue entry carries its OWN ancestor path (every function from
  // the entry down to, but not including, itself) so a call back to
  // something already on THIS path can be told apart from an ordinary
  // diamond-shaped reconvergence (two siblings both calling the same
  // shared helper, which is NOT recursion and needs no incompleteness
  // note) -- a plain global visited-set cannot make that distinction.
  struct QueueEntry {
    const Function *fn;
    unsigned depth;
    std::vector<const Function *> ancestorPath;
    std::map<unsigned, std::vector<unsigned>> localArgToEntryArg;
  };

  std::map<unsigned, std::vector<unsigned>> entryIdentityMap;
  for (unsigned i = 0; i < entry->arg_size(); ++i) entryIdentityMap[i] = {i};

  std::set<const Function *> seen; // included, or already enqueued toward inclusion
  std::deque<QueueEntry> queue;
  queue.push_back({entry, 0, {}, entryIdentityMap});
  seen.insert(entry);

  while (!queue.empty()) {
    QueueEntry qe = std::move(queue.front());
    queue.pop_front();
    const Function *F = qe.fn;
    bool isEntry = (F == entry);
    unsigned n = countInstructions(*F);

    // ---- entry instruction-budget policy ----
    // The entry function is ALWAYS included regardless of maxInstructions
    // -- a prompt missing the very function being classified would be
    // useless, not merely incomplete. Exceeding the budget by itself still
    // sets sliceComplete=false with an explicit note (this is POLICY 2 of
    // the two documented alternatives: "the entry is always included, but
    // exceeding the budget marks the slice incomplete", chosen over
    // POLICY 1 -- "an oversized entry produces a rejected slice outright"
    // -- because a human/LLM can still usefully inspect a truncated-but-
    // present entry body, whereas an outright rejection would print
    // nothing to inspect at all). A non-entry function that would push
    // the running total over budget is dropped instead (not included, its
    // own calls never explored), which is the ordinary truncation case.
    if (!isEntry && evidence.includedInstructionCount + n > limits.maxInstructions) {
      evidence.sliceComplete = false;
      evidence.notes.push_back({EvidenceNoteKind::Incomplete,
          "stopped including '" + F->getName().str() + "': would exceed the " +
          std::to_string(limits.maxInstructions) + "-instruction slice budget "
          "(already at " + std::to_string(evidence.includedInstructionCount) + ")",
          std::nullopt});
      continue;
    }
    if (isEntry && n > limits.maxInstructions) {
      evidence.sliceComplete = false;
      evidence.notes.push_back({EvidenceNoteKind::Incomplete,
          "the entry function '" + F->getName().str() + "' alone (" +
          std::to_string(n) + " instructions) exceeds the configured "
          "instruction budget of " + std::to_string(limits.maxInstructions) +
          " -- included anyway (a prompt without the entry function's own "
          "body would not be usable), but no callee is explored",
          std::nullopt});
    }

    evidence.includedFunctions.push_back(F);
    evidence.includedInstructionCount += n;
    std::vector<const Function *> path = qe.ancestorPath;
    path.push_back(F);

    TaintWalkResult tw = walkTaint(*F, qe.localArgToEntryArg);
    for (EvidenceNote &note : tw.notes) {
      if (note.kind == EvidenceNoteKind::Incomplete) evidence.sliceComplete = false;
      evidence.notes.push_back(std::move(note));
    }

    for (const TaintedCall &tc : tw.calls) {
      // Best-effort origin for any note below: the smallest entry argument
      // any tainted operand of this call traces back to, if any.
      std::optional<unsigned> noteOrigin;
      for (const auto &[calleeArg, callerArgs] : tc.argOrigins) {
        (void)calleeArg;
        for (unsigned ca : callerArgs) {
          auto it = qe.localArgToEntryArg.find(ca);
          if (it == qe.localArgToEntryArg.end()) continue;
          auto one = pickOne(it->second);
          if (one && (!noteOrigin || *one < *noteOrigin)) noteOrigin = one;
        }
      }

      // A table-dispatch call carries its full set of raw table entries
      // (see resolveFunctionPointerTable) -- each is resolved here through
      // EXACTLY the same resident-definition index (resolveDeclaration) an
      // ordinary direct call's callee goes through, so a table entry can
      // never be treated as more trustworthy than a hand-written call.
      // Only a PROVABLY NULL entry may be silently skipped; a missing body,
      // an ambiguous resolution, or any other unresolvable entry each make
      // the slice incomplete individually, exactly like an ordinary
      // unresolved direct call would.
      std::vector<const Function *> callees;
      if (tc.isTableDispatch) {
        std::set<const Function *> uniq;
        for (const TableEntry &te : tc.tableEntries) {
          if (te.isNull) continue; // a provable "no kernel for this slot" placeholder
          if (!te.fn) {
            evidence.sliceComplete = false;
            evidence.notes.push_back({EvidenceNoteKind::Incomplete,
                "a function-pointer table entry reached from a call inside " +
                F->getName().str() + " is neither a resolvable function "
                "reference nor a provable null -- not followed",
                noteOrigin});
            continue;
          }
          bool ambiguous = false;
          const Function *resolved = resolveDeclaration(te.fn, calleeIndex, ambiguous);
          if (!resolved) {
            evidence.sliceComplete = false;
            std::string reason = ambiguous
                ? "a function-pointer table entry ('" + te.fn->getName().str() +
                    "') reached from a call inside " + F->getName().str() +
                    " resolves to more than one externally-linked definition "
                    "across resident modules -- not followed (genuinely ambiguous)"
                : "a function-pointer table entry ('" + te.fn->getName().str() +
                    "') reached from a call inside " + F->getName().str() +
                    " has no available body in any resident module -- not followed";
            evidence.notes.push_back({EvidenceNoteKind::Incomplete, reason, noteOrigin});
            continue;
          }
          if (uniq.insert(resolved).second) callees.push_back(resolved);
        }
        if (!callees.empty()) {
          std::string names;
          for (size_t i = 0; i < callees.size(); ++i) {
            if (i) names += ", ";
            names += "'" + callees[i]->getName().str() + "'";
          }
          evidence.notes.push_back({EvidenceNoteKind::Informational,
              "a call inside " + F->getName().str() + " dispatches through a "
              "compile-time-constant function-pointer table -- every "
              "successfully resolved candidate callee (" + names + ") is "
              "explored below",
              noteOrigin});
        }
      } else {
        bool ambiguous = false;
        const Function *callee = resolveCallee(tc.cb, calleeIndex, ambiguous);
        if (!callee) {
          evidence.sliceComplete = false;
          std::string calleeName = tc.cb->getCalledFunction()
              ? tc.cb->getCalledFunction()->getName().str()
              : std::string("<indirect>");
          std::string reason;
          if (ambiguous)
            reason = "call to '" + calleeName + "' inside " + F->getName().str() +
                " resolves to more than one externally-linked definition across "
                "resident modules -- not followed (genuinely ambiguous)";
          else if (!tc.cb->getCalledFunction())
            reason = "indirect call inside " + F->getName().str() +
                " reached by a traced pointer argument -- callee unknown, not followed";
          else
            reason = "call to '" + calleeName + "' inside " + F->getName().str() +
                " has no available body in any resident module -- not followed";
          evidence.notes.push_back({EvidenceNoteKind::Incomplete, reason, noteOrigin});
          continue;
        }
        callees = {callee};
      }

      for (const Function *callee : callees) {
        CallEdgeRecord edge;
        edge.caller = F;
        edge.callee = callee;
        edge.callSite = computeCallSiteId(tc.cb);
        for (const auto &[calleeArg, callerArgs] : tc.argOrigins) {
          std::vector<unsigned> sorted = callerArgs;
          std::sort(sorted.begin(), sorted.end());
          sorted.erase(std::unique(sorted.begin(), sorted.end()), sorted.end());
          edge.argumentMap.push_back({calleeArg, std::move(sorted)});
        }
        evidence.callEdges.push_back(std::move(edge));

        // Compose the callee's own local-arg-to-entry-arg map from this call
        // edge's argument correspondences and F's OWN map (already
        // entry-relative) -- used only if this callee is actually enqueued
        // below, but computed unconditionally since it's cheap and keeps the
        // enqueue logic uncluttered.
        std::map<unsigned, std::vector<unsigned>> childMap;
        for (const auto &[calleeArg, callerArgs] : tc.argOrigins) {
          std::vector<unsigned> merged;
          for (unsigned ca : callerArgs) {
            auto it = qe.localArgToEntryArg.find(ca);
            if (it == qe.localArgToEntryArg.end()) continue;
            merged = mergeOrigins(merged, it->second);
          }
          if (!merged.empty()) childMap[calleeArg] = std::move(merged);
        }

        bool isRecursive = std::find(path.begin(), path.end(), callee) != path.end();
        if (isRecursive) {
          evidence.sliceComplete = false;
          evidence.notes.push_back({EvidenceNoteKind::Incomplete,
              "call to '" + callee->getName().str() + "' inside " + F->getName().str() +
              " closes a recursive cycle back to an already-explored function on "
              "the same call path -- recursive pointer flow cannot be fully "
              "represented by a bounded, acyclic slice",
              noteOrigin});
          continue;
        }
        if (seen.count(callee)) continue; // diamond reconvergence, not a gap
        if (qe.depth + 1 > limits.maxCallDepth) {
          evidence.sliceComplete = false;
          evidence.notes.push_back({EvidenceNoteKind::Incomplete,
              "call to '" + callee->getName().str() + "' inside " + F->getName().str() +
              " is at depth " + std::to_string(qe.depth + 1) + ", past the " +
              std::to_string(limits.maxCallDepth) + "-hop call-depth limit -- not followed",
              noteOrigin});
          continue;
        }
        if (evidence.includedFunctions.size() + queue.size() >= limits.maxFunctions) {
          evidence.sliceComplete = false;
          evidence.notes.push_back({EvidenceNoteKind::Incomplete,
              "call to '" + callee->getName().str() + "' inside " + F->getName().str() +
              " would exceed the " + std::to_string(limits.maxFunctions) +
              "-function slice limit -- not followed",
              noteOrigin});
          continue;
        }
        seen.insert(callee);
        queue.push_back({callee, qe.depth + 1, path, std::move(childMap)});
      }
    }
  }

  // ---- rendered IR bodies (built once, reused for the prompt AND the
  // canonical input-hash blob below, so they can never disagree) ----
  std::vector<std::string> renderedBodies;
  renderedBodies.reserve(evidence.includedFunctions.size());
  for (const Function *F : evidence.includedFunctions)
    renderedBodies.push_back(renderFunctionIR(*F));

  static const char *const kPromptVersion = "marshal-ir-v2";
  static const char *const kResponseSchemaVersion = "marshal-response-v6";

  // ---- prompt text assembly ----
  std::string prompt;
  prompt += "=== TASK ===\n";
  prompt +=
      "Infer the marshalling semantics of this library function using only "
      "the provided LLVM IR. Classify every boundary pointer parameter "
      "using only the allowed output vocabulary. Do not guess: use unknown "
      "wherever the complete accessed region cannot be established from the "
      "supplied evidence. Underestimating a pointer region is unsafe.\n\n"
      "For a pointer whose evidence spans more than one execution path "
      "(a branch, a delegated call reached only on some paths, ...): infer "
      "its PATH-INSENSITIVE potential-access envelope. Union the regions "
      "actually read or written across every path -- a path that touches "
      "nothing contributes nothing to the union and does not, by itself, "
      "force \"unknown\". Union direction the same way (touched as a read "
      "on one path and a write on another is \"inout\"). Do not encode "
      "branch predicates in the answer -- there is no conditional extent, "
      "only one flat classification per argument. Use \"unknown\" only "
      "when the unioned region is unbounded, or when it cannot be "
      "expressed with the extent kinds and operand forms defined below -- "
      "never merely because the evidence involves more than one path.\n\n";

  prompt += "=== ALLOWED CLASSIFICATIONS (provisional -- Stage 1 preview vocabulary) ===\n";
  prompt +=
      "pointer direction: in | out | inout | unknown\n"
      "pointer extent: one | constant | argument | c_string | stride_vector | unknown\n\n"
      "The exact JSON shape for extent_operand -- when it is present, and "
      "the composite-operand structure of any operand inside it -- is "
      "enforced by the response format this request is configured with. "
      "You cannot produce a structurally malformed answer; write only the "
      "VALUES (which argument, which extent, which operand kind) your "
      "reasoning below actually supports.\n\n"
      "Units (fixed by the runtime consumer, not a matter of interpretation):\n"
      "  - \"constant\" extent: the size operand's value is a BYTE count.\n"
      "  - \"argument\" extent: the size operand's value IS a BYTE count "
      "directly -- no element-size multiplication is ever applied.\n"
      "  - \"stride_vector\" extent: size and stride are ELEMENT counts/steps, "
      "in units of whatever pointee type the relevant getelementptr "
      "instruction(s) in the IR evidence below index over (e.g. "
      "`getelementptr double, ptr %x, i32 %i` walks 8-byte elements); the "
      "runtime separately knows the element byte size from the pointee's "
      "own type and multiplies it in -- do not convert to bytes yourself.\n"
      "  - \"argument_pointee\" operand source: the named argument is "
      "itself a POINTER; its value is obtained by reading a 32-bit integer "
      "through it (the classic Fortran-by-reference convention, e.g. a "
      "`int *n` argument actually meaning the integer n). Only a 4-byte "
      "integer pointee is representable this way -- if the real pointee is "
      "some other width or type, use extent \"unknown\" instead.\n"
      "  - A NEGATIVE stride is rejected by the runtime at DISPATCH TIME "
      "with a hard, unconditional abort of the whole call -- not a graceful "
      "skip, not a zero-byte transfer for that input. A raw, sign-unproven "
      "stride is therefore never safe to use directly. See the "
      "path-insensitive union principle above and \"abs\" below for the two "
      "ways this is still resolvable; failing those, use \"unknown\".\n\n"
      "Beyond a plain leaf operand (an argument's value, its pointee, or a "
      "fixed constant), an operand may instead be:\n"
      "  - the PRODUCT of two INDEPENDENT quantities (e.g. a matrix's "
      "leading dimension times its row count, `lda * n`) -- use this when "
      "the bound is a multiplication of two separately-varying arguments, "
      "not when one operand is already provably equal to the other;\n"
      "  - the ABSOLUTE VALUE of one quantity (e.g. `abs(incx)`) -- use "
      "this ONLY when the path-insensitive union above resolves to a "
      "magnitude-based envelope: either (a) the caller REBASES the base "
      "pointer to compensate for a possibly-negative value before the real "
      "walk, so the touched region -- relative to the ORIGINAL pointer "
      "argument -- is exactly this shape, an EXACT description; or (b) "
      "every path where the value is negative touches NO memory at all (a "
      "dominating guard makes it a safe no-op), so this envelope is a safe "
      "SUPERSET of what those paths actually do (nothing). Do NOT use "
      "\"abs\" for a function that genuinely walks backward through memory "
      "on a negative value with no such guard or rebase established by the "
      "evidence -- an unestablished backward walk is \"unknown\", not a "
      "guess a magnitude can paper over;\n"
      "  - the MAXIMUM of two candidate quantities (e.g. a matrix's leading "
      "dimension times whichever of two named arguments is really its other "
      "dimension, `lda * max(m, k)`) -- use this ONLY when the evidence "
      "shows the true value is drawn from EXACTLY one of these two named "
      "arguments, chosen by a runtime flag (e.g. a transpose/order flag "
      "selecting which dimension applies) that your reasoning cannot "
      "resolve to a single branch, and no value outside this two-candidate "
      "set is ever possible. Like \"abs\", this is a safe SUPERSET, never a "
      "precise answer -- if a third value is possible, or the true value "
      "could be some combination of the two rather than exactly one of "
      "them, \"max\" does not apply and the argument is \"unknown\".\n\n"
      "\"add\" and \"divide\" (each taking two operands like \"product\") "
      "exist for an EXACT closed-form size formula, e.g. classic BLAS "
      "packed upper/lower-triangular storage's `N*(N+1)/2` element count: "
      "`divide(product(N, add(N, 1)), 2)`. \"divide\" always rounds UP "
      "(ceiling), so it never undercounts even on a formula that happens "
      "not to divide evenly -- do not withhold a known closed-form formula "
      "as \"unknown\" merely because the division might not be exact; only "
      "use \"unknown\" when no such formula is established by the "
      "evidence at all.\n\n"
      "These compose freely with each other and with themselves -- a "
      "composite operand is not limited to leaf operands, "
      "and a single pointer can need MORE than one composite layered "
      "together. For example, an in-place buffer that is READ through one "
      "leading-dimension argument but WRITTEN BACK through a DIFFERENT "
      "leading-dimension argument (so the two can legitimately disagree) "
      "needs a `max` on EACH side of the multiplication, not just one: "
      "`product(max(lda, ldb), max(rows, cols))` is a safe superset "
      "covering both the read region and the write region in one answer, "
      "by the same safe-superset reasoning as a single `max` above -- "
      "compose it this way whenever more than one flag-selected quantity "
      "governs the same pointer's extent, rather than defaulting to "
      "\"unknown\" merely because more than one composite layer is "
      "needed.\n\n";

  prompt += "=== REQUIRED OUTPUT FORMAT (response_schema_version: " +
            std::string(kResponseSchemaVersion) + ") ===\n";
  prompt +=
      "Respond with EXACTLY ONE JSON object matching the configured "
      "response format -- no prose before or after it, no markdown code "
      "fence. One entry in \"pointer_arguments\" per POINTER "
      "entry_argument listed below (see each one's llvm_type); omit scalar "
      "arguments entirely. Use \"id\" values exactly as given (\"arg0\", "
      "\"arg1\", ...). Do not guess: use extent \"unknown\" with "
      "extent_operand set to null whenever the accessed region cannot be "
      "established from the supplied evidence.\n\n"
      "Example (illustrative values only; use the REAL evidence below to "
      "decide the real ones):\n\n"
      "{\n"
      "  \"response_schema_version\": \"" + std::string(kResponseSchemaVersion) + "\",\n"
      "  \"function\": \"example_function\",\n"
      "  \"pointer_arguments\": [\n"
      "    {\"id\": \"arg1\", \"direction\": \"in\", \"extent\": \"stride_vector\", "
      "\"extent_operand\": {\"size\": {\"source\": \"argument_value\", "
      "\"argument_id\": \"arg0\", \"constant_value\": null}, \"stride\": "
      "{\"op\": \"abs\", \"operand\": {\"source\": \"argument_value\", "
      "\"argument_id\": \"arg2\", \"constant_value\": null}}}},\n"
      "    {\"id\": \"arg3\", \"direction\": \"in\", \"extent\": \"argument\", "
      "\"extent_operand\": {\"size\": {\"op\": \"product\", \"operands\": ["
      "{\"source\": \"argument_value\", \"argument_id\": \"arg4\", "
      "\"constant_value\": null}, {\"source\": \"argument_value\", "
      "\"argument_id\": \"arg5\", \"constant_value\": null}]}}},\n"
      "    {\"id\": \"arg7\", \"direction\": \"in\", \"extent\": \"argument\", "
      "\"extent_operand\": {\"size\": {\"op\": \"product\", \"operands\": ["
      "{\"source\": \"argument_value\", \"argument_id\": \"arg8\", "
      "\"constant_value\": null}, {\"op\": \"max\", \"operands\": ["
      "{\"source\": \"argument_value\", \"argument_id\": \"arg4\", "
      "\"constant_value\": null}, {\"source\": \"argument_value\", "
      "\"argument_id\": \"arg5\", \"constant_value\": null}]}]}}},\n"
      "    {\"id\": \"arg6\", \"direction\": \"unknown\", \"extent\": \"unknown\", "
      "\"extent_operand\": null}\n"
      "  ]\n"
      "}\n\n"
      "Every leaf operand carries BOTH \"argument_id\" and \"constant_value\", "
      "with the one the leaf's \"source\" does not use set to null (never "
      "omitted) -- as shown above.\n\n";

  // Everything above this point (TASK, ALLOWED CLASSIFICATIONS, REQUIRED
  // OUTPUT FORMAT) is byte-identical across every function generated at
  // this prompt/response-schema version -- only what follows (the boundary
  // signature, IR evidence, call-edge mappings, completeness notes) is
  // specific to this one function. Recorded in the manifest as
  // `prompt_shared_prefix_bytes` so an API runner can split the request
  // into a stable prefix (sent as, e.g., a provider's own cacheable/system
  // slot) and a per-function remainder, instead of re-billing this whole
  // preamble as fresh input tokens on every query.
  const size_t sharedPreambleBytes = prompt.size();

  std::map<unsigned, std::string> entryNames = collectParamNames(*entry);
  prompt += "=== BOUNDARY FUNCTION: " + entry->getName().str() + " ===\n";
  for (unsigned idx = 0; idx < entry->arg_size(); ++idx) {
    const Argument *A = entry->getArg(idx);
    prompt += "arg" + std::to_string(idx) + ": " + argLabel(idx, entryNames) + ", " +
              typeStr(A->getType()) + ", " + argKind(A->getType()) + "\n";
  }
  prompt += "\n";

  prompt += "=== IR EVIDENCE (LLVM IR excerpt -- debug-info, alias/loop "
            "metadata, and attribute-group references stripped; calling "
            "convention, parameter attributes, memory-effect summaries, and "
            "control flow are preserved) ===\n\n";
  for (size_t i = 0; i < evidence.includedFunctions.size(); ++i) {
    const Function *F = evidence.includedFunctions[i];
    prompt += "--- " + F->getName().str() +
              (F == entry ? " (entry)" : " (callee)") + " ---\n";
    prompt += renderedBodies[i];
    prompt += "\n";
  }

  prompt += "=== CALL-EDGE ARGUMENT MAPPINGS ===\n";
  if (evidence.callEdges.empty()) {
    prompt += "(none -- the entry function makes no relevant delegating call)\n";
  } else {
    for (const CallEdgeRecord &e : evidence.callEdges) {
      std::map<unsigned, std::string> callerNames = collectParamNames(*e.caller);
      std::map<unsigned, std::string> calleeNames = collectParamNames(*e.callee);
      std::string siteTag = " [call site bb" + std::to_string(e.callSite.basicBlockOrdinal) +
                            "/inst" + std::to_string(e.callSite.instructionOrdinal) + "]";
      for (const ArgumentCorrespondence &ac : e.argumentMap) {
        std::string calleeLabel = e.callee->getName().str() + "." +
            argLabel(ac.calleeArgument, calleeNames);
        for (unsigned callerArg : ac.callerArguments) {
          std::string callerLabel = e.caller->getName().str() + "." +
              argLabel(callerArg, callerNames);
          prompt += callerLabel + " -> " + calleeLabel + siteTag + "\n";
        }
      }
    }
  }
  prompt += "\n";

  prompt += "=== COMPLETENESS NOTES ===\n";
  if (evidence.notes.empty()) {
    prompt += "(none)\n";
  } else {
    for (const EvidenceNote &note : evidence.notes) {
      prompt += std::string(note.kind == EvidenceNoteKind::Incomplete
                                ? "[incomplete] " : "[info] ") +
                note.message;
      if (note.originatingArgument)
        prompt += " (traced from entry arg" + std::to_string(*note.originatingArgument) + ")";
      prompt += "\n";
    }
  }
  prompt += "\nslice_complete: " + std::string(evidence.sliceComplete ? "true" : "false") + "\n";

  result.promptText = prompt;

  // ---- canonical input blob (for input_hash) ----
  // Deliberately a SEPARATE serialization from the rendered prompt above,
  // not merely a substring of it: input_hash is defined over the
  // SEMANTIC evidence (function identities, IR bodies, call mappings,
  // completeness records, limits, versions), so a future purely-cosmetic
  // change to the prompt's own wording/layout does not change it, while
  // prompt_hash (over the exact emitted prompt bytes) always would.
  std::string canonical;
  canonical += "prompt_version=" + std::string(kPromptVersion) + "\n";
  canonical += "response_schema_version=" + std::string(kResponseSchemaVersion) + "\n";
  canonical += "function=" + entry->getName().str() + "\n";
  canonical += "limits=call_depth:" + std::to_string(limits.maxCallDepth) +
               ",function_count:" + std::to_string(limits.maxFunctions) +
               ",max_instructions:" + std::to_string(limits.maxInstructions) + "\n";
  canonical += "included_functions=[";
  for (size_t i = 0; i < evidence.includedFunctions.size(); ++i)
    canonical += (i ? "," : "") + evidence.includedFunctions[i]->getName().str();
  canonical += "]\n";
  for (size_t i = 0; i < evidence.includedFunctions.size(); ++i) {
    canonical += "--- function: " + evidence.includedFunctions[i]->getName().str() + " ---\n";
    canonical += renderedBodies[i];
  }
  for (const CallEdgeRecord &e : evidence.callEdges) {
    canonical += "call_edge:" + e.caller->getName().str() + "(bb" +
                 std::to_string(e.callSite.basicBlockOrdinal) + "/inst" +
                 std::to_string(e.callSite.instructionOrdinal) + ")->" +
                 e.callee->getName().str() + ":";
    for (const ArgumentCorrespondence &ac : e.argumentMap) {
      canonical += "arg" + std::to_string(ac.calleeArgument) + "<-[";
      for (size_t i = 0; i < ac.callerArguments.size(); ++i)
        canonical += (i ? "," : "") + std::to_string(ac.callerArguments[i]);
      canonical += "]";
    }
    canonical += "\n";
  }
  for (const EvidenceNote &note : evidence.notes) {
    canonical += std::string(note.kind == EvidenceNoteKind::Incomplete ? "I:" : "i:") +
                 note.message;
    if (note.originatingArgument) canonical += "@arg" + std::to_string(*note.originatingArgument);
    canonical += "\n";
  }
  canonical += "slice_complete=" + std::string(evidence.sliceComplete ? "true" : "false") + "\n";

  std::string inputHash = sha256Hex(canonical);
  std::string promptHash = sha256Hex(result.promptText);

  // ---- manifest JSON assembly ----
  std::string m = "{\n";
  m += "  \"format_version\": 1,\n";
  m += "  \"prompt_version\": \"" + std::string(kPromptVersion) + "\",\n";
  m += "  \"response_schema_version\": \"" + std::string(kResponseSchemaVersion) + "\",\n";
  m += "  \"function\": \"" + jsonEscape(entry->getName()) + "\",\n";
  m += "  \"entry_arguments\": [\n";
  for (unsigned idx = 0; idx < entry->arg_size(); ++idx) {
    const Argument *A = entry->getArg(idx);
    auto it = entryNames.find(idx);
    std::string name = it != entryNames.end() ? it->second : ("arg" + std::to_string(idx));
    m += "    {\"id\": \"arg" + std::to_string(idx) + "\", \"name\": \"" +
         jsonEscape(name) + "\", \"llvm_type\": \"" + jsonEscape(typeStr(A->getType())) +
         "\"}" + (idx + 1 < entry->arg_size() ? "," : "") + "\n";
  }
  m += "  ],\n";

  m += "  \"included_functions\": [";
  for (size_t i = 0; i < evidence.includedFunctions.size(); ++i)
    m += (i ? ", " : "") + ("\"" + jsonEscape(evidence.includedFunctions[i]->getName()) + "\"");
  m += "],\n";

  m += "  \"call_edges\": [\n";
  for (size_t i = 0; i < evidence.callEdges.size(); ++i) {
    const CallEdgeRecord &e = evidence.callEdges[i];
    m += "    {\"caller\": \"" + jsonEscape(e.caller->getName()) + "\", \"callee\": \"" +
         jsonEscape(e.callee->getName()) + "\", \"call_site\": {\"basic_block\": " +
         std::to_string(e.callSite.basicBlockOrdinal) + ", \"instruction\": " +
         std::to_string(e.callSite.instructionOrdinal) + "}, \"argument_map\": [\n";
    for (size_t j = 0; j < e.argumentMap.size(); ++j) {
      const ArgumentCorrespondence &ac = e.argumentMap[j];
      m += "      {\"callee_argument\": \"arg" + std::to_string(ac.calleeArgument) +
           "\", \"caller_arguments\": [";
      for (size_t k = 0; k < ac.callerArguments.size(); ++k)
        m += (k ? ", " : "") + ("\"arg" + std::to_string(ac.callerArguments[k]) + "\"");
      m += "]}" + std::string(j + 1 < e.argumentMap.size() ? "," : "") + "\n";
    }
    m += "    ]}" + std::string(i + 1 < evidence.callEdges.size() ? "," : "") + "\n";
  }
  m += "  ],\n";

  m += "  \"slice_complete\": " + std::string(evidence.sliceComplete ? "true" : "false") + ",\n";
  m += "  \"eligible_for_llm_inference\": " +
       std::string(isEligibleForLlmInference(evidence) ? "true" : "false") + ",\n";
  m += "  \"notes\": [\n";
  for (size_t i = 0; i < evidence.notes.size(); ++i) {
    const EvidenceNote &note = evidence.notes[i];
    m += "    {\"kind\": \"" +
         std::string(note.kind == EvidenceNoteKind::Incomplete ? "incomplete" : "informational") +
         "\", \"message\": \"" + jsonEscape(note.message) + "\", \"originating_argument\": " +
         (note.originatingArgument ? ("\"arg" + std::to_string(*note.originatingArgument) + "\"")
                                    : std::string("null")) +
         "}" + (i + 1 < evidence.notes.size() ? "," : "") + "\n";
  }
  m += "  ],\n";
  m += "  \"limits\": {\"call_depth\": " + std::to_string(limits.maxCallDepth) +
       ", \"function_count\": " + std::to_string(limits.maxFunctions) +
       ", \"max_instructions\": " + std::to_string(limits.maxInstructions) + "},\n";
  m += "  \"included_instruction_count\": " + std::to_string(evidence.includedInstructionCount) + ",\n";
  m += "  \"prompt_shared_prefix_bytes\": " + std::to_string(sharedPreambleBytes) + ",\n";
  m += "  \"input_hash\": \"sha256:" + inputHash + "\",\n";
  m += "  \"prompt_hash\": \"sha256:" + promptHash + "\"\n";
  m += "}\n";
  result.manifestJson = m;

  return true;
}

} // namespace marshal
