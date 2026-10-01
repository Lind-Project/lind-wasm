// marshal-infer — automated marshalling-spec inference for Lind library
// interposition. Reads a wasm32 LLVM bitcode module (from `lind_compile
// --emit-llvm`, carrying DWARF) and emits, per exported function, a JSON
// "inference record" describing how each argument must be marshalled across
// cages: kind, direction, size, pointee layout (wasm32 offsets), return kind,
// opaque-handle flags, and a residue/warnings list. Output is intended as a
// sidecar (<lib>.marshal.json) generated alongside the .cwasm artifact.
//
// Usage: marshal-infer <module.bc> [--json] [-o out] [--all] [--module NAME]
//   --json        emit JSON (default: human-readable tree)
//   -o <file>     write output to file (default: stdout)
//   --all         include internal/static functions
//   --module NAME label the module in JSON output (default: input path)
#include "Annotations.h"
#include "Config.h"
#include "Infer.h"
#include "LlmPrompt.h"
#include "ParamTree.h"

#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/GlobalAlias.h"
#include "llvm/IR/Module.h"
#include "llvm/IRReader/IRReader.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/Format.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/Path.h"
#include "llvm/Support/SourceMgr.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/SmallString.h"
#include "llvm/ADT/StringSet.h"

#include <fstream>
#include <map>
#include <string>

using namespace llvm;
using namespace marshal;

static cl::list<std::string> InputFiles(cl::Positional, cl::OneOrMore,
                                        cl::desc("<module.bc>..."));
static cl::opt<bool> AsJson("json", cl::desc("emit JSON inference records"));
static cl::opt<bool> ShowAll("all", cl::desc("include internal/static functions"));
static cl::opt<std::string> OutFile("o", cl::desc("output file (default stdout)"),
                                    cl::value_desc("file"));
static cl::opt<std::string> ModuleName("module",
                                       cl::desc("module label for JSON"));
static cl::opt<std::string> AnnoFile("annotations",
    cl::desc("JSON file extending the built-in handle-type/allocator/searcher "
             "tables (per-library customization)"),
    cl::value_desc("file"));
static cl::opt<std::string> ExportsFile("exports",
    cl::desc("only emit functions whose name is in this newline-separated file "
             "(e.g. the library's exported-symbol list)"),
    cl::value_desc("file"));
static cl::opt<std::string> ConfigFile("config",
    cl::desc("versioned config file (see CONFIG.md): analysis knobs, "
             "checked-in per-symbol contracts, and coverage thresholds. "
             "Omitting this reproduces the built-in defaults exactly."),
    cl::value_desc("file"));

// An inert inspection mode that writes the exact prompt (+ manifest) a
// future LLM backend would receive for one function, and exits -- no
// network request, no model SDK, no provider credentials, no marshal
// decision. Kept as a wholly separate early-exit path in main() (see below)
// rather than threaded through the ordinary inference pipeline, so passing
// none of these flags reproduces ordinary static-inference output
// byte-for-byte.
static cl::opt<bool> LlmPromptOnly("llm-prompt-only",
    cl::desc("write the Stage 1 LLM prompt/manifest for --function and exit; "
             "makes no network request and requires no credentials"));
static cl::opt<std::string> LlmFunction("function",
    cl::desc("function name to build an LLM prompt for (with --llm-prompt-only)"),
    cl::value_desc("name"));
static cl::opt<std::string> LlmPromptOutput("prompt-output",
    cl::desc("directory to write FUNCTION.prompt.txt/.json into "
             "(with --llm-prompt-only)"),
    cl::value_desc("dir"));
static cl::opt<unsigned> LlmMaxCallDepth("llm-max-call-depth",
    cl::desc("relevant-callee discovery depth limit (default 2)"), cl::init(2));
static cl::opt<unsigned> LlmMaxFunctions("llm-max-functions",
    cl::desc("relevant-callee discovery function-count limit (default 8)"),
    cl::init(8));
static cl::opt<unsigned> LlmMaxInstructions("llm-max-instructions",
    cl::desc("relevant-callee discovery total-instruction limit (default 6000)"),
    cl::init(6000));

// --------------------------------------------------------------------------
// Human-readable tree view (debugging)
// --------------------------------------------------------------------------
static void printNode(raw_ostream &os, const TreeNode *n, unsigned indent) {
  for (unsigned i = 0; i < indent; ++i) os << "  ";
  if (!n->fieldName.empty()) os << "." << n->fieldName << " ";
  os << "[" << nodeKindName(n->kind) << "] " << n->typeName
     << " (size=" << n->sizeBytes;
  if (indent > 0) os << ", off=" << n->offsetBytes;
  os << ")";
  if (n->isPointer()) {
    if (n->dir != Dir::NA) os << " dir=" << dirName(n->dir);
    if (n->sizeKind != SizeKind::NA) {
      os << " size=" << sizeKindName(n->sizeKind);
      if (n->sizeKind == SizeKind::FromArg ||
          n->sizeKind == SizeKind::FromArgPointee)
        os << "(arg" << n->sizeArgIndex << ")";
      if (n->sizeKind == SizeKind::Const) os << "(" << n->constSize << ")";
    }
    if (n->isHandle) os << " HANDLE[" << n->handleClass << "]";
  }
  if (!n->note.empty()) os << "  // " << n->note;
  os << "\n";
  for (const auto &c : n->children) printNode(os, c.get(), indent + 1);
}

static void printFunctionTree(raw_ostream &os, const FunctionTrees *ft) {
  os << "=== " << ft->funcName << " ===  ret=" << retKindName(ft->retKind);
  if (ft->retKind == RetKind::PtrAliasArg) os << "(arg" << ft->retAliasArg << ")";
  os << "\n";
  if (ft->retSretArg) {
    os << "  arg0 (sret):\n";
    printNode(os, ft->retSretArg.get(), 2);
  }
  for (size_t i = 0; i < ft->params.size(); ++i) {
    os << "  arg" << (i + (ft->retSretArg ? 1 : 0)) << ":\n";
    printNode(os, ft->params[i].get(), 2);
  }
  for (const auto &w : ft->warnings) os << "  ! " << w << "\n";
  os << "\n";
}

// --------------------------------------------------------------------------
// JSON view (the deliverable sidecar)
// --------------------------------------------------------------------------
static void jsonStr(raw_ostream &os, StringRef s) {
  os << '"';
  for (char c : s) {
    switch (c) {
    case '"': os << "\\\""; break;
    case '\\': os << "\\\\"; break;
    case '\n': os << "\\n"; break;
    case '\t': os << "\\t"; break;
    default: os << c;
    }
  }
  os << '"';
}

// `argRemap`, when non-null, maps a ft.params-internal top-level argument
// index to its FINAL position in the emitted JSON "args" array. Needed
// whenever ft->retSretArg and/or any preceding argument's abiSlots>1 (see
// ParamTree.h's TreeNode::abiSlots and FunctionTrees::retSretArg comments)
// shift the JSON array out of 1:1 correspondence with ft.params's own
// indexing — sizeArgIndex/intoArgIndex are computed in ft.params-internal
// terms (by dwarfIndexOf and friends in Infer.cpp) but consumed by
// gen_grate.py/lind_marshal_dispatch as literal positions into the FINAL
// spec->args[] array, so they must be translated at emission time. Threaded
// through recursive calls (not just the top-level one) because
// TreeNode::intoArgIndex (the ptrIntoArg case, e.g. strtol's endptr) lives on
// a NESTED pointee one level below the top-level argument, yet still refers
// to a top-level sibling argument's index, same as a top-level FromArg. A
// struct FIELD's sizeArgIndex (isField true) is a sibling-FIELD index within
// the same struct, never a top-level argument index — never remapped.
static void jsonNode(raw_ostream &os, const TreeNode *n, unsigned ind,
                     const std::vector<size_t> *argRemap = nullptr) {
  std::string pad(ind * 2, ' ');
  bool isField = !n->fieldName.empty();
  auto remapArg = [&](int idx) -> long long {
    if (argRemap && !isField && idx >= 0 && (size_t)idx < argRemap->size())
      return (long long)(*argRemap)[idx];
    return idx;
  };
  os << pad << "{";

  // A handle is its own canonical kind; it carries no dir/size/pointee.
  if (n->isHandle) {
    os << "\"kind\":\"handle\"";
    if (isField) {
      os << ",\"field\":"; jsonStr(os, n->fieldName);
      os << ",\"offset\":" << n->offsetBytes;
      os << ",\"touched\":" << (n->touched ? "true" : "false");
    }
    os << ",\"handle_class\":"; jsonStr(os, n->handleClass);
    os << "}";
    return;
  }

  // An inner pointer translated as an offset into another argument (strtol
  // endptr) — into_arg always names a TOP-LEVEL sibling argument, regardless
  // of the fact that this node itself is one level below it (the pointee of
  // a T** argument), so it's remapped the same as a top-level FromArg would be.
  if (n->ptrIntoArg) {
    os << "\"kind\":\"ptr_into_arg\",\"into_arg\":" << remapArg(n->intoArgIndex) << "}";
    return;
  }

  os << "\"kind\":"; jsonStr(os, nodeKindName(n->kind));
  if (isField) {
    os << ",\"field\":"; jsonStr(os, n->fieldName);
    os << ",\"offset\":" << n->offsetBytes;
    os << ",\"touched\":" << (n->touched ? "true" : "false");
  }
  os << ",\"type\":"; jsonStr(os, n->typeName);
  os << ",\"size\":" << n->sizeBytes;
  if (n->isPointer()) {
    os << ",\"dir\":"; jsonStr(os, dirName(n->dir));
    os << ",\"size_kind\":"; jsonStr(os, sizeKindName(n->sizeKind));
    if (n->sizeKind == SizeKind::PtrArray) os << ",\"terminator\":\"null\"";
    if (n->sizeKind == SizeKind::FromArg ||
        n->sizeKind == SizeKind::FromArgPointee)
      os << (isField ? ",\"size_field_index\":" : ",\"size_arg_index\":")
         << (isField ? (long long)n->sizeArgIndex : remapArg(n->sizeArgIndex));
    if (n->sizeKind == SizeKind::Const)
      os << ",\"const_size\":" << n->constSize;
    if (n->sizeKind == SizeKind::StrideVector) {
      // Always top-level operands (struct-field stride detection isn't
      // implemented), so always remapped -- no isField branch needed, unlike
      // FromArg/FromArgPointee above. Each operand is its own object, not a
      // bare index: `source` records whether the raw argument slot IS the
      // value or points to it (see ExtentOperand/ExtentSource in
      // ParamTree.h) -- collapsing that into a single index is exactly the
      // bug this shape exists to avoid (a Fortran-by-reference scalar's raw
      // slot holds a pointer, not the number itself). A Constant-sourced
      // operand has no argument at all (arg_index is always -1, meaningless
      // -- remapArg passes it through unchanged); "const_value" is what a
      // reader/consumer should use instead.
      auto operand = [&](const char *key, const ExtentOperand &op) {
        os << ",\"" << key << "\":{\"arg_index\":" << remapArg(op.argIndex)
           << ",\"source\":"; jsonStr(os, extentSourceName(op.source));
        if (op.source == ExtentSource::Constant)
          os << ",\"const_value\":" << op.constValue;
        os << "}";
      };
      operand("size_operand", n->sizeOperand);
      operand("stride_operand", n->strideOperand);
      os << ",\"const_size\":" << n->constSize;
      // Confidence: proven by static analysis, or configured by a
      // checked-in contract -- always emitted for a StrideVector node,
      // never omitted, so a reader never has to guess whether "absent"
      // means "proven" or just "not recorded".
      os << ",\"confidence\":"; jsonStr(os, confidenceName(n->confidence));
    }
    if (n->shallow) os << ",\"shallow\":true";
    if (n->cursor) os << ",\"cursor\":true";
  }

  // Recurse: pointer -> "pointee"; struct/union -> "fields". (Arrays are leaves.)
  bool recurse = (n->kind == NodeKind::Pointer || n->kind == NodeKind::Struct ||
                  n->kind == NodeKind::Union) && !n->children.empty();
  if (recurse) {
    const char *key = n->isPointer() ? "pointee" : "fields";
    os << ",\"" << key << "\":[\n";
    for (size_t i = 0; i < n->children.size(); ++i) {
      jsonNode(os, n->children[i].get(), ind + 1, argRemap);
      os << (i + 1 < n->children.size() ? ",\n" : "\n");
    }
    os << pad << "]";
  }
  os << "}";
}

// Emits ONE raw ABI slot for a node whose abiSlots > 1 (see ParamTree.h's
// TreeNode::abiSlots comment) — a plain scalar, no dir/size_kind/pointee,
// since a multi-slot value is never an address, just raw passed-through bits.
// `idx` is 0-based (0 = low-order slot, matching the confirmed wasm32 fp128
// argument order: low i64 first, then high).
static void jsonAbiSlot(raw_ostream &os, const TreeNode *n, unsigned idx,
                        unsigned ind) {
  std::string pad(ind * 2, ' ');
  uint64_t slotSize = n->abiSlots ? n->sizeBytes / n->abiSlots : n->sizeBytes;
  std::string label = (n->abiSlots == 2)
      ? (idx == 0 ? " (lo)" : " (hi)")          // the only confirmed case
      : (" (slot " + std::to_string(idx) + ")"); // future-proofing, unconfirmed order
  os << pad << "{\"kind\":\"scalar\",\"type\":";
  jsonStr(os, n->typeName + label);
  os << ",\"size\":" << slotSize << "}";
}

static void jsonWarnings(raw_ostream &os, const FunctionTrees *ft) {
  os << "\"warnings\":[";
  for (size_t i = 0; i < ft->warnings.size(); ++i) {
    if (i) os << ",";
    os << "\n        "; jsonStr(os, ft->warnings[i]);
  }
  if (!ft->warnings.empty()) os << "\n      ";
  os << "]";
}

static void jsonFunction(raw_ostream &os, const FunctionTrees *ft) {
  os << "    {\n      \"name\":"; jsonStr(os, ft->funcName);
  os << ",\n      \"decision\":";
  jsonStr(os, ft->forceLocal ? "force_local" : "marshal");
  if (ft->isVariadic) os << ",\n      \"variadic\":true";

  // Compact record for force_local: the runtime ignores args/ret.
  if (ft->forceLocal) {
    os << ",\n      "; jsonWarnings(os, ft);
    os << "\n    }";
    return;
  }

  os << ",\n      \"ret\":{\"kind\":"; jsonStr(os, retKindName(ft->retKind));
  if (ft->retKind == RetKind::PtrAliasArg || ft->retKind == RetKind::PtrIntoArg)
    os << ",\"alias_arg\":" << ft->retAliasArg;
  else if (ft->retKind == RetKind::PtrIntoCursor)
    os << ",\"cursor_arg\":" << ft->retAliasArg;
  else if (ft->retKind == RetKind::PtrToStatic)
    os << ",\"copyout_bytes\":" << ft->retStaticSize; // 0 => NUL-terminated cstr
  else if (ft->retKind == RetKind::PtrAlloc) {
    if (ft->retAllocSizeArgs.size() == 1)
      os << ",\"size_arg_index\":" << ft->retAllocSizeArgs[0];
    else if (!ft->retAllocSizeArgs.empty()) {
      os << ",\"size_arg_indices\":[";
      for (size_t i = 0; i < ft->retAllocSizeArgs.size(); ++i)
        os << (i ? "," : "") << ft->retAllocSizeArgs[i];
      os << "]";
    }
  } else if (ft->retKind == RetKind::Handle) {
    os << ",\"handle_class\":"; jsonStr(os, ft->retHandleClass);
  }
  os << "},\n      \"args\":[";
  // ft->retSretArg (if set) is the hidden sret/fp128-lowered return pointer --
  // spliced in as args[0] here, matching its real position as the first raw
  // wasm-level call argument. A params[] entry with abiSlots>1 (fp128 -- see
  // ParamTree.h's TreeNode::abiSlots comment) similarly expands to N
  // consecutive raw-scalar entries. Both are emission-only concerns:
  // ft->params itself is never reindexed/resized for either.
  size_t total = (ft->retSretArg ? 1 : 0);
  for (const auto &pn : ft->params) total += std::max<uint32_t>(1, pn->abiSlots);

  // sizeArgIndex/intoArgIndex are computed (in Infer.cpp) as ft->params-
  // internal indices, but the runtime (lind_marshal_dispatch's
  // _lind_compute_size, LIND_SIZE_FROM_ARG case) indexes the FINAL raw_args[]
  // array, which gen_grate.py builds 1:1 from this JSON "args" array. That
  // array only matches ft->params's own indexing when there's no retSretArg
  // and every param has abiSlots==1; otherwise it's shifted, so translate
  // here. (A multi-slot param's own remapped position is never actually
  // referenced -- FromArg/ptrIntoArg always target a single-slot
  // scalar/pointer sibling -- but it's filled in for completeness.)
  std::vector<size_t> argRemap(ft->params.size());
  {
    size_t pos = ft->retSretArg ? 1 : 0;
    for (size_t i = 0; i < ft->params.size(); ++i) {
      argRemap[i] = pos;
      pos += std::max<uint32_t>(1, ft->params[i]->abiSlots);
    }
  }

  if (total > 0) {
    os << "\n";
    size_t emitted = 0;
    if (ft->retSretArg) {
      jsonNode(os, ft->retSretArg.get(), 4, &argRemap);
      os << (++emitted < total ? ",\n" : "\n");
    }
    for (size_t i = 0; i < ft->params.size(); ++i) {
      const TreeNode *pn = ft->params[i].get();
      if (pn->abiSlots > 1) {
        for (unsigned s = 0; s < pn->abiSlots; ++s) {
          jsonAbiSlot(os, pn, s, 4);
          os << (++emitted < total ? ",\n" : "\n");
        }
      } else {
        jsonNode(os, pn, 4, &argRemap);
        os << (++emitted < total ? ",\n" : "\n");
      }
    }
    os << "      ";
  }
  os << "]";
  os << ",\n      "; jsonWarnings(os, ft);
  os << "\n    }";
}

int main(int argc, char **argv) {
  cl::ParseCommandLineOptions(argc, argv, "marshal-infer: lind marshalling inference\n");

  // Extend the built-in handle/allocator/searcher tables with a per-library file.
  if (!AnnoFile.empty()) {
    std::string aerr;
    if (!loadAnnotationsFile(AnnoFile, aerr)) {
      errs() << "marshal-infer: --annotations " << AnnoFile << ": " << aerr << "\n";
      return 1;
    }
  }

  // Versioned config (issue #27): analysis knobs, checked-in contracts,
  // and coverage thresholds. A malformed/invalid file is a HARD error
  // (unlike --annotations' best-effort merge) -- see Config.h's own
  // comment on why this schema is closed and strict rather than permissive.
  Config config;
  bool haveConfig = !ConfigFile.empty();
  if (haveConfig) {
    std::string cerr;
    if (!loadConfig(ConfigFile, config, cerr)) {
      errs() << "marshal-infer: --config: " << cerr << "\n";
      return 1;
    }
  }

  // Output stream.
  std::error_code ec;
  std::unique_ptr<raw_fd_ostream> fileOut;
  if (!OutFile.empty()) {
    fileOut = std::make_unique<raw_fd_ostream>(OutFile, ec, sys::fs::OF_Text);
    if (ec) { errs() << "marshal-infer: cannot open " << OutFile << ": "
                     << ec.message() << "\n"; return 1; }
  }
  raw_ostream &os = fileOut ? *fileOut : outs();

  // Optional export filter: only emit functions whose name is listed.
  StringSet<> exports;
  bool haveExports = !ExportsFile.empty();
  if (haveExports) {
    std::string exportsPath = ExportsFile;
    std::ifstream in(exportsPath);
    if (!in) { errs() << "marshal-infer: cannot read exports " << exportsPath
                      << "\n"; return 1; }
    std::string line;
    while (std::getline(in, line)) {
      StringRef l = StringRef(line).trim();
      if (!l.empty()) exports.insert(l);
    }
  }

  // Collect inference for each interface-candidate function across ALL input
  // modules, keyed by exported name. glibc exports many symbols as weak aliases
  // (strlen -> __strlen), so we resolve GlobalAliases to their defining function
  // and emit under the alias. `emitted` dedupes across translation units.
  std::vector<std::unique_ptr<FunctionTrees>> records;
  StringSet<> emitted;
  LLVMContext ctx;
  unsigned modOk = 0, modBad = 0;

  auto wanted = [&](StringRef n) {
    return (!haveExports || exports.count(n)) && !emitted.count(n);
  };

  // Pass 1: load every input .bc into a RESIDENT Module (all sharing `ctx`,
  // kept alive for the rest of main()) and index every EXTERNALLY-LINKED
  // defined function by name across all of them. Two passes, not one,
  // because a callee needed for one-hop delegation analysis (see CalleeIndex
  // in Infer.h) may live in a .bc processed later in file order than its
  // caller -- the old single-pass loop discarded each Module before moving
  // to the next, so a caller could only ever see an external declaration for
  // a callee compiled separately, never its body.
  //
  // Internal/static-linkage functions are deliberately excluded: they're
  // invisible outside their own TU, so indexing one by name would let an
  // unrelated same-named static function in a different TU silently resolve
  // some other module's genuinely external declaration (see CalleeIndex's
  // comment). A same-module callee never needs this index at all -- the call
  // site already references its body directly (detectDelegatedArrayBound).
  //
  // A name with more than one externally-linked definition across resident
  // modules is genuinely ambiguous (which one a given cross-module
  // declaration actually resolves to isn't knowable from IR alone) --
  // recorded as nullptr rather than silently keeping whichever was inserted
  // first.
  std::vector<std::unique_ptr<Module>> mods;
  CalleeIndex calleeIndex;
  for (const std::string &input : InputFiles) {
    SMDiagnostic err;
    std::unique_ptr<Module> mod = parseIRFile(input, err, ctx);
    if (!mod) { ++modBad; continue; } // skip unreadable TU
    ++modOk;
    for (Function &f : *mod) {
      if (f.isDeclaration() || f.hasLocalLinkage())
        continue;
      auto res = calleeIndex.try_emplace(f.getName(), &f);
      if (!res.second && res.first->second != &f)
        res.first->second = nullptr; // >1 conflicting definition -- ambiguous
    }
    mods.push_back(std::move(mod));
  }

  // LLM-prompt early exit: entirely separate from the ordinary inference
  // pipeline below it -- reuses only module loading and `calleeIndex` (so
  // cross-module delegation resolves identically to static inference),
  // never touches Config/contracts/coverage, and never reaches Pass 2 or
  // JSON emission. No network request, no model SDK.
  if (LlmPromptOnly) {
    if (LlmFunction.empty() || LlmPromptOutput.empty()) {
      errs() << "marshal-infer: --llm-prompt-only requires both --function "
                "and --prompt-output\n";
      return 1;
    }
    // maxFunctions=0 is incompatible with the entry-always-included policy
    // (LlmPrompt.cpp's own documented choice) -- there would be no way to
    // include even the entry function itself. Rejected here, at the
    // boundary, rather than given silent/surprising behavior inside
    // buildLlmPrompt (see LlmPromptLimits's own precondition comment).
    if (LlmMaxFunctions == 0) {
      errs() << "marshal-infer: --llm-max-functions must be at least 1 (the "
                "entry function itself always counts as one)\n";
      return 1;
    }
    LlmPromptLimits limits;
    limits.maxCallDepth = LlmMaxCallDepth;
    limits.maxFunctions = LlmMaxFunctions;
    limits.maxInstructions = LlmMaxInstructions;

    LlmPromptResult result;
    std::string perr;
    if (!buildLlmPrompt(LlmFunction, mods, calleeIndex, limits, result, perr)) {
      errs() << "marshal-infer: --llm-prompt-only: " << perr << "\n";
      return 1;
    }

    std::error_code dec = sys::fs::create_directories(LlmPromptOutput);
    if (dec) {
      errs() << "marshal-infer: cannot create " << LlmPromptOutput << ": "
             << dec.message() << "\n";
      return 1;
    }
    // The artifact's on-disk name is a SANITIZED, deterministic derivation
    // of the function name (never the raw symbol) -- a symbol containing a
    // path separator or other unusual character must never be able to
    // place a written file outside LlmPromptOutput. The exact original
    // name is still recoverable from the manifest's own "function" field.
    std::string baseName = sanitizeFunctionNameForFilename(LlmFunction);
    SmallString<256> promptPath(LlmPromptOutput);
    sys::path::append(promptPath, baseName + ".prompt.txt");
    SmallString<256> manifestPath(LlmPromptOutput);
    sys::path::append(manifestPath, baseName + ".prompt.json");

    // sanitizeFunctionNameForFilename's character allowlist ([A-Za-z0-9_.-])
    // makes both path components single, separator-free segments by
    // construction -- no '/', no '\', and the ".."-as-a-whole-result case
    // is caught and prefixed there specifically -- so `baseName` can never
    // resolve outside LlmPromptOutput once joined with sys::path::append.
    assert(sys::path::filename(promptPath) == StringRef(baseName + ".prompt.txt") &&
           "sanitized artifact name must be a single path component");

    std::error_code ec1, ec2;
    raw_fd_ostream promptOut(promptPath, ec1, sys::fs::OF_Text);
    if (ec1) {
      errs() << "marshal-infer: cannot write " << promptPath << ": "
             << ec1.message() << "\n";
      return 1;
    }
    promptOut << result.promptText;
    promptOut.close();

    raw_fd_ostream manifestOut(manifestPath, ec2, sys::fs::OF_Text);
    if (ec2) {
      errs() << "marshal-infer: cannot write " << manifestPath << ": "
             << ec2.message() << "\n";
      return 1;
    }
    manifestOut << result.manifestJson;
    manifestOut.close();

    errs() << "marshal-infer: wrote " << promptPath << " and " << manifestPath
           << " (no LLM was called; eligible_for_llm_inference="
           << (isEligibleForLlmInference(result.evidence) ? "true" : "false") << ")\n";
    return 0;
  }

  // Contract validation failures: a stale or incompatible checked-in
  // contract is a hard configuration error, not a warning routed around --
  // collected across every function so a single run reports every
  // offending entry at once, then aborts (no JSON written at all: unlike a
  // coverage-threshold shortfall, a contract that fails this check could
  // otherwise bake a wrong-typed or out-of-range operand straight into the
  // emitted spec, so nothing from this run should be treated as
  // trustworthy output).
  std::vector<std::string> contractErrors;

  // Build one inference record for a (public name, defining function) pair.
  auto buildRecord = [&](StringRef name,
                         const Function &f) -> std::unique_ptr<FunctionTrees> {
    DISubprogram *sp = f.getSubprogram();
    if (!sp) return nullptr; // no debug info — needs -g
    auto ft = buildFunctionTrees(sp, haveConfig ? config.maxTypeDepth : 6);
    if (!ft) return nullptr;
    ft->funcName = name.str();
    if (haveConfig) {
      auto it = config.contracts.find(ft->funcName);
      if (it != config.contracts.end()) {
        std::string verr;
        if (!validateContractAgainstSignature(*ft, it->second, verr))
          contractErrors.push_back(verr);
      }
    }
    inferFunction(f, *ft, calleeIndex, haveConfig ? &config : nullptr);
    return ft;
  };

  // Pass 2: run inference over every resident module.
  for (const std::unique_ptr<Module> &mod : mods) {
    for (Function &f : *mod) {
      if (f.isDeclaration()) continue;
      if (!ShowAll && f.hasLocalLinkage()) continue;
      if (!wanted(f.getName())) continue;
      if (auto ft = buildRecord(f.getName(), f)) {
        emitted.insert(f.getName());
        records.push_back(std::move(ft));
      }
    }
    for (GlobalAlias &ga : mod->aliases()) {
      if (!ShowAll && ga.hasLocalLinkage()) continue;
      if (!wanted(ga.getName())) continue;
      auto *f = dyn_cast_or_null<Function>(ga.getAliaseeObject());
      if (!f || f->isDeclaration()) continue;
      if (auto ft = buildRecord(ga.getName(), *f)) {
        emitted.insert(ga.getName());
        records.push_back(std::move(ft));
      }
    }
  }

  // A contract entry that was NEVER consulted (its symbol was never emitted
  // at all, or the specific argument never reached the contract-check
  // branch in Infer.cpp -- e.g. a stale contract left over after a
  // refactor, or one written for the wrong argument index) is the same
  // class of "quietly wrong" configuration state as an out-of-range
  // operand: a hard error (added to contractErrors below), not a warning
  // routed around.
  if (haveConfig && !config.contracts.empty()) {
    std::map<std::string, const FunctionTrees *> byName;
    for (const auto &r : records) byName[r->funcName] = r.get();
    for (const auto &fc : config.contracts) {
      auto it = byName.find(fc.first);
      if (it == byName.end()) {
        contractErrors.push_back("config contract for '" + fc.first +
            "' never applied -- no such symbol was emitted");
        continue;
      }
      const FunctionTrees *ft = it->second;
      for (const auto &argc : fc.second) {
        int argIdx = argc.first;
        bool applied = ft->forceLocal ? false
            : (size_t)argIdx < ft->params.size() &&
              ft->params[argIdx]->confidence == Confidence::Configured;
        if (!applied)
          contractErrors.push_back("config contract for '" + fc.first +
              "' arg" + std::to_string(argIdx) +
              " never applied (the function force_localed for an "
              "unrelated reason)");
      }
    }
  }

  // Abort the whole run -- no JSON written -- on any contract problem
  // found either above (stale/never-applied) or during buildRecord
  // (out-of-range or wrong-typed operand): see contractErrors' own comment
  // for why this fails closed rather than emitting output that could bake
  // in a spec nothing has actually verified.
  if (!contractErrors.empty()) {
    for (const std::string &e : contractErrors)
      errs() << "marshal-infer: ERROR: " << e << "\n";
    return 1;
  }

  size_t marshalCount = 0;
  for (const auto &r : records)
    if (!r->forceLocal) ++marshalCount;

  if (AsJson) {
    std::string label =
        ModuleName.empty() ? std::string(InputFiles.front()) : ModuleName;
    os << "{\n  \"module\":"; jsonStr(os, label);
    // Config identity (issue #27): which checked-in profile, if any,
    // produced this output -- so a reader (or a diff between two runs) can
    // always tell what configuration was in effect, not just what the
    // results were. Omitted entirely when no --config was given, matching
    // this tool's built-in-default behavior exactly (backward compatible).
    if (haveConfig) {
      os << ",\n  \"config\":{\"version\":" << config.configVersion;
      os << ",\"profile_name\":"; jsonStr(os, config.profileName);
      os << ",\"source_path\":"; jsonStr(os, config.sourcePath);
      os << "}";
    }
    os << ",\n  \"function_count\":" << records.size();
    os << ",\n  \"functions\":[\n";
    for (size_t i = 0; i < records.size(); ++i) {
      jsonFunction(os, records[i].get());
      os << (i + 1 < records.size() ? ",\n" : "\n");
    }
    os << "  ]\n}\n";
  } else {
    for (const auto &r : records) printFunctionTree(os, r.get());
  }

  // Coverage report to stderr (so it doesn't pollute JSON on stdout).
  errs() << "marshal-infer: " << records.size() << " function(s) from " << modOk
         << " module(s)";
  if (modBad) errs() << " (" << modBad << " unreadable)";
  if (haveExports)
    errs() << "; " << records.size() << "/" << exports.size()
           << " exported symbols covered";
  errs() << "; " << marshalCount << "/" << records.size() << " marshal\n";

  // Coverage-threshold enforcement (issue #27): fail the WHOLE run --
  // nonzero exit, output already written above so it's still inspectable --
  // when this library's marshal rate drops below what its checked-in
  // profile expects. This is deliberately checked LAST, after every other
  // output has been produced: a threshold failure should stop a build
  // pipeline, not hide what was actually inferred.
  if (haveConfig && config.coverage.enabled) {
    size_t denom = haveExports ? exports.size() : records.size();
    double pct = denom ? (100.0 * (double)marshalCount / (double)denom) : 0.0;
    bool countOk = marshalCount >= (size_t)config.coverage.minMarshalCount;
    bool pctOk = pct >= config.coverage.minMarshalPct;
    if (!countOk || !pctOk) {
      errs() << "marshal-infer: COVERAGE THRESHOLD FAILED (" << config.sourcePath
             << "): got " << marshalCount << " marshal ("
             << format("%.1f", pct)
             << "%), required >= " << config.coverage.minMarshalCount
             << " and >= " << format("%.1f", config.coverage.minMarshalPct) << "%\n";
      return 1;
    }
  }
  return 0;
}
