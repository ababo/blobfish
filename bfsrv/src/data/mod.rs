pub mod campaign;
pub mod capability;
pub mod node;
pub mod payment;
pub mod token;
pub mod user;

use axum::http::StatusCode;

/// Data error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("lettre")]
    Lettre(
        #[from]
        #[source]
        lettre::error::Error,
    ),
    #[error("lettre_address")]
    LettreAddress(
        #[from]
        #[source]
        lettre::address::AddressError,
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
            Lettre(_) | LettreAddress(_) | Postgres(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Kind code.
    pub fn code(&self) -> &str {
        use Error::*;
        match self {
            Lettre(_) => "lettre",
            LettreAddress(_) => "lettre_address",
            Postgres(_) => "postgres",
        }
    }
}

/// Data result.
pub type Result<T> = std::result::Result<T, Error>;
