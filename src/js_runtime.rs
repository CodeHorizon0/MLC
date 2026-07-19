use std::sync::mpsc::{Receiver, SyncSender};
use std::thread;
use std::time::Instant;

use rquickjs::{Context, Runtime};

use crate::config::RunReport;
use crate::error::{KernelError, KernelResult};
use crate::graph::DependencyGraph;
use crate::util::utc_now_rfc3339_nanos;

pub struct JsTask {
    pub graph: DependencyGraph,
    pub reply: std::sync::mpsc::Sender<KernelResult<RunReport>>,
}

pub fn spawn_js_worker() -> KernelResult<SyncSender<JsTask>> {
    let (tx, rx) = std::sync::mpsc::sync_channel(64);
    thread::Builder::new()
        .name("kernel-js-worker".into())
        .spawn(move || {
            if let Err(err) = js_worker_loop(rx) {
                eprintln!("js worker stopped: {err}");
            }
        })
        .map_err(|e| KernelError::WorkerInit(e.to_string()))?;
    Ok(tx)
}

fn js_worker_loop(rx: Receiver<JsTask>) -> KernelResult<()> {
    let runtime = Runtime::new().map_err(|e| KernelError::Js(e.to_string()))?;
    let context = Context::full(&runtime).map_err(|e| KernelError::Js(e.to_string()))?;

    for task in rx {
        let started = Instant::now();
        let result = execute_js_task(&context, task.graph);
        eprintln!("[kernel:js] finished {} {:?}", utc_now_rfc3339_nanos(), started.elapsed());
        let _ = task.reply.send(result);
    }
    Ok(())
}

fn execute_js_task(context: &Context, graph: DependencyGraph) -> KernelResult<RunReport> {
    let source = std::fs::read_to_string(&graph.entry_path)?;
    let discovered_modules = graph.discovered_module_names();
    context.with(|ctx| {
        ctx.eval::<(), _>(source)
            .map_err(|e| KernelError::Js(e.to_string()))
    })?;

    Ok(RunReport {
        language: graph.language,
        entry_path: graph.entry_path,
        discovered_modules: discovered_modules,
    })
}
