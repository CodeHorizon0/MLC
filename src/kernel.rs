use std::sync::mpsc::{self, SyncSender};

use crate::config::{KernelConfig, Language, RunReport, RunRequest, ScriptInput};
use crate::config::ScriptInputSource;
use crate::error::{KernelError, KernelResult};
use crate::graph::{build_graph, DependencyGraph};
use crate::lua_runtime::{spawn_lua_worker, LuaTask};
use crate::js_runtime::{spawn_js_worker, JsTask};
use crate::python_runtime::{spawn_python_worker, PythonTask};

pub struct Kernel {
    config: KernelConfig,
    python_tx: SyncSender<PythonTask>,
    lua_tx: SyncSender<LuaTask>,
    js_tx: SyncSender<JsTask>,
}

impl Kernel {
    pub fn new(config: KernelConfig) -> KernelResult<Self> {
        // Workers are persistent to avoid runtime startup cost on repeated runs.
        let python_tx = if config.python_enabled {
            spawn_python_worker()?
        } else {
            return Err(KernelError::WorkerInit("python runtime disabled".into()));
        };

        let lua_tx = if config.lua_enabled {
            spawn_lua_worker()?
        } else {
            return Err(KernelError::WorkerInit("lua runtime disabled".into()));
        };

        let js_tx = if config.js_enabled {
            spawn_js_worker()?
        } else {
            return Err(KernelError::WorkerInit("js runtime disabled".into()));
        };

        Ok(Self {
            config,
            python_tx,
            lua_tx,
            js_tx,
        })
    }

    pub fn config(&self) -> &KernelConfig {
        &self.config
    }

    pub fn run_python<I>(&self, input: I) -> KernelResult<RunReport>
    where
        I: ScriptInputSource,
    {
        let request = RunRequest {
            language: Language::Python,
            input: input.into_script_input(&self.config.base_dir),
            working_dir: None,
        };
        self.run(request)
    }

    pub fn run_lua<I>(&self, input: I) -> KernelResult<RunReport>
    where
        I: ScriptInputSource,
    {
        let request = RunRequest {
            language: Language::Lua,
            input: input.into_script_input(&self.config.base_dir),
            working_dir: None,
        };
        self.run(request)
    }

    pub fn run_js<I>(&self, input: I) -> KernelResult<RunReport>
    where
        I: ScriptInputSource,
    {
        self.run(RunRequest { language: Language::Js, input: input.into_script_input(&self.config.base_dir), working_dir: None })
    }

    pub fn run(&self, request: RunRequest) -> KernelResult<RunReport> {
        let base_dir = request.working_dir.as_deref().unwrap_or(&self.config.base_dir);
        let graph = build_graph(request.language, request.input, base_dir)?;
        match request.language {
            Language::Python => self.dispatch_python(graph),
            Language::Lua => self.dispatch_lua(graph),
            Language::Js => self.dispatch_js(graph),
        }
    }

    pub fn run_python_input(&self, input: ScriptInput) -> KernelResult<RunReport> {
        self.run(RunRequest {
            language: Language::Python,
            input,
            working_dir: None,
        })
    }

    pub fn run_lua_input(&self, input: ScriptInput) -> KernelResult<RunReport> {
        self.run(RunRequest {
            language: Language::Lua,
            input,
            working_dir: None,
        })
    }

    fn dispatch_python(&self, graph: DependencyGraph) -> KernelResult<RunReport> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.python_tx
            .send(PythonTask { graph, reply: reply_tx })
            .map_err(|_| KernelError::WorkerClosed)?;
        reply_rx.recv().map_err(|_| KernelError::WorkerClosed)?
    }

    fn dispatch_lua(&self, graph: DependencyGraph) -> KernelResult<RunReport> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.lua_tx
            .send(LuaTask { graph, reply: reply_tx })
            .map_err(|_| KernelError::WorkerClosed)?;
        reply_rx.recv().map_err(|_| KernelError::WorkerClosed)?
    }

    fn dispatch_js(&self, graph: DependencyGraph) -> KernelResult<RunReport> {
        let (reply_tx, reply_rx) = mpsc::channel();
        self.js_tx
        .send(JsTask { graph, reply: reply_tx })
        .map_err(|_| KernelError::WorkerClosed)?;
    reply_rx.recv().map_err(|_| KernelError::WorkerClosed)?
}
}