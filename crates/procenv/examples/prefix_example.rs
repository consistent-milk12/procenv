use procenv::EnvConfig;

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct Config {
    /// Reads APP_DATABASE_URL
    #[env(var = "DATABASE_URL")]
    db_url: String,

    /// Reads APP_PORT
    #[env(var = "PORT", default = "8080")]
    port: u16,

    /// Reads GLOBAL_DEBUG (skips prefix)
    #[env(var = "GLOBAL_DEBUG", no_prefix, default = "false")]
    debug: bool,
}

fn main() {
    match Config::from_env() {
        Ok(config) => {
            println!("Config loaded!");
            println!("  db_url (APP_DATABASE_URL): {}", config.db_url);
            println!("  port (APP_PORT): {}", config.port);
            println!("  debug (GLOBAL_DEBUG): {}", config.debug);
        }
        Err(e) => {
            eprintln!("{:?}", miette::Report::from(e));
            std::process::exit(1);
        }
    }
}
