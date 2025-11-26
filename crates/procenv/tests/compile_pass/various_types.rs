//! Test: Various FromStr types compile

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "HOST")]
    host: String,

    #[env(var = "PORT")]
    port: u16,

    #[env(var = "TIMEOUT")]
    timeout: u64,

    #[env(var = "ENABLED")]
    enabled: bool,

    #[env(var = "RATIO")]
    ratio: f64,

    #[env(var = "IP_ADDR", optional)]
    ip_addr: Option<IpAddr>,

    #[env(var = "SOCKET_ADDR", optional)]
    socket_addr: Option<SocketAddr>,

    #[env(var = "DATA_PATH", optional)]
    data_path: Option<PathBuf>,
}

fn main() {}
