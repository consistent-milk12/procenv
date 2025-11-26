//! Test: All features combined compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "DEBUG", default = "false")]
    debug: bool,

    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    #[env(var = "MAX_CONNECTIONS", optional)]
    max_connections: Option<u32>,

    #[env(var = "SECRET_TOKEN", secret)]
    secret_token: String,

    #[env(var = "SECRET_API_KEY", secret, optional)]
    secret_api_key: Option<String>,
}

fn main() {}
