# procenv

An experimental Rust derive macro for declarative environment variable configuration with error accumulation and miette diagnostics.

> **Status:** Learning project / Experimental. Uses Rust 2024 edition and nightly features.
>
> For production use, consider [figment](https://docs.rs/figment) or [config-rs](https://docs.rs/config).

## Why This Exists

Most configuration crates stop at the first error. **procenv shows you everything that's wrong at once.**

```
procenv::multiple_errors

  × 3 configuration error(s) occurred

Error: missing required environment variable: DATABASE_URL
  help: set DATABASE_URL in your environment or .env file

Error: missing required environment variable: API_KEY
  help: set API_KEY in your environment or .env file

Error: failed to parse PORT: expected u16, got "not_a_number"
  help: expected a valid u16
```

This is a learning project exploring what's possible when you combine proc-macros with modern error handling.

## Unique Features

| Feature                      | Description                                                      |
| ---------------------------- | ---------------------------------------------------------------- |
| **Error Accumulation**       | Shows ALL config errors at once—no more fix-one-run-again cycles |
| **miette Diagnostics**       | Error codes, help text, and source spans in file configs         |
| **Built-in CLI Integration** | Generates clap arguments from attributes                         |
| **.env.example Generation**  | Auto-generate documentation for your config                      |
| **Secret Masking**           | Sensitive values hidden in Debug output and error messages       |

## Requirements

- **Rust 1.91.1+** (stable) - Uses Rust 2024 edition with `let_chains` (stabilized in 1.88)

## Quick Start

```rust
use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    database_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    #[env(var = "SECRET_TOKEN", secret)]
    secret: String,
}

fn main() -> Result<(), procenv::Error> {
    let config = Config::from_env()?;
    println!("Server starting on port {}", config.port);
    Ok(())
}
```

## Installation

```toml
[dependencies]
# Default features include: dotenv, secrecy, clap, file-all (toml/yaml/json)
procenv = "0.1"

# Minimal (no default features)
procenv = { version = "0.1", default-features = false }

# With validation support
procenv = { version = "0.1", features = ["validator"] }
```

### Feature Flags

Default features: `secrecy`, `dotenv`, `file-all`, `clap`

| Feature     | Default   | Description                                         |
| ----------- | --------- | --------------------------------------------------- |
| `secrecy`   | **Yes**   | `SecretString` support for sensitive data           |
| `dotenv`    | **Yes**   | Load `.env` files automatically                     |
| `clap`      | **Yes**   | CLI argument integration                            |
| `file-all`  | **Yes**   | Meta-feature: enables `toml` + `yaml` + `json`      |
| `file`      | via above | Base file config (JSON); enabled by format features |
| `toml`      | via above | TOML file support (implies `file`)                  |
| `yaml`      | via above | YAML file support (implies `file`)                  |
| `json`      | via above | JSON file support (implies `file`)                  |
| `validator` | No        | Validation integration with `validator` crate       |
| `serde`     | No        | Standalone serde support (without file loading)     |
| `tracing`   | No        | Tracing instrumentation                             |
| `provider`  | No        | Provider trait and ConfigLoader                     |
| `async`     | No        | Async provider support (requires `provider`)        |
| `full`      | No        | Enable all features                                 |

## Attribute Reference

### Field Attributes

```rust
#[derive(EnvConfig)]
struct Config {
    // Required field - errors if missing
    #[env(var = "DATABASE_URL")]
    database_url: String,

    // Default value - used when env var is missing
    #[env(var = "PORT", default = "8080")]
    port: u16,

    // Optional field - becomes None if missing
    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    // Secret field - masked in errors and Debug output
    #[env(var = "SECRET_TOKEN", secret)]
    secret: String,

    // JSON/complex types via serde
    #[env(var = "ALLOWED_HOSTS", format = "json")]
    hosts: Vec<String>,

    // CLI argument override
    #[env(var = "VERBOSE", arg = "verbose", short = 'v')]
    verbose: bool,

    // Skip the struct prefix for this field
    #[env(var = "GLOBAL_VAR", no_prefix)]
    global: String,
}
```

### Struct Attributes

```rust
#[derive(EnvConfig)]
#[env_config(
    prefix = "APP_",                         // Prefix all env vars
    dotenv,                                  // Load .env file
    file_optional = "config.toml",           // Optional config file
    profile_env = "APP_ENV",                 // Profile selection var
    profiles = ["dev", "staging", "prod"],   // Valid profiles
    validate                                 // Enable validation methods
)]
struct Config {
    #[env(var = "DATABASE_URL")]
    #[profile(
        dev = "postgres://localhost/dev",
        staging = "postgres://staging/app",
        prod = "postgres://prod/app"
    )]
    database_url: String,
}
```

## File Configuration

Load configuration from files with environment variable overrides:

```rust
use procenv::EnvConfig;
use serde::Deserialize;

#[derive(EnvConfig, Deserialize)]
#[env_config(
    prefix = "APP_",
    file_optional = "config.toml",
    dotenv
)]
struct Config {
    #[env(var = "DATABASE_URL")]
    database_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,
}

// Load with layered priority: defaults < file < env
let config = Config::from_config()?;

// Or with source attribution
let (config, sources) = Config::from_config_with_sources()?;
```

### File Error Diagnostics

Type mismatches show the exact location in your config file:

```
   ╭─[config.toml:5:8]
 4 │ host = "localhost"
 5 │ port = "not_a_number"
   ·        ───────┬──────
   ·               ╰── invalid type: string "not_a_number", expected u16
   ╰────
  help: check that the value matches the expected type
```

## Source Attribution

Track where each configuration value originated:

```rust
let (config, sources) = Config::from_env_with_sources()?;
println!("{}", sources);
```

Output:

```
Configuration Source:
--------------------------------------------------
  database_url    <- Environment variable [DATABASE_URL]
  port            <- Config file (config.toml) [PORT]
  api_key         <- .env file [API_KEY]
  debug           <- Default value [DEBUG]
  database.host   <- Config file (config.toml) [database.host]
```

Sources: `Environment`, `ConfigFile`, `DotenvFile`, `Profile`, `Default`, `Cli`, `NotSet`

## .env.example Generation

```rust
println!("{}", Config::env_example());
```

Output:

```bash
# Database connection URL (required)
DATABASE_URL=

# Server port (default: 8080)
# PORT=8080

# API key for external service (optional)
# API_KEY=

# Authentication token [SECRET]
SECRET_TOKEN=
```

## CLI Integration

```rust
#[derive(EnvConfig)]
struct Config {
    #[env(var = "PORT", default = "8080", arg = "port", short = 'p')]
    port: u16,

    #[env(var = "VERBOSE", arg = "verbose", short = 'v')]
    verbose: bool,
}

// Priority: CLI args > Environment > Defaults
let config = Config::from_args()?;
```

```bash
# All equivalent:
PORT=3000 ./myapp
./myapp --port 3000
./myapp -p 3000
```

## Nested Configuration

```rust
#[derive(EnvConfig)]
struct DatabaseConfig {
    #[env(var = "HOST", default = "localhost")]
    host: String,

    #[env(var = "PORT", default = "5432")]
    port: u16,
}

#[derive(EnvConfig)]
#[env_config(prefix = "APP_")]
struct Config {
    #[env(flatten, prefix = "DB_")]
    database: DatabaseConfig,  // Reads APP_DB_HOST, APP_DB_PORT
}
```

## Validation

Combine environment loading with runtime validation using the `validator` crate:

```rust
use procenv::EnvConfig;
use validator::Validate;

#[derive(EnvConfig, Validate)]
#[env_config(validate)]  // Enable from_env_validated() method
struct Config {
    #[env(var = "PORT", default = "8080")]
    #[validate(range(min = 1, max = 65535))]
    port: u16,

    #[env(var = "ADMIN_EMAIL")]
    #[validate(email)]
    admin_email: String,

    #[env(var = "API_KEY")]
    #[validate(length(min = 32))]
    api_key: String,
}

// One-step load AND validate
let config = Config::from_env_validated()?;
```

Validation errors are accumulated and shown together:

```
procenv::validation_error

  × 2 validation error(s) occurred

Error: field `port` failed validation: range
  help: value must be between 1 and 65535

Error: field `admin_email` failed validation: email
  help: must be a valid email address
```

## Custom Providers

Create custom configuration sources for services like Vault or AWS SSM:

```rust
use procenv::ConfigLoader;
use procenv::provider::{Provider, ProviderResult, ProviderSource, ProviderValue};
use std::collections::HashMap;

struct VaultProvider {
    secrets: HashMap<String, String>,
}

impl Provider for VaultProvider {
    fn name(&self) -> &str { "vault" }

    fn get(&self, key: &str) -> ProviderResult<ProviderValue> {
        match self.secrets.get(key) {
            Some(value) => Ok(Some(ProviderValue {
                value: value.clone(),
                source: ProviderSource::custom("vault", Some(format!("secret/app/{}", key))),
                secret: true,  // Mark as sensitive
            })),
            None => Ok(None),  // Key not found, try next provider
        }
    }

    fn priority(&self) -> u32 { 40 }  // Between env (20) and file (50)
}

// Chain providers with ConfigLoader
let mut loader = ConfigLoader::new()
    .with_env()
    .with_provider(Box::new(VaultProvider::new()));

if let Some(value) = loader.get("database_password") {
    println!("Source: {}", value.source);  // "vault (secret/app/database_password)"
}
```

Run the example: `cargo run --example custom_provider --features provider`

## Runtime Access

Access configuration values dynamically by key after loading:

```rust
let config = Config::from_env()?;

// Get all field names
let keys = Config::keys();  // ["database_url", "port", "api_key"]

// Access fields by key
if let Some(port) = config.get_str("port") {
    println!("Port: {}", port);
}

// Check if key exists
if Config::has_key("database_url") {
    // ...
}

// Nested config access (via flatten)
let db_host = config.get_str("database.host");
```

For partial loading without instantiating a full config struct, use `ConfigLoader`:

```rust
use procenv::ConfigLoader;

let mut loader = ConfigLoader::new().with_env();

// Get raw string value
let port = loader.get_str("PORT");

// Get with type inference
let value = loader.get_value_infer("PORT");
if let Some(port) = value.and_then(|v| v.to_u16()) {
    println!("Port: {}", port);
}

// Get with source attribution
if let Some((value, source)) = loader.get_with_source("DATABASE_URL") {
    println!("{} from {}", value, source);
}
```

**Note:** Secret fields return `<redacted>` from `get_str()`. Fields using `format` attribute (serde deserialization) are excluded from runtime access.

## Generated Methods

| Method                              | Description                                                                |
| ----------------------------------- | -------------------------------------------------------------------------- |
| `from_env()`                        | Load from environment variables                                            |
| `from_env_with_sources()`           | Load with source attribution                                               |
| `from_config()`                     | Load from files + env (requires `file` feature)                            |
| `from_config_with_sources()`        | File loading with source attribution                                       |
| `from_args()`                       | Load from CLI args + env (requires `clap` feature)                         |
| `from_env_validated()`              | Load + validate (requires `validator` feature + `#[env_config(validate)]`) |
| `from_env_validated_with_sources()` | Load + validate with source attribution                                    |
| `env_example()`                     | Generate `.env.example` content                                            |
| `keys()`                            | Returns all field names as static strings                                  |
| `get_str(key)`                      | Get field value as string by key                                           |
| `has_key(key)`                      | Check if a key exists                                                      |

## Comparison with Established Crates

| Feature                 | procenv      | figment    | config-rs  | envy   |
| ----------------------- | ------------ | ---------- | ---------- | ------ |
| **Maturity**            | Experimental | Production | Production | Stable |
| Error accumulation      | **Yes**      | No         | No         | No     |
| miette diagnostics      | **Yes**      | No         | No         | No     |
| .env.example generation | **Yes**      | No         | No         | No     |
| CLI integration         | **Yes**      | No         | No         | No     |
| Validation integration  | **Yes**      | No         | No         | No     |
| Compile-time derive     | Yes          | No         | No         | Yes    |
| File configs            | Yes          | Yes        | Yes        | No     |
| Custom providers        | **Yes**      | Yes        | Yes        | No     |
| Runtime value access    | **Yes**      | Yes        | Yes        | No     |
| Hot reload              | No           | No         | Partial    | No     |

## Performance

Benchmarks on Linux (AMD Ryzen, divan):

| Scenario                  | Time     | Notes                          |
| ------------------------- | -------- | ------------------------------ |
| Baseline `std::env::var`  | ~145 ns  | Single lookup + parse          |
| Small config (3 fields)   | ~478 ns  | ~3x baseline, minimal overhead |
| Medium config (6 fields)  | ~1.8 µs  | Linear scaling                 |
| Large config (30 fields)  | ~15 µs   | ~500 ns/field                  |
| Nested config (4 levels)  | ~1.3 µs  | Flatten has no overhead        |
| With source tracking      | ~2x      | `from_env_with_sources()`      |
| Secret field handling     | ~555 ns  | Negligible SecretString cost   |

**Key takeaways:**
- ~500 ns per field for env var lookup + parsing
- Source attribution doubles cost (still <35 µs for 30 fields)
- Nested/flattened configs scale linearly
- Secret masking adds minimal overhead

Run benchmarks: `cargo bench --bench config_loading`

## Known Limitations

- **No hot reload** - Can't watch for config file changes
- **Zero production usage** - Untested in real-world applications

## Project Structure

```
procenv/
├── crates/
│   ├── procenv/              # Main crate (runtime + re-exports)
│   │   ├── src/
│   │   │   ├── lib.rs        # Crate root, public exports
│   │   │   ├── error.rs      # Error types with miette diagnostics
│   │   │   ├── source.rs     # Source attribution types
│   │   │   ├── loader.rs     # ConfigLoader orchestrator
│   │   │   ├── file/         # File configuration support
│   │   │   └── provider/     # Provider extensibility framework
│   │   ├── examples/
│   │   └── tests/
│   └── procenv_macro/        # Proc-macro implementation
│       └── src/
│           ├── lib.rs        # Macro entry point
│           ├── parse.rs      # Attribute parsing
│           ├── field/        # Field type classification & code gen
│           │   ├── mod.rs    # FieldGenerator trait + factory
│           │   ├── required.rs
│           │   ├── default.rs
│           │   ├── optional.rs
│           │   ├── flatten.rs
│           │   └── secret.rs
│           └── expand/       # Code generation orchestration
│               ├── mod.rs    # Expander orchestrator
│               ├── env.rs    # from_env() generation
│               ├── config.rs # from_config() generation
│               ├── args.rs   # from_args() CLI support
│               ├── runtime.rs # Runtime access (keys, get_str, has_key)
│               └── ...
├── PROGRESS.md               # Development roadmap
└── CLAUDE.md                 # AI assistant instructions
```

## Development Status

**Completed:**

- Core derive macro with error accumulation
- File configs (TOML/JSON/YAML) with source spans
- CLI integration via clap
- Profile support (dev/staging/prod)
- Source attribution for all loading methods
- Secret masking in errors and Debug
- Validation integration with `validator` crate
- Provider extensibility (custom sources like Vault, SSM)
- Runtime access (`keys()`, `get_str()`, `has_key()`)

**Planned (see [PROGRESS.md](PROGRESS.md)):**

- Phase E: Hot reload
- Phase F: Documentation & examples
- Phase G: Advanced features (interactive mode, schema export)
- Phase H: Ecosystem integration (axum, actix, tracing)
- Phase I: Production hardening

## Running Tests

```bash
# Standard test runner
cargo test

# Nextest (faster, parallel)
cargo nextest run

# Run examples
cargo run --example basic
cargo run --example source_attribution
```

## AI-Assisted Development

This project was developed with assistance from [Claude](https://claude.ai), including tests, documentation, and some implementation code. It serves as a learning exercise for proc-macro development.

## License

MIT
