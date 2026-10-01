// LlmPrompt.h — offline evidence collection and prompt preview for a future
// LLM-assisted marshal-inference backend. Builds the exact prompt (and a
// machine-readable manifest describing how it was assembled) that a future
// LLM backend would receive for ONE boundary function, entirely offline --
// no network request, no model SDK, no provider credentials. This is an
// inspection/preview mode: it makes no marshalling decision and never
// touches the JSON inference records `inferFunction` produces.
#pragma once

#include "Infer.h"

#include "llvm/ADT/StringRef.h"

#include <memory>
#include <optional>
#include <string>
#include <vector>

namespace llvm {
class Function;
class Module;
}

namespace marshal {

// Deterministic bounds on how far relevant-callee discovery walks the call
// graph and how much IR it pulls in. Crossing any of these sets
// slice_complete=false in the manifest rather than silently truncating.
//
// maxFunctions must be at least 1 (the entry function itself always counts
// as one and is always included regardless of maxInstructions -- see
// LlmPrompt.cpp's "entry instruction-budget policy"); callers must validate
// this before calling buildLlmPrompt, it is not re-validated here.
struct LlmPromptLimits {
  unsigned maxCallDepth = 2;
  unsigned maxFunctions = 8;
  unsigned maxInstructions = 6000; // summed across every included function body
};

enum class EvidenceNoteKind { Informational, Incomplete };

// A single fact recorded during evidence collection. `kind` decides
// whether it merely documents something worth knowing (a memcpy touching a
// traced pointer -- fully visible evidence, not a gap) or an actual gap in
// what the prompt can prove (a store escape, an unresolved call, a
// depth/function/instruction limit crossed, a recursive cycle, ...).
// `originatingArgument`, when known, is the ENTRY function's OWN argument
// index this note's pointer ultimately traces back to (composed across
// call edges as evidence collection walks deeper) -- recorded even though
// this version's `isEligibleForLlmInference` only reads the function-wide
// `PromptEvidence::sliceComplete` flag, so a later per-pointer completeness
// check has real provenance to work from instead of re-deriving it.
struct EvidenceNote {
  EvidenceNoteKind kind;
  std::string message;
  std::optional<unsigned> originatingArgument;
};

// One (calleeArgument -> callerArguments) correspondence at ONE call site.
// Deliberately an element of an ARRAY (see CallEdgeRecord::argumentMap),
// not a JSON object keyed by argument: a callee argument can receive
// values merged (via a PHI/select) from MULTIPLE caller arguments, which a
// keyed-by-callee-argument JSON object can express, but only as long as
// every callee argument is a distinct key -- an object representation
// silently breaks the moment the SAME caller/callee pair recurs at a
// different call site (see CallEdgeRecord::callSite), since JSON object
// keys are not scoped per call site.
struct ArgumentCorrespondence {
  unsigned calleeArgument;
  std::vector<unsigned> callerArguments; // sorted ascending, deduplicated
};

// Stable identity for a call instruction within its defining function -- a
// basic-block ordinal and an instruction ordinal within that block, both
// assigned by one deterministic walk of the function's own IR. Never a
// pointer address: those are not reproducible across separate runs of the
// same input.
struct CallSiteId {
  unsigned basicBlockOrdinal = 0;
  unsigned instructionOrdinal = 0;
};

struct CallEdgeRecord {
  const llvm::Function *caller = nullptr;
  const llvm::Function *callee = nullptr;
  CallSiteId callSite;
  std::vector<ArgumentCorrespondence> argumentMap; // sorted by calleeArgument
};

// Every deterministically-derived fact `buildLlmPrompt` collects before
// rendering the prompt/manifest text -- kept as structured data (not just
// rendered text) so `isEligibleForLlmInference` and tests can reason about
// it directly.
struct PromptEvidence {
  const llvm::Function *entry = nullptr;
  std::vector<const llvm::Function *> includedFunctions; // entry first, BFS order
  std::vector<CallEdgeRecord> callEdges;
  std::vector<EvidenceNote> notes;
  bool sliceComplete = true;
  unsigned includedInstructionCount = 0;
  LlmPromptLimits limits;
};

// Whether a future production LLM invocation would be permitted to
// actually call the model for this evidence. Currently exactly
// `evidence.sliceComplete` -- kept as its own named predicate (rather than
// every future call site reading `sliceComplete` directly) so a later,
// separate capability check (e.g. a maximum prompt byte budget a specific
// target model enforces) can be added here without touching every caller.
// Prompt-only preview mode (see main.cpp) does NOT gate on this: writing
// an incomplete prompt/manifest for human inspection is deliberately still
// allowed, since debugging incompleteness is the whole point of Stage 1.
bool isEligibleForLlmInference(const PromptEvidence &evidence);

struct LlmPromptResult {
  std::string promptText;
  std::string manifestJson;
  PromptEvidence evidence;
};

// Builds `functionName`'s Stage 1 prompt and manifest from the already-loaded
// modules in `mods` (the same resident set main.cpp builds `calleeIndex`
// from, so cross-module delegation resolves identically to ordinary static
// inference). Returns false (with a specific, actionable message in `err`)
// only if the function cannot be found at all. Debug info is NOT required:
// a function or callee with no DWARF still gets a full, correctly-labeled
// (arg0, arg1, ...) prompt -- the canonical argument identity is always the
// LLVM argument index (adjusted for a hidden sret pointer, the one ABI
// mapping this tool supports), and a DWARF name, when present, is only an
// additional label on top of it, never a requirement.
bool buildLlmPrompt(llvm::StringRef functionName,
                    const std::vector<std::unique_ptr<llvm::Module>> &mods,
                    const CalleeIndex &calleeIndex,
                    const LlmPromptLimits &limits,
                    LlmPromptResult &result, std::string &err);

// Deterministic, filesystem-safe artifact basename for `functionName` (no
// extension) -- collapses anything outside [A-Za-z0-9_.-] to '_' so a
// symbol containing a path separator or other unusual character can never
// place a written file outside the requested output directory. The exact
// original name always stays recoverable from the manifest's own
// "function" field, which is never sanitized.
std::string sanitizeFunctionNameForFilename(llvm::StringRef functionName);

} // namespace marshal
