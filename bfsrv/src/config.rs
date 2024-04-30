use std::net::SocketAddr;

use clap::Parser;
use url::Url;

/// Service configuration.
#[derive(Parser)]
pub struct Config {
    #[clap(long, env = "INFSRV_URL")]
    pub infsrv_url: Url,
    #[clap(long, env = "SERVER_ADDRESS")]
    pub server_address: SocketAddr,
}
