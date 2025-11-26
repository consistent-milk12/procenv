//! Test: optional flag requires Option<T> type

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "API_KEY", optional)]
    api_key: String,
}

fn main() {}
