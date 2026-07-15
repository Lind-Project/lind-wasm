// Annotations.h — the curated, *can't-be-inferred* knowledge as DATA rather than
// inline code. These are the cases where static analysis has no signal: which
// struct types are opaque handles, which functions are allocators (when the
// `allocsize` attribute is stripped), and which return a pointer into arg0 when
// the body is in another TU. Built-in defaults cover glibc/zlib; `--annotations
// <file.json>` extends them per library. The general dataflow detectors live in
// Infer.cpp — only the genuinely-uninferable lookups live here.
#pragma once

#include "llvm/ADT/StringRef.h"
#include <map>
#include <set>
#include <string>
#include <vector>

namespace marshal {

struct AllocSpec {
  std::vector<int> sizeArgs; // byte size = product of these arg values
  bool forceLocal = false;   // realloc-style (alloc+free+copy) -> run locally
};

struct Annotations {
  std::map<std::string, std::string> handleTypes;    // DWARF struct name -> class
  std::map<std::string, AllocSpec>   allocators;     // fn base name -> alloc spec
  std::set<std::string>              returnIntoArg0;  // searcher fn base names
  // fn base name -> arg indices that are NULL-terminated arrays of cstr pointers
  // (argv/envp); the array walk is interprocedural (exec -> syscall), so listed.
  std::map<std::string, std::vector<int>> ptrArrayArgs;
};

// The active annotations: built-in defaults, extended by any --annotations file.
Annotations &annotations();

// Merge a JSON annotations file into the active set. False (+ err) on failure.
bool loadAnnotationsFile(llvm::StringRef path, std::string &err);

} // namespace marshal
