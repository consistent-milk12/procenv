# procenv

A Rust derive macro for configuration that shows you _all_ your errors at once.

> **NOTE:** This is a learning project. For production, check out [figment](https://docs.rs/figment) or [config-rs](https://docs.rs/config).

## The Problem It Solves

Most config libraries stop at the first error. You fix it, run again, hit another error, fix that... procenv just shows you everything that's wrong:

```
Error: 3 configuration error(s) occurred

  × missing required environment variable: DATABASE_URL
  × failed to parse PORT: expected u16, got "not_a_number"
  × missing required environment variable: SECRET
```

File configs get source spans pointing to the exact line:

```
   ╭─[config.toml:2:8]
 2 │ port = "not_a_number"
   ·        ───────┬──────
   ·               ╰── expected u16
   ╰────
```

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

    #[env(var = "SECRET", secret)]  // Masked in errors
    secret: String,
}

fn main() -> Result<(), procenv::Error> {
    let config = Config::from_env()?;
    println!("Running on port {}", config.port);
    Ok(())
}
```

## Installation

```toml
[dependencies]
procenv = "0.1"  # Just env vars + dotenv

# Or with file configs (no serde needed on your structs!)
procenv = { version = "0.1", features = ["file-all"] }

# Everything
procenv = { version = "0.1", features = ["full"] }
```

Requires **Rust 1.91.1+** (uses 2024 edition).

## What You Get

The `#[derive(EnvConfig)]` macro generates:

- `from_env()` - Load from environment
- `from_config()` - Load from files + env (with `file` feature)
- `from_args()` - Load from CLI + env (with `clap` feature)
- `env_example()` - Generate `.env.example` docs
- A `Debug` impl that redacts secrets

## Features

| Feature            | What it does                          |
| ------------------ | ------------------------------------- |
| `dotenv` (default) | Load `.env` files                     |
| `file-all`         | TOML/JSON/YAML support (serde-free!)  |
| `clap`             | CLI argument generation               |
| `validator`        | Validation via `validator` crate      |
| `secrecy`          | `SecretString` for runtime protection |
| `watch`            | Hot reload on file changes            |

## File Configs

Load from TOML/JSON/YAML without adding serde to your structs:

```rust
#[derive(EnvConfig)]
#[env_config(prefix = "APP_", file_optional = "config.toml")]
struct Config {
    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(flatten)]  // Nested configs work too
    database: DatabaseConfig,
}

let config = Config::from_config()?;  // files + env layered
```

## Profiles

```rust
#[derive(EnvConfig)]
#[env_config(profile_env = "APP_ENV", profiles = ["dev", "prod"])]
struct Config {
    #[env(var = "DATABASE_URL")]
    #[profile(dev = "postgres://localhost/dev", prod = "postgres://prod/db")]
    database_url: String,
}
```

## Source Tracking

See where each value came from:

```rust
let (config, sources) = Config::from_env_with_sources()?;
println!("{}", sources);
```

```
  database_url  <- Environment variable [DATABASE_URL]
  port          <- Default value [PORT]
  api_key       <- .env file [API_KEY]
```

## Secret Handling

Fields marked `secret` are never shown in error messages:

```rust
#[env(var = "API_KEY", secret)]
api_key: String,  // Shows "<redacted>" in errors
```

For runtime protection, use `SecretString` (requires `secrecy` feature):

```rust
#[env(var = "API_KEY", secret)]
api_key: SecretString,  // Requires .expose_secret() to access
```

## Other Stuff

- **Validation:** Works with `validator` crate via `#[env_config(validate)]`
- **CLI:** Auto-generates clap args with `#[env(arg = "port", short = 'p')]`
- **Hot reload:** Watch files for changes with `WatchBuilder`
- **Custom providers:** Implement `Provider` trait for Vault, SSM, etc.

## Examples

```bash
cargo run --example basic
cargo run --example file_config --features file-all
cargo run --example hot_reload --features watch
```

## Status

This started as a learning project for proc-macros. It works, has decent test coverage (~345 tests), but hasn't seen real production use. The API might change.

## License

MIT
