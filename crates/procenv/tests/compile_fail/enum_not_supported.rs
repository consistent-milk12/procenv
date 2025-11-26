//! Test: EnvConfig cannot be derived on enums

use procenv::EnvConfig;

#[derive(EnvConfig)]
enum Config {
    Development,
    Production,
}

fn main() {}
