mod middleware;
mod payment;
mod token;
mod transcribe;
mod user;

use crate::{
    currency_converter::CurrencyConverter,
    infsrv_pool::{self, InfsrvPool},
    mailer::Mailer,
    paypal::PaypalProcessor,
    util::fmt::ErrorChainDisplay,
};
use axum::{
    extract::rejection,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, patch, post},
    Json, Router,
};
use deadpool_postgres::Pool as PgPool;
use log::{debug, error, info};
use serde_json::json;
use std::{future::Future, net::SocketAddr, sync::Arc};

/// Server error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("web server error")]
    Axum(
        #[from]
        #[source]
        axum::Error,
    ),
    #[error("malformed JSON payload")]
    AxumJsonRejection(
        #[from]
        #[source]
        rejection::JsonRejection,
    ),
    #[error("malformed URL query")]
    AxumQueryRejection(
        #[from]
        #[source]
        rejection::QueryRejection,
    ),
    #[error("bad request ({0})")]
    BadRequest(String),
    #[error("promotion campaign not found")]
    CampaignNotFound,
    #[error("failed to convert currency")]
    CurrencyConverter(
        #[from]
        #[source]
        crate::currency_converter::Error,
    ),
    #[error("database error")]
    Data(
        #[from]
        #[source]
        crate::data::Error,
    ),
    #[error("database connection error")]
    DeadpoolPool(
        #[from]
        #[source]
        deadpool_postgres::PoolError,
    ),
    #[error("user with email already registered")]
    EmailAlreadyRegistered,
    #[error("endpoint not found")]
    HandlerNotFound,
    #[error("worker node error")]
    InfsrvPool(
        #[from]
        #[source]
        infsrv_pool::Error,
    ),
    #[error("internal error ({0})")]
    Internal(String),
    #[error("io")]
    Io(
        #[from]
        #[source]
        std::io::Error,
    ),
    #[error("mailing error")]
    Mailer(
        #[from]
        #[source]
        crate::mailer::Error,
    ),
    #[error("payment not found")]
    PaymentNotFound,
    #[error("paypal")]
    Paypal(
        #[from]
        #[source]
        crate::paypal::Error,
    ),
    #[error("database query error")]
    Postgres(
        #[from]
        #[source]
        tokio_postgres::Error,
    ),
    #[error("unauthorized access ({0})")]
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
            AxumJsonRejection(_)
            | AxumQueryRejection(_)
            | BadRequest(_)
            | CampaignNotFound
            | EmailAlreadyRegistered => StatusCode::BAD_REQUEST,
            CurrencyConverter(err) => err.status(),
            Data(err) => err.status(),
            HandlerNotFound | PaymentNotFound => StatusCode::NOT_FOUND,
            InfsrvPool(err) => err.status(),
            Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Mailer(err) => err.status(),
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
            CampaignNotFound => "campaign_not_found",
            CurrencyConverter(err) => err.code(),
            Data(err) => err.code(),
            DeadpoolPool(_) => "deadpool_pool",
            EmailAlreadyRegistered => "email_already_registered",
            HandlerNotFound => "handler_not_found",
            InfsrvPool(err) => err.code(),
            Internal(_) => "internal",
            Io(_) => "io",
            Mailer(err) => err.code(),
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
                error!("failed to serve HTTP request: {}", ErrorChainDisplay(&self));
            }
            _ => {
                debug!("failed to serve HTTP request: {}", ErrorChainDisplay(&self));
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
    pg_pool: PgPool,
    infsrv_pool: InfsrvPool,
    currency_converter: CurrencyConverter,
    paypal: PaypalProcessor,
    mailer: Mailer,
}

impl Server {
    /// Create a new Server instance.
    pub fn new(
        pg_pool: PgPool,
        infsrv_pool: InfsrvPool,
        currency_converter: CurrencyConverter,
        paypal: PaypalProcessor,
        mailer: Mailer,
    ) -> Self {
        Self {
            pg_pool,
            infsrv_pool,
            currency_converter,
            paypal,
            mailer,
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
            .route("/token", post(token::handle_token_post))
            .route("/transcribe", get(transcribe::handle_transcribe))
            .route("/user", post(user::handle_user_post))
            .fallback(handle_fallback)
            .with_state(self)
            .into_make_service_with_connect_info::<SocketAddr>();

        info!("started HTTP/WS server");

        let listener = tokio::net::TcpListener::bind(address).await?;

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal)
            .await
            .map_err(Into::into)
    }
}
