use anyhow::{anyhow, Result};
use rquickjs::{AsyncContext, AsyncRuntime, Function, Value};
use std::sync::mpsc;
use tokio::sync::oneshot;

type Request = (String, oneshot::Sender<Result<String>>);

pub struct JsWorker {
    sender: mpsc::Sender<Request>,
}

impl JsWorker {
    pub async fn new(source: String) -> Result<Self> {
        let (sender, receiver) = mpsc::channel::<Request>();
        let (ready_tx, ready_rx) = oneshot::channel::<Result<()>>();

        std::thread::spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(value) => value,
                Err(error) => {
                    let _ = ready_tx.send(Err(anyhow!(error)));
                    return;
                }
            };

            runtime.block_on(async move {
                let quick_runtime = match AsyncRuntime::new() {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_tx.send(Err(anyhow!(error)));
                        return;
                    }
                };

                let context = match AsyncContext::full(&quick_runtime).await {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready_tx.send(Err(anyhow!(error)));
                        return;
                    }
                };

                let init_result = context
                    .with(|ctx| ctx.eval::<(), _>(source.as_str()))
                    .await
                    .map_err(anyhow::Error::from);

                if let Err(error) = init_result {
                    let _ = ready_tx.send(Err(error));
                    return;
                }

                let _ = ready_tx.send(Ok(()));

                while let Ok((request, response)) = receiver.recv() {
                    let result = execute_handler(&context, request).await;
                    let _ = response.send(result);
                }
            });
        });

        ready_rx
            .await
            .map_err(|_| anyhow!("worker initialization channel closed"))??;

        Ok(Self { sender })
    }

    pub async fn execute(&self, request: String) -> Result<String> {
        let (response_tx, response_rx) = oneshot::channel();

        self.sender
            .send((request, response_tx))
            .map_err(|_| anyhow!("worker is not available"))?;

        response_rx
            .await
            .map_err(|_| anyhow!("worker response channel closed"))?
    }
}

async fn execute_handler(context: &AsyncContext, request: String) -> Result<String> {
    let response: Result<String, rquickjs::Error> = context
        .with(|ctx| {
            let handler: Function = ctx.globals().get("handler")?;
            let value: Value = handler.call((request,))?;

            match value.as_string() {
                Some(value) => Ok(value.to_string()?),
                None => Ok(String::new()),
            }
        })
        .await;

    response.map_err(anyhow::Error::from)
}
