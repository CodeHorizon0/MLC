use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use mlua::{Lua, StdLib};

use crate::config::RunReport;
use crate::error::{KernelError, KernelResult};
use crate::graph::DependencyGraph;
use crate::util::utc_now_rfc3339_nanos;

#[derive(Debug)]
pub struct LuaTask {
    pub graph: DependencyGraph,
    pub reply: std::sync::mpsc::Sender<KernelResult<RunReport>>,
}

pub fn spawn_lua_worker() -> KernelResult<SyncSender<LuaTask>> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<LuaTask>(64);

    thread::Builder::new()
        .name("kernel-lua-worker".to_string())
        .spawn(move || {
            if let Err(err) = lua_worker_loop(rx) {
                eprintln!("lua worker stopped: {err}");
            }
        })
        .map_err(|err| KernelError::WorkerInit(err.to_string()))?;

    Ok(tx)
}

fn lua_worker_loop(rx: Receiver<LuaTask>) -> KernelResult<()> {
    let lua = Lua::new_with(StdLib::ALL_SAFE, mlua::LuaOptions::default())
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    install_lua_host_api(&lua)?;

    let mut previous_modules = BTreeSet::<String>::new();

    for task in rx {
        let started = Instant::now();
        let started_wall = utc_now_rfc3339_nanos();
        let graph = task.graph;
        let current_modules = graph.modules.keys().cloned().collect::<BTreeSet<_>>();
        let stale_modules = previous_modules
            .union(&current_modules)
            .cloned()
            .collect::<Vec<_>>();
        let result = execute_lua_task(&lua, graph, &stale_modules);
        let finished_wall = utc_now_rfc3339_nanos();
        let elapsed = started.elapsed();

        match &result {
            Ok(report) => {
                eprintln!(
                    "[kernel:lua] finished {} -> {} in {:?} for {:?}",
                    started_wall,
                    finished_wall,
                    elapsed,
                    report.entry_path
                );
            }
            Err(err) => {
                eprintln!(
                    "[kernel:lua] failed {} -> {} after {:?}: {}",
                    started_wall,
                    finished_wall,
                    elapsed,
                    err
                );
            }
        }

        previous_modules = current_modules;
        let _ = task.reply.send(result);
    }

    Ok(())
}

fn execute_lua_task(lua: &Lua, graph: DependencyGraph, stale_modules: &[String]) -> KernelResult<RunReport> {
    let discovered_modules = graph.discovered_module_names();
    let language = graph.language;
    let entry_path = graph.entry_path.clone();

    clear_lua_loaded(lua, stale_modules)?;
    install_lua_preloaders(lua, &graph)?;

    let source = std::fs::read_to_string(&entry_path).map_err(KernelError::Io)?;
    let chunk = lua.load(source);

    chunk
        .set_name(entry_path.to_string_lossy())
        .exec()
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    Ok(RunReport {
        language,
        entry_path,
        discovered_modules,
    })
}

fn install_lua_host_api(lua: &Lua) -> KernelResult<()> {
    let host = lua.create_table().map_err(|err| KernelError::Lua(err.to_string()))?;

    let log = lua
        .create_function(|_, (level, message): (String, String)| {
            eprintln!("[host:{level}] {message}");
            Ok(())
        })
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    let read_text = lua
        .create_function(|_, path: String| {
            std::fs::read_to_string(path).map_err(|err| mlua::Error::external(err.to_string()))
        })
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    let write_text = lua
        .create_function(|_, (path, content): (String, String)| {
            if let Some(parent) = std::path::Path::new(&path).parent() {
                std::fs::create_dir_all(parent).map_err(|err| mlua::Error::external(err.to_string()))?;
            }
            std::fs::write(path, content).map_err(|err| mlua::Error::external(err.to_string()))?;
            Ok(())
        })
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    let cwd = lua
        .create_function(|_, ()| {
            std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .map_err(|err| mlua::Error::external(err.to_string()))
        })
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    host.set("log", log).map_err(|err| KernelError::Lua(err.to_string()))?;
    host.set("read_text", read_text).map_err(|err| KernelError::Lua(err.to_string()))?;
    host.set("write_text", write_text).map_err(|err| KernelError::Lua(err.to_string()))?;
    host.set("cwd", cwd).map_err(|err| KernelError::Lua(err.to_string()))?;

    lua.globals()
        .set("host", host)
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    Ok(())
}

fn clear_lua_loaded(lua: &Lua, module_names: &[String]) -> KernelResult<()> {
    let package: mlua::Table = lua
        .globals()
        .get("package")
        .map_err(|err| KernelError::Lua(err.to_string()))?;
    let loaded: mlua::Table = package
        .get("loaded")
        .map_err(|err| KernelError::Lua(err.to_string()))?;
    let preload: mlua::Table = package
        .get("preload")
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    for module in module_names {
        loaded
            .set(module.as_str(), mlua::Value::Nil)
            .map_err(|err| KernelError::Lua(err.to_string()))?;
        preload
            .set(module.as_str(), mlua::Value::Nil)
            .map_err(|err| KernelError::Lua(err.to_string()))?;
    }

    Ok(())
}

fn install_lua_preloaders(lua: &Lua, graph: &DependencyGraph) -> KernelResult<()> {
    let package: mlua::Table = lua
        .globals()
        .get("package")
        .map_err(|err| KernelError::Lua(err.to_string()))?;
    let preload: mlua::Table = package
        .get("preload")
        .map_err(|err| KernelError::Lua(err.to_string()))?;

    for (module, path) in &graph.modules {
        let module_name = module.clone();
        let file_path = path.clone();
        let loader = lua
            .create_function(move |lua, ()| {
                let source = std::fs::read_to_string(&file_path)
                    .map_err(|err| mlua::Error::external(err.to_string()))?;
                let value = lua
                    .load(source)
                    .set_name(&module_name)
                    .eval::<mlua::Value>()?;
                Ok(value)
            })
            .map_err(|err| KernelError::Lua(err.to_string()))?;
        preload
            .set(module.as_str(), loader)
            .map_err(|err| KernelError::Lua(err.to_string()))?;
    }

    Ok(())
}
