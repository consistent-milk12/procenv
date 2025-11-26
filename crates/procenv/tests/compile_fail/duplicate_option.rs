//! Test: Duplicate options in #[env(...)] are rejected

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "PORT", var = "OTHER_PORT")]
    port: u16,
}

fn main() {}
