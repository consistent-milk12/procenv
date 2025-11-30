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

- **Rust nightly** (uses 2024 edition features)
- `let_chains` and `if_let_guard` unstable features

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

**Note:** Not yet published to crates.io. Clone the repo to use.

```toml
[dependencies]
# Default features include: dotenv, secrecy, clap, file-all (toml/yaml/json)
procenv = { path = "path/to/procenv" }

# Minimal (no default features)
procenv = { path = "path/to/procenv", default-features = false }

# With validation support
procenv = { path = "path/to/procenv", features = ["validator"] }
```

### Feature Flags

Default features: `secrecy`, `dotenv`, `file-all`, `clap`

| Feature     | Default  | Description                                     |
| ----------- | -------- | ----------------------------------------------- |
| `secrecy`   | **Yes**  | `SecretString` support for sensitive data       |
| `dotenv`    | **Yes**  | Load `.env` files automatically                 |
| `clap`      | **Yes**  | CLI argument integration                        |
| `file-all`  | **Yes**  | Meta-feature: enables `toml` + `yaml` + `json`  |
| `file`      | via above| Base file config (JSON); enabled by format features |
| `toml`      | via above| TOML file support (implies `file`)              |
| `yaml`      | via above| YAML file support (implies `file`)              |
| `json`      | via above| JSON file support (implies `file`)              |
| `validator` | No       | Validation integration with `validator` crate   |
| `serde`     | No       | Standalone serde support (without file loading) |
| `tracing`   | No       | Tracing instrumentation                         |
| `provider`  | No       | Provider trait and ConfigLoader                 |
| `async`     | No       | Async provider support (requires `provider`)    |
| `full`      | No       | Enable all features                             |

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
| Runtime value access    | No           | Yes        | Yes        | No     |
| Hot reload              | No           | No         | Partial    | No     |

## Known Limitations

- **All-or-nothing loading** - No runtime access to individual config values
- **No hot reload** - Can't watch for config file changes
- **Zero production usage** - Untested in real-world applications
- **Requires nightly** - Uses unstable Rust features

## Project Structure

```
procenv/
├── crates/
│   ├── procenv/           # Main crate (runtime + re-exports)
│   │   ├── src/
│   │   │   ├── lib.rs     # Error types, Source enum
│   │   │   └── file.rs    # ConfigBuilder, file parsing
│   │   ├── examples/
│   │   └── tests/
│   └── procenv_macro/     # Proc-macro implementation
│       └── src/
│           ├── lib.rs     # Macro entry point
│           ├── parse.rs   # Attribute parsing
│           ├── field.rs   # Field code generation
│           └── expand.rs  # Macro expansion
├── PROGRESS.md            # Development roadmap
└── CLAUDE.md              # AI assistant instructions
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

**Planned (see [PROGRESS.md](PROGRESS.md)):**

- Phase D: Runtime value access
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
