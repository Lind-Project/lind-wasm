//! A host-process runtime for tests. Memory operations use `host_memory`; every other
//! operation is recorded and succeeds unless a failure was injected for it.

use crate::*;
use std::collections::HashMap;
use std::sync::Mutex;
use sysdefs::constants::err_const::Errno;

pub struct RecordingRuntime {
    id: RuntimeId,
    calls: Mutex<Vec<String>>,
    failures: Mutex<HashMap<&'static str, Errno>>,
}

impl RecordingRuntime {
    pub fn new(id: RuntimeId) -> Self {
        Self {
            id,
            calls: Mutex::new(Vec::new()),
            failures: Mutex::new(HashMap::new()),
        }
    }

    pub fn fail(&self, operation: &'static str, errno: Errno) {
        self.failures.lock().unwrap().insert(operation, errno);
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, operation: &'static str, detail: String) -> LindResult<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("{operation}({detail})"));
        match self.failures.lock().unwrap().get(operation) {
            Some(errno) => Err(LindError::Other(*errno)),
            None => Ok(()),
        }
    }
}

impl LindRuntime for RecordingRuntime {
    fn runtime_id(&self) -> RuntimeId {
        self.id
    }

    fn belongs_to_runtime(&self, exe_path: &str) -> LindResult<bool> {
        self.record("belongs_to_runtime", exe_path.to_owned())?;
        Ok(true)
    }

    fn get_host_address(&self, cage: CageId, guest: GuestAddress) -> LindResult<HostAddress> {
        self.record("get_host_address", format!("{cage}, {guest:#x}"))?;
        Ok(guest)
    }

    fn get_guest_address(&self, cage: CageId, host: HostAddress) -> LindResult<GuestAddress> {
        self.record("get_guest_address", format!("{cage}, {host:#x}"))?;
        Ok(host)
    }

    fn get_cage_abi(&self, cage: CageId) -> LindResult<AbiDescriptor> {
        self.record("get_cage_abi", cage.to_string())?;
        Ok(AbiDescriptor::Wasm32)
    }

    fn init_cage(
        &self,
        cage: CageId,
        exe_path: &str,
        _init_args: LindInitArgs,
        is_staging: bool,
    ) -> LindResult<()> {
        self.record("init_cage", format!("{cage}, {exe_path}, {is_staging}"))
    }

    fn apply_staging_cage(&self, staging: CageId, target: CageId) -> LindResult<()> {
        self.record("apply_staging_cage", format!("{staging:#x}, {target}"))
    }

    fn duplicate_cage(&self, cage: CageId, new_cage: CageId) -> LindResult<()> {
        self.record("duplicate_cage", format!("{cage}, {new_cage}"))
    }

    fn terminate_cage(&self, cage: CageId) -> LindResult<()> {
        self.record("terminate_cage", format!("{cage:#x}"))
    }

    fn create_thread(&self, cage: CageId, thread: ThreadId, _args: LindCloneArgs) -> LindResult<()> {
        self.record("create_thread", format!("{cage:#x}, {thread}"))
    }

    fn duplicate_thread(
        &self,
        source_cage: CageId,
        source_thread: ThreadId,
        destination_cage: CageId,
        new_thread: ThreadId,
        args: LindThreadDuplicateArgs,
    ) -> LindResult<()> {
        self.record(
            "duplicate_thread",
            format!(
                "{source_cage}, {source_thread}, {destination_cage}, {new_thread}, {:?}",
                args.mode
            ),
        )
    }

    fn terminate_thread(&self, cage: CageId, thread: ThreadId) -> LindResult<()> {
        self.record("terminate_thread", format!("{cage}, {thread}"))
    }

    fn send_signal(&self, cage: CageId, thread: ThreadId, info: LindSigInfo) -> LindResult<()> {
        self.record("send_signal", format!("{cage}, {thread}, {}", info.signo))
    }

    fn run_handler(&self, cage: CageId, args: LindHandlerArgs) -> LindResult<HandlerReturn> {
        self.record("run_handler", format!("{cage}, {:#x}", args.handler))?;
        Ok(0)
    }

    fn do_mmap(
        &self,
        cage: CageId,
        args: LindMmapArgs,
        is_reservation: bool,
    ) -> LindResult<HostAddress> {
        self.record("do_mmap", format!("{cage}, {is_reservation}"))?;
        host_memory::mmap(args)
    }

    fn do_mprotect(&self, cage: CageId, args: LindMprotectArgs) -> LindResult<()> {
        self.record("do_mprotect", cage.to_string())?;
        host_memory::mprotect(args)
    }

    fn do_munmap(&self, cage: CageId, args: LindMunmapArgs) -> LindResult<()> {
        self.record("do_munmap", cage.to_string())?;
        host_memory::munmap(args)
    }
}
