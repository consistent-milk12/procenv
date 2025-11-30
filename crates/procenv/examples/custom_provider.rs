//! Example: Custom configuration provider
//!
//! This example demonstrates how to create a custom provider
//! that could fetch configuration from an external source
//! like HashiCorp Vault, AWS Secrets Manager, or a database.
//!
//! Run with:
//!   cargo run --example custom_provider

use procenv::ConfigLoader;
use procenv::provider::{Provider, ProviderResult, ProviderSource, ProviderValue};
use std::collections::HashMap;

/// A mock "Vault" provider for demonstration.
///
/// In a real implementation, this would connect to HashiCorp Vault
/// and fetch secrets from a specific path.
struct VaultProvider {
    /// Simulated secrets storage
    secrets: HashMap<String, String>,
    /// The Vault path prefix (e.g., "secret/data/myapp")
    path_prefix: String,
}

impl VaultProvider {
    fn new(path_prefix: &str) -> Self {
        let mut secrets = HashMap::new();
        // Simulate some secrets that would come from Vault
        secrets.insert(
            "database_password".to_string(),
            "super-secret-password".to_string(),
        );
        secrets.insert("api_key".to_string(), "vault-api-key-12345".to_string());

        Self {
            secrets,
            path_prefix: path_prefix.to_string(),
        }
    }
}

impl Provider for VaultProvider {
    fn name(&self) -> &str {
        "vault"
    }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        println!("  [Vault] Looking up: {}", key);

        match self.secrets.get(key) {
            Some(value) => {
                println!("  [Vault] Found secret for: {}", key);
                Ok(Some(ProviderValue {
                    value: value.clone(),
                    source: ProviderSource::custom(
                        "vault",
                        Some(format!("{}/{}", self.path_prefix, key)),
                    ),
                    // Mark secrets from Vault as sensitive
                    secret: true,
                }))
            }
            None => {
                println!("  [Vault] No secret found for: {}", key);
                Ok(None)
            }
        }
    }

    fn priority(&self) -> u32 {
        // Lower priority than env (20) so env vars can override
        // but higher than file config (50)
        40
    }
}

fn main() {
    println!("=== Custom Provider Example ===\n");

    // Create a loader with multiple providers
    let mut loader = ConfigLoader::new()
        // Environment variables have highest priority
        .with_env()
        // Vault provider for secrets
        .with_provider(Box::new(VaultProvider::new("secret/data/myapp")));

    println!("Looking up configuration values:\n");

    // Try to get database_password (from Vault)
    println!("1. database_password:");
    if let Some(value) = loader.get("database_password") {
        println!(
            "   Value: {} (secret={})",
            if value.secret {
                "<redacted>"
            } else {
                &value.value
            },
            value.secret
        );
        println!("   Source: {}\n", value.source);
    }

    // Try to get api_key (from Vault)
    println!("2. api_key:");
    if let Some(value) = loader.get("api_key") {
        println!(
            "   Value: {} (secret={})",
            if value.secret {
                "<redacted>"
            } else {
                &value.value
            },
            value.secret
        );
        println!("   Source: {}\n", value.source);
    }

    // Try to get something not in Vault (will fall through)
    println!("3. PORT (not in Vault, using default):");
    let port = loader.get_with_default("PORT", "PORT", "8080");
    println!("   Value: {}", port.value);
    println!("   Source: {}\n", port.source);

    // Show source attribution
    println!("=== Source Attribution ===");
    for (field, source) in loader.sources().iter() {
        println!("  {} <- {}", field, source.source);
    }
}
