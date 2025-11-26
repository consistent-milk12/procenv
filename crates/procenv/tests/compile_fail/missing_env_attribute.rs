//! Test: Fields must have #[env(...)] attribute

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    db_url: String,
}

fn main() {}
