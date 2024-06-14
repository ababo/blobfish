mod config;
mod currency_converter;
mod data;
mod infsrv_pool;
mod ledger;
mod mailer;
mod paypal;
mod server;
mod util;

use crate::{config::Config, ledger::Ledger};
use clap::Parser;
use currency_converter::CurrencyConverter;
use data::{node::Node, user::User};
use deadpool_postgres::{Config as DeadpoolClient, ManagerConfig, Pool, RecyclingMethod, Runtime};
use infsrv_pool::InfsrvPool;
use mailer::Mailer;
use paypal::PaypalProcessor;
use server::Server;
use std::{future::Future, sync::Arc};
use tokio_postgres::NoTls;
use util::fmt::ErrorChainDisplay;

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("data")]
    Data(
        #[from]
        #[source]
        crate::data::Error,
    ),
    #[error("deadpool pool")]
    DeadpoolPool(
        #[from]
        #[source]
        deadpool_postgres::PoolError,
    ),
}

type Result<T> = std::result::Result<T, Error>;

#[tokio::main]
async fn main() {
    let config = Config::parse();
    if let Err(err) = run(config).await {
        eprintln!("exited with error: {}", ErrorChainDisplay(&err));
    }
}

async fn run(config: Config) -> Result<()> {
    env_logger::builder().format_timestamp_millis().init();

    let pg_pool = create_pg_pool(&config).await?;
    let ledger = Ledger::new(pg_pool.clone());
    let infsrv_pool = InfsrvPool::new(ledger);
    let currency_converter = CurrencyConverter::new(config.currency.clone());
    let paypal = new_paypal(&config);
    let mailer = Mailer::new(&config);

    let server = Arc::new(Server::new(
        config,
        pg_pool,
        infsrv_pool,
        currency_converter,
        paypal,
        mailer,
    ));
    let server_handle = tokio::spawn(async move {
        server
            .serve(shutdown_signal())
            .await
            .expect("failed to serve HTTP/WS requests")
    });

    let (server_result,) = tokio::join!(server_handle);
    server_result.expect("failed to join HTTP/WS server");

    Ok(())
}

async fn create_pg_pool(config: &Config) -> Result<Pool> {
    let mut deadpool_config = DeadpoolClient::new();
    deadpool_config.url = Some(config.database_url.to_string());
    deadpool_config.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    let pool = deadpool_config
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .unwrap();

    let client = pool.get().await?;
    Node::clear_loads(&client).await?;
    User::clear_allocated_fees(&client).await?;

    Ok(pool)
}

fn new_paypal(config: &Config) -> PaypalProcessor {
    PaypalProcessor::new(
        config.paypal_sandbox,
        config.paypal_client_id.clone(),
        config.paypal_secret_key.clone(),
        config.paypal_return_url.clone(),
        config.paypal_cancel_url.clone(),
    )
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
