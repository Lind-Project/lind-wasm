// Stage-1 PoC: a trivial "library" exposed to the lind host as a long-lived
// reactor. It exports two pure-scalar functions (int(int,int)) that the host
// can call by name via `lind-boot --call <name>`.
//
// `export_name` makes each function appear as a Wasm export under exactly that
// name, so the host's `instance.get_func("add")` lookup succeeds. There is no
// `main`/`_start`: `--call` invokes an export directly, so the module needs no
// entry point. Build it with the default (dynamic) `lind_compile` mode — the
// static `-s` mode would require an entry point.

__attribute__((export_name("add")))
int add(int a, int b) {
    return a + b;
}

__attribute__((export_name("subtract")))
int subtract(int a, int b) {
    return a - b;
}
