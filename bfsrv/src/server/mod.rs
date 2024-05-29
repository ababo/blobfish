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
    extract::rejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use deadpool_postgres::Pool;
use log::{debug, error, info};
use serde_json::json;
use std::{future::Future, net::SocketAddr, sync::Arc};

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("axum: {0}")]
    Axum(#[from] axum::Error),
    #[error("axum json rejection: {0}")]
    AxumJsonRejection(#[from] rejection::JsonRejection),
    #[error("axum query rejection: {0}")]
    AxumQueryRejection(#[from] rejection::QueryRejection),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("currency converter: {0}")]
    CurrencyConverter(#[from] crate::currency_converter::Error),
    #[error("data: {0}")]
    Data(#[from] crate::data::Error),
    #[error("deadpool pool: {0}")]
    DeadpoolPool(#[from] deadpool_postgres::PoolError),
    #[error("handler not found")]
    HandlerNotFound,
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

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match &self {
            Axum(_) | DeadpoolPool(_) | Internal(_) | Postgres(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            AxumJsonRejection(_) | AxumQueryRejection(_) | BadRequest(_) => StatusCode::BAD_REQUEST,
            CurrencyConverter(err) => err.status(),
            Data(err) => err.status(),
            HandlerNotFound | PaymentNotFound => StatusCode::NOT_FOUND,
            InfsrvPool(err) => err.status(),
            Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Paypal(err) => err.status(),
            Unauthorized(_) => StatusCode::UNAUTHORIZED,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match &self {
            Axum(_) => "axum",
            AxumJsonRejection(_) => "axum_json_rejection",
            AxumQueryRejection(_) => "axum_query_rejection",
            BadRequest(_) => "bad_request",
            CurrencyConverter(err) => err.code(),
            Data(err) => err.code(),
            DeadpoolPool(_) => "deadpool_pool",
            HandlerNotFound => "handler_not_found",
            InfsrvPool(err) => err.code(),
            Internal(_) => "internal",
            Io(_) => "io",
            PaymentNotFound => "payment_not_found",
            Paypal(err) => err.code(),
            Postgres(_) => "postgres",
            Unauthorized(_) => "unauthorized",
        }
    }
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = self.status();
        match status {
            StatusCode::INTERNAL_SERVER_ERROR | StatusCode::TOO_MANY_REQUESTS => {
                error!("failed to serve HTTP request: {self:#}");
            }
            _ => {
                debug!("failed to serve HTTP request: {self:#}");
            }
        }

        let response = json!({
            "error": {
                "code": self.code(),
                "message": self.to_string()
            }
        });
        (status, Json(response)).into_response()
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
        async fn handle_fallback() -> Result<Response> {
            Err(Error::HandlerNotFound)
        }

        let app = Router::<Arc<Server>>::new()
            .route("/payment", patch(payment::handle_payment_patch))
            .route("/payment", post(payment::handle_payment_post))
            .route("/transcribe", get(transcribe::handle_transcribe))
            .fallback(handle_fallback)
            .with_state(self);

        info!("started HTTP/WS server");

        let listener = tokio::net::TcpListener::bind(address).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Into::into)
    }
}
