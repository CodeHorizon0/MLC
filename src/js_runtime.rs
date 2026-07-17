
use std::collections::BTreeSet;
use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use rquickjs::{Context, Object, Runtime, Function};

use crate::config::RunReport;
use crate::error::{KernelError, KernelResult};
use crate::graph::DependencyGraph;
use crate::util::utc_now_rfc3339_nanos;

#[derive(Debug)]
pub struct JsTask {
    pub graph: DependencyGraph,
    pub reply: std::sync::mpsc::Sender<KernelResult<RunReport>>,
}

pub fn spawn_js_worker() -> KernelResult<SyncSender<JsTask>> {
    let (tx, rx) = std::sync::mpsc::sync_channel::<JsTask>(64);

    thread::Builder::new()
        .name("kernel-js-worker".into())
        .spawn(move || {
            if let Err(err) = js_worker_loop(rx) {
                eprintln!("js worker stopped: {err}");
            }
        })
        .map_err(|err| KernelError::WorkerInit(err.to_string()))?;

    Ok(tx)
}

fn js_worker_loop(rx: Receiver<JsTask>) -> KernelResult<()> {
    let runtime = Runtime::new().map_err(|e| KernelError::Js(e.to_string()))?;
    let context = Context::full(&runtime).map_err(|e| KernelError::Js(e.to_string()))?;
    install_js_runtime(&context)?;

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

        let result = execute_js_task(&context, graph, &stale_modules);

        let finished_wall = utc_now_rfc3339_nanos();
        match &result {
            Ok(report) => eprintln!(
                "[kernel:js] finished {} -> {} in {:?} for {:?}",
                started_wall,
                finished_wall,
                started.elapsed(),
                report.entry_path
            ),
            Err(err) => eprintln!(
                "[kernel:js] failed {} -> {} after {:?}: {}",
                started_wall,
                finished_wall,
                started.elapsed(),
                err
            ),
        }

        previous_modules = current_modules;
        let _ = task.reply.send(result);
    }

    Ok(())
}

fn install_js_runtime(context: &Context) -> KernelResult<()> {
    context.with(|ctx| {
        let console = Object::new(ctx.clone())
            .map_err(|e| KernelError::Js(e.to_string()))?;

        let log = Function::new(ctx.clone(), |message: String| -> Result<(), rquickjs::Error> {
            println!("{}", message);
            Ok(())
        }).map_err(|e| KernelError::Js(e.to_string()))?;

        console.set("log", log)
            .map_err(|e| KernelError::Js(e.to_string()))?;

        ctx.globals()
            .set("console", console)
            .map_err(|e| KernelError::Js(e.to_string()))?;

        Ok(())
    })
}
fn execute_js_task(
    context: &Context,
    graph: DependencyGraph,
    _stale_modules: &[String],
) -> KernelResult<RunReport> {
    let source = std::fs::read_to_string(&graph.entry_path)?;
    let discovered_modules = graph.discovered_module_names();

    context.with(|ctx| {
        ctx.eval::<(), _>(source)
            .map_err(|e| KernelError::Js(e.to_string()))
    })?;

    Ok(RunReport {
        language: graph.language,
        entry_path: graph.entry_path,
        discovered_modules,
    })
}
