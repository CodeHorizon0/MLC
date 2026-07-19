use crate::runtime::FunctionsRuntime;
use axum::{
    body::Body,
    extract::{Path, State},
    response::Response,
    routing::post,
    Router,
};
use std::sync::Arc;
use tower_http::cors::CorsLayer;

pub async fn start(runtime: FunctionsRuntime) -> anyhow::Result<()> {
    let runtime = Arc::new(runtime);

    let app = Router::new()
        .route("/functions/{name}", post(handler).get(browser_handler))
        .layer(CorsLayer::permissive())
        .with_state(runtime);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;

    println!("Functions server: http://127.0.0.1:8080");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn handler(
    State(runtime): State<Arc<FunctionsRuntime>>,
    Path(name): Path<String>,
    body: String,
) -> Response<Body> {
    match runtime.execute(&name, body).await {
        Ok(value) => Response::new(Body::from(value)),
        Err(error) => Response::builder()
            .status(500)
            .body(Body::from(error.to_string()))
            .unwrap(),
    }
}

async fn browser_handler(
    State(runtime): State<Arc<FunctionsRuntime>>,
    Path(name): Path<String>,
) -> Response<Body> {
    match runtime.execute(&name, "{}".to_string()).await {
        Ok(value) => Response::new(Body::from(value)),
        Err(error) => Response::builder()
            .status(500)
            .body(Body::from(error.to_string()))
            .unwrap(),
    }
}
