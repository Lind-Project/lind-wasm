use crate::lind_wasmtime::execute::read_wasm_or_cwasm_pub;
use crate::lind_wasmtime::library_search::find_library_path;
use anyhow::{Context, Result, anyhow, bail};
use std::collections::{HashMap, HashSet, VecDeque};
use wasmtime::{Engine, Module};
use wasmtime_lind_common::LindEnviron;

/// Collect the Needed library names declared in a module's dylink.0 section.
pub fn get_needed_libs(module: &Module) -> Vec<String> {
    module
        .dylink_neededinfo()
        .map(|info| info.needed.clone())
        .unwrap_or_default()
}

/// DFS-based topological sort that detects cycles.
///
/// `state`: 0 = unvisited, 1 = on DFS stack (visiting), 2 = done
/// `order`: accumulates library names in dependency-first order
fn topo_visit(
    name: &str,
    graph: &HashMap<String, Vec<String>>,
    state: &mut HashMap<String, u8>,
    order: &mut Vec<String>,
) -> Result<()> {
    match state.get(name).copied().unwrap_or(0) {
        2 => return Ok(()),
        1 => bail!(
            "cyclic library dependency involving '{}'. \
             Cyclic dependencies are not supported in the Lind WASM environment \
             because WASM modules must be fully instantiated before they can \
             resolve symbols for other modules.",
            name
        ),
        _ => {}
    }

    state.insert(name.to_string(), 1);

    if let Some(deps) = graph.get(name) {
        for dep in deps {
            topo_visit(dep, graph, state, order)
                .with_context(|| format!("required by '{}'", name))?;
        }
    }

    state.insert(name.to_string(), 2);
    order.push(name.to_string());
    Ok(())
}

/// Shared discovery and topological sort engine.
///
/// Loads all transitive library dependencies starting from `initial_queue`,
/// treating anything already in `loaded` as already known (won't be loaded again).
/// Returns all discovered libraries in dependency-first link order.
///
/// `loaded` maps library name → (host-path, Module, direct-dep-names).
/// Names in `exclude_from_output` are omitted from the returned list.
fn discover_and_sort(
    mut loaded: HashMap<String, (String, Module, Vec<String>)>,
    mut queue: VecDeque<String>,
    exclude_from_output: &HashSet<String>,
    engine: &Engine,
    environ: &LindEnviron,
) -> Result<Vec<(String, String, Module)>> {
    // Discover all transitive dependencies via BFS.
    while let Some(lib_name) = queue.pop_front() {
        if loaded.contains_key(&lib_name) {
            continue;
        }

        let lib_path = find_library_path(&lib_name, environ).ok_or_else(|| {
            anyhow!(
                "shared library '{}' not found in LD_LIBRARY_PATH or the \
                 default search paths (/lib, /usr/lib). \
                 The Lind instance cannot launch without it.",
                lib_name
            )
        })?;

        let module = read_wasm_or_cwasm_pub(engine, &lib_path)
            .with_context(|| format!("failed to load shared library '{}'", lib_name))?;

        let needed = get_needed_libs(&module);

        for dep in &needed {
            if !loaded.contains_key(dep) {
                queue.push_back(dep.clone());
            }
        }

        loaded.insert(
            lib_name,
            (lib_path.to_string_lossy().to_string(), module, needed),
        );
    }

    // Build an adjacency list for topological sort.
    // An edge lib → dep means lib depends on dep (dep must be linked first).
    let graph: HashMap<String, Vec<String>> = loaded
        .iter()
        .map(|(name, (_, _, deps))| {
            let internal_deps: Vec<String> = deps
                .iter()
                .filter(|d| loaded.contains_key(*d))
                .cloned()
                .collect();
            (name.clone(), internal_deps)
        })
        .collect();

    // Topological sort (DFS, deps before dependents).
    let mut state: HashMap<String, u8> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for name in loaded.keys() {
        topo_visit(name, &graph, &mut state, &mut order)?;
    }

    // Map sorted names back to (name, path, Module), skipping excluded entries.
    // The linker module name is always "env" per the WASM dynamic linking convention;
    // the library filename is preserved in the path field for identification.
    let mut result = Vec::new();
    for name in order {
        if exclude_from_output.contains(&name) {
            continue;
        }
        if let Some((path, module, _)) = loaded.remove(&name) {
            result.push(("env".to_string(), path, module));
        }
    }

    Ok(result)
}

/// Resolve all transitive library dependencies for the boot-time preload set and
/// return them in dependency-first link order.
///
/// `seed_modules[0]` is the main executable (name `""`); subsequent entries are
/// explicit `--preload` libraries. The function scans the Needed section of every
/// seed module, loads any new libraries it finds, and repeats until the full
/// transitive closure is known.  The result is a topologically-sorted list of all
/// libraries (explicit preloads merged with auto-discovered ones) ready for linking.
/// The main executable (index 0) is never included in the result.
pub fn resolve_load_order(
    seed_modules: &[(String, String, Module)],
    engine: &Engine,
    environ: &LindEnviron,
) -> Result<Vec<(String, String, Module)>> {
    // Pre-populate `loaded` with explicit preloads so they aren't re-discovered.
    let mut loaded: HashMap<String, (String, Module, Vec<String>)> = HashMap::new();
    for (name, path, module) in seed_modules.iter().skip(1) {
        let needed = get_needed_libs(module);
        loaded.insert(name.clone(), (path.clone(), module.clone(), needed));
    }

    // Seed the BFS queue from the main module's Needed section and from each
    // explicit preload's Needed section.
    let mut queue: VecDeque<String> = VecDeque::new();

    for lib in get_needed_libs(&seed_modules[0].2) {
        if !loaded.contains_key(&lib) {
            queue.push_back(lib);
        }
    }
    for (name, _, _) in seed_modules.iter().skip(1) {
        if let Some((_, _, deps)) = loaded.get(name) {
            for dep in deps.clone() {
                if !loaded.contains_key(&dep) {
                    queue.push_back(dep);
                }
            }
        }
    }

    // No entries to exclude from output; all discovered libs should be returned.
    let exclude: HashSet<String> = HashSet::new();
    discover_and_sort(loaded, queue, &exclude, engine, environ)
}

/// Resolve transitive dependencies of a library being opened via `dlopen`.
///
/// Discovers all libraries that `lib_module` depends on (transitively), skipping
/// any library whose name is in `already_loaded`.  Returns a list in dependency-first
/// order — every returned library must be linked before `lib_module` itself.
/// `lib_name` and `already_loaded` names are excluded from the result.
pub fn resolve_dlopen_dependencies(
    lib_name: &str,
    lib_module: &Module,
    already_loaded: &HashSet<String>,
    engine: &Engine,
    environ: &LindEnviron,
) -> Result<Vec<(String, String, Module)>> {
    // Pre-populate `loaded` with lib_name (already have the module in memory)
    // and with already-loaded library names (as empty placeholders so the BFS
    // treats them as known and skips re-loading them).
    let mut loaded: HashMap<String, (String, Module, Vec<String>)> = HashMap::new();

    let lib_needed = get_needed_libs(lib_module);
    loaded.insert(
        lib_name.to_string(),
        (String::new(), lib_module.clone(), lib_needed.clone()),
    );

    for name in already_loaded {
        // Use lib_module as a placeholder Module; it won't be returned since
        // these names are in `exclude_from_output`.
        loaded.insert(
            name.clone(),
            (String::new(), lib_module.clone(), Vec::new()),
        );
    }

    // Seed the queue from lib_module's direct dependencies.
    let mut queue: VecDeque<String> = VecDeque::new();
    for dep in &lib_needed {
        if !loaded.contains_key(dep) {
            queue.push_back(dep.clone());
        }
    }

    // Exclude lib_name and already_loaded names from the output.
    let mut exclude: HashSet<String> = already_loaded.clone();
    exclude.insert(lib_name.to_string());

    discover_and_sort(loaded, queue, &exclude, engine, environ)
}
