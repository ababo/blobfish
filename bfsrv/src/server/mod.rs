mod middleware;
mod payment;
mod transcribe;

use crate::{
    config::Config,
    currency_converter::CurrencyConverter,
    infsrv_pool::{self, InfsrvPool},
    paypal::PaypalProcessor,
};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
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
    #[error("currency converter: {0}")]
    CurrencyConverter(#[from] crate::currency_converter::Error),
    #[error("data: {0}")]
    Data(#[from] crate::data::Error),
    #[error("deadpool pool: {0}")]
    DeadpoolPool(#[from] deadpool_postgres::PoolError),
    #[error("infsrv pool: {0}")]
    InfsrvPool(#[from] infsrv_pool::Error),
    #[error("internal: {0}")]
    Internal(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("payment not found")]
    PaymentNotFound,
    #[error("paypal: {0}")]
    Paypal(#[from] crate::paypal::Error),
    #[error("postgres: {0}")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        use Error::*;
        let status = match &self {
            Axum(_) | CurrencyConverter(_) | Data(_) | DeadpoolPool(_) | Internal(_)
            | Postgres(_) => StatusCode::INTERNAL_SERVER_ERROR,
            BadRequest(_) | PaymentNotFound => StatusCode::BAD_REQUEST,
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
            Paypal(err) => {
                if err.is_internal() {
                    StatusCode::INTERNAL_SERVER_ERROR
                } else {
                    StatusCode::BAD_REQUEST
                }
            }
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
    currency_converter: CurrencyConverter,
    paypal: PaypalProcessor,
}

impl Server {
    /// Create a new Server instance.
    pub fn new(config: Arc<Config>, pool: Pool, infsrv_pool: InfsrvPool) -> Self {
        let currency_converter = CurrencyConverter::new(config.currency.clone());
        let paypal = PaypalProcessor::new(
            config.paypal_sandbox,
            config.paypal_client_id.clone(),
            config.paypal_secret_key.clone(),
            config.paypal_return_url.clone(),
            config.paypal_cancel_url.clone(),
        );
        Self {
            _config: config,
            pool,
            infsrv_pool,
            currency_converter,
            paypal,
        }
    }

    /// Serve HTTP/WS requests with graceful shutdown on a given signal.
    pub async fn serve<F>(self: Arc<Self>, address: &SocketAddr, shutdown_signal: F) -> Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let app = Router::<Arc<Server>>::new()
            .route("/payment", patch(payment::handle_payment_patch))
            .route("/payment", post(payment::handle_payment_post))
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
