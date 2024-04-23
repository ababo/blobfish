use std::net::SocketAddr;

use clap::Parser;

/// Service configuration.
#[derive(Parser)]
pub struct Config {
    #[clap(long, env = "SERVER_ADDRESS")]
    pub server_address: SocketAddr,
}
