//! Test: Optional field with Option<T> type compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,
}

fn main() {}
