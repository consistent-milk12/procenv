//! Test: Various FromStr types compile
//!
//! Note: PathBuf doesn't implement Display, so it's tested in a separate
//! non-runtime-access config. Types used with get_str() must implement Display.

use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;

use procenv::EnvConfig;

/// Wrapper around PathBuf that implements Display for runtime access support.
#[derive(Debug)]
struct DisplayPath(PathBuf);

impl fmt::Display for DisplayPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl std::str::FromStr for DisplayPath {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(DisplayPath(PathBuf::from(s)))
    }
}

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
    data_path: Option<DisplayPath>,
}

fn main() {}
