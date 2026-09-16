// Example 07 stub — the native .so whose exported symbols run in the sandbox AND
// register host-side callbacks the guest calls back into.
//
// New vs earlier examples: before `init_sandboxed_lib`, we `register_upcall` one
// handler per slot. When the guest calls `__lind_upcall(slot, ...)`, the lind runtime
// invokes the matching handler here. `UpcallCtx` reverse-marshals pointer args out of
// guest memory. This is the mechanism OpenBLAS's xerbla will use to reach a host error
// handler; here the two handlers just prove the round trip and the memory read.

use std::sync::{Mutex, OnceLock};

use lind_boot::{CliOptions, SandboxedLib, UpcallCtx, init_sandboxed_lib, register_upcall};

static LIB: OnceLock<Mutex<SandboxedLib>> = OnceLock::new();

fn module_path() -> String {
    std::env::var("LIND_MODULE").unwrap_or_else(|_| "guest.cwasm".to_string())
}

fn lib() -> &'static Mutex<SandboxedLib> {
    LIB.get_or_init(|| {
        // Register callbacks BEFORE init — they live in a process-global registry the
        // runtime consults whenever the guest calls __lind_upcall.

        // slot 0: scalar round trip — double the value the guest sent.
        register_upcall(0, Box::new(|_ctx: &UpcallCtx, a: [i32; 3]| a[0] * 2));

        // slot 1: reverse marshalling — read the guest string (ptr=a[0], len=a[1])
        // out of the sandbox and return its length, proving the host can pull a guest
        // buffer across the boundary (the shape xerbla needs for the routine name).
        register_upcall(
            1,
            Box::new(|ctx: &UpcallCtx, a: [i32; 3]| {
                let bytes = ctx.read_bytes(a[0], a[1]).unwrap_or_default();
                bytes.len() as i32
            }),
        );

        let cli = CliOptions::for_sandboxed_lib(module_path());
        let sandboxed_lib = init_sandboxed_lib(cli)
            .unwrap_or_else(|e| panic!("lind sandboxed-lib init failed: {e:?}"));
        Mutex::new(sandboxed_lib)
    })
}

fn call(name: &str, args: &[i32]) -> i32 {
    lib()
        .lock()
        .unwrap()
        .call_scalar(name, args)
        .unwrap_or_else(|e| panic!("lind call `{name}` failed: {e:?}"))
}

#[unsafe(no_mangle)]
pub extern "C" fn apply_cb(x: i32) -> i32 {
    call("apply_cb", &[x])
}

#[unsafe(no_mangle)]
pub extern "C" fn report_name() -> i32 {
    call("report_name", &[])
}
