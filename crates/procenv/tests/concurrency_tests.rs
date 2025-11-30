//! Concurrency and thread safety tests.
//!
//! Tests for verifying thread-safe behavior of config loading.

use procenv::EnvConfig;
use std::sync::{Arc, Barrier};
use std::thread;

// ============================================================================
// Basic Thread Safety
// ============================================================================

#[derive(EnvConfig, Clone)]
struct ThreadSafeConfig {
    #[env(var = "THREAD_HOST", default = "localhost")]
    host: String,

    #[env(var = "THREAD_PORT", default = "8080")]
    port: u16,

    #[env(var = "THREAD_TIMEOUT", default = "30")]
    timeout: u32,
}

#[test]
fn test_concurrent_config_loading() {
    // Clean up before test
    unsafe {
        std::env::remove_var("THREAD_HOST");
        std::env::remove_var("THREAD_PORT");
        std::env::remove_var("THREAD_TIMEOUT");
    }

    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for i in 0..num_threads {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            // Wait for all threads to be ready
            barrier.wait();

            // All threads load config simultaneously
            let config = ThreadSafeConfig::from_env().expect("should load config");

            // Verify default values (no env vars set)
            assert_eq!(config.host, "localhost");
            assert_eq!(config.port, 8080);
            assert_eq!(config.timeout, 30);

            i
        });

        handles.push(handle);
    }

    // Collect results
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert_eq!(results.len(), num_threads);
}

// ============================================================================
// Concurrent Loading with Different Values
// ============================================================================

#[derive(EnvConfig, Clone)]
struct IsolatedConfig {
    #[env(var = "ISO_VALUE", default = "default")]
    value: String,
}

#[test]
fn test_concurrent_loading_uses_defaults() {
    // Ensure no env var is set
    unsafe {
        std::env::remove_var("ISO_VALUE");
    }

    let num_threads = 20;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            // Each thread loads independently
            let config = IsolatedConfig::from_env().expect("should load");
            config.value
        });

        handles.push(handle);
    }

    // All threads should get the same default value
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    for result in &results {
        assert_eq!(result, "default");
    }
}

// ============================================================================
// Large Concurrent Load Test
// ============================================================================

#[derive(EnvConfig, Clone)]
struct LargeConcurrentConfig {
    #[env(var = "LARGE_A", default = "a")]
    a: String,
    #[env(var = "LARGE_B", default = "b")]
    b: String,
    #[env(var = "LARGE_C", default = "c")]
    c: String,
    #[env(var = "LARGE_D", default = "d")]
    d: String,
    #[env(var = "LARGE_E", default = "e")]
    e: String,
    #[env(var = "LARGE_F", default = "f")]
    f: String,
    #[env(var = "LARGE_G", default = "g")]
    g: String,
    #[env(var = "LARGE_H", default = "h")]
    h: String,
    #[env(var = "LARGE_NUM1", default = "1")]
    num1: u32,
    #[env(var = "LARGE_NUM2", default = "2")]
    num2: u32,
}

#[test]
fn test_large_config_concurrent_loading() {
    // Clean up
    for var in [
        "LARGE_A",
        "LARGE_B",
        "LARGE_C",
        "LARGE_D",
        "LARGE_E",
        "LARGE_F",
        "LARGE_G",
        "LARGE_H",
        "LARGE_NUM1",
        "LARGE_NUM2",
    ] {
        unsafe {
            std::env::remove_var(var);
        }
    }

    let num_threads = 50;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            let config = LargeConcurrentConfig::from_env().expect("should load large config");

            // Verify all fields
            assert_eq!(config.a, "a");
            assert_eq!(config.b, "b");
            assert_eq!(config.c, "c");
            assert_eq!(config.d, "d");
            assert_eq!(config.e, "e");
            assert_eq!(config.f, "f");
            assert_eq!(config.g, "g");
            assert_eq!(config.h, "h");
            assert_eq!(config.num1, 1);
            assert_eq!(config.num2, 2);
        });

        handles.push(handle);
    }

    // All threads should complete successfully
    for handle in handles {
        handle.join().expect("thread should not panic");
    }
}

// ============================================================================
// Concurrent Source Attribution
// ============================================================================

#[derive(EnvConfig, Clone)]
struct SourceAttrConfig {
    #[env(var = "SRC_REQUIRED")]
    required: String,

    #[env(var = "SRC_DEFAULT", default = "default")]
    defaulted: String,

    #[env(var = "SRC_OPTIONAL", optional)]
    optional: Option<String>,
}

#[test]
fn test_concurrent_source_attribution() {
    unsafe {
        std::env::set_var("SRC_REQUIRED", "required_value");
        std::env::remove_var("SRC_DEFAULT");
        std::env::remove_var("SRC_OPTIONAL");
    }

    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            let (config, sources) =
                SourceAttrConfig::from_env_with_sources().expect("should load with sources");

            // Verify values
            assert_eq!(config.required, "required_value");
            assert_eq!(config.defaulted, "default");
            assert!(config.optional.is_none());

            // Verify sources
            let req_src = sources.get("required").expect("should have required");
            assert!(matches!(req_src.source, procenv::Source::Environment));

            let def_src = sources.get("defaulted").expect("should have defaulted");
            assert!(matches!(def_src.source, procenv::Source::Default));
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should not panic");
    }

    // Cleanup
    unsafe {
        std::env::remove_var("SRC_REQUIRED");
    }
}

// ============================================================================
// Stress Test with Rapid Loading
// ============================================================================

#[derive(EnvConfig, Clone)]
struct StressConfig {
    #[env(var = "STRESS_VAL", default = "stress")]
    value: String,
}

#[test]
fn test_rapid_config_loading() {
    unsafe {
        std::env::remove_var("STRESS_VAL");
    }

    let num_iterations = 1000;
    let mut handles = vec![];

    for _ in 0..num_iterations {
        let handle = thread::spawn(|| {
            let config = StressConfig::from_env().expect("should load");
            assert_eq!(config.value, "stress");
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should not panic");
    }
}

// ============================================================================
// Thread Safety with Nested Config
// ============================================================================

#[derive(EnvConfig, Clone)]
struct InnerConcurrent {
    #[env(var = "CONC_INNER_HOST", default = "inner")]
    host: String,
}

#[derive(EnvConfig, Clone)]
#[env_config(prefix = "CONC_")]
struct OuterConcurrent {
    #[env(var = "NAME", default = "outer")]
    name: String,

    #[env(flatten)]
    inner: InnerConcurrent,
}

#[test]
fn test_concurrent_nested_config() {
    unsafe {
        std::env::remove_var("CONC_NAME");
        std::env::remove_var("CONC_INNER_HOST"); // Full var name since flatten doesn't add prefix
    }

    let num_threads = 20;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();

            let config = OuterConcurrent::from_env().expect("should load nested");

            assert_eq!(config.name, "outer");
            assert_eq!(config.inner.host, "inner");
        });

        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread should not panic");
    }
}
