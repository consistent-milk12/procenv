//! Test: Basic required field compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,
}

fn main() {}
