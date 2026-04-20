use crate::func::HostFunc;
use crate::instance::InstancePre;
use crate::store::{StoreId, StoreOpaque};
use crate::{prelude::*, Global, GlobalType, IntoFunc, MemoryType, SharedMemory, Table};
use crate::{
    AsContext, AsContextMut, Caller, Engine, Extern, ExternType, Func, FuncType, ImportType,
    Instance, Module, StoreContextMut, Val, ValRaw, ValType, WasmTyList,
};
use alloc::sync::Arc;
use cage::DashMap;
use core::fmt;
#[cfg(feature = "async")]
use core::future::Future;
use core::marker;
#[cfg(feature = "async")]
use core::pin::Pin;
use hashbrown::hash_map::{Entry, HashMap};
use log::warn;
use std::sync::LazyLock;
use sysdefs::constants::FPCAST_FUNC_SIGNATURE;
use wasmtime_environ::{EntityIndex, GlobalIndex};
use wasmtime_lind_utils::symbol_table::SymbolMap;
use wasmtime_lind_utils::LindGOT;

use super::{InstanceId, InstantiateType};

/// Structure used to link wasm modules/instances together.
///
/// This structure is used to assist in instantiating a [`Module`]. A [`Linker`]
/// is a way of performing name resolution to make instantiating a module easier
/// than specifying positional imports to [`Instance::new`]. [`Linker`] is a
/// name-based resolver where names are dynamically defined and then used to
/// instantiate a [`Module`].
///
/// An important method is [`Linker::instantiate`] which takes a module to
/// instantiate into the provided store. This method will automatically select
/// all the right imports for the [`Module`] to be instantiated, and will
/// otherwise return an error if an import isn't satisfied.
///
/// ## Name Resolution
///
/// As mentioned previously, `Linker` is a form of name resolver. It will be
/// using the string-based names of imports on a module to attempt to select a
/// matching item to hook up to it. This name resolution has two-levels of
/// namespaces, a module level and a name level. Each item is defined within a
/// module and then has its own name. This basically follows the wasm standard
/// for modularization.
///
/// Names in a `Linker` cannot be defined twice, but allowing duplicates by
/// shadowing the previous definition can be controlled with the
/// [`Linker::allow_shadowing`] method.
///
/// ## Commands and Reactors
///
/// The [`Linker`] type provides conveniences for working with WASI Commands and
/// Reactors through the [`Linker::module`] method. This will automatically
/// handle instantiation and calling `_start` and such as appropriate
/// depending on the inferred type of module.
///
/// ## Type parameter `T`
///
/// It's worth pointing out that the type parameter `T` on [`Linker<T>`] does
/// not represent that `T` is stored within a [`Linker`]. Rather the `T` is used
/// to ensure that linker-defined functions and stores instantiated into all use
/// the same matching `T` as host state.
///
/// ## Multiple `Store`s
///
/// The [`Linker`] type is designed to be compatible, in some scenarios, with
/// instantiation in multiple [`Store`]s. Specifically host-defined functions
/// created in [`Linker`] with [`Linker::func_new`], [`Linker::func_wrap`], and
/// their async versions are compatible to instantiate into any [`Store`]. This
/// enables programs which want to instantiate lots of modules to create one
/// [`Linker`] value at program start up and use that continuously for each
/// [`Store`] created over the lifetime of the program.
///
/// Note that once [`Store`]-owned items, such as [`Global`], are defined within
/// a [`Linker`] then it is no longer compatible with any [`Store`]. At that
/// point only the [`Store`] that owns the [`Global`] can be used to instantiate
/// modules.
///
/// ## Multiple `Engine`s
///
/// The [`Linker`] type is not compatible with usage between multiple [`Engine`]
/// values. An [`Engine`] is provided when a [`Linker`] is created and only
/// stores and items which originate from that [`Engine`] can be used with this
/// [`Linker`]. If more than one [`Engine`] is used with a [`Linker`] then that
/// may cause a panic at runtime, similar to how if a [`Func`] is used with the
/// wrong [`Store`] that can also panic at runtime.
///
/// [`Store`]: crate::Store
/// [`Global`]: crate::Global
pub struct Linker<T> {
    engine: Engine,
    string2idx: HashMap<Arc<str>, usize>,
    strings: Vec<Arc<str>>,
    map: HashMap<ImportKey, Definition>,
    allow_shadowing: bool,
    allow_unknown_exports: bool,
    _marker: marker::PhantomData<fn() -> T>,
}

impl<T> Clone for Linker<T> {
    fn clone(&self) -> Linker<T> {
        Linker {
            engine: self.engine.clone(),
            string2idx: self.string2idx.clone(),
            strings: self.strings.clone(),
            map: self.map.clone(),
            allow_shadowing: self.allow_shadowing,
            allow_unknown_exports: self.allow_unknown_exports,
            _marker: self._marker,
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq)]
struct ImportKey {
    name: usize,
    module: usize,
}

#[derive(Clone)]
pub enum ClonedMemory {
    Thread(SharedMemory),
    New(MemoryType),
}

pub enum ChildLibraryType<'a> {
    Process,
    Thread(&'a mut u32), // stack address
}

#[derive(Clone)]
pub(crate) enum Definition {
    Extern(Extern, DefinitionType),
    HostFunc(Arc<HostFunc>),
}

/// This is a sort of slimmed down `ExternType` which notably doesn't have a
/// `FuncType`, which is an allocation, and additionally retains the current
/// size of the table/memory.
#[derive(Clone)]
pub(crate) enum DefinitionType {
    Func(wasmtime_environ::VMSharedTypeIndex),
    Global(wasmtime_environ::Global),
    // Note that tables and memories store not only the original type
    // information but additionally the current size of the table/memory, as
    // this is used during linking since the min size specified in the type may
    // no longer be the current size of the table/memory.
    Table(wasmtime_environ::Table, u32),
    Memory(wasmtime_environ::Memory, u64),
}

impl<T> Linker<T> {
    /// Creates a new [`Linker`].
    ///
    /// The linker will define functions within the context of the `engine`
    /// provided and can only instantiate modules for a [`Store`][crate::Store]
    /// that is also defined within the same [`Engine`]. Usage of stores with
    /// different [`Engine`]s may cause a panic when used with this [`Linker`].
    pub fn new(engine: &Engine) -> Linker<T> {
        Linker {
            engine: engine.clone(),
            map: HashMap::new(),
            string2idx: HashMap::new(),
            strings: Vec::new(),
            allow_shadowing: false,
            allow_unknown_exports: false,
            _marker: marker::PhantomData,
        }
    }

    /// Returns the [`Engine`] this is connected to.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Configures whether this [`Linker`] will shadow previous duplicate
    /// definitions of the same signature.
    ///
    /// By default a [`Linker`] will disallow duplicate definitions of the same
    /// signature. This method, however, can be used to instead allow duplicates
    /// and have the latest definition take precedence when linking modules.
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// let mut linker = Linker::<()>::new(&engine);
    /// linker.func_wrap("", "", || {})?;
    ///
    /// // by default, duplicates are disallowed
    /// assert!(linker.func_wrap("", "", || {}).is_err());
    ///
    /// // but shadowing can be configured to be allowed as well
    /// linker.allow_shadowing(true);
    /// linker.func_wrap("", "", || {})?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn allow_shadowing(&mut self, allow: bool) -> &mut Self {
        self.allow_shadowing = allow;
        self
    }

    /// Configures whether this [`Linker`] will allow unknown exports from
    /// command modules.
    ///
    /// By default a [`Linker`] will error when unknown exports are encountered
    /// in a command module while using [`Linker::module`].
    ///
    /// This method can be used to allow unknown exports from command modules.
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let module = Module::new(&engine, "(module)")?;
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// linker.allow_unknown_exports(true);
    /// linker.module(&mut store, "mod", &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn allow_unknown_exports(&mut self, allow: bool) -> &mut Self {
        self.allow_unknown_exports = allow;
        self
    }

    /// Implement any imports of the given [`Module`] with a function which traps.
    ///
    /// By default a [`Linker`] will error when unknown imports are encountered
    /// in a command module while using [`Linker::module`].
    ///
    /// This method can be used to allow unknown imports from command modules.
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let module = Module::new(&engine, "(module (import \"unknown\" \"import\" (func)))")?;
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// linker.define_unknown_imports_as_traps(&module)?;
    /// linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn define_unknown_imports_as_traps(&mut self, module: &Module) -> anyhow::Result<()> {
        for import in module.imports() {
            if let Err(import_err) = self._get_by_import(&import) {
                if let ExternType::Func(func_ty) = import_err.ty() {
                    #[cfg(feature = "debug-dylink")]
                    println!(
                        "[debug] Warning: link undefined symbol \"{}\" to trap",
                        import.name()
                    );
                    self.func_new(import.module(), import.name(), func_ty, move |_, _, _| {
                        bail!(import_err.clone());
                    })?;
                }
            }
        }
        Ok(())
    }

    /// Define unresolved *weak* function imports as trap stubs.
    ///
    /// ELF semantics: an undefined weak reference resolves to NULL/0 instead of
    /// causing a link error. Many native programs rely on this to probe optional
    /// features via `if (sym) { ... }`.
    ///
    pub fn define_weak_imports_as_traps(&mut self, module: &Module) -> anyhow::Result<()> {
        // for weak symbols, it should be resolved to NULL if it is not defined by any module
        // in wasm, we will instead link these symbols into a panic function
        let weak_imports = module.dylink_importinfo();
        if weak_imports.is_none() {
            return Ok(());
        }
        let weak_imports = weak_imports.unwrap();
        for import in module.imports() {
            // Only synthesize a trap stub for weak symbols that are NOT already defined
            // in the linker/environment.
            if let Err(import_err) = self._get_by_import(&import) {
                if let ExternType::Func(func_ty) = import_err.ty() {
                    // check if the undefined symbol is listed as a weak symbol
                    if !weak_imports.is_weak_symbol(import.module(), import.name()) {
                        continue;
                    }
                    #[cfg(feature = "debug-dylink")]
                    println!(
                        "[debug] define weak symbol {}.{} into trap",
                        import.module(),
                        import.name()
                    );
                    self.func_new(import.module(), import.name(), func_ty, move |_, _, _| {
                        bail!(import_err.clone());
                    })?;
                }
            }
        }
        Ok(())
    }

    /// redirect all GOT functions/symbols into a centralized dispatcher function
    ///
    /// We create mutable global "slots" as placeholders for GOT entries so they can be
    /// patched after instantiation (once exports and relocation targets are known).
    /// This enables dynamic updates of symbol addresses/indices without re-instantiating
    /// the module.
    pub fn define_GOT_dispatcher(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module: &Module,
        got: &mut LindGOT,
    ) -> anyhow::Result<()> {
        for import in module.imports() {
            if let Err(import_err) = self._get_by_import(&import) {
                #[cfg(feature = "debug-dylink")]
                println!(
                    "[debug]: define_GOT_dispatcher: import module: {}, name: {}",
                    import.module(),
                    import.name()
                );
                if import.module() == "GOT.func" || import.module() == "GOT.mem" {
                    // ASSUMPTION: all GOT imports are represented as `mut i32` globals.
                    // If the module's import type does not match this representation,
                    // `Global::new` / linking will fail (currently via unwrap/panic).

                    // Create a mutable global slot initialized to 0 as a GOT placeholder.
                    //
                    // Rationale:
                    // - Some GOT entries are resolved only after instantiation, since
                    //   the module's exports become available only once instantiation
                    //   completes and initialization runs.
                    // - By linking imports to placeholder globals, we allow the instance
                    //   to instantiate successfully, then patch the globals to the final
                    //   resolved values (e.g., function indices or memory addresses)
                    //   during/after relocation.
                    let got_placeholder = Global::new(
                        &mut store,
                        GlobalType::new(ValType::I32, crate::Mutability::Var),
                        Val::I32(0),
                    )
                    .unwrap();
                    self.define(&mut store, import.module(), import.name(), got_placeholder);

                    // Record the backing address of the placeholder slot in LindGOT so
                    // the dynamic loader can update it later when the symbol is resolved.
                    let handler = got_placeholder.get_handler_as_u32(&mut store);
                    got.new_entry(import.name().to_string(), handler);
                }
            }
        }
        Ok(())
    }

    pub fn get_linker_snapshot_for_child(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        share_memory: bool,
    ) -> (
        Vec<(String, String, GlobalType, Val)>,
        Vec<(String, String, Arc<HostFunc>)>,
        Option<(String, String, ClonedMemory)>,
    ) {
        // we collect two items from parent linker to child linker
        // 1. GOT entries defined as wasm global (i.e. GOT.mem)
        // 2. Host-defined functions, which are independent to Store
        let mut globals = vec![];
        let mut funcs = vec![];
        let mut memory = None;

        for (key, item) in self.map.iter() {
            let module = &*self.strings[key.module];
            let name = &*self.strings[key.name];

            match item {
                Definition::Extern(ext, _) => {
                    match ext {
                        Extern::Global(global) => {
                            if module == "GOT.mem" || module == "GOT.func" // GOT entries
                               || (module == "env" && name == "__memory_base") // main module memory base
                               || (module == "env" && name == "__table_base") // main module table base
                               || module == "lib.memory_base" // library __memory_base
                               || (module == "env" && name == "__stack_pointer") // stack pointer
                               || (module == "lind" && name == "epoch")
                            // signal epoch
                            {
                                // NOTE: asyncify state is intentionally not copied
                                let global_ty = global.ty(&store);
                                let value = global.get(&mut store);
                                globals.push((
                                    module.to_string(),
                                    name.to_string(),
                                    global_ty.clone(),
                                    value.clone(),
                                ));
                            }
                        }
                        Extern::Func(func) => {
                            // no-np
                            // Those are functions defined by library
                            // Those has to be "cloned" by re-instantiating
                            // the library and link the exported function again
                        }
                        Extern::Table(table) => {
                            // no-op
                            // indirect_table is handled explicitly by the fork logic
                        }
                        Extern::Memory(memory) => {
                            // lind wasm instance shouldn't have any Memory
                            // (They are all SharedMemory instead)
                            unreachable!()
                        }
                        Extern::SharedMemory(shared_memory) => {
                            if share_memory {
                                // if memory should be shared between new linker
                                // (e.g. in case of new thread creation)
                                // collect the SharedMemory Instance as well
                                memory = Some((
                                    module.to_string(),
                                    name.to_string(),
                                    ClonedMemory::Thread(shared_memory.clone()),
                                ));
                            } else {
                                // otherwise, collect memory type only
                                // and have new instance creating a new one for itself
                                memory = Some((
                                    module.to_string(),
                                    name.to_string(),
                                    ClonedMemory::New(shared_memory.ty().clone()),
                                ));
                            }
                        }
                    }
                }
                Definition::HostFunc(host_func) => {
                    funcs.push((module.to_string(), name.to_string(), host_func.clone()));
                }
            }
        }

        (globals, funcs, memory)
    }

    pub fn new_child_linker(
        mut store: impl AsContextMut<Data = T>,
        engine: &Engine,
        got_table: &mut Option<LindGOT>,
        globals: &Vec<(String, String, GlobalType, Val)>,
        hostfuncs: &Vec<(String, String, Arc<HostFunc>)>,
        shared_memory: &Option<(String, String, ClonedMemory)>,
    ) -> Result<(Self, HashMap<String, i32>, Option<*mut u64>, Option<u64>)> {
        let mut new_linker = Self::new(&engine);

        // a mapping of library name to its memory base value
        let mut memory_base_table = HashMap::new();

        let mut epoch_handler = None;
        let mut memory_base = None;

        // define globals to the new linker
        for (module, name, ty, val) in globals {
            let cloned_global = Global::new(&mut store, ty.clone(), val.clone())?;

            if let Some(got) = got_table.as_mut() {
                if module == "GOT.func" || module == "GOT.mem" {
                    let handler = cloned_global.get_handler_as_u32(&mut store);
                    got.new_entry(name.clone(), handler);
                }
            }

            // collect library's memory base for quick look up when instantiate the child library
            if module == "lib.memory_base" {
                let memory_base = val.i32().unwrap();
                memory_base_table.insert(name.clone(), memory_base);
            }

            if module == "lind" && name == "epoch" {
                epoch_handler = Some(cloned_global.get_handler_as_u64(&mut store));
            }

            new_linker.define(&mut store, &module, &name, cloned_global)?;
        }

        // attach host functions to the new linker
        for (ref module, ref name, hostfunc) in hostfuncs {
            let key = new_linker.import_key(module, Some(name));
            new_linker.insert(key, Definition::HostFunc(hostfunc.clone()))?;
        }

        // attach the SharedMemory if exist
        if let Some((module, name, memory)) = shared_memory {
            match memory {
                ClonedMemory::Thread(shared_memory) => {
                    memory_base = Some(shared_memory.get_memory_base());
                    new_linker.define(&mut store, &module, &name, shared_memory.clone())?;
                }
                ClonedMemory::New(memory_type) => {
                    let mem = SharedMemory::new(&engine, memory_type.clone())?;
                    memory_base = Some(mem.get_memory_base());
                    new_linker.define(&mut store, &module, &name, mem)?;
                }
            }
        }

        Ok((new_linker, memory_base_table, epoch_handler, memory_base))
    }

    /// Implement any function imports of the [`Module`] with a function that
    /// ignores its arguments and returns default values.
    ///
    /// Default values are either zero or null, depending on the value type.
    ///
    /// This method can be used to allow unknown imports from command modules.
    ///
    /// # Example
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let module = Module::new(&engine, "(module (import \"unknown\" \"import\" (func)))")?;
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// linker.define_unknown_imports_as_default_values(&module)?;
    /// linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn define_unknown_imports_as_default_values(
        &mut self,
        module: &Module,
    ) -> anyhow::Result<()> {
        for import in module.imports() {
            if let Err(import_err) = self._get_by_import(&import) {
                if let ExternType::Func(func_ty) = import_err.ty() {
                    let result_tys: Vec<_> = func_ty.results().collect();

                    for ty in &result_tys {
                        if ty.as_ref().map_or(false, |r| !r.is_nullable()) {
                            bail!("no default value exists for type `{ty}`")
                        }
                    }

                    self.func_new(
                        import.module(),
                        import.name(),
                        func_ty,
                        move |_caller, _args, results| {
                            for (result, ty) in results.iter_mut().zip(&result_tys) {
                                *result = match ty {
                                    ValType::I32 => Val::I32(0),
                                    ValType::I64 => Val::I64(0),
                                    ValType::F32 => Val::F32(0.0_f32.to_bits()),
                                    ValType::F64 => Val::F64(0.0_f64.to_bits()),
                                    ValType::V128 => Val::V128(0_u128.into()),
                                    ValType::Ref(r) => {
                                        debug_assert!(r.is_nullable());
                                        Val::null_ref(r.heap_type())
                                    }
                                };
                            }
                            Ok(())
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Defines a new item in this [`Linker`].
    ///
    /// This method will add a new definition, by name, to this instance of
    /// [`Linker`]. The `module` and `name` provided are what to name the
    /// `item`.
    ///
    /// # Errors
    ///
    /// Returns an error if the `module` and `name` already identify an item
    /// of the same type as the `item` provided and if shadowing is disallowed.
    /// For more information see the documentation on [`Linker`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// let ty = GlobalType::new(ValType::I32, Mutability::Const);
    /// let global = Global::new(&mut store, ty, Val::I32(0x1234))?;
    /// linker.define(&store, "host", "offset", global)?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "host" "offset" (global i32))
    ///         (memory 1)
    ///         (data (global.get 0) "foo")
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn define(
        &mut self,
        store: impl AsContext<Data = T>,
        module: &str,
        name: &str,
        item: impl Into<Extern>,
    ) -> Result<&mut Self> {
        let store = store.as_context();
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::new(store.0, item.into()))?;
        Ok(self)
    }

    pub fn define_with_inner(
        &mut self,
        store: &StoreOpaque,
        module: &str,
        name: &str,
        item: impl Into<Extern>,
    ) -> Result<&mut Self> {
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::new(store, item.into()))?;
        Ok(self)
    }

    /// Same as [`Linker::define`], except only the name of the import is
    /// provided, not a module name as well.
    ///
    /// This is only relevant when working with the module linking proposal
    /// where one-level names are allowed (in addition to two-level names).
    /// Otherwise this method need not be used.
    pub fn define_name(
        &mut self,
        store: impl AsContext<Data = T>,
        name: &str,
        item: impl Into<Extern>,
    ) -> Result<&mut Self> {
        let store = store.as_context();
        let key = self.import_key(name, None);
        self.insert(key, Definition::new(store.0, item.into()))?;
        Ok(self)
    }

    /// Creates a [`Func::new`]-style function named in this linker.
    ///
    /// For more information see [`Linker::func_wrap`].
    ///
    /// # Panics
    ///
    /// Panics if the given function type is not associated with the same engine
    /// as this linker.
    pub fn func_new(
        &mut self,
        module: &str,
        name: &str,
        ty: FuncType,
        func: impl Fn(Caller<'_, T>, &[Val], &mut [Val]) -> Result<()> + Send + Sync + 'static,
    ) -> Result<&mut Self> {
        assert!(ty.comes_from_same_engine(self.engine()));
        let func = HostFunc::new(&self.engine, ty, func);
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::HostFunc(Arc::new(func)))?;
        Ok(self)
    }

    /// Creates a [`Func::new_unchecked`]-style function named in this linker.
    ///
    /// For more information see [`Linker::func_wrap`].
    ///
    /// # Panics
    ///
    /// Panics if the given function type is not associated with the same engine
    /// as this linker.
    pub unsafe fn func_new_unchecked(
        &mut self,
        module: &str,
        name: &str,
        ty: FuncType,
        func: impl Fn(Caller<'_, T>, &mut [ValRaw]) -> Result<()> + Send + Sync + 'static,
    ) -> Result<&mut Self> {
        assert!(ty.comes_from_same_engine(self.engine()));
        let func = HostFunc::new_unchecked(&self.engine, ty, func);
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::HostFunc(Arc::new(func)))?;
        Ok(self)
    }

    /// Creates a [`Func::new_async`]-style function named in this linker.
    ///
    /// For more information see [`Linker::func_wrap`].
    ///
    /// # Panics
    ///
    /// This method panics in the following situations:
    ///
    /// * This linker is not associated with an [async
    ///   config](crate::Config::async_support).
    ///
    /// * If the given function type is not associated with the same engine as
    ///   this linker.
    #[cfg(all(feature = "async", feature = "cranelift"))]
    pub fn func_new_async<F>(
        &mut self,
        module: &str,
        name: &str,
        ty: FuncType,
        func: F,
    ) -> Result<&mut Self>
    where
        F: for<'a> Fn(
                Caller<'a, T>,
                &'a [Val],
                &'a mut [Val],
            ) -> Box<dyn Future<Output = Result<()>> + Send + 'a>
            + Send
            + Sync
            + 'static,
    {
        assert!(
            self.engine.config().async_support,
            "cannot use `func_new_async` without enabling async support in the config"
        );
        assert!(ty.comes_from_same_engine(self.engine()));
        self.func_new(module, name, ty, move |mut caller, params, results| {
            let async_cx = caller
                .store
                .as_context_mut()
                .0
                .async_cx()
                .expect("Attempt to spawn new function on dying fiber");
            let mut future = Pin::from(func(caller, params, results));
            match unsafe { async_cx.block_on(future.as_mut()) } {
                Ok(Ok(())) => Ok(()),
                Ok(Err(trap)) | Err(trap) => Err(trap),
            }
        })
    }

    /// Define a host function within this linker.
    ///
    /// For information about how the host function operates, see
    /// [`Func::wrap`]. That includes information about translating Rust types
    /// to WebAssembly native types.
    ///
    /// This method creates a host-provided function in this linker under the
    /// provided name. This method is distinct in its capability to create a
    /// [`Store`](crate::Store)-independent function. This means that the
    /// function defined here can be used to instantiate instances in multiple
    /// different stores, or in other words the function can be loaded into
    /// different stores.
    ///
    /// Note that the capability mentioned here applies to all other
    /// host-function-defining-methods on [`Linker`] as well. All of them can be
    /// used to create instances of [`Func`] within multiple stores. In a
    /// multithreaded program, for example, this means that the host functions
    /// could be called concurrently if different stores are executing on
    /// different threads.
    ///
    /// # Errors
    ///
    /// Returns an error if the `module` and `name` already identify an item
    /// of the same type as the `item` provided and if shadowing is disallowed.
    /// For more information see the documentation on [`Linker`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// let mut linker = Linker::new(&engine);
    /// linker.func_wrap("host", "double", |x: i32| x * 2)?;
    /// linker.func_wrap("host", "log_i32", |x: i32| println!("{}", x))?;
    /// linker.func_wrap("host", "log_str", |caller: Caller<'_, ()>, ptr: i32, len: i32| {
    ///     // ...
    /// })?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "host" "double" (func (param i32) (result i32)))
    ///         (import "host" "log_i32" (func (param i32)))
    ///         (import "host" "log_str" (func (param i32 i32)))
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    ///
    /// // instantiate in multiple different stores
    /// for _ in 0..10 {
    ///     let mut store = Store::new(&engine, ());
    ///     linker.instantiate(&mut store, &module)?;
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn func_wrap<Params, Args>(
        &mut self,
        module: &str,
        name: &str,
        func: impl IntoFunc<T, Params, Args>,
    ) -> Result<&mut Self> {
        let func = HostFunc::wrap(&self.engine, func);
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::HostFunc(Arc::new(func)))?;
        Ok(self)
    }

    /// Asynchronous analog of [`Linker::func_wrap`].
    #[cfg(feature = "async")]
    pub fn func_wrap_async<F, Params: WasmTyList, Args: crate::WasmRet>(
        &mut self,
        module: &str,
        name: &str,
        func: F,
    ) -> Result<&mut Self>
    where
        F: for<'a> Fn(Caller<'a, T>, Params) -> Box<dyn Future<Output = Args> + Send + 'a>
            + Send
            + Sync
            + 'static,
    {
        assert!(
            self.engine.config().async_support,
            "cannot use `func_wrap_async` without enabling async support on the config",
        );
        let func = HostFunc::wrap_inner(
            &self.engine,
            move |mut caller: Caller<'_, T>, args: Params| {
                let async_cx = caller
                    .store
                    .as_context_mut()
                    .0
                    .async_cx()
                    .expect("Attempt to start async function on dying fiber");
                let mut future = Pin::from(func(caller, args));
                match unsafe { async_cx.block_on(future.as_mut()) } {
                    Ok(ret) => ret.into_fallible(),
                    Err(e) => Args::fallible_from_error(e),
                }
            },
        );
        let key = self.import_key(module, Some(name));
        self.insert(key, Definition::HostFunc(Arc::new(func)))?;
        Ok(self)
    }

    /// Convenience wrapper to define an entire [`Instance`] in this linker.
    ///
    /// This function is a convenience wrapper around [`Linker::define`] which
    /// will define all exports on `instance` into this linker. The module name
    /// for each export is `module_name`, and the name for each export is the
    /// name in the instance itself.
    ///
    /// Note that when this API is used the [`Linker`] is no longer compatible
    /// with multi-[`Store`][crate::Store] instantiation because the items
    /// defined within this store will belong to the `store` provided, and only
    /// the `store` provided.
    ///
    /// # Errors
    ///
    /// Returns an error if the any item is redefined twice in this linker (for
    /// example the same `module_name` was already defined) and shadowing is
    /// disallowed, or if `instance` comes from a different
    /// [`Store`](crate::Store) than this [`Linker`] originally was created
    /// with.
    ///
    /// # Panics
    ///
    /// Panics if `instance` does not belong to `store`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    ///
    /// // Instantiate a small instance...
    /// let wat = r#"(module (func (export "run") ))"#;
    /// let module = Module::new(&engine, wat)?;
    /// let instance = linker.instantiate(&mut store, &module)?;
    ///
    /// // ... and inform the linker that the name of this instance is
    /// // `instance1`. This defines the `instance1::run` name for our next
    /// // module to use.
    /// linker.instance(&mut store, "instance1", instance)?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "instance1" "run" (func $instance1_run))
    ///         (func (export "run")
    ///             call $instance1_run
    ///         )
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// let instance = linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn instance(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module_name: &str,
        instance: Instance,
    ) -> Result<&mut Self> {
        let mut store = store.as_context_mut();
        let exports = instance
            .exports(&mut store)
            .map(|e| {
                (
                    self.import_key(module_name, Some(e.name())),
                    e.into_extern(),
                )
            })
            .collect::<Vec<_>>();
        for (key, export) in exports {
            self.insert(key, Definition::new(store.0, export))?;
        }
        Ok(self)
    }

    /// same as instance, but with a few blacklist symbols for dynamic loading usage
    pub fn instance_dylink(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module_name: &str,
        instance: Instance,
    ) -> Result<&mut Self> {
        let mut store = store.as_context_mut();
        let exports = instance
            .exports(&mut store)
            .filter_map(|e| {
                (if module_name == "env"
                    && match e.name() {
                        // stack pointer and tls base are per-module exclusive and should not be exposed to child module
                        "__stack_pointer"
                        | "__tls_base"
                        // constructor functions, which is module exclusive and should not be exposed to child module
                        | "__wasm_call_ctors"
                        // those symbols are used internally by wasmtime and shouldn't be exposed to the child module
                        | "__wasm_apply_data_relocs"
                        | "__wasm_apply_global_relocs"
                        | "__wasm_apply_tls_relocs"
                        | "__wasm_init_tls"
                        // asyncify symbols
                        | "asyncify_start_unwind"
                        | "asyncify_stop_unwind"
                        | "asyncify_start_rewind"
                        | "asyncify_stop_rewind"
                        | "asyncify_get_state"
                        // lind custom symbols
                        | "__get_aligned_tls_size" => true,
                        // fpcast-emu exports, which shouldn't linked directly
                        | s if s.starts_with(FPCAST_FUNC_SIGNATURE) => true,
                        _ => false,
                    }
                {
                    None
                } else {
                    Some((
                        self.import_key(module_name, Some(e.name())),
                        e.into_extern(),
                    ))
                })
            })
            .collect::<Vec<_>>();
        for (key, export) in exports {
            self.insert(key, Definition::new(store.0, export))?;
        }
        Ok(self)
    }

    /// Define automatic instantiations of a [`Module`] in this linker.
    ///
    /// This automatically handles [Commands and Reactors] instantiation and
    /// initialization.
    ///
    /// Exported functions of a Command module may be called directly, however
    /// instead of having a single instance which is reused for each call,
    /// each call creates a new instance, which lives for the duration of the
    /// call. The imports of the Command are resolved once, and reused for
    /// each instantiation, so all dependencies need to be present at the time
    /// when `Linker::module` is called.
    ///
    /// For Reactors, a single instance is created, and an initialization
    /// function is called, and then its exports may be called.
    ///
    /// Ordinary modules which don't declare themselves to be either Commands
    /// or Reactors are treated as Reactors without any initialization calls.
    ///
    /// [Commands and Reactors]: https://github.com/WebAssembly/WASI/blob/main/legacy/application-abi.md#current-unstable-abi
    ///
    /// # Errors
    ///
    /// Returns an error if the any item is redefined twice in this linker (for
    /// example the same `module_name` was already defined) and shadowing is
    /// disallowed, if `instance` comes from a different
    /// [`Store`](crate::Store) than this [`Linker`] originally was created
    /// with, or if a Reactor initialization function traps.
    ///
    /// # Panics
    ///
    /// Panics if any item used to instantiate the provided [`Module`] is not
    /// owned by `store`, or if the `store` provided comes from a different
    /// [`Engine`] than this [`Linker`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    ///
    /// // Instantiate a small instance and inform the linker that the name of
    /// // this instance is `instance1`. This defines the `instance1::run` name
    /// // for our next module to use.
    /// let wat = r#"(module (func (export "run") ))"#;
    /// let module = Module::new(&engine, wat)?;
    /// linker.module(&mut store, "instance1", &module)?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "instance1" "run" (func $instance1_run))
    ///         (func (export "run")
    ///             call $instance1_run
    ///         )
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// let instance = linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// For a Command, a new instance is created for each call.
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    ///
    /// // Create a Command that attempts to count the number of times it is run, but is
    /// // foiled by each call getting a new instance.
    /// let wat = r#"
    ///     (module
    ///         (global $counter (mut i32) (i32.const 0))
    ///         (func (export "_start")
    ///             (global.set $counter (i32.add (global.get $counter) (i32.const 1)))
    ///         )
    ///         (func (export "read_counter") (result i32)
    ///             (global.get $counter)
    ///         )
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// linker.module(&mut store, "commander", &module)?;
    /// let run = linker.get_default(&mut store, "")?
    ///     .typed::<(), ()>(&store)?
    ///     .clone();
    /// run.call(&mut store, ())?;
    /// run.call(&mut store, ())?;
    /// run.call(&mut store, ())?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "commander" "_start" (func $commander_start))
    ///         (import "commander" "read_counter" (func $commander_read_counter (result i32)))
    ///         (func (export "run") (result i32)
    ///             call $commander_start
    ///             call $commander_start
    ///             call $commander_start
    ///             call $commander_read_counter
    ///         )
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// linker.module(&mut store, "", &module)?;
    /// let run = linker.get(&mut store, "", "run").unwrap().into_func().unwrap();
    /// let count = run.typed::<(), i32>(&store)?.call(&mut store, ())?;
    /// assert_eq!(count, 0, "a Command should get a fresh instance on each invocation");
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn module(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module_name: &str,
        module: &Module,
    ) -> Result<&mut Self>
    where
        T: 'static,
    {
        // NB: this is intended to function the same as `Linker::module_async`,
        // they should be kept in sync.

        // This assert isn't strictly necessary since it'll bottom out in the
        // `HostFunc::to_func` method anyway. This is placed earlier for this
        // function though to prevent the functions created here from delaying
        // the panic until they're called.
        assert!(
            Engine::same(&self.engine, store.as_context().engine()),
            "different engines for this linker and the store provided"
        );
        match ModuleKind::categorize(module)? {
            ModuleKind::Command => {
                self.command(
                    store,
                    module_name,
                    module,
                    |store, func_ty, export_name, instance_pre| {
                        Func::new(
                            store,
                            func_ty.clone(),
                            move |mut caller, params, results| {
                                // Create a new instance for this command execution.
                                let instance = instance_pre.instantiate(&mut caller)?;

                                // `unwrap()` everything here because we know the instance contains a
                                // function export with the given name and signature because we're
                                // iterating over the module it was instantiated from.
                                instance
                                    .get_export(&mut caller, &export_name)
                                    .unwrap()
                                    .into_func()
                                    .unwrap()
                                    .call(&mut caller, params, results)?;

                                Ok(())
                            },
                        )
                    },
                )
            }
            ModuleKind::Reactor => {
                let instance = self.instantiate(&mut store, &module)?;

                if let Some(export) = instance.get_export(&mut store, "_initialize") {
                    if let Extern::Func(func) = export {
                        func.typed::<(), ()>(&store)
                            .and_then(|f| f.call(&mut store, ()).map_err(Into::into))
                            .context("calling the Reactor initialization function")?;
                    }
                }

                self.instance(store, module_name, instance)
            }
        }
    }

    pub fn module_with_preload(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        cageid: u64,
        module_name: &str,
        module: &Module,
        table: &mut Table,
        table_base: i32,
        got: &LindGOT,
        path: String,
    ) -> Result<&mut Self>
    where
        T: 'static,
    {
        assert!(
            Engine::same(&self.engine, store.as_context().engine()),
            "different engines for this linker and the store provided"
        );
        match ModuleKind::categorize(module)? {
            ModuleKind::Command => {
                unreachable!()
            }
            ModuleKind::Reactor => {
                // We clone the linker to instantiate the library with instance-specific globals.
                // `__memory_base` and `__table_base` are specific to each library instance,
                // so we create a cloned linker, override these two globals in the clone,
                // and then instantiate the library using that cloned linker.
                //
                // `allow_shadowing(true)` permits redefining these globals in the cloned
                // linker without affecting the original linker state.
                let mut module_linker = self.clone();

                module_linker.allow_shadowing(true);
                // Create a placeholder for `__memory_base` for library instantiation.
                //
                // In Lind, the final linear-memory base address is only known after the
                // main module's shared memory is created and `vmmap` is initialized.
                // We therefore link a dummy `__memory_base` global (initialized to 0)
                // and pass its backing slot (`handler`) into `InstantiateLib`, so
                // `instantiate_with_lind` can patch the global once the real base is known.
                let memory_base = module_linker.attach_memory_base(&mut store, 0)?;
                // keep a recording of this memory base in main linker as well
                self.define(
                    &mut store,
                    "lib.memory_base",
                    &module.name().unwrap(),
                    memory_base,
                );
                let handler = memory_base.get_handler_as_u32(&mut store);

                // attach the table base for the library module
                module_linker.attach_table_base(&mut store, table_base)?;

                module_linker.allow_shadowing(false);

                // Resolve any remaining unknown imports to trap stubs so the library can
                // instantiate even when it has optional/unused imports.
                // TODO: we probably want to remove this in the future
                module_linker.define_unknown_imports_as_traps(module);

                // Instantiate the library module. `InstantiateLib(handler)` tells the Lind instantiation
                // path where to patch the `__memory_base` placeholder once the shared-memory base is known.
                let (instance, _, instance_id) = module_linker.instantiate_with_lind(
                    &mut store,
                    &module,
                    InstantiateType::InstantiateLib {
                        cageid,
                        memory_base: handler,
                    },
                )?;

                // Register this instance under its intrinsic wasm name so
                // get_global_snapshot can find it without scanning all INSTANCE_NUMBER slots.
                // We use module.name() (the name embedded in the binary) because
                // the snapshot lookup in child processes also uses module.name().
                if let Some(wasm_name) = module.name() {
                    store
                        .as_context_mut()
                        .register_named_instance(wasm_name.to_string(), instance_id);
                }

                // After instantiation, the loader has patched `__memory_base`; read it back from the slot.
                let memory_base = unsafe { *handler };

                let fpcast_enabled = self.engine.config().fpcast_enabled;
                instance.apply_GOT_relocs(
                    &mut store,
                    Some(got),
                    table,
                    Some(memory_base),
                    fpcast_enabled,
                )?;

                // If the module has a start function, run it (Wasm start semantics).
                if let Some(start) = module.compiled_module().module().start_func {
                    instance.start_raw(&mut store.as_context_mut(), start)?;
                }

                // run data relocation functions and constructor functions
                instance.apply_relocs_func_and_constructor(&mut store)?;

                self.instance_dylink(store, module_name, instance)
            }
        }
    }

    pub fn module_with_child(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        cageid: u64,
        module_name: &str,
        module: &Module,
        table: &mut Table,
        table_base: i32,
        memory_base: i32,
        child_type: ChildLibraryType,
        snapshots: &[(GlobalIndex, i64)],
    ) -> Result<&mut Self>
    where
        T: 'static,
    {
        assert!(
            Engine::same(&self.engine, store.as_context().engine()),
            "different engines for this linker and the store provided"
        );
        match ModuleKind::categorize(module)? {
            ModuleKind::Command => {
                unreachable!();
            }
            ModuleKind::Reactor => {
                // We clone the linker to instantiate the library with instance-specific globals.
                // `__memory_base` and `__table_base` are specific to each library instance,
                // so we create a cloned linker, override these two globals in the clone,
                // and then instantiate the library using that cloned linker.
                //
                // `allow_shadowing(true)` permits redefining these globals in the cloned
                // linker without affecting the original linker state.
                let mut module_linker = self.clone();

                module_linker.allow_shadowing(true);
                // Create a placeholder for `__memory_base` for library instantiation.
                //
                // In Lind, the final linear-memory base address is only known after the
                // main module's shared memory is created and `vmmap` is initialized.
                // We therefore link a dummy `__memory_base` global (initialized to 0)
                // and pass its backing slot (`handler`) into `InstantiateLib`, so
                // `instantiate_with_lind` can patch the global once the real base is known.
                let memory_base = module_linker.attach_memory_base(&mut store, memory_base)?;
                let handler = memory_base.get_handler_as_u32(&mut store);
                // NOTE: we do not create a record in main linker like in `module_with_preload`
                // because the recording is already copied into main linker when we clone it

                // Provide `__table_base` for the library (used by indirect calls / table relocs).
                module_linker.attach_table_base(&mut store, table_base)?;

                module_linker.allow_shadowing(false);

                // Resolve any remaining unknown imports to trap stubs so the library can
                // instantiate even when it has optional/unused imports.
                module_linker.define_unknown_imports_as_traps(module);
                // Instantiate the library module. Do not need to do any initialization for the module
                // since all the state are already copied from parent
                let (instance, _stack_arena_size, instance_id) =
                    module_linker.instantiate_with_lind_thread(&mut store, &module, true)?;

                if let Some(wasm_name) = module.name() {
                    store
                        .as_context_mut()
                        .register_named_instance(wasm_name.to_string(), instance_id);
                }

                let fpcast_enabled = self.engine.config().fpcast_enabled;
                // for child library, just append the library function into function table without doing GOT relocation
                instance.apply_GOT_relocs(&mut store, None, table, None, fpcast_enabled)?;

                // clone the wasm global for the child instance
                instance.apply_global_snapshots(&mut store, snapshots);

                if let ChildLibraryType::Thread(stack_addr) = child_type {
                    // if the child library is a thread, we need to initialize the TLS for the library
                    // on the thread's stack
                    if let Ok(init_tls) = instance
                        .get_typed_func::<i32, ()>(store.as_context_mut(), "__wasm_init_tls")
                    {
                        let get_tls_size = instance.get_typed_func::<(), i32>(
                            store.as_context_mut(),
                            "__get_aligned_tls_size",
                        )?;

                        let tls_size = get_tls_size.call(store.as_context_mut(), ()).unwrap();
                        *stack_addr -= tls_size as u32;
                        let _ = init_tls
                            .call(store.as_context_mut(), *stack_addr as i32)
                            .unwrap();
                    }
                }

                self.instance_dylink(store, module_name, instance)
            }
        }
    }

    /// An alternative library instantiation path similar to `module`.
    ///
    /// This variant is used when instantiating a library module from a running
    /// instance, rather than during the initial preload phase. As a result,
    /// it takes slightly different arguments to accommodate the different
    /// execution context and available state between module preloading and
    /// runtime library loading.
    pub fn module_with_caller(
        &mut self,
        mut store: &mut crate::Caller<T>,
        cageid: u64,
        module_name: &str,
        module: &Module,
        table_base: i32,
        got: &LindGOT,
        mut symbol_map: SymbolMap,
        path: String,
    ) -> Result<u64>
    where
        T: 'static,
    {
        assert!(
            Engine::same(&self.engine, store.as_context().engine()),
            "different engines for this linker and the store provided"
        );
        match ModuleKind::categorize(module)? {
            ModuleKind::Command => {
                unreachable!();
            }
            ModuleKind::Reactor => {
                // We clone the linker to instantiate the library with instance-specific globals.
                // `__memory_base` and `__table_base` are specific to each library instance,
                // so we create a cloned linker, override these two globals in the clone,
                // and then instantiate the library using that cloned linker.
                //
                // `allow_shadowing(true)` permits redefining these globals in the cloned
                // linker without affecting the original linker state.
                let mut module_linker = self.clone();

                module_linker.allow_shadowing(true);
                // Create a placeholder for `__memory_base` for library instantiation.
                //
                // In Lind, the final linear-memory base address is only known after the
                // main module's shared memory is created and `vmmap` is initialized.
                // We therefore link a dummy `__memory_base` global (initialized to 0)
                // and pass its backing slot (`handler`) into `InstantiateLib`, so
                // `instantiate_with_lind` can patch the global once the real base is known.
                let memory_base = module_linker.attach_memory_base(&mut store, 0)?;
                // keep a recording of this memory base in main linker as well
                self.define(
                    &mut store,
                    "lib.memory_base",
                    &module.name().unwrap(),
                    memory_base,
                );
                let handler = memory_base.get_handler_as_u32(&mut store);

                // attach the table base for the library module
                module_linker.attach_table_base(&mut store, table_base)?;

                module_linker.allow_shadowing(false);

                // Resolve any remaining unknown imports to trap stubs so the library can
                // instantiate even when it has optional/unused imports.
                module_linker.define_unknown_imports_as_traps(module);

                // Instantiate the library module. `InstantiateLib(handler)` tells the Lind instantiation
                // path where to patch the `__memory_base` placeholder once the shared-memory base is known.
                let (instance, _stack_arena_size, instance_id) = module_linker.instantiate_with_lind(
                    &mut store,
                    &module,
                    InstantiateType::InstantiateLib {
                        cageid,
                        memory_base: handler,
                    },
                )?;

                if let Some(wasm_name) = module.name() {
                    store
                        .as_context_mut()
                        .register_named_instance(wasm_name.to_string(), instance_id);
                }

                // After instantiation, the loader has patched `__memory_base`; read it back from the slot.
                let memory_base = unsafe { *handler };

                let fpcast_enabled = self.engine.config().fpcast_enabled;

                // Collect exports first to avoid mutating the store/linker while iterating exports.
                // We split funcs and globals because they are relocated differently.
                // TODO: probably want to unify this with apply_GOT_relocs in the future
                let mut funcs = vec![];
                let mut globals = vec![];
                for export in instance.exports(&mut store) {
                    let name = export.name().to_owned();
                    match export.into_extern() {
                        Extern::Func(func) => {
                            funcs.push((name, func));
                        }
                        Extern::Global(global) => {
                            globals.push((name, global));
                        }
                        _ => {}
                    }
                }

                for (name, func) in funcs {
                    // skip updating GOT only if fpcast is enabled
                    // and the function is NOT a fpcast function
                    let should_skip = if fpcast_enabled {
                        if name.starts_with(FPCAST_FUNC_SIGNATURE) {
                            false
                        } else {
                            true
                        }
                    } else {
                        false
                    };

                    if !should_skip {
                        // TODO: probably needs to skip if the symbol is internal symbols (e.g. epoch symbols)
                        let index = store.grow_table_lib(1, crate::Ref::Func(Some(func)))?;

                        let final_name = {
                            if fpcast_enabled {
                                // restore to its original name
                                name.strip_prefix(FPCAST_FUNC_SIGNATURE).unwrap()
                            } else {
                                &name
                            }
                        };

                        // update GOT entry
                        if got.update_entry_if_unresolved(&final_name, index) {
                            #[cfg(feature = "debug-dylink")]
                            println!("[debug] update GOT.func.{} to {}", final_name, index);
                        }

                        // append the symbol into mappings
                        symbol_map.add(final_name.to_string(), index);
                    }
                }
                for (name, global) in globals {
                    // Only relocate globals that are actually registered in the GOT.
                    // Applying memory_base to an unrelated exported global would overflow.
                    if !got.has_entry(&name) {
                        continue;
                    }
                    // TODO: probably needs to skip if the symbol is internal symbols (e.g. epoch symbols)
                    let val = global.get(&mut store);
                    // relocate the variable
                    let val = val.i32().unwrap() as u32 + memory_base;
                    // append the symbol into mappings
                    symbol_map.add(name.clone(), val);
                    // update GOT entry
                    if got.update_entry_if_unresolved(&name, val) {
                        #[cfg(feature = "debug-dylink")]
                        println!("[debug] update GOT.mem.{} to {}", name, val);
                    }
                }

                let is_local = symbol_map.is_local();

                // append the symbol mapping of this library into the global lookup table
                let handler = store.push_library_symbols(symbol_map).unwrap() as u64;

                // If the module has a start function, run it (Wasm start semantics).
                if let Some(start) = module.compiled_module().module().start_func {
                    instance.start_raw(&mut store.as_context_mut(), start)?;
                }

                // run data relocation functions and constructor functions
                instance.apply_relocs_func_and_constructor(&mut store)?;

                if !is_local {
                    // only attach library symbol to Linker if it is global scope
                    self.instance_dylink(store, module_name, instance);
                }

                Ok(handler)
            }
        }
    }

    /// Define automatic instantiations of a [`Module`] in this linker.
    ///
    /// This is the same as [`Linker::module`], except for async `Store`s.
    #[cfg(all(feature = "async", feature = "cranelift"))]
    pub async fn module_async(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module_name: &str,
        module: &Module,
    ) -> Result<&mut Self>
    where
        T: Send + 'static,
    {
        // NB: this is intended to function the same as `Linker::module`, they
        // should be kept in sync.
        assert!(
            Engine::same(&self.engine, store.as_context().engine()),
            "different engines for this linker and the store provided"
        );
        match ModuleKind::categorize(module)? {
            ModuleKind::Command => self.command(
                store,
                module_name,
                module,
                |store, func_ty, export_name, instance_pre| {
                    let upvars = Arc::new((instance_pre, export_name));
                    Func::new_async(
                        store,
                        func_ty.clone(),
                        move |mut caller, params, results| {
                            let upvars = upvars.clone();
                            Box::new(async move {
                                let (instance_pre, export_name) = &*upvars;
                                let instance = instance_pre.instantiate_async(&mut caller).await?;

                                instance
                                    .get_export(&mut caller, &export_name)
                                    .unwrap()
                                    .into_func()
                                    .unwrap()
                                    .call_async(&mut caller, params, results)
                                    .await?;
                                Ok(())
                            })
                        },
                    )
                },
            ),
            ModuleKind::Reactor => {
                let instance = self.instantiate_async(&mut store, &module).await?;

                if let Some(export) = instance.get_export(&mut store, "_initialize") {
                    if let Extern::Func(func) = export {
                        let func = func
                            .typed::<(), ()>(&store)
                            .context("loading the Reactor initialization function")?;
                        func.call_async(&mut store, ())
                            .await
                            .context("calling the Reactor initialization function")?;
                    }
                }

                self.instance(store, module_name, instance)
            }
        }
    }

    fn command(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        module_name: &str,
        module: &Module,
        mk_func: impl Fn(&mut StoreContextMut<T>, &FuncType, String, InstancePre<T>) -> Func,
    ) -> Result<&mut Self>
    where
        T: 'static,
    {
        let mut store = store.as_context_mut();
        for export in module.exports() {
            if let Some(func_ty) = export.ty().func() {
                let instance_pre = self.instantiate_pre(module)?;
                let export_name = export.name().to_owned();
                let func = mk_func(&mut store, func_ty, export_name, instance_pre);
                let key = self.import_key(module_name, Some(export.name()));
                self.insert(key, Definition::new(store.0, func.into()))?;
            } else if export.name() == "memory" && export.ty().memory().is_some() {
                // Allow an exported "memory" memory for now.
            } else if export.name() == "__indirect_function_table" && export.ty().table().is_some()
            {
                // Allow an exported "__indirect_function_table" table for now.
            } else if export.name() == "table" && export.ty().table().is_some() {
                // Allow an exported "table" table for now.
            } else if export.name() == "__data_end" && export.ty().global().is_some() {
                // Allow an exported "__data_end" memory for compatibility with toolchains
                // which use --export-dynamic, which unfortunately doesn't work the way
                // we want it to.
                warn!("command module exporting '__data_end' is deprecated");
            } else if export.name() == "__heap_base" && export.ty().global().is_some() {
                // Allow an exported "__data_end" memory for compatibility with toolchains
                // which use --export-dynamic, which unfortunately doesn't work the way
                // we want it to.
                warn!("command module exporting '__heap_base' is deprecated");
            } else if export.name() == "__dso_handle" && export.ty().global().is_some() {
                // Allow an exported "__dso_handle" memory for compatibility with toolchains
                // which use --export-dynamic, which unfortunately doesn't work the way
                // we want it to.
                warn!("command module exporting '__dso_handle' is deprecated")
            } else if export.name() == "__rtti_base" && export.ty().global().is_some() {
                // Allow an exported "__rtti_base" memory for compatibility with
                // AssemblyScript.
                warn!("command module exporting '__rtti_base' is deprecated; pass `--runtime half` to the AssemblyScript compiler");
            } else if !self.allow_unknown_exports {
                bail!("command export '{}' is not a function", export.name());
            }
        }

        Ok(self)
    }

    /// Aliases one item's name as another.
    ///
    /// This method will alias an item with the specified `module` and `name`
    /// under a new name of `as_module` and `as_name`.
    ///
    /// # Errors
    ///
    /// Returns an error if any shadowing violations happen while defining new
    /// items, or if the original item wasn't defined.
    pub fn alias(
        &mut self,
        module: &str,
        name: &str,
        as_module: &str,
        as_name: &str,
    ) -> Result<&mut Self> {
        let src = self.import_key(module, Some(name));
        let dst = self.import_key(as_module, Some(as_name));
        match self.map.get(&src).cloned() {
            Some(item) => self.insert(dst, item)?,
            None => bail!("no item named `{}::{}` defined", module, name),
        }
        Ok(self)
    }

    /// Aliases one module's name as another.
    ///
    /// This method will alias all currently defined under `module` to also be
    /// defined under the name `as_module` too.
    ///
    /// # Errors
    ///
    /// Returns an error if any shadowing violations happen while defining new
    /// items.
    pub fn alias_module(&mut self, module: &str, as_module: &str) -> Result<()> {
        let module = self.intern_str(module);
        let as_module = self.intern_str(as_module);
        let items = self
            .map
            .iter()
            .filter(|(key, _def)| key.module == module)
            .map(|(key, def)| (key.name, def.clone()))
            .collect::<Vec<_>>();
        for (name, item) in items {
            self.insert(
                ImportKey {
                    module: as_module,
                    name,
                },
                item,
            )?;
        }
        Ok(())
    }

    fn insert(&mut self, key: ImportKey, item: Definition) -> Result<()> {
        match self.map.entry(key) {
            Entry::Occupied(_) if !self.allow_shadowing => {
                let module = &self.strings[key.module];
                let desc = match self.strings.get(key.name) {
                    Some(name) => format!("{}::{}", module, name),
                    None => module.to_string(),
                };
                bail!("import of `{}` defined twice", desc)
            }
            Entry::Occupied(mut o) => {
                #[cfg(feature = "debug-dylink")]
                {
                    let module = &self.strings[key.module];
                    let desc = match self.strings.get(key.name) {
                        Some(name) => format!("{}::{}", module, name),
                        None => module.to_string(),
                    };
                    println!("[debug]: warning: {:?} definition overwrite", desc);
                }
                o.insert(item);
            }
            Entry::Vacant(v) => {
                v.insert(item);
            }
        }
        Ok(())
    }

    fn import_key(&mut self, module: &str, name: Option<&str>) -> ImportKey {
        ImportKey {
            module: self.intern_str(module),
            name: name
                .map(|name| self.intern_str(name))
                .unwrap_or(usize::max_value()),
        }
    }

    fn intern_str(&mut self, string: &str) -> usize {
        if let Some(idx) = self.string2idx.get(string) {
            return *idx;
        }
        let string: Arc<str> = string.into();
        let idx = self.strings.len();
        self.strings.push(string.clone());
        self.string2idx.insert(string, idx);
        idx
    }

    /// Attempts to instantiate the `module` provided.
    ///
    /// This method will attempt to assemble a list of imports that correspond
    /// to the imports required by the [`Module`] provided. This list
    /// of imports is then passed to [`Instance::new`] to continue the
    /// instantiation process.
    ///
    /// Each import of `module` will be looked up in this [`Linker`] and must
    /// have previously been defined. If it was previously defined with an
    /// incorrect signature or if it was not previously defined then an error
    /// will be returned because the import can not be satisfied.
    ///
    /// Per the WebAssembly spec, instantiation includes running the module's
    /// start function, if it has one (not to be confused with the `_start`
    /// function, which is not run).
    ///
    /// # Errors
    ///
    /// This method can fail because an import may not be found, or because
    /// instantiation itself may fail. For information on instantiation
    /// failures see [`Instance::new`]. If an import is not found, the error
    /// may be downcast to an [`UnknownImportError`].
    ///
    ///
    /// # Panics
    ///
    /// Panics if any item used to instantiate `module` is not owned by
    /// `store`. Additionally this will panic if the [`Engine`] that the `store`
    /// belongs to is different than this [`Linker`].
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// linker.func_wrap("host", "double", |x: i32| x * 2)?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "host" "double" (func (param i32) (result i32)))
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// linker.instantiate(&mut store, &module)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn instantiate(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &Module,
    ) -> Result<Instance> {
        self._instantiate_pre(module, Some(store.as_context_mut().0))?
            .instantiate(store)
    }

    /// Instantiates a Wasm module as a new lind-wasm thread and returns both the created instance
    /// and its `InstanceId`. This function is used when creating a new thread within an existing cage.
    /// Unlike cage creation, thread instantiation does not require any lind-specific memory initialization
    /// or vmmap manipulation, because the thread executes within an already-established address space.
    ///
    /// The primary difference from Wasmtime’s standard instantiation path is that this function returns
    /// the `InstanceId`. Lind-3i relies on the `InstanceId` to later recover the `VMContext` pointer
    /// associated with this thread, enabling cross-cage and cross-grate runtime transfers.
    ///
    /// Internally, this function reuses Wasmtime’s pre-instantiation logic and delegates the actual
    /// instantiation to `instantiate_with_lind_thread` on the prepared instance, minimizing
    /// divergence from upstream Wasmtime behavior.
    pub fn instantiate_with_lind_thread(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &Module,
        no_start: bool,
    ) -> Result<(Instance, u32, InstanceId)> {
        self._instantiate_pre(module, Some(store.as_context_mut().0))?
            .instantiate_with_lind_thread(store, no_start)
    }

    /// Instantiates a Wasm module as a new lind-wasm cage and returns both the created instance and
    /// its `InstanceId`.
    ///
    /// This function extends Wasmtime’s standard instantiation path with lind-wasm–specific initialization
    /// logic required for correct RawPOSIX semantics. Depending on `InstantiateType`, this may involve
    /// initializing the first cage’s linear memory, setting up vmmap state, or cloning an existing cage’s
    /// memory when implementing `fork` semantics.
    ///
    /// As with thread instantiation, this function returns the `InstanceId` so that lind-3i can later obtain
    /// the corresponding `VMContext` pointer and re-enter the correct Wasmtime runtime state during cross-cage
    /// or cross-grate execution.
    ///
    /// The separation between `instantiate_with_lind` and `instantiate_with_lind_thread` is intentional:
    /// cage creation and thread creation have different memory semantics in lind-wasm, and keeping them
    /// as distinct entry points minimizes changes to upstream Wasmtime code while making the required
    /// lind-specific behavior explicit.
    pub fn instantiate_with_lind(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &Module,
        instantiate_type: InstantiateType,
    ) -> Result<(Instance, u32, InstanceId)> {
        self._instantiate_pre(module, Some(store.as_context_mut().0))?
            .instantiate_with_lind(store, instantiate_type)
    }

    /// Attempts to instantiate the `module` provided. This is the same as
    /// [`Linker::instantiate`], except for async `Store`s.
    #[cfg(feature = "async")]
    pub async fn instantiate_async(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &Module,
    ) -> Result<Instance>
    where
        T: Send,
    {
        self._instantiate_pre(module, Some(store.as_context_mut().0))?
            .instantiate_async(store)
            .await
    }

    /// Performs all checks necessary for instantiating `module` with this
    /// linker, except that instantiation doesn't actually finish.
    ///
    /// This method is used for front-loading type-checking information as well
    /// as collecting the imports to use to instantiate a module with. The
    /// returned [`InstancePre`] represents a ready-to-be-instantiated module,
    /// which can also be instantiated multiple times if desired.
    ///
    /// # Errors
    ///
    /// Returns an error which may be downcast to an [`UnknownImportError`] if
    /// the module has any unresolvable imports.
    ///
    /// # Examples
    ///
    /// ```
    /// # use wasmtime::*;
    /// # fn main() -> anyhow::Result<()> {
    /// # let engine = Engine::default();
    /// # let mut store = Store::new(&engine, ());
    /// let mut linker = Linker::new(&engine);
    /// linker.func_wrap("host", "double", |x: i32| x * 2)?;
    ///
    /// let wat = r#"
    ///     (module
    ///         (import "host" "double" (func (param i32) (result i32)))
    ///     )
    /// "#;
    /// let module = Module::new(&engine, wat)?;
    /// let instance_pre = linker.instantiate_pre(&module)?;
    ///
    /// // Finish instantiation after the type-checking has all completed...
    /// let instance = instance_pre.instantiate(&mut store)?;
    ///
    /// // ... and we can even continue to keep instantiating if desired!
    /// instance_pre.instantiate(&mut store)?;
    /// instance_pre.instantiate(&mut store)?;
    ///
    /// // Note that functions defined in a linker with `func_wrap` and similar
    /// // constructors are not owned by any particular `Store`, so we can also
    /// // instantiate our `instance_pre` in other stores because no imports
    /// // belong to the original store.
    /// let mut new_store = Store::new(&engine, ());
    /// instance_pre.instantiate(&mut new_store)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn instantiate_pre(&self, module: &Module) -> Result<InstancePre<T>> {
        self._instantiate_pre(module, None)
    }

    /// This is split out to optionally take a `store` so that when the
    /// `.instantiate` API is used we can get fresh up-to-date type information
    /// for memories and their current size, if necessary.
    ///
    /// Note that providing a `store` here is not required for correctness
    /// per-se. If one is not provided, such as the with the `instantiate_pre`
    /// API, then the type information used for memories and tables will reflect
    /// their size when inserted into the linker rather than their current size.
    /// This isn't expected to be much of a problem though since
    /// per-store-`Linker` types are likely using `.instantiate(..)` and
    /// per-`Engine` linkers don't have memories/tables in them.
    fn _instantiate_pre(
        &self,
        module: &Module,
        store: Option<&StoreOpaque>,
    ) -> Result<InstancePre<T>> {
        let mut imports = module
            .imports()
            .map(|import| {
                let definition = self._get_by_import(&import);
                definition
            })
            .collect::<Result<Vec<_>, _>>()
            .err2anyhow()?;
        if let Some(store) = store {
            for import in imports.iter_mut() {
                import.update_size(store);
            }
        }
        unsafe { InstancePre::new(module, imports) }
    }

    /// Returns an iterator over all items defined in this `Linker`, in
    /// arbitrary order.
    ///
    /// The iterator returned will yield 3-tuples where the first two elements
    /// are the module name and item name for the external item, and the third
    /// item is the item itself that is defined.
    ///
    /// Note that multiple `Extern` items may be defined for the same
    /// module/name pair.
    ///
    /// # Panics
    ///
    /// This function will panic if the `store` provided does not come from the
    /// same [`Engine`] that this linker was created with.
    pub fn iter<'a: 'p, 'p>(
        &'a self,
        mut store: impl AsContextMut<Data = T> + 'p,
    ) -> impl Iterator<Item = (&'a str, &'a str, Extern)> + 'p {
        self.map.iter().map(move |(key, item)| {
            let store = store.as_context_mut();
            (
                &*self.strings[key.module],
                &*self.strings[key.name],
                // Should be safe since `T` is connecting the linker and store
                unsafe { item.to_extern(store.0) },
            )
        })
    }

    /// Looks up a previously defined value in this [`Linker`], identified by
    /// the names provided.
    ///
    /// Returns `None` if this name was not previously defined in this
    /// [`Linker`].
    ///
    /// # Panics
    ///
    /// This function will panic if the `store` provided does not come from the
    /// same [`Engine`] that this linker was created with.
    pub fn get(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &str,
        name: &str,
    ) -> Option<Extern> {
        let store = store.as_context_mut().0;
        // Should be safe since `T` is connecting the linker and store
        Some(unsafe { self._get(module, name)?.to_extern(store) })
    }

    fn _get(&self, module: &str, name: &str) -> Option<&Definition> {
        let key = ImportKey {
            module: *self.string2idx.get(module)?,
            name: *self.string2idx.get(name)?,
        };
        self.map.get(&key)
    }

    /// Looks up a value in this `Linker` which matches the `import` type
    /// provided.
    ///
    /// Returns `None` if no match was found.
    ///
    /// # Panics
    ///
    /// This function will panic if the `store` provided does not come from the
    /// same [`Engine`] that this linker was created with.
    pub fn get_by_import(
        &self,
        mut store: impl AsContextMut<Data = T>,
        import: &ImportType,
    ) -> Option<Extern> {
        let store = store.as_context_mut().0;
        // Should be safe since `T` is connecting the linker and store
        Some(unsafe { self._get_by_import(import).ok()?.to_extern(store) })
    }

    fn _get_by_import(&self, import: &ImportType) -> Result<Definition, UnknownImportError> {
        match self._get(import.module(), import.name()) {
            Some(item) => Ok(item.clone()),
            None => Err(UnknownImportError::new(import)),
        }
    }

    /// Returns the "default export" of a module.
    ///
    /// An export with an empty string is considered to be a "default export".
    /// "_start" is also recognized for compatibility.
    ///
    /// # Panics
    ///
    /// Panics if the default function found is not owned by `store`. This
    /// function will also panic if the `store` provided does not come from the
    /// same [`Engine`] that this linker was created with.
    pub fn get_default(
        &self,
        mut store: impl AsContextMut<Data = T>,
        module: &str,
    ) -> Result<Func> {
        if let Some(external) = self.get(&mut store, module, "") {
            if let Extern::Func(func) = external {
                return Ok(func.clone());
            }
            bail!("default export in '{}' is not a function", module);
        }

        // For compatibility, also recognize "_start".
        if let Some(external) = self.get(&mut store, module, "_start") {
            if let Extern::Func(func) = external {
                return Ok(func.clone());
            }
            bail!("`_start` in '{}' is not a function", module);
        }

        // Otherwise return a no-op function.
        Ok(Func::wrap(store, || {}))
    }
}

/// Additional APIs for attaching common imports used by the Lind runtime and toolchain.
impl<T> Linker<T> {
    // attach the asyncify imports used by the asyncify transform.
    pub fn attach_asyncify(&mut self, mut store: impl AsContextMut<Data = T>) -> Result<()> {
        let __asyncify_state = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Var),
            Val::I32(0),
        )?;
        let __asyncify_data = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Var),
            Val::I32(0),
        )?;
        self.define(&mut store, "env", "__asyncify_state", __asyncify_state)?;
        self.define(&mut store, "env", "__asyncify_data", __asyncify_data)?;
        Ok(())
    }

    // attach the epoch global used by the Lind runtime for signal usage
    pub fn attach_epoch(&mut self, mut store: impl AsContextMut<Data = T>) -> Result<u64> {
        let lind_epoch = Global::new(
            &mut store,
            GlobalType::new(ValType::I64, crate::Mutability::Var),
            Val::I64(0),
        )?;

        self.define(&mut store, "lind", "epoch", lind_epoch)?;

        Ok(lind_epoch.get_handler_as_u64(&mut store) as u64)
    }

    // attach the `__indirect_function_table` used by the Lind runtime for dynamic loading
    pub fn attach_function_table(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        size: u32,
    ) -> Result<Table> {
        let ty = crate::TableType::new(crate::RefType::FUNCREF, size, None);
        let table = Table::new(&mut store, ty, crate::Ref::Func(None))?;
        self.define(&mut store, "env", "__indirect_function_table", table)?;

        Ok(table)
    }

    // attach the `__stack_low`, `__stack_high`, and `__stack_pointer` globals used by the Lind runtime for stack management.
    pub fn attach_stack_imports(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        stack_low: i32,
        stack_high: i32,
    ) -> Result<()> {
        let stack_low_global = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Var),
            Val::I32(stack_low),
        )?;

        let stack_high_global = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Var),
            Val::I32(stack_high),
        )?;

        // stack grows downwards, so the initial stack pointer is set to the high address.
        let stack_pointer = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Var),
            Val::I32(stack_high),
        )?;

        self.define(&mut store, "GOT.mem", "__stack_low", stack_low_global)?;
        self.define(&mut store, "GOT.mem", "__stack_high", stack_high_global)?;
        self.define(&mut store, "env", "__stack_pointer", stack_pointer)?;

        Ok(())
    }

    // attach the `__memory_base` global used by the Lind runtime for dynamic loading.
    pub fn attach_memory_base(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        memory_base: i32,
    ) -> Result<Global> {
        let memory_base = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Const),
            Val::I32(memory_base),
        )?;

        self.define(&mut store, "env", "__memory_base", memory_base)?;

        Ok(memory_base)
    }

    // attach the `__table_base` global used by the Lind runtime for dynamic loading.
    pub fn attach_table_base(
        &mut self,
        mut store: impl AsContextMut<Data = T>,
        table_base: i32,
    ) -> Result<()> {
        let table_base = Global::new(
            &mut store,
            GlobalType::new(ValType::I32, crate::Mutability::Const),
            Val::I32(table_base),
        )?;

        self.define(&mut store, "env", "__table_base", table_base)?;

        Ok(())
    }
}

impl<T> Default for Linker<T> {
    fn default() -> Linker<T> {
        Linker::new(&Engine::default())
    }
}

impl Definition {
    fn new(store: &StoreOpaque, item: Extern) -> Definition {
        let ty = DefinitionType::from(store, &item);
        Definition::Extern(item, ty)
    }

    pub(crate) fn ty(&self) -> DefinitionType {
        match self {
            Definition::Extern(_, ty) => ty.clone(),
            Definition::HostFunc(func) => DefinitionType::Func(func.sig_index()),
        }
    }

    /// Note the unsafety here is due to calling `HostFunc::to_func`. The
    /// requirement here is that the `T` that was originally used to create the
    /// `HostFunc` matches the `T` on the store.
    pub(crate) unsafe fn to_extern(&self, store: &mut StoreOpaque) -> Extern {
        match self {
            Definition::Extern(e, _) => e.clone(),
            Definition::HostFunc(func) => func.to_func(store).into(),
        }
    }

    pub(crate) fn comes_from_same_store(&self, store: &StoreOpaque) -> bool {
        match self {
            Definition::Extern(e, _) => e.comes_from_same_store(store),
            Definition::HostFunc(_func) => true,
        }
    }

    fn update_size(&mut self, store: &StoreOpaque) {
        match self {
            Definition::Extern(Extern::Memory(m), DefinitionType::Memory(_, size)) => {
                *size = m.internal_size(store);
            }
            Definition::Extern(Extern::SharedMemory(m), DefinitionType::Memory(_, size)) => {
                *size = m.size();
            }
            Definition::Extern(Extern::Table(m), DefinitionType::Table(_, size)) => {
                *size = m.internal_size(store);
            }
            _ => {}
        }
    }
}

impl DefinitionType {
    pub(crate) fn from(store: &StoreOpaque, item: &Extern) -> DefinitionType {
        let data = store.store_data();
        match item {
            Extern::Func(f) => DefinitionType::Func(f.type_index(data)),
            Extern::Table(t) => DefinitionType::Table(*t.wasmtime_ty(data), t.internal_size(store)),
            Extern::Global(t) => DefinitionType::Global(*t.wasmtime_ty(data)),
            Extern::Memory(t) => {
                DefinitionType::Memory(*t.wasmtime_ty(data), t.internal_size(store))
            }
            Extern::SharedMemory(t) => DefinitionType::Memory(*t.ty().wasmtime_memory(), t.size()),
        }
    }

    pub(crate) fn desc(&self) -> &'static str {
        match self {
            DefinitionType::Func(_) => "function",
            DefinitionType::Table(..) => "table",
            DefinitionType::Memory(..) => "memory",
            DefinitionType::Global(_) => "global",
        }
    }
}

/// Modules can be interpreted either as Commands or Reactors.
enum ModuleKind {
    /// The instance is a Command, meaning an instance is created for each
    /// exported function and lives for the duration of the function call.
    Command,

    /// The instance is a Reactor, meaning one instance is created which
    /// may live across multiple calls.
    Reactor,
}

impl ModuleKind {
    /// Determine whether the given module is a Command or a Reactor.
    fn categorize(module: &Module) -> Result<ModuleKind> {
        let command_start = module.get_export("_start");
        let reactor_start = module.get_export("_initialize");
        match (command_start, reactor_start) {
            (Some(command_start), None) => {
                if let Some(_) = command_start.func() {
                    Ok(ModuleKind::Command)
                } else {
                    bail!("`_start` must be a function")
                }
            }
            (None, Some(reactor_start)) => {
                if let Some(_) = reactor_start.func() {
                    Ok(ModuleKind::Reactor)
                } else {
                    bail!("`_initialize` must be a function")
                }
            }
            (None, None) => {
                // Module declares neither of the recognized functions, so treat
                // it as a reactor with no initialization function.
                Ok(ModuleKind::Reactor)
            }
            (Some(_), Some(_)) => {
                // Module declares itself to be both a Command and a Reactor.
                bail!("Program cannot be both a Command and a Reactor")
            }
        }
    }
}

/// Error for an unresolvable import.
///
/// Returned - wrapped in an [`anyhow::Error`] - by [`Linker::instantiate`] and
/// related methods for modules with unresolvable imports.
#[derive(Clone, Debug)]
pub struct UnknownImportError {
    module: String,
    name: String,
    ty: ExternType,
}

impl UnknownImportError {
    fn new(import: &ImportType) -> Self {
        Self {
            module: import.module().to_string(),
            name: import.name().to_string(),
            ty: import.ty(),
        }
    }

    /// Returns the module name that the unknown import was expected to come from.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Returns the field name of the module that the unknown import was expected to come from.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the type of the unknown import.
    pub fn ty(&self) -> ExternType {
        self.ty.clone()
    }
}

impl fmt::Display for UnknownImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "unknown import: `{}::{}` has not been defined",
            self.module, self.name,
        )
    }
}

#[cfg(feature = "std")]
impl std::error::Error for UnknownImportError {}
