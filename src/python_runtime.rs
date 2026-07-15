use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, Sender};
use std::thread;
use std::time::Instant;

use pyo3::prelude::*;

use crate::config::RunReport;
use crate::error::{KernelError, KernelResult};
use crate::graph::DependencyGraph;
use crate::host::{build_python_manifest_json, register_python_host_module};
use crate::util::{to_cstring, utc_now_rfc3339_nanos};

#[derive(Debug)]
pub struct PythonTask {
    pub graph: DependencyGraph,
    pub reply: Sender<KernelResult<RunReport>>,
}

pub fn spawn_python_worker() -> KernelResult<Sender<PythonTask>> {
    let (tx, rx) = std::sync::mpsc::channel::<PythonTask>();

    thread::Builder::new()
        .name("kernel-python-worker".to_string())
        .spawn(move || {
            if let Err(err) = python_worker_loop(rx) {
                eprintln!("python worker stopped: {err}");
            }
        })
        .map_err(|err| KernelError::WorkerInit(err.to_string()))?;

    Ok(tx)
}

fn python_worker_loop(rx: Receiver<PythonTask>) -> KernelResult<()> {
    register_python_host_module()?;
    Python::initialize();

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
        let result = execute_python_task(graph, &stale_modules);
        let finished_wall = utc_now_rfc3339_nanos();
        let elapsed = started.elapsed();

        match &result {
            Ok(report) => {
                eprintln!(
                    "[kernel:python] finished {} -> {} in {:?} for {:?}",
                    started_wall,
                    finished_wall,
                    elapsed,
                    report.entry_path
                );
            }
            Err(err) => {
                eprintln!(
                    "[kernel:python] failed {} -> {} after {:?}: {}",
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

fn execute_python_task(graph: DependencyGraph, stale_modules: &[String]) -> KernelResult<RunReport> {
    let manifest_entries = graph
        .modules
        .iter()
        .map(|(name, path)| (name.clone(), path.clone()))
        .collect::<Vec<_>>();
    let discovered_modules = graph.discovered_module_names();
    let language = graph.language;
    let entry_path = graph.entry_path.clone();
    let entry_path_string = entry_path.to_string_lossy().to_string();
    let entry_module = graph.entry_module.clone().unwrap_or_default();
    let use_module_runner = should_use_python_module_runner(&graph);
    let execution_profile = classify_python_profile(&graph);
    let manifest = build_python_manifest_json(&manifest_entries)?;
    let stale_modules_json = serde_json::to_string(stale_modules)
        .map_err(|err| KernelError::Json(err))?;
    let base_dir = graph.base_dir.to_string_lossy().to_string();

    Python::attach(|py| -> KernelResult<RunReport> {
        let script = build_python_bootstrap(
            &manifest,
            &base_dir,
            &entry_path_string,
            &entry_module,
            use_module_runner,
            execution_profile,
            &stale_modules_json,
        );
        let code = to_cstring(&script)?;
        py.run(code.as_c_str(), None, None)
            .map_err(|err| KernelError::Python(err.to_string()))?;
        Ok(RunReport {
            language,
            entry_path,
            discovered_modules,
        })
    })
}

#[derive(Debug, Clone, Copy)]
enum PythonExecutionProfile {
    Ephemeral,
    Service,
    Looping,
}

fn classify_python_profile(graph: &DependencyGraph) -> PythonExecutionProfile {
    if graph.entry_is_code {
        return PythonExecutionProfile::Ephemeral;
    }

    let module_count = graph.modules.len();
    let file_name = graph
        .entry_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if file_name.contains("bot")
        || file_name.contains("monitor")
        || file_name.contains("watch")
        || file_name.contains("daemon")
    {
        return PythonExecutionProfile::Looping;
    }

    if module_count >= 5 {
        PythonExecutionProfile::Service
    } else {
        PythonExecutionProfile::Ephemeral
    }
}

fn should_use_python_module_runner(graph: &DependencyGraph) -> bool {
    if graph.entry_is_code {
        return false;
    }
    if graph.entry_module.is_none() {
        return false;
    }
    if graph.entry_path.extension().and_then(|s| s.to_str()) != Some("py") {
        return false;
    }
    graph.entry_path.starts_with(&graph.base_dir)
}

fn build_python_bootstrap(
    manifest_json: &str,
    base_dir: &str,
    entry_path: &str,
    entry_module: &str,
    use_module_runner: bool,
    profile: PythonExecutionProfile,
    stale_modules_json: &str,
) -> String {
    let profile_name = match profile {
        PythonExecutionProfile::Ephemeral => "ephemeral",
        PythonExecutionProfile::Service => "service",
        PythonExecutionProfile::Looping => "looping",
    };

    format!(
        r#"
import importlib.abc
import importlib.util
import json
import runpy
import sys

_manifest = json.loads(r'''{manifest_json}''')
_base_dir = r'''{base_dir}'''
_entry_path = r'''{entry_path}'''
_entry_module = r'''{entry_module}'''
_profile = r'''{profile_name}'''
_stale_modules = json.loads(r'''{stale_modules_json}''')

class _KernelFinder(importlib.abc.MetaPathFinder):
    def find_spec(self, fullname, path=None, target=None):
        file_path = _manifest.get(fullname)
        if file_path is None:
            return None
        return importlib.util.spec_from_file_location(fullname, file_path)

if _base_dir not in sys.path:
    sys.path.insert(0, _base_dir)

if not any(type(item).__name__ == '_KernelFinder' for item in sys.meta_path):
    sys.meta_path.insert(0, _KernelFinder())

for module_name in _stale_modules:
    sys.modules.pop(module_name, None)

if {use_module_runner}:
    runpy.run_module(_entry_module, run_name='__main__')
else:
    runpy.run_path(_entry_path, run_name='__main__')
"#,
        manifest_json = manifest_json,
        base_dir = base_dir,
        entry_path = entry_path,
        entry_module = entry_module,
        profile_name = profile_name,
        stale_modules_json = stale_modules_json,
        use_module_runner = if use_module_runner { "True" } else { "False" },
    )
}
