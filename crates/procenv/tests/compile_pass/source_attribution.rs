//! Test: Source attribution methods compile correctly

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,
}

fn main() {
    // Test that from_env_with_sources compiles and returns correct types
    let _result: Result<(Config, procenv::ConfigSources), procenv::Error> =
        Config::from_env_with_sources();

    // Test that sources() method compiles
    let _sources_result: Result<procenv::ConfigSources, procenv::Error> = Config::sources();
}
