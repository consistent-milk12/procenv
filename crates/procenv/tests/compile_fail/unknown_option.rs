//! Test: Unknown options in #[env(...)] are rejected

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "PORT", unknown_option)]
    port: u16,
}

fn main() {}
