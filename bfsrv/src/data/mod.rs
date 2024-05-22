pub mod capability;
pub mod node;
pub mod user;

/// Data error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("postgres: {0}")]
    Postgres(#[from] tokio_postgres::Error),
}

/// Data result.
pub type Result<T> = std::result::Result<T, Error>;
