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
#include "Infer.h"
#include "ParamTree.h"

#include "llvm/IR/DebugInfoMetadata.h"
#include "llvm/IR/Function.h"
#include "llvm/IR/GlobalAlias.h"
#include "llvm/IR/Module.h"
#include "llvm/IRReader/IRReader.h"
#include "llvm/Support/CommandLine.h"
#include "llvm/Support/FileSystem.h"
#include "llvm/Support/MemoryBuffer.h"
#include "llvm/Support/SourceMgr.h"
#include "llvm/Support/raw_ostream.h"
#include "llvm/ADT/StringSet.h"

#include <fstream>
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
  for (size_t i = 0; i < ft->params.size(); ++i) {
    os << "  arg" << i << ":\n";
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

static void jsonNode(raw_ostream &os, const TreeNode *n, unsigned ind) {
  std::string pad(ind * 2, ' ');
  bool isField = !n->fieldName.empty();
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

  // An inner pointer translated as an offset into another argument (strtol endptr).
  if (n->ptrIntoArg) {
    os << "\"kind\":\"ptr_into_arg\",\"into_arg\":" << n->intoArgIndex << "}";
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
         << n->sizeArgIndex;
    if (n->sizeKind == SizeKind::Const)
      os << ",\"const_size\":" << n->constSize;
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
      jsonNode(os, n->children[i].get(), ind + 1);
      os << (i + 1 < n->children.size() ? ",\n" : "\n");
    }
    os << pad << "]";
  }
  os << "}";
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
  if (!ft->params.empty()) {
    os << "\n";
    for (size_t i = 0; i < ft->params.size(); ++i) {
      jsonNode(os, ft->params[i].get(), 4);
      os << (i + 1 < ft->params.size() ? ",\n" : "\n");
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

  // Build one inference record for a (public name, defining function) pair.
  auto buildRecord = [&](StringRef name,
                         const Function &f) -> std::unique_ptr<FunctionTrees> {
    DISubprogram *sp = f.getSubprogram();
    if (!sp) return nullptr; // no debug info — needs -g
    auto ft = buildFunctionTrees(sp);
    if (!ft) return nullptr;
    ft->funcName = name.str();
    inferFunction(f, *ft);
    return ft;
  };

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

  for (const std::string &input : InputFiles) {
    SMDiagnostic err;
    std::unique_ptr<Module> mod = parseIRFile(input, err, ctx);
    if (!mod) { ++modBad; continue; } // skip unreadable TU
    ++modOk;
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

  if (AsJson) {
    std::string label =
        ModuleName.empty() ? std::string(InputFiles.front()) : ModuleName;
    os << "{\n  \"module\":"; jsonStr(os, label);
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
  errs() << "\n";
  return 0;
}
