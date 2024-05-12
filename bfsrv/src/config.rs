use std::net::SocketAddr;

use clap::Parser;

/// Service configuration.
#[derive(Parser)]
pub struct Config {
    #[clap(long, env = "SERVER_ADDRESS", default_value = "127.0.0.1:9321")]
    pub server_address: SocketAddr,
}
