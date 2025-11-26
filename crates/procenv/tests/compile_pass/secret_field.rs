//! Test: Secret field compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "API_SECRET", secret)]
    secret: String,
}

fn main() {}
