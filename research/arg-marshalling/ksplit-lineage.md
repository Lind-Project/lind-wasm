# KSplit and Its Four Isolation Ancestors — What They Teach Us About a Practical Argument-Marshalling Approach

*Focus papers: KSplit (OSDI'22) + its background citations BGI [18] (SOSP'09), XFI [25] (OSDI'06), LXFI [62] (SOSP'11), LXDs [66] (ATC'19). All five PDFs in `papers/`. This note exists to answer one question: **what is the practical way for Lind to marshal library-function arguments?***

---

## 0. The one axis that organizes all five papers

Every one of these systems isolates untrusted code that was written to run in one shared
address space, and each must answer: *when the trusted and untrusted sides touch the same
data, how is that data made safe to share?* There are exactly two answers, and the five
papers split cleanly along that line — which is the single most important thing to take away,
because **it tells Lind which camp it is structurally forced into.**

```
            CHECK-IN-PLACE                         COPY-ACROSS
   (one shared address space;            (separate domains; private copies;
    gate every access with a check)       marshal state on each call)
   ────────────────────────────          ─────────────────────────────
        XFI ('06)                              LXDs ('19)  ── manual IDL/projections
        BGI ('09)                              KSplit ('22) ── auto-generated IDL
        LXFI ('11)                                    │
                                                      └── + LXFI's capabilities for "who may touch what"
```

**Lind's cages have separate Wasm linear memories.** There is no shared address space to gate,
so the check-in-place trick (XFI/BGI) is *not available* — Lind is structurally in the
**copy-across** camp, alongside LXDs and KSplit. That single fact decides the shape of the
practical answer: **Lind cannot avoid marshalling; it must copy/serialize state across the
boundary, so it needs a good descriptor of *what* to copy.** The check-in-place papers are
still worth reading — they tell us what we give up (zero-copy) and lend us a validation idea —
but LXDs→KSplit is the lineage Lind actually lives in.

A second axis runs through the copy-across camp: **how the "what to copy" descriptor is
produced** — by hand (LXDs), or automatically (KSplit). That is exactly Lind's "today vs goal."

---

## 1. The five papers, briefly — and how each touches Lind's scope

### KSplit (OSDI'22) — *automate the marshalling descriptor*
**What:** static analysis (PDG + parameter trees + CCured pointer classes + read/write access
analysis) that **auto-generates** an interface-definition-language (IDL) describing how to
synchronize the state a kernel↔driver call shares, plus warnings for the ~2% it can't resolve.
**Lind scope:** this is Lind's **layer-1 goal** — replacing hand-written `lind_abi_spec` with
inference. It is the closest match in the literature to what Lind wants to build.
**Marshalling takeaway:** directions (`in/out/inout`) come *for free* from read/write access
analysis; "copy only touched fields" (projections) cuts the `z_stream`-style transitive copy by
orders of magnitude; ~98% can be inferred, the rest flagged. *(Full detail in `ksplit-analysis.md`.)*

### LXDs (ATC'19) — *the manual descriptor KSplit automates; Lind's "today"*
**What:** a framework to run kernel subsystems in isolated domains, driven by a **hand-written
IDL** whose central concept is the **projection**: a named, per-function subset of a struct's
fields that crosses the boundary, with `[in]/[out]` directions and `alloc/bind/dealloc`
qualifiers controlling shadow-copy lifetime. An IDL compiler emits the glue; a small runtime
(libLXD) plus an L4-like capability microkernel back it; calls are asynchronous.
**Lind scope:** **this is essentially where Lind is today** (a human writes the marshalling
descriptor) — and its descriptor design is almost exactly the shape `lind_abi_spec` should take.
**Marshalling takeaway:** the projection + `[in]/[out]` + `alloc/bind/dealloc` vocabulary is a
*proven, hand-writable* descriptor — evidence that Lind's manual phase is viable, and a concrete
template. Crucially, even LXDs already **infers default direction from usage**, and its
`bind`/`alloc` qualifiers are a **shadow-object handle table** — the mechanism Lind needs for
opaque handles and out-params.

### LXFI (SOSP'11) — *the capability/ownership dimension pure copy-descriptors miss*
**What:** software fault isolation for kernel modules built around **API integrity**: it splits a
shared module into **principals**, and programmers write **lightweight annotations** expressing
the interface contract in terms of **capabilities** that are *granted, checked, and transferred*
between modules (a compiler plugin instruments the code to enforce this at runtime).
**Lind scope:** LXFI is the closest prior art to **Lind's opaque-handle-as-capability** and to
its **untrusted-grate trust model** — it answers "which side is *allowed* to read/write/own this
reference," not just "how many bytes." That is the dimension a byte-copy descriptor cannot express.
**Marshalling takeaway:** treat a handle/reference as a **capability with grant/transfer
semantics**, not as bytes to copy. The "who may touch what" contract is part of the marshalling
spec, and it is what makes interposition *safe* when the handler is untrusted.

### BGI (SOSP'09) — *the check-in-place alternative, and a validation idea*
**What:** runs kernel extensions in **separate protection domains that share one address space**,
associating a **byte-granularity access-control list (ACL)** with every byte of memory; an
**interposition library** mediates every extension↔kernel call, **granting and revoking access
rights per the semantics of each call**, while inline checks enforce them. No copying — the data
stays in place and access is gated.
**Lind scope:** the *enforcement* model is **not available** to Lind (no shared address space),
so BGI is the road not taken. But its **interposition library that grants/revokes per-call access
rights** is conceptually exactly Lind's trusted portal/`safe_copy` deciding *what region a call is
permitted to touch* — a useful validation framing.
**Marshalling takeaway:** if you *could* share memory (same machine, shmem transport), per-region
access rights are an alternative to copying — relevant to a future zero-copy `shmem://` fast path.
Also: BGI's "rights follow call semantics" is the same idea as in/out directions, just enforced
instead of copied.

### XFI (OSDI'06) — *the SFI foundation; trust only the verifier*
**What:** the foundational software-fault-isolation system: **inline software guards** +
control-flow integrity + two stacks (a scoped stack and an allocation stack), with a **stand-alone
verifier** as the only trusted component — the host trusts the verifier, *not* how the module was
produced (hand-written, compiled, or rewritten).
**Lind scope:** least directly about marshalling, but its **"trust only a small verifier, not the
module"** principle maps onto Lind's TCB philosophy: keep the trusted mechanism tiny
(`safe_copy` + `translate_handle` + validation), let untrusted handlers be produced any way.
**Marshalling takeaway:** mostly architectural — minimize and isolate the trusted checker; the
marshalling glue itself can be untrusted/generated as long as a small trusted core validates the
memory operations it requests.

---

## 2. The lineage as one story (and where Lind sits in it)

```
 XFI '06 ─► BGI '09 ─► LXFI '11        LXDs '19 ──────────► KSplit '22
 (guards)   (byte ACLs) (capabilities/   (manual projection    (auto-generated
            + interpos.  API integrity     IDL; alloc/bind;       projection IDL;
            library)     annotations)      infers direction)      ~98% automated)
   └──────── check-in-place ────────┘     └──────────── copy-across ───────────┘
                                                    ▲                    ▲
                                            Lind TODAY            Lind GOAL
                                       (hand-written          (inferred
                                        lind_abi_spec)         lind_abi_spec)
```

Read left-to-right it is a 16-year migration from *"gate every access to shared memory"* to
*"copy a precisely-described projection of state between private domains, and generate that
description automatically."* **Lind is born on the right end** (separate address spaces force
copy-across) and is currently at the LXDs stage (manual descriptor), aiming for the KSplit stage
(inferred descriptor). The two check-in-place ancestors (XFI/BGI) and the capability ancestor
(LXFI) feed in sideways as, respectively, a *validation* idea and the *ownership/handle* idea.

---

## 3. Practical takeaways for Lind argument marshalling

Ordered by how directly they answer "what should we actually build."

1. **Accept that Lind must marshal (copy/serialize), and design the descriptor well.** Separate
   linear memories rule out BGI/XFI-style in-place checking. The whole game is the quality of the
   "what to copy" descriptor — so invest there.

2. **Model `lind_abi_spec` on the LXDs projection, not a flat arg list.** Adopt: a **projection =
   field-subset of a struct** (don't copy whole structs — the `z_stream` win), **per-field
   `[in]/[out]`**, and **`alloc/bind/dealloc`** lifetime qualifiers. This is a *proven,
   hand-writable* design — so the manual phase Lind is in is sound, and there is a concrete grammar
   to copy rather than invent.

3. **`bind`/`alloc` IS the opaque-handle mechanism.** LXDs' shadow-object table (look up the
   callee-side copy bound to a caller-side object, or allocate one) is exactly the **handle
   translation table** Lind needs for `FILE*`/fd/remote-pointer arguments. Add a `bind`-style
   qualifier + a trusted handle table; this generalizes cleanly to inter-machine (the token is a
   remote id) — the universal-model requirement.

4. **Borrow LXFI's capability/ownership layer for safety, since Lind's handlers are untrusted.**
   A marshalling descriptor that only says "copy N bytes in/out" is unsafe when the grate is
   untrusted. Express handles as **capabilities with grant/transfer semantics** and have the
   trusted core check them — this is what lets an untrusted handler marshal without being able to
   forge access to memory it shouldn't touch. (Pairs with BGI's "rights follow call semantics.")

5. **Direction inference is low-hanging fruit — even LXDs did a partial version.** Before the full
   KSplit pipeline, a modest read/write analysis can fill most `ptr_direction` fields
   automatically. Start there; it removes the most error-prone hand-annotation.

6. **Keep the trusted core tiny (XFI's lesson).** Trusted = validate + `safe_copy` +
   `translate_handle`; everything else (the projection-walking glue, transport, routing) is
   untrusted/generated. The glue can be produced by hand now and by a KSplit-style analyzer later
   without growing the TCB.

7. **A realistic staged plan falls straight out of the lineage:**
   - **Now (LXDs stage):** hand-written projection-style `lind_abi_spec`; per-field in/out;
     `alloc/bind` for handles; flat trusted `safe_copy`.
   - **Next:** capability checks on handles (LXFI) for the untrusted-grate threat model; an
     address-independent serialization layer under the same descriptor for inter-process/machine.
   - **Later (KSplit stage):** LLVM analysis that auto-emits the projection descriptor + glue
     (the coordinator grate), with warnings for the residue — closing the manual→automatic gap.

**The shortest statement of the practical approach:** *build LXDs-style projections as
`lind_abi_spec` (proven, hand-writable), make handles first-class via `bind`/capability
(LXFI/shadow-table), keep a tiny trusted copy+validate core (XFI), and automate the descriptor's
generation later (KSplit).* BGI marks the zero-copy path Lind can only take on a shared-memory
fast path, not in general.
