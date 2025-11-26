//! Test: #[env(...)] must contain var = "NAME"

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(default = "8080")]
    port: u16,
}

fn main() {}
