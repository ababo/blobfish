mod config;
mod data;
mod infsrv_pool;
mod ledger;
mod server;
mod util;

use crate::{config::Config, ledger::Ledger};
use clap::Parser;
use deadpool_postgres::{Config as DeadpoolClient, ManagerConfig, RecyclingMethod, Runtime};
use infsrv_pool::InfsrvPool;
use server::Server;
use std::{future::Future, sync::Arc};
use tokio_postgres::NoTls;

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
    env_logger::builder().format_timestamp_millis().init();

    let mut deadpool_config = DeadpoolClient::new();
    deadpool_config.url = Some(config.database_url.to_string());
    deadpool_config.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    let pool = deadpool_config
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .unwrap();

    let ledger = Ledger::new(pool.clone());
    let server = Arc::new(Server::new(config.clone(), pool, InfsrvPool::new(ledger)));
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
