mod middleware;
mod transcribe;

use axum::{routing::get, Router};
use log::info;
use std::{future::Future, net::SocketAddr, sync::Arc};

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("axum: {0}")]
    Axum(#[from] axum::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

/// Server result.
pub type Result<T> = std::result::Result<T, Error>;

/// HTTP/WS server for Handler.
pub struct Server {}

impl Server {
    /// Create a new Server instance.
    pub fn new() -> Server {
        Server {}
    }

    /// Serve HTTP/WS requests with graceful shutdown on a given signal.
    pub async fn serve<F>(self: Arc<Self>, address: &SocketAddr, shutdown_signal: F) -> Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let app = Router::<Arc<Server>>::new()
            .route("/transcribe", get(transcribe::handle_transcribe))
            .with_state(self);

        info!("started HTTP/WS server");

        let listener = tokio::net::TcpListener::bind(address).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Into::into)
    }
}
