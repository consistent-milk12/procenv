//! Test: Prefix support compiles correctly

use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Field that skips the prefix
    #[env(var = "GLOBAL_FLAG", no_prefix)]
    global_flag: bool,
}

#[derive(EnvConfig)]
#[env_config(prefix = "MYAPP_", dotenv)]
struct ConfigWithDotenv {
    #[env(var = "HOST")]
    host: String,
}

fn main() {
    // These should look for APP_DATABASE_URL, APP_PORT, but GLOBAL_FLAG (no prefix)
    let _result: Result<Config, procenv::Error> = Config::from_env();

    // This should look for MYAPP_HOST and load .env
    let _result2: Result<ConfigWithDotenv, procenv::Error> = ConfigWithDotenv::from_env();
}
