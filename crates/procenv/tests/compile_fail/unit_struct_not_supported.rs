//! Test: EnvConfig cannot be derived on unit structs

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config;

fn main() {}
