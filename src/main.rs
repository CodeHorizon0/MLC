mod runtime;
mod server;
mod worker;

use runtime::FunctionsRuntime;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let runtime = FunctionsRuntime::new();
    runtime.load_directory(PathBuf::from("./functions")).await?;
    server::start(runtime).await?;
    Ok(())
}
