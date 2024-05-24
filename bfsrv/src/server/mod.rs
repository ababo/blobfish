mod middleware;
mod transcribe;

use crate::{
    config::Config,
    infsrv_pool::{self, InfsrvPool},
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use deadpool_postgres::Pool;
use log::{debug, error, info};
use std::{future::Future, net::SocketAddr, sync::Arc};

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("axum: {0}")]
    Axum(#[from] axum::Error),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("data: {0}")]
    Data(#[from] crate::data::Error),
    #[error("deadpool pool: {0}")]
    DeadpoolPool(#[from] deadpool_postgres::PoolError),
    #[error("infsrv pool: {0}")]
    InfsrvPool(#[from] infsrv_pool::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        use Error::*;
        let status = match &self {
            Axum(_) | Data(_) | DeadpoolPool(_) => StatusCode::INTERNAL_SERVER_ERROR,
            BadRequest(_) => StatusCode::BAD_REQUEST,
            InfsrvPool(err) => {
                use infsrv_pool::Error::*;
                match err {
                    Internal | Reqwest(_) | SerdeJson(_) | Tungstanite(_) => {
                        StatusCode::INTERNAL_SERVER_ERROR
                    }
                    Ledger(err) => {
                        if err.is_internal() {
                            StatusCode::INTERNAL_SERVER_ERROR
                        } else {
                            StatusCode::PAYMENT_REQUIRED
                        }
                    }
                }
            }
            Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Unauthorized(_) => StatusCode::UNAUTHORIZED,
        };

        if status == StatusCode::INTERNAL_SERVER_ERROR {
            error!("failed to serve HTTP request: {self:#}");
        } else {
            debug!("failed to serve HTTP request: {self:#}");
        }

        // TODO: Support error codes.
        (status, self.to_string()).into_response()
    }
}

/// Server result.
pub type Result<T> = std::result::Result<T, Error>;

/// HTTP/WS server for Handler.
pub struct Server {
    _config: Arc<Config>,
    pool: Pool,
    infsrv_pool: InfsrvPool,
}

impl Server {
    /// Create a new Server instance.
    pub fn new(_config: Arc<Config>, pool: Pool, infsrv_pool: InfsrvPool) -> Self {
        Self {
            _config,
            pool,
            infsrv_pool,
        }
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
