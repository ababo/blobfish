pub mod capability;
pub mod node;
pub mod payment;
pub mod token;
pub mod user;

use axum::http::StatusCode;

/// Data error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("email address")]
    EmailAddress(
        #[from]
        #[source]
        email_address::Error,
    ),
    #[error("postgres")]
    Postgres(
        #[from]
        #[source]
        tokio_postgres::Error,
    ),
}

impl Error {
    /// HTTP status code.
    pub fn status(&self) -> StatusCode {
        use Error::*;
        match self {
            EmailAddress(_) | Postgres(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            EmailAddress(_) => "email_address",
            Postgres(_) => "postgres",
        }
    }
}

/// Data result.
pub type Result<T> = std::result::Result<T, Error>;
