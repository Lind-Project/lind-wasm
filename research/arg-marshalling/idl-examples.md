# IDL Examples: LXDs (hand-written) and KSplit (auto-generated)

*Concrete IDL listings from the two copy-across papers, annotated, then mapped to Lind's
`lind_abi_spec` and the zlib grate. Listings are verbatim from the papers (OCR glyphs like
`∗` normalized to `*`); the Lind section is illustrative.*

Relationship in one line: **LXDs is the hand-written projection IDL; KSplit auto-generates the
same kind of IDL** and adds attributes (`alloc_sized`, `within`) for messy low-level idioms.

---

## 1. LXDs IDL — *hand-written* (the proven manual descriptor; ~ where Lind is today)

### 1a. Declare the interface — a `module` of remote calls
```c
module net() {
    rpc int  register_netdevice(projection net_device *dev);
    rpc void ether_setup(projection net_device *dev);
    ...
}
```
The IDL compiler turns each `rpc` into caller/callee stubs + dispatch loops on both sides.

### 1b. A `projection` = the subset of a struct's fields that crosses the boundary
```c
projection <struct net_device> net_device {
    unsigned int flags;
    unsigned int priv_flags;
    ...
    projection net_device_ops [alloc(caller)] *netdev_ops;   // nested projection
}
```
From the paper:
- Lists **only the fields the other side uses**; private fields (e.g. pointers to unrelated
  structures) are **omitted** — the "don't copy the whole struct" idea.
- **Lexical scope**: the same struct can be projected *differently* per function.
- **`[in]` / `[out]`** per field set copy direction; *mostly optional* — "the IDL compiler can
  infer the default direction from the way the projection is used." Default above is `[in]`.
- **`alloc` / `bind` / `dealloc`** control the **shadow copy lifetime**: `[alloc(callee)]` =
  allocate a fresh copy on the callee side and marshal all fields in; `[alloc(caller)]` = caller
  owns it; `dealloc` = free it.

### 1c. Opaque cross-domain references (the handle table)
> "each remote reference is a number that is resolved through a fast hash that is private to each
> thread … the IDL pairs every local object with a reference that is used to look up a
> corresponding shadow copy in another domain."

A cross-domain object is a **token resolved through a per-thread hash** — i.e. the shadow handle
table. (LXDs notes these are "similar to a capability in the LXD microkernel.")

### 1d. Function pointers / callbacks — `[bind]` resolves the existing shadow
```c
projection <struct net_device_ops> net_device_ops {
    rpc [alloc] int (*ndo_open)(projection netdev_empty [bind] *dev);
    rpc [alloc] int (*ndo_stop)(projection netdev_empty [bind] *dev);
    rpc [alloc] int (*ndo_start_xmit)(projection sk_buff *skb,
                                      projection net_device [bind] *dev);
    ...
}
```
`[bind]` = "don't copy a new `dev`; resolve the already-registered shadow by its reference."
Callbacks use **hidden arguments**: a trampoline carries cross-domain context behind an
unmodified C signature.

---

## 2. KSplit IDL — *auto-generated* (same surface language, richer attributes)

### 2a. The generated rpc declaration
```c
rpc netdev_tx_t ixgbe_xmit_frame(projection sk_buff [alloc(callee)] *skb,
                                 projection net_device *netdev)
```

### 2b. The generated `sk_buff` projection (Listing 1 in the paper)
```c
projection<struct sk_buff> skb_xmit {
    projection net_device *dev;                              // nested projection
    unsigned int len;
    unsigned int data_len;
    ...
    void * [alloc_sized<callee>(self->true_size)] head;      // callee allocates; size = a field
    void * [within<self->head, self->true_size>] data;       // ptr must lie inside head's range
    unsigned int [within<_, self->true_size>] tail;          // offset within range
    unsigned int [within<_, self->true_size>] end;
};
```
What KSplit adds beyond LXDs:
- **`alloc_sized<callee>(expr)`** — array/buffer size taken from another field (auto-derived).
- **`within<base, size>`** — for collocated / pointer-arithmetic fields (`tail`/`end` are offsets
  into the `data` buffer). KSplit *detects* the pattern, but the **range bound is filled in
  manually** (one of its ~2% residue cases).
- The whole projection is **produced by static analysis** (PDG + read/write access → direction;
  CCured → singleton/array/wild), not hand-written.

---

## 3. What this maps to in Lind (illustrative — tied to the zlib grate)

The `zlib-python` grate intercepts `deflate(z_streamp strm, int flush)` operating on a
`z_stream`. In LXDs/KSplit projection style, the descriptor Lind wants is essentially:

```c
// illustrative Lind/LXDs-style projection of zlib's z_stream for deflate()
rpc int deflate(projection z_stream [bind] *strm, int flush);

projection <struct z_stream> z_stream {
    Bytef * [in,  alloc_sized(self->avail_in)]  next_in;    // input buffer
    uInt          avail_in;                                  // in/out scalar
    uLong         total_in;
    Bytef * [out, alloc_sized(self->avail_out)] next_out;   // output buffer
    uInt          avail_out;
    uLong         total_out;
    void  * [handle] state;                                  // opaque internal state — NEVER copy
    ...
}
```
Three takeaways this makes concrete for `lind_abi_spec`:
- `next_in` / `next_out` are **counted buffers** sized by sibling fields (`alloc_sized`) with
  **opposite directions** — a per-field projection, not a flat arg list.
- `state` is an **opaque handle** (`[handle]`/`[bind]`): pass the token, never dereference. This
  is the category the current `lind_abi_spec` lacks and the one whose mishandling corrupts memory.
- `strm` is `[bind]`: the same `z_stream` recurs across `deflateInit2_` → `deflate` →
  `deflateEnd`, so it resolves to one shadow via the handle table rather than being re-copied.

---

### Sources
- LXDs IDL: `papers/lxds_atc19.pdf` (module/rpc + projection + `[in]/[out]` + `alloc/bind/dealloc`
  + remote references + function-pointer projection with hidden arguments).
- KSplit IDL: `papers/ksplit_osdi22.pdf` (generated rpc + Listing 1 `sk_buff` projection with
  `alloc_sized`/`within`).
- Deeper analysis: `ksplit-analysis.md`, `ksplit-lineage.md`.
