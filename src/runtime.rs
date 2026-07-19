use crate::worker::JsWorker;
use anyhow::{anyhow, Result};
use dashmap::DashMap;
use oxc_allocator::Allocator;
use oxc_codegen::Codegen;
use oxc_parser::{Parser, ParserReturn};
use oxc_semantic::SemanticBuilder;
use oxc_span::SourceType;
use oxc_transformer::{
    ClassPropertiesOptions, CompilerAssumptions, DecoratorOptions, ES2022Options, ES2026Options,
    EnvOptions, HelperLoaderOptions, JsxOptions, Module, ProposalOptions, TransformOptions,
    Transformer, TypeScriptOptions,
};
use std::{
    path::Path,
    sync::Arc,
};
use tokio::fs;

pub struct FunctionsRuntime {
    workers: Arc<DashMap<String, Arc<JsWorker>>>,
}

impl FunctionsRuntime {
    pub fn new() -> Self {
        Self {
            workers: Arc::new(DashMap::new()),
        }
    }

    pub async fn load_directory(&self, path: impl AsRef<Path>) -> Result<()> {
        let root = path.as_ref();
        let mut stack = vec![root.to_path_buf()];

        while let Some(current) = stack.pop() {
            let mut entries = match fs::read_dir(&current).await {
                Ok(entries) => entries,
                Err(error) if current == root => return Err(error.into()),
                Err(_) => continue,
            };

            while let Some(entry) = entries.next_entry().await? {
                let path = entry.path();
                let file_type = entry.file_type().await?;

                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }

                if file_type.is_file() && is_supported_script_file(&path) {
                    let name = script_name(&path)?;
                    let code = fs::read_to_string(&path).await?;
                    self.deploy(&name, &code, &path).await?;
                }
            }
        }

        Ok(())
    }

    pub async fn deploy(&self, name: &str, source: &str, path: &Path) -> Result<()> {
        let compiled = compile_source(path, source)?;
        let worker = Arc::new(JsWorker::new(compiled).await?);
        self.workers.insert(name.to_string(), worker);
        Ok(())
    }

    pub async fn execute(&self, name: &str, request: String) -> Result<String> {
        let worker = self
            .workers
            .get(name)
            .ok_or_else(|| anyhow!("function not found"))?;

        worker.execute(request).await
    }
}

fn is_supported_script_file(path: &Path) -> bool {
    let file_name = match path.file_name().and_then(|value| value.to_str()) {
        Some(value) => value.to_ascii_lowercase(),
        None => return false,
    };

    if file_name.ends_with(".d.ts")
        || file_name.ends_with(".d.tsx")
        || file_name.ends_with(".d.mts")
        || file_name.ends_with(".d.cts")
    {
        return false;
    }

    matches!(
        path.extension().and_then(|value| value.to_str()),
        Some("js" | "mjs" | "cjs" | "ts" | "mts" | "cts")
    )
}

fn script_name(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .ok_or_else(|| anyhow!("invalid file name: {}", path.display()))?
        .to_string_lossy()
        .to_string();

    Ok(stem)
}

fn compile_source(path: &Path, source: &str) -> Result<String> {
    let allocator = Allocator::default();
    let source_type = SourceType::from_path(path).unwrap_or_default();

    let ParserReturn {
        mut program,
        diagnostics,
        ..
    } = Parser::new(&allocator, source, source_type).parse();

    if !diagnostics.is_empty() {
        return Err(anyhow!(
            "failed to parse {}: {diagnostics:?}",
            path.display()
        ));
    }

    let scoping = SemanticBuilder::new().build(&program).semantic.into_scoping();

    let transformer = Transformer::new(
        &allocator,
        path,
        &TransformOptions {
            assumptions: CompilerAssumptions {
                set_public_class_fields: false,
                ..Default::default()
            },
            decorator: DecoratorOptions::default(),
            jsx: JsxOptions::default(),
            typescript: TypeScriptOptions {
                only_remove_type_imports: false,
                ..Default::default()
            },
            env: EnvOptions {
                module: Module::Preserve,
                es2022: ES2022Options {
                    class_static_block: true,
                    class_properties: Some(ClassPropertiesOptions { loose: false }),
                    top_level_await: false,
                    ..Default::default()
                },
                es2026: ES2026Options {
                    explicit_resource_management: true,
                },
                ..Default::default()
            },
            proposals: ProposalOptions::default(),
            helper_loader: HelperLoaderOptions::default(),
            ..Default::default()
        },
    );

    let oxc_transformer::TransformerReturn { diagnostics, .. } =
        transformer.build_with_scoping(scoping, &mut program);

    if !diagnostics.is_empty() {
        return Err(anyhow!(
            "failed to transform {}: {diagnostics:?}",
            path.display()
        ));
    }

    let code = Codegen::new().build(&program).code;
    Ok(code)
}