use clap::Parser;
use std::net::SocketAddr;
use url::Url;

/// Service configuration.
#[derive(Parser)]
pub struct Config {
    #[clap(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://127.0.0.1/blobfish"
    )]
    pub database_url: Url,
    #[clap(long, env = "SERVER_ADDRESS", default_value = "127.0.0.1:9321")]
    pub server_address: SocketAddr,
}
