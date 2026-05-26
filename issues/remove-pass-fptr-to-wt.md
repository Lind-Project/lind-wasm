# Issue: Eliminate pass_fptr_to_wt by invoking grate functions directly via WASM table

## Background

Every grate currently must export a `pass_fptr_to_wt` function:

```c
int pass_fptr_to_wt(uint64_t fn_ptr_uint, uint64_t cageid, ...) {
    int (*fn)(...) = (int (*)(...)) (uintptr_t) fn_ptr_uint;
    return fn(cageid, arg1, arg1cage, ...);
}
```

When the runtime needs to dispatch into a grate handler, it calls this export,
passing the stored function pointer (a WASM indirect function table index) as
the first argument. `pass_fptr_to_wt` casts it back to a function pointer and
calls it. This is boilerplate that every grate author must copy verbatim.

## Proposed fix

The function pointer stored in HANDLERTABLE is a WASM indirect function table
index. Wasmtime exposes the table as a first-class object, so the runtime can
look up and call the function directly:

```rust
// grate instance exports "__indirect_function_table"
let table = grate_instance.get_table(&mut store, "__indirect_function_table")?;
if let Some(Ref::Func(Some(func))) = table.get(&mut store, fn_index as u32) {
    func.call(&mut store, &args, &mut results)?;
}
```

This eliminates `pass_fptr_to_wt` entirely — grate authors no longer need to
include it as a required export.

## Complication: store aliasing

The main obstacle is store aliasing. Calling into the grate's instance requires
`&mut Store`. If the cage's call is already executing inside the same store,
there would be conflicting mutable borrows. The `pass_fptr_to_wt` approach
sidesteps this because the re-entry happens through a normal WASM-to-WASM call
boundary that wasmtime manages internally.

The direct table approach needs one of:
- Grate runs in a separate Store (currently blocked — see issue #961)
- The mutex POC from issue #961 applied here as well

## Impact

- Removes required boilerplate from every grate
- Makes grate authoring simpler and less error-prone
- More principled: uses the WASM table directly rather than an integer cast trick
