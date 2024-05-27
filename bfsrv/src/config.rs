use clap::Parser;
use std::net::SocketAddr;
use url::Url;

/// Service configuration.
#[derive(Parser)]
pub struct Config {
    #[clap(long, env = "CURRENCY", default_value = "USD")]
    pub currency: String,
    #[clap(
        long,
        env = "DATABASE_URL",
        default_value = "postgres://127.0.0.1/blobfish"
    )]
    pub database_url: Url,
    #[clap(long, env = "SERVER_ADDRESS", default_value = "127.0.0.1:9321")]
    pub server_address: SocketAddr,
    #[clap(long, env = "PAYPAL_CANCEL_URL")]
    pub paypal_cancel_url: Url,
    #[clap(long, env = "PAYPAL_CLIENT_ID")]
    pub paypal_client_id: String,
    #[clap(long, env = "PAYPAL_RETURN_URL")]
    pub paypal_return_url: Url,
    #[clap(long, env = "PAYPAL_SANDBOX", default_value = "true")]
    pub paypal_sandbox: bool,
    #[clap(long, env = "PAYPAL_SECRET_KEY")]
    pub paypal_secret_key: String,
}
