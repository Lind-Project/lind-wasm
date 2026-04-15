use sysdefs::constants::lind_platform_const;
use threei::threei_const;
use std::sync::{Condvar, Mutex, MutexGuard};
use std::collections::HashMap;
use anyhow::{Context, Result, anyhow};
use wasmtime::{Engine, Module, Linker, Store, Instance, TypedFunc};

type PassFptrTyped = TypedFunc<
    (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
    i32,
>;

type WorkerId = u64;

pub struct GrateTemplate<T> {
    pub engine: Engine,
    pub module: Module,
    pub linker: Linker<T>,
}

pub struct GrateRequest {
    pub handler_addr: u64,
    pub cageid: u64,
    pub arg1: u64,
    pub arg1cageid: u64,
    pub arg2: u64,
    pub arg2cageid: u64,
    pub arg3: u64,
    pub arg3cageid: u64,
    pub arg4: u64,
    pub arg4cageid: u64,
    pub arg5: u64,
    pub arg5cageid: u64,
    pub arg6: u64,
    pub arg6cageid: u64,
}

struct SerialExecutor {
    lock: Mutex<()>,
}

impl SerialExecutor {
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
        }
    }

    fn enter(&self) -> MutexGuard<'_, ()> {
        match self.lock.lock() {
            Ok(guard) => {
                println!("SerialExecutor: acquired lock");
                guard
            }
            Err(poisoned) => {
                println!("Serial execution lock poisoned; continuing because it is only used as a gate");
                poisoned.into_inner()
            }
        }
    }
}

struct GrateWorker<T> {
    worker_id: WorkerId,
    store: Store<T>,
    instance: Instance,
    pass_fptr_func: Option<PassFptrTyped>,
}

struct WorkerLease<'a, T> {
    owner: &'a GrateHandler<T>,
    worker: Option<GrateWorker<T>>,
}

impl<'a, T> WorkerLease<'a, T> {
    fn new(owner: &'a GrateHandler<T>, worker: GrateWorker<T>) -> Self {
        Self {
            owner,
            worker: Some(worker),
        }
    }

    fn worker_mut(&mut self) -> &mut GrateWorker<T> {
        self.worker.as_mut().unwrap()
    }
}

impl<'a, T> Drop for WorkerLease<'a, T> {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            self.owner.return_worker(worker);
        }
    }
}

pub enum ConcurrencyMode {
    Parallel,
    Serialized,
}

pub struct GrateHandler<T> {
    grate_id: u64,
    main_worker: WorkerId,
    concurrency_mode: ConcurrencyMode,
    serial_executor: SerialExecutor,

    inner: Mutex<GrateHandlerInner<T>>,
    cv: Condvar,
}

struct GrateHandlerInner<T> {
    workers: VecDeque<GrateWorker<T>>,
}

impl<T: Clone> GrateHandler<T> {
    fn init_ten_workers(
        &mut self,
        template: &GrateTemplate<T>,
        host: &T,
        cageid: u64,
    ) -> anyhow::Result<()> {
        for handler_id in 1_u64..=10 {
            let worker = create_worker(
                template,
                host.clone(),
                cageid,
                handler_id,
            )
            .with_context(|| {
                format!(
                    "failed to create worker {} for cageid {}",
                    handler_id, cageid
                )
            })?;

            self.inner.lock().unwrap().workers.push_back(worker);
        }

        self.main_worker = 1;
        Ok(())
    }

    
}

impl<T> GrateHandler<T> {
    fn take_worker_blocking(&self) -> GrateWorker<T> {
        let mut inner = self.inner.lock().unwrap();

        loop {
            if let Some(worker) = inner.workers.pop_front() {
                return worker;
            }
            inner = self.cv.wait(inner).unwrap();
        }
    }

    fn return_worker(&self, worker: GrateWorker<T>) {
        let mut inner = self.inner.lock().unwrap();
        inner.workers.push_back(worker);
        self.cv.notify_one();
    }

    fn submit_serialized(&self, req: GrateRequest) -> anyhow::Result<i32> {
        let _serial_guard = self.serial_executor.enter();
        let worker = self.take_worker_blocking();
        let mut lease = WorkerLease::new(self, worker);
        Ok(lease.worker_mut().run(req))
    }

    pub fn submit(&self, req: GrateRequest) -> anyhow::Result<i32> {
        match self.concurrency_mode {
            ConcurrencyMode::Serialized => self.submit_serialized(req),
            ConcurrencyMode::Parallel => {
                anyhow::bail!("parallel mode is not implemented yet")
            }
        }
    }
}

impl<T> GrateWorker<T> {
    fn run(&mut self, req: GrateRequest) -> i32 {
        println!("Worker {} handling grate request for cage {}, handler_addr: {:#x}", self.worker_id, req.cageid, req.handler_addr);
        match &self.pass_fptr_func {
            Some(func) => {
                let ret = func.call(&mut self.store,
                    (
                        req.handler_addr,
                        req.cageid,
                        req.arg1,
                        req.arg1cageid,
                        req.arg2,
                        req.arg2cageid,
                        req.arg3,
                        req.arg3cageid,
                        req.arg4,
                        req.arg4cageid,
                        req.arg5,
                        req.arg5cageid,
                        req.arg6,
                        req.arg6cageid,
                    )).unwrap_or_else(|e| {
                        panic!(
                            "failed to call pass_fptr_to_wt in worker {}: {}",
                            self.worker_id, e
                        )
                    });
                println!("Worker {} got result {} from pass_fptr_to_wt", self.worker_id, ret);
                ret
            }
            None => {
                panic!("no pass_fptr_func found in worker {}", self.worker_id);
            }
        }
        
    }
}

pub fn create_worker<T>(
    template: &GrateTemplate<T>,
    host: T,
    _cageid: u64,
    worker_id: WorkerId,
) -> anyhow::Result<GrateWorker<T>>
where
    T: Clone,
{
    let mut store = Store::new(&template.engine, host);

    let mut linker: Linker<T> = template.linker.clone();

    let (instance, _) = linker
        .instantiate_with_lind_thread(&mut store, &template.module, false)
        .context("failed to instantiate grate module")?;

    let pass_fptr_func = match instance.get_export(&mut store, "pass_fptr_to_wt") {
        Some(_) => Some(
            instance.get_typed_func::<
                (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64),
                i32,
            >(&mut store, "pass_fptr_to_wt")?
        ),
        None => None,
    };

    Ok(GrateWorker {
        worker_id,
        store,
        instance,
        pass_fptr_func,
    })
}

pub fn create_handler_for_cage<T: Clone>(
    template: &GrateTemplate<T>,
    host: T,
    cageid: u64,
    concurrency_mode: ConcurrencyMode,
) -> anyhow::Result<GrateHandler<T>> {
    let mut handler = GrateHandler {
        grate_id: cageid,
        main_worker: 1,
        concurrency_mode,
        serial_executor: SerialExecutor::new(),
        inner: Mutex::new(GrateHandlerInner {
            workers: VecDeque::new(),
        }),
        cv: Condvar::new(),
    };

    handler.init_ten_workers(template, &host, cageid)?;

    Ok(handler)
}


use std::collections::{VecDeque};
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::{OnceLock};

#[derive(Clone, Copy)]
pub struct VmCtxWrapper {
    pub vmctx: NonNull<c_void>,
}

unsafe impl Send for VmCtxWrapper {}
unsafe impl Sync for VmCtxWrapper {}

impl VmCtxWrapper {
    // exposes the raw mutable pointer
    #[inline]
    pub fn as_ptr(self) -> *mut c_void {
        self.vmctx.as_ptr()
    }
}

static VMCTX_THREADS: OnceLock<Vec<Mutex<HashMap<u64, VmCtxWrapper>>>> = OnceLock::new();

pub fn init_vmctx_pool() {
    VMCTX_THREADS.get_or_init(|| {
        (0..lind_platform_const::MAX_CAGEID)
            .map(|_| Mutex::new(HashMap::new()))
            .collect()
    });
}

pub fn set_vmctx_thread(cage_id: u64, tid: u64, vmctx: VmCtxWrapper) {
    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().insert(tid, vmctx);
}

/// Look up the VMContext
///
/// Returns `None` if the thread has exited or was never registered.
pub fn get_vmctx_thread(cage_id: u64, tid: u64) -> Option<VmCtxWrapper> {
    let tables = VMCTX_THREADS.get().expect("VMCTX_THREADS not initialized");
    let t = tables.get(cage_id as usize).expect("invalid cage_id");
    t.lock().unwrap().get(&tid).copied()
}

/// Remove a single thread entry
pub fn rm_vmctx_thread(cage_id: u64, tid: u64) -> bool {
    let Some(tables) = VMCTX_THREADS.get() else {
        return false;
    };
    let Some(t) = tables.get(cage_id as usize) else {
        return false;
    };
    t.lock().unwrap().remove(&tid).is_some()
}
