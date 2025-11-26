//! Test: EnvConfig cannot be derived on unions

use procenv::EnvConfig;

#[derive(EnvConfig)]
union Config {
    int_val: u32,
    float_val: f32,
}

fn main() {}
