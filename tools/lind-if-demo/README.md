# Lind-IF flow demo

A dependency-free Python 3.10+ **deterministic simulator** of the 16 November
2026 Lind-IF discussion draft. It runs no guest code, does not call real
Wasm/MPK runtimes, and does not change the Rust implementation. Its purpose is
to make the contract's interactions, ordering constraints and unanswered
questions executable and easy to discuss.

## Try it

From this directory:

```sh
python3 -m lind_demo list
python3 -m lind_demo run fork-memory
python3 -m lind_demo run interposition cross-runtime-exec
python3 -m lind_demo run all --quiet
python3 -m lind_demo coverage
python3 -m lind_demo limitations
python3 -m lind_demo repl
```

Each named sequence starts with a fresh world and includes assertions.
Trace lines identify **Core**, **3i**, **RawPOSIX**, **ABI/toolkit helpers** and
**runtime:wasm / runtime:mpk**. Runtime-local scheduling state is visible for
inspection, not proposed as public Lind-IF state or notifications.
Each runtime has its own simulated worker; an idle worker is not execution
completion. `tick` advances every eligible thread once in deterministic order.

No installation, external libraries, services or host privileges are needed.
To use the existing development container instead:

```sh
docker run --rm --entrypoint python3 \
  -e PYTHONDONTWRITEBYTECODE=1 \
  -v "$PWD:/demo:ro" -w /demo \
  lind-mnt-dev:mpk_latest -m lind_demo run all --quiet
```

The explicit entrypoint avoids the image's repository-ownership setup.

## Write your own sequences

```sh
python3 -m lind_demo file examples/custom.json
python3 -m lind_demo export handler-fork > /tmp/my-flow.json
# Edit /tmp/my-flow.json, then:
python3 -m lind_demo file /tmp/my-flow.json
python3 -m lind_demo file /tmp/my-flow.json --json
python3 -m lind_demo commands
```

`--json` emits JSON Lines events for visualization or scripting. Unexpected
errors and failed assertions exit nonzero. `--quiet` suppresses the event trace.
`file -` reads a JSON array from stdin. JSON is parsed as data; nothing is
evaluated as Python or passed to a shell.

A step names a command and its keyword arguments. `commands` prints signatures.
For example:

```json
[
  {"op": "launch", "cage": 1, "image": "app.wasm"},
  {"op": "fork", "source": 1, "child": 2, "save": "fork"},
  {"op": "tick"},
  {"op": "check", "path": "last", "equals": []},
  {"op": "unwind", "barrier": "$fork"},
  {"op": "signal", "cage": 2, "signal": "STOP"},
  {"op": "exec", "cage": 2, "image": "new.elf"},
  {"op": "check", "path": "state.cages.2.stopped", "equals": true},
  {"op": "signal", "cage": 2, "signal": "CONT"},
  {"op": "state"}
]
```

- `.wasm` selects the Wasm provider; `.elf` selects the MPK provider.
  These are labels, not files that must exist.
- Cage and thread IDs are positive integers supplied by the sequence.
- `save` stores a command's result; `"$name"` references it in later arguments.
- `check` follows a dot path through `state` or `last`, including list indices.
  It does not replace `last`.
- `expect_error` requires an exact model error code, e.g.
  `{"op":"fork","source":1,"child":3,"fail":"memory","expect_error":"ENOMEM"}`.
  An unexpected error **or unexpected success** fails the sequence.
- Guest addresses are small integers in a 64 KiB reservation; data is hex.
  Host addresses are deliberately different between cages. `translate` alone
  establishes neither mapping permissions nor validity for access.
- `call` preserves `caller`, `target`, target `tid`, and argument `owner`.
  Arguments such as `{"value":64,"owner":1,"kind":"pointer"}` remain untranslated
  in 3i. `copy` explicitly identifies source and destination owners.

In the REPL, enter **one JSON step per line**, or `commands`, `state`,
`limitations`, `quit`. Saved values and state persist. An error is printed
without discarding the world.

## Flow boundaries worth stepping through

- **Initial entry:** `init` -> Core records -> `create` ->
  immediately eligible. `run_before_return:true` demonstrates execution before
  the   call returns, after Core lifetime setup. `exit_before_return:23` demonstrates
  immediate exit with status 23. Staging and logical STOP override that
  eligibility. Inject `create` failure with `fail:true`.
- **Fork:** `fork` -> `DuplicateCage` -> `DuplicateThread` ->
  Core TID writes/publication -> **held**. `unwind` explicitly crosses the
  handler/3i boundary and releases eligible continuations. There is no CONT just
  to start a fork child. `tick` between fork and unwind cannot run either
  continuation; unaffected sibling threads can run. Memory/thread setup faults
  roll back the child without a zombie.
- **Pthread:** `clone` uses `DuplicateThread`, not `CreateThread`, with the same
  scheduling barrier, fresh symbolic TLS and shared cage memory. Optional
  `clear_tid` models a combined set/clear-TID word and futex wake on thread exit.
  It is not a full clone-flag implementation.
- **On behalf:** `fork` has separate `caller`, `source`, `tid` and `mode`.
  `mode:"raw"` requires a stopped source and
  `no_supervisor_writes:true`; the MPK model accepts it without injecting fork
  return values. Wasm rejects raw capture rather than inventing Asyncify support.
  Syscall-mode on-behalf forwarding assumes an already selected syscall boundary.
  `forwarded-fork` demonstrates a grate forwarding fork for its suspended caller,
  preserving that caller's selected continuation rather than duplicating itself.
- **Exec:** `exec` composes staging preparation and apply. Alternatively use
  `init(staging:true)`, `create`, `tick`, then `apply` to see the staging gate.
  Apply preserves target identity, parent/wait state, outbound routes and STOP;
  removes sibling contexts; invalidates old continuations/ABI generation;
  and hands execution to the destination runtime's own worker.
  `fail:"prepare"` preserves the old image; `fail:"commit"` kills it.
- **Interposition:** `register` -> `call` -> `RunHandler` ->
  `return_handler`. The call remains pending while stepping other events:
  no ordinary syscall return is supplied until the handler finishes.
  Nested calls must return inside-out. STOP halts handlers in that cage too.
  A handler fork child cannot return through the original invocation; it can
  exec a new image instead. Single-thread exit inside handlers is rejected.
- **Teardown:** `exit_thread`, `terminate` (exit-group), `signal KILL`, `wait`.
  Killing a suspended caller severs its invocation without waiting for the
  handler. Forced provider death cancels dependent calls recursively.
  Core retains root status and does not complete while descendants are alive.
- **Signals:** STOP/STOP/CONT is non-counting. Ordinary USR1/TERM acceptance is
  asynchronous; ticks deliver pending cage signals only when eligible.
  `sigreturn(valid_frame:false)` faults the cage instead of returning EFAULT.

## Interface coverage

`coverage` runs the sequences and reports 21/21 **named draft operation
observations**, not full Linux semantics, execution-path coverage, or proof of
runtime conformance. Helpers with `XYZ` use an illustrative iovec layout.
Optional convenience helpers are demonstrated, not claimed as finalized APIs.

| Draft operations | Demo commands / reference scenarios |
|---|---|
| BelongsToRuntime, InitCage, CreateThread | `init`, `create`, `launch`; `startup` |
| DuplicateCage, DuplicateThread | `fork`, `clone`; `fork-memory`, `pthread`, `stop-and-capture` |
| ApplyStagingCage | `apply`, `exec`; `cross-runtime-exec`, `rollback` |
| TerminateCage, TerminateThread | `terminate`, `exit_thread`; `handler-teardown`, `fork-memory` |
| SendSignal | `signal`; `signals`, `stop-and-capture`, `interposition` |
| RunHandler, 3i::make_syscall | `call`, `return_handler`, lifecycle dispatch; `interposition`, `handler-fork`, `handler-fault` |
| DoMmap, DoMprotect, DoMunmap | `map`, `protect`, `unmap`, reservation bootstrap; `memory-abi`, `fork-memory` |
| GetHostAddress, GetGuestAddress, GetCageABI | `translate`, `abi`; `memory-abi`, `cross-runtime-exec` |
| GetRuntimeXYZfromLindXYZ, GetLindXYZfromRuntimeXYZ | `convert`; `memory-abi` |
| SetIovecBase, SetGuestIovecBase | `set_iovec`; `memory-abi` |
| Candidate CopyFromGuest / CopyToGuest | `read`, `write`, `copy`; `memory-abi` |
| Candidate AccessGuestMemory | `probe` (explicitly not a stable guard); `memory-abi` |

## Limitations exposed, not hidden

`limitations` prints these assumptions; `list` associates them with sequences:

1. **Bootstrap has no guest thread yet.** `InitCage` requests reservation through
   3i before `CreateThread`. The trace uses `target_tid:null` for that bootstrap
   request; the draft's thread-ID argument still needs an explicit threadless
   identity/sentinel convention. This is a demo assumption, not a finalized ABI.
2. **Synchronous capture is the key Wasm gap.** The simulator assumes the
   required syscall driver/stack-switch boundary. Current Asyncify cannot simply
   implement synchronous DuplicateThread inside its hostcall. Passing a scenario
   does not solve this runtime problem.
3. **Targeting/capture fields are unfinished.** We choose explicit source TIDs,
   distinguish raw from syscall capture, and reject thread-only STOP. Cage STOP
   is not proof that supervisor writes have drained.
4. **Exec needs a committed-failure outcome.** `EEXEC_COMMITTED` is a demo
   decision, not an agreed errno/API. Preparation and apply are atomic commands;
   the model does not prove concurrent handler-registration/lifecycle exclusion.
5. **Exceptional handler outcomes need policy.** We choose recursive
   caller-fault cancellation. Other policies are possible. Handler exec while an
   invocation remains active is conservatively rejected by this small model,
   even after unregistering; a richer handoff/unwind model is needed.
6. **Memory safety needs real mechanisms.** Bytearrays and serialized commands
   cannot establish fault containment, race-safe mappings, native permissions,
   MPK code ownership, or a consistent application snapshot. Partial copies
   preserve progress and fault owner/address; faults do not roll back writes.
7. **ABI validity spans exec.** Explicit generations model stale-token
   detection, not a finalized public ABI lease. Only iovec32/iovec64 little-endian
   conversions are implemented; no full structure or syscall catalog.
8. **Independent workers are necessary, not sufficient.** Ticks run eligible
   contexts but cannot validate progress under synchronous cross-runtime waits,
   green-thread blocking, or scheduler deadlocks.
9. **This is deliberately not Linux emulation.** No real stack/TLS/Asyncify
   frames, arbitrary clone flags, signal masks/queueing rules, fd inheritance,
   vfork, start constructors, or shared-code reclamation. Whole-map memory
   protection/unmap is a demo simplification; addresses need not be page-aligned.

## Files and validation

```text
lind-if-demo/
  README.md
  lind_demo/
    __init__.py
    __main__.py       CLI, JSON trace, REPL
    model.py          Core, 3i, runtime actors and invariants
    runner.py         validated custom steps and assertions
    scenarios.py     standard reference flows
  examples/
    custom.json
  tests/
    test_demo.py
```

```sh
python3 -B -m unittest discover -s tests -v
python3 -B -m lind_demo run all --quiet
python3 -B -m lind_demo file examples/custom.json --quiet
python3 -B -m lind_demo coverage
```
