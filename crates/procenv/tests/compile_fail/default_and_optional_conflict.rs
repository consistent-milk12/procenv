//! Test: Cannot use both default and optional

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "API_KEY", default = "key", optional)]
    api_key: Option<String>,
}

fn main() {}
