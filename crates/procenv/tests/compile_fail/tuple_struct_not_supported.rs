//! Test: EnvConfig cannot be derived on tuple structs

use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config(String, u16);

fn main() {}
