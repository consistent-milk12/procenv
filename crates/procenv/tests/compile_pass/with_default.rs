//! Test: Field with default value compiles

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "PORT", default = "8080")]
    port: u16,
}

fn main() {}
