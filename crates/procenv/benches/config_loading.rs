//! Performance benchmarks for procenv config loading.
//!
//! Run with: `cargo bench -p procenv`
//!
//! Includes comparison benchmarks against:
//! - `envy` - Simple env-to-struct via serde
//! - `figment` - Layered configuration library
//! - `config` - Hierarchical configuration library

use procenv::EnvConfig;
use serde::Deserialize;

fn main() {
    // Setup environment variables for benchmarks
    setup_env();
    divan::main();
}

fn setup_env() {
    // Small config vars
    unsafe {
        std::env::set_var("SMALL_HOST", "localhost");
        std::env::set_var("SMALL_PORT", "8080");
        std::env::set_var("SMALL_DEBUG", "true");

        // Medium config vars
        std::env::set_var("MED_HOST", "localhost");
        std::env::set_var("MED_PORT", "8080");
        std::env::set_var("MED_DEBUG", "true");
        std::env::set_var("MED_LOG_LEVEL", "info");
        std::env::set_var("MED_WORKERS", "4");
        std::env::set_var("MED_TIMEOUT", "30");
        std::env::set_var("MED_RETRIES", "3");
        std::env::set_var("MED_CACHE_SIZE", "1000");
        std::env::set_var("MED_BATCH_SIZE", "100");
        std::env::set_var("MED_RATE_LIMIT", "1000");

        // Large config vars (50 fields)
        for i in 0..50 {
            std::env::set_var(format!("LARGE_FIELD_{}", i), format!("value_{}", i));
        }

        // Nested config vars
        std::env::set_var("NEST_APP_NAME", "myapp");
        std::env::set_var("NEST_DB_HOST", "db.example.com");
        std::env::set_var("NEST_DB_PORT", "5432");
        std::env::set_var("NEST_CACHE_HOST", "cache.example.com");
        std::env::set_var("NEST_CACHE_PORT", "6379");

        // Deep nested config vars
        std::env::set_var("DEEP_L1", "level1");
        std::env::set_var("DEEP_L2", "level2");
        std::env::set_var("DEEP_L3", "level3");

        // Comparison benchmark vars (for envy, figment, config crates)
        // These use uppercase field names as expected by envy
        std::env::set_var("CMP_HOST", "localhost");
        std::env::set_var("CMP_PORT", "8080");
        std::env::set_var("CMP_DEBUG", "true");
        std::env::set_var("CMP_LOG_LEVEL", "info");
        std::env::set_var("CMP_WORKERS", "4");
        std::env::set_var("CMP_TIMEOUT", "30");
    }
}

// ============================================================================
// Small Config (3 fields)
// ============================================================================

#[derive(EnvConfig)]
struct SmallConfig {
    #[env(var = "SMALL_HOST", default = "localhost")]
    host: String,

    #[env(var = "SMALL_PORT", default = "8080")]
    port: u16,

    #[env(var = "SMALL_DEBUG", default = "false")]
    debug: bool,
}

#[divan::bench]
fn small_config_from_env() -> SmallConfig {
    SmallConfig::from_env().unwrap()
}

#[divan::bench]
fn small_config_with_sources() -> (SmallConfig, procenv::ConfigSources) {
    SmallConfig::from_env_with_sources().unwrap()
}

// ============================================================================
// Medium Config (10 fields)
// ============================================================================

#[derive(EnvConfig)]
struct MediumConfig {
    #[env(var = "MED_HOST", default = "localhost")]
    host: String,

    #[env(var = "MED_PORT", default = "8080")]
    port: u16,

    #[env(var = "MED_DEBUG", default = "false")]
    debug: bool,

    #[env(var = "MED_LOG_LEVEL", default = "info")]
    log_level: String,

    #[env(var = "MED_WORKERS", default = "4")]
    workers: u32,

    #[env(var = "MED_TIMEOUT", default = "30")]
    timeout: u64,

    #[env(var = "MED_RETRIES", default = "3")]
    retries: u8,

    #[env(var = "MED_CACHE_SIZE", default = "1000")]
    cache_size: usize,

    #[env(var = "MED_BATCH_SIZE", default = "100")]
    batch_size: usize,

    #[env(var = "MED_RATE_LIMIT", default = "1000")]
    rate_limit: u32,
}

#[divan::bench]
fn medium_config_from_env() -> MediumConfig {
    MediumConfig::from_env().unwrap()
}

#[divan::bench]
fn medium_config_with_sources() -> (MediumConfig, procenv::ConfigSources) {
    MediumConfig::from_env_with_sources().unwrap()
}

// ============================================================================
// Large Config (50 fields)
// ============================================================================

#[derive(EnvConfig)]
struct LargeConfig {
    #[env(var = "LARGE_FIELD_0", default = "default")]
    field_0: String,
    #[env(var = "LARGE_FIELD_1", default = "default")]
    field_1: String,
    #[env(var = "LARGE_FIELD_2", default = "default")]
    field_2: String,
    #[env(var = "LARGE_FIELD_3", default = "default")]
    field_3: String,
    #[env(var = "LARGE_FIELD_4", default = "default")]
    field_4: String,
    #[env(var = "LARGE_FIELD_5", default = "default")]
    field_5: String,
    #[env(var = "LARGE_FIELD_6", default = "default")]
    field_6: String,
    #[env(var = "LARGE_FIELD_7", default = "default")]
    field_7: String,
    #[env(var = "LARGE_FIELD_8", default = "default")]
    field_8: String,
    #[env(var = "LARGE_FIELD_9", default = "default")]
    field_9: String,
    #[env(var = "LARGE_FIELD_10", default = "default")]
    field_10: String,
    #[env(var = "LARGE_FIELD_11", default = "default")]
    field_11: String,
    #[env(var = "LARGE_FIELD_12", default = "default")]
    field_12: String,
    #[env(var = "LARGE_FIELD_13", default = "default")]
    field_13: String,
    #[env(var = "LARGE_FIELD_14", default = "default")]
    field_14: String,
    #[env(var = "LARGE_FIELD_15", default = "default")]
    field_15: String,
    #[env(var = "LARGE_FIELD_16", default = "default")]
    field_16: String,
    #[env(var = "LARGE_FIELD_17", default = "default")]
    field_17: String,
    #[env(var = "LARGE_FIELD_18", default = "default")]
    field_18: String,
    #[env(var = "LARGE_FIELD_19", default = "default")]
    field_19: String,
    #[env(var = "LARGE_FIELD_20", default = "default")]
    field_20: String,
    #[env(var = "LARGE_FIELD_21", default = "default")]
    field_21: String,
    #[env(var = "LARGE_FIELD_22", default = "default")]
    field_22: String,
    #[env(var = "LARGE_FIELD_23", default = "default")]
    field_23: String,
    #[env(var = "LARGE_FIELD_24", default = "default")]
    field_24: String,
    #[env(var = "LARGE_FIELD_25", default = "default")]
    field_25: String,
    #[env(var = "LARGE_FIELD_26", default = "default")]
    field_26: String,
    #[env(var = "LARGE_FIELD_27", default = "default")]
    field_27: String,
    #[env(var = "LARGE_FIELD_28", default = "default")]
    field_28: String,
    #[env(var = "LARGE_FIELD_29", default = "default")]
    field_29: String,
    #[env(var = "LARGE_FIELD_30", default = "default")]
    field_30: String,
    #[env(var = "LARGE_FIELD_31", default = "default")]
    field_31: String,
    #[env(var = "LARGE_FIELD_32", default = "default")]
    field_32: String,
    #[env(var = "LARGE_FIELD_33", default = "default")]
    field_33: String,
    #[env(var = "LARGE_FIELD_34", default = "default")]
    field_34: String,
    #[env(var = "LARGE_FIELD_35", default = "default")]
    field_35: String,
    #[env(var = "LARGE_FIELD_36", default = "default")]
    field_36: String,
    #[env(var = "LARGE_FIELD_37", default = "default")]
    field_37: String,
    #[env(var = "LARGE_FIELD_38", default = "default")]
    field_38: String,
    #[env(var = "LARGE_FIELD_39", default = "default")]
    field_39: String,
    #[env(var = "LARGE_FIELD_40", default = "default")]
    field_40: String,
    #[env(var = "LARGE_FIELD_41", default = "default")]
    field_41: String,
    #[env(var = "LARGE_FIELD_42", default = "default")]
    field_42: String,
    #[env(var = "LARGE_FIELD_43", default = "default")]
    field_43: String,
    #[env(var = "LARGE_FIELD_44", default = "default")]
    field_44: String,
    #[env(var = "LARGE_FIELD_45", default = "default")]
    field_45: String,
    #[env(var = "LARGE_FIELD_46", default = "default")]
    field_46: String,
    #[env(var = "LARGE_FIELD_47", default = "default")]
    field_47: String,
    #[env(var = "LARGE_FIELD_48", default = "default")]
    field_48: String,
    #[env(var = "LARGE_FIELD_49", default = "default")]
    field_49: String,
}

#[divan::bench]
fn large_config_from_env() -> LargeConfig {
    LargeConfig::from_env().unwrap()
}

#[divan::bench]
fn large_config_with_sources() -> (LargeConfig, procenv::ConfigSources) {
    LargeConfig::from_env_with_sources().unwrap()
}

// ============================================================================
// Nested Config (2 levels)
// ============================================================================

#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "NEST_DB_HOST", default = "localhost")]
    host: String,

    #[env(var = "NEST_DB_PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
struct CacheConfig {
    #[env(var = "NEST_CACHE_HOST", default = "localhost")]
    host: String,

    #[env(var = "NEST_CACHE_PORT", default = "6379")]
    port: u16,
}

#[derive(EnvConfig)]
struct NestedConfig {
    #[env(var = "NEST_APP_NAME", default = "app")]
    app_name: String,

    #[env(flatten)]
    database: DatabaseConfig,

    #[env(flatten)]
    cache: CacheConfig,
}

#[divan::bench]
fn nested_config_from_env() -> NestedConfig {
    NestedConfig::from_env().unwrap()
}

#[divan::bench]
fn nested_config_with_sources() -> (NestedConfig, procenv::ConfigSources) {
    NestedConfig::from_env_with_sources().unwrap()
}

// ============================================================================
// Deep Nested Config (3 levels)
// ============================================================================

#[derive(EnvConfig)]
struct Level3Config {
    #[env(var = "DEEP_L3", default = "l3")]
    value: String,
}

#[derive(EnvConfig)]
struct Level2Config {
    #[env(var = "DEEP_L2", default = "l2")]
    value: String,

    #[env(flatten)]
    level3: Level3Config,
}

#[derive(EnvConfig)]
struct DeepNestedConfig {
    #[env(var = "DEEP_L1", default = "l1")]
    value: String,

    #[env(flatten)]
    level2: Level2Config,
}

#[divan::bench]
fn deep_nested_config_from_env() -> DeepNestedConfig {
    DeepNestedConfig::from_env().unwrap()
}

#[divan::bench]
fn deep_nested_config_with_sources() -> (DeepNestedConfig, procenv::ConfigSources) {
    DeepNestedConfig::from_env_with_sources().unwrap()
}

// ============================================================================
// Config with Prefix
// ============================================================================

#[derive(EnvConfig)]
#[env_config(prefix = "MED_")]
struct PrefixedConfig {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "DEBUG", default = "false")]
    debug: bool,
}

#[divan::bench]
fn prefixed_config_from_env() -> PrefixedConfig {
    PrefixedConfig::from_env().unwrap()
}

// ============================================================================
// Config with Optional Fields
// ============================================================================

#[derive(EnvConfig)]
struct OptionalFieldsConfig {
    #[env(var = "OPT_REQUIRED")]
    required: String,

    #[env(var = "OPT_OPTIONAL", optional)]
    optional: Option<String>,

    #[env(var = "OPT_DEFAULT", default = "default")]
    with_default: String,
}

#[divan::bench]
fn optional_fields_config_from_env() -> OptionalFieldsConfig {
    // Set required field for this benchmark
    unsafe {
        std::env::set_var("OPT_REQUIRED", "required_value");
    }
    let result = OptionalFieldsConfig::from_env().unwrap();
    unsafe {
        std::env::remove_var("OPT_REQUIRED");
    }
    result
}

// ============================================================================
// Config with Secrets
// ============================================================================

#[derive(EnvConfig)]
struct SecretConfig {
    #[env(var = "SEC_HOST", default = "localhost")]
    host: String,

    #[env(var = "SEC_PASSWORD", default = "secret", secret)]
    password: String,

    #[env(var = "SEC_API_KEY", default = "key123", secret)]
    api_key: String,
}

#[divan::bench]
fn secret_config_from_env() -> SecretConfig {
    SecretConfig::from_env().unwrap()
}

// ============================================================================
// Baseline: Raw std::env access
// ============================================================================

#[divan::bench]
fn baseline_env_var_lookup() -> String {
    std::env::var("SMALL_HOST").unwrap_or_else(|_| "localhost".to_string())
}

#[divan::bench]
fn baseline_env_var_parse() -> u16 {
    std::env::var("SMALL_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap()
}

// ============================================================================
// Comparison Benchmarks: procenv vs envy vs figment vs config
// ============================================================================

/// Small config struct for comparison benchmarks (serde-based)
#[derive(Deserialize)]
struct CmpSmallConfig {
    host: String,
    port: u16,
    debug: bool,
}

/// Medium config struct for comparison benchmarks (serde-based)
#[derive(Deserialize)]
struct CmpMediumConfig {
    host: String,
    port: u16,
    debug: bool,
    log_level: String,
    workers: u32,
    timeout: u64,
}

/// procenv version of small comparison config
#[derive(EnvConfig)]
struct ProcenvCmpSmall {
    #[env(var = "CMP_HOST")]
    host: String,
    #[env(var = "CMP_PORT")]
    port: u16,
    #[env(var = "CMP_DEBUG")]
    debug: bool,
}

/// procenv version of medium comparison config
#[derive(EnvConfig)]
struct ProcenvCmpMedium {
    #[env(var = "CMP_HOST")]
    host: String,
    #[env(var = "CMP_PORT")]
    port: u16,
    #[env(var = "CMP_DEBUG")]
    debug: bool,
    #[env(var = "CMP_LOG_LEVEL")]
    log_level: String,
    #[env(var = "CMP_WORKERS")]
    workers: u32,
    #[env(var = "CMP_TIMEOUT")]
    timeout: u64,
}

// ----------------------------------------------------------------------------
// Small Config Comparison (3 fields)
// ----------------------------------------------------------------------------

#[divan::bench(name = "cmp_small_procenv")]
fn cmp_small_procenv() -> ProcenvCmpSmall {
    ProcenvCmpSmall::from_env().unwrap()
}

#[divan::bench(name = "cmp_small_envy")]
fn cmp_small_envy() -> CmpSmallConfig {
    envy::prefixed("CMP_").from_env::<CmpSmallConfig>().unwrap()
}

#[divan::bench(name = "cmp_small_figment")]
fn cmp_small_figment() -> CmpSmallConfig {
    use figment::{Figment, providers::Env};
    Figment::new()
        .merge(Env::prefixed("CMP_"))
        .extract::<CmpSmallConfig>()
        .unwrap()
}

#[divan::bench(name = "cmp_small_config")]
fn cmp_small_config() -> CmpSmallConfig {
    use config::{Config, Environment};
    Config::builder()
        .add_source(Environment::with_prefix("CMP"))
        .build()
        .unwrap()
        .try_deserialize::<CmpSmallConfig>()
        .unwrap()
}

// ----------------------------------------------------------------------------
// Medium Config Comparison (6 fields)
// ----------------------------------------------------------------------------

#[divan::bench(name = "cmp_medium_procenv")]
fn cmp_medium_procenv() -> ProcenvCmpMedium {
    ProcenvCmpMedium::from_env().unwrap()
}

#[divan::bench(name = "cmp_medium_envy")]
fn cmp_medium_envy() -> CmpMediumConfig {
    envy::prefixed("CMP_").from_env::<CmpMediumConfig>().unwrap()
}

#[divan::bench(name = "cmp_medium_figment")]
fn cmp_medium_figment() -> CmpMediumConfig {
    use figment::{Figment, providers::Env};
    Figment::new()
        .merge(Env::prefixed("CMP_"))
        .extract::<CmpMediumConfig>()
        .unwrap()
}

#[divan::bench(name = "cmp_medium_config")]
fn cmp_medium_config() -> CmpMediumConfig {
    use config::{Config, Environment};
    Config::builder()
        .add_source(Environment::with_prefix("CMP"))
        .build()
        .unwrap()
        .try_deserialize::<CmpMediumConfig>()
        .unwrap()
}
