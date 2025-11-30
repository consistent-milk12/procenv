//! Profile support runtime tests.
//!
//! Tests for profile-based configuration (dev/staging/prod).

use procenv::{EnvConfig, Source};
use serial_test::serial;

fn cleanup_env(vars: &[&str]) {
    unsafe {
        for k in vars {
            std::env::remove_var(*k);
        }
    }
}

fn with_env<F, R>(vars: &[(&str, &str)], f: F) -> R
where
    F: FnOnce() -> R,
{
    unsafe {
        for (k, v) in vars {
            std::env::set_var(*k, *v);
        }
    }

    let result = f();

    unsafe {
        for (k, _) in vars {
            std::env::remove_var(*k);
        }
    }

    result
}

// ============================================================================
// Basic Profile Selection
// ============================================================================

#[derive(EnvConfig)]
#[env_config(
    profile_env = "PROF_ENV",
    profiles = ["dev", "staging", "prod"]
)]
struct BasicProfileConfig {
    #[env(var = "PROF_DB_URL")]
    #[profile(
        dev = "postgres://localhost/dev",
        staging = "postgres://staging/app",
        prod = "postgres://prod/app"
    )]
    database_url: String,

    #[env(var = "PROF_PORT", default = "8080")]
    port: u16,
}

#[test]
#[serial]
fn test_profile_dev_selection() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(&[("PROF_ENV", "dev")], || {
        let config = BasicProfileConfig::from_env().expect("should load dev profile");
        assert_eq!(config.database_url, "postgres://localhost/dev");
        assert_eq!(config.port, 8080);
    })
}

#[test]
#[serial]
fn test_profile_staging_selection() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(&[("PROF_ENV", "staging")], || {
        let config = BasicProfileConfig::from_env().expect("should load staging profile");
        assert_eq!(config.database_url, "postgres://staging/app");
    })
}

#[test]
#[serial]
fn test_profile_prod_selection() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(&[("PROF_ENV", "prod")], || {
        let config = BasicProfileConfig::from_env().expect("should load prod profile");
        assert_eq!(config.database_url, "postgres://prod/app");
    })
}

// ============================================================================
// Environment Override of Profile Values
// ============================================================================

#[test]
#[serial]
fn test_env_overrides_profile() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(
        &[
            ("PROF_ENV", "dev"),
            ("PROF_DB_URL", "postgres://custom/override"),
        ],
        || {
            let config = BasicProfileConfig::from_env().expect("should load with override");
            assert_eq!(config.database_url, "postgres://custom/override");
        },
    )
}

#[test]
#[serial]
fn test_env_override_source_attribution() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(
        &[
            ("PROF_ENV", "dev"),
            ("PROF_DB_URL", "postgres://custom/override"),
        ],
        || {
            let (_, sources) =
                BasicProfileConfig::from_env_with_sources().expect("should load with sources");

            let db_source = sources.get("database_url").expect("should have db source");
            assert!(
                matches!(db_source.source, Source::Environment),
                "should be Environment when overridden, got {:?}",
                db_source.source
            );
        },
    )
}

// ============================================================================
// Profile Source Attribution
// ============================================================================

#[test]
#[serial]
fn test_profile_source_attribution() {
    cleanup_env(&["PROF_ENV", "PROF_DB_URL", "PROF_PORT"]);

    with_env(&[("PROF_ENV", "staging")], || {
        let (_, sources) =
            BasicProfileConfig::from_env_with_sources().expect("should load with sources");

        let db_source = sources.get("database_url").expect("should have db source");
        assert!(
            matches!(db_source.source, Source::Profile(ref p) if p == "staging"),
            "should be Profile(staging), got {:?}",
            db_source.source
        );

        let port_source = sources.get("port").expect("should have port source");
        assert!(
            matches!(port_source.source, Source::Default),
            "port should be Default, got {:?}",
            port_source.source
        );
    })
}

// ============================================================================
// Partial Profile Coverage
// ============================================================================

#[derive(EnvConfig)]
#[env_config(
    profile_env = "PARTIAL_ENV",
    profiles = ["dev", "prod"]
)]
struct PartialProfileConfig {
    #[env(var = "PARTIAL_LOG", default = "info")]
    #[profile(dev = "debug")]
    log_level: String,

    #[env(var = "PARTIAL_CACHE", default = "false")]
    #[profile(prod = "true")]
    enable_cache: bool,
}

#[test]
#[serial]
fn test_partial_profile_dev_has_log_not_cache() {
    cleanup_env(&["PARTIAL_ENV", "PARTIAL_LOG", "PARTIAL_CACHE"]);

    with_env(&[("PARTIAL_ENV", "dev")], || {
        let config = PartialProfileConfig::from_env().expect("should load");
        assert_eq!(config.log_level, "debug"); // from profile
        assert!(!config.enable_cache); // from default (no dev profile)
    })
}

#[test]
#[serial]
fn test_partial_profile_prod_has_cache_not_log() {
    cleanup_env(&["PARTIAL_ENV", "PARTIAL_LOG", "PARTIAL_CACHE"]);

    with_env(&[("PARTIAL_ENV", "prod")], || {
        let config = PartialProfileConfig::from_env().expect("should load");
        assert_eq!(config.log_level, "info"); // from default (no prod profile)
        assert!(config.enable_cache); // from profile
    })
}

#[test]
#[serial]
fn test_partial_profile_source_attribution() {
    cleanup_env(&["PARTIAL_ENV", "PARTIAL_LOG", "PARTIAL_CACHE"]);

    with_env(&[("PARTIAL_ENV", "prod")], || {
        let (_, sources) =
            PartialProfileConfig::from_env_with_sources().expect("should load with sources");

        // log_level should be Default (prod has no profile for it)
        let log_source = sources.get("log_level").expect("should have log source");
        assert!(
            matches!(log_source.source, Source::Default),
            "log_level should be Default for prod, got {:?}",
            log_source.source
        );

        // enable_cache should be Profile(prod)
        let cache_source = sources
            .get("enable_cache")
            .expect("should have cache source");
        assert!(
            matches!(cache_source.source, Source::Profile(ref p) if p == "prod"),
            "enable_cache should be Profile(prod), got {:?}",
            cache_source.source
        );
    })
}

// ============================================================================
// No Profile Set (Uses Defaults)
// ============================================================================

#[test]
#[serial]
fn test_no_profile_uses_defaults() {
    cleanup_env(&["PARTIAL_ENV", "PARTIAL_LOG", "PARTIAL_CACHE"]);

    // No PARTIAL_ENV set
    let config = PartialProfileConfig::from_env().expect("should load with defaults");
    assert_eq!(config.log_level, "info");
    assert!(!config.enable_cache);
}

// ============================================================================
// Profile with Prefix
// ============================================================================

#[derive(EnvConfig)]
#[env_config(
    prefix = "MYAPP_",
    profile_env = "MYAPP_ENV",
    profiles = ["local", "cloud"]
)]
struct PrefixedProfileConfig {
    #[env(var = "HOST", default = "localhost")]
    #[profile(local = "127.0.0.1", cloud = "0.0.0.0")]
    host: String,

    #[env(var = "TLS", default = "false")]
    #[profile(cloud = "true")]
    tls_enabled: bool,
}

#[test]
#[serial]
fn test_profile_with_prefix() {
    cleanup_env(&["MYAPP_ENV", "MYAPP_HOST", "MYAPP_TLS"]);

    with_env(&[("MYAPP_ENV", "cloud")], || {
        let config = PrefixedProfileConfig::from_env().expect("should load cloud profile");
        assert_eq!(config.host, "0.0.0.0");
        assert!(config.tls_enabled);
    })
}

#[test]
#[serial]
fn test_profile_with_prefix_env_override() {
    cleanup_env(&["MYAPP_ENV", "MYAPP_HOST", "MYAPP_TLS"]);

    with_env(
        &[("MYAPP_ENV", "cloud"), ("MYAPP_HOST", "custom.host")],
        || {
            let config = PrefixedProfileConfig::from_env().expect("should load with override");
            assert_eq!(config.host, "custom.host");
            assert!(config.tls_enabled);
        },
    )
}
