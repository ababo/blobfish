mod config;
mod server;

use crate::config::Config;
use clap::Parser;
use server::Server;
use std::{future::Future, sync::Arc};

#[derive(Debug, thiserror::Error)]
enum Error {}

type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    let config = Arc::new(Config::parse());
    if let Err(err) = run(config).await {
        eprintln!("exited with error: {err:#}");
    }
}

async fn run(config: Arc<Config>) -> Result<()> {
    let server = Arc::new(Server::new());
    let server_handle = tokio::spawn(async move {
        server
            .serve(&config.server_address, shutdown_signal())
            .await
            .expect("failed to serve HTTP/WS requests")
    });

    let (server_result,) = tokio::join!(server_handle);
    server_result.expect("failed to join HTTP/WS server");

    Ok(())
}

fn shutdown_signal() -> impl Future<Output = ()> + Unpin {
    Box::pin(async move {
        use tokio::signal;
        let ctrl_c = async { signal::ctrl_c().await.unwrap() };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .unwrap()
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }
    })
}
