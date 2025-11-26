# procenv

A Rust procedural macro for declarative environment variable configuration.

## Features

- **Error Accumulation** - Shows ALL config errors at once (unique to procenv)
- **Rich Diagnostics** - Powered by `miette` with helpful error messages
- **Secret Masking** - Sensitive values hidden in Debug/error output
- **Type Conversion** - Automatic parsing via `FromStr` trait
- **Default Values** - Fallback values when env vars are missing
- **Optional Fields** - `Option<T>` fields that become `None` when missing
- **.env.example Generation** - Auto-generate documentation for your config

## Usage

```rust
use procenv::EnvConfig;

#[derive(EnvConfig)]
struct Config {
    #[env(var = "DATABASE_URL")]
    db_url: String,

    #[env(var = "PORT", default = "8080")]
    port: u16,

    #[env(var = "API_KEY", optional)]
    api_key: Option<String>,

    #[env(var = "SECRET_TOKEN", secret)]
    secret: String,
}

fn main() -> Result<(), procenv::Error> {
    let config = Config::from_env()?;
    println!("Connected to: {}", config.db_url);
    Ok(())
}
```

## Attribute Syntax

| Attribute                                 | Description                             |
| ----------------------------------------- | --------------------------------------- |
| `#[env(var = "NAME")]`                    | Required field, errors if missing       |
| `#[env(var = "NAME", default = "value")]` | Uses default if env var is missing      |
| `#[env(var = "NAME", optional)]`          | Field is `Option<T>`, `None` if missing |
| `#[env(var = "NAME", secret)]`            | Masks value in Debug/error output       |

Combine attributes: `#[env(var = "KEY", secret, optional)]`

## Running Tests

```bash
# Install nextest (recommended)
cargo install cargo-nextest

# Run all tests
cargo nextest run

# Run specific package
cargo nextest run --package procenv
```

## Example

```bash
# Shows error accumulation for missing vars
cargo run --package procenv --example basic

# With required variables
DATABASE_URL=postgres://localhost SECRET=mysecret \
  cargo run --package procenv --example basic
```

## Project Structure

```
procenv/
├── crates/
│   ├── procenv/              # Main crate (re-exports macro + error types)
│   │   ├── src/lib.rs
│   │   ├── examples/
│   │   └── tests/
│   └── procenv_macro/        # Proc-macro implementation
│       └── src/
│           ├── lib.rs        # Macro entry point
│           ├── parse.rs      # Attribute parsing
│           ├── field.rs      # Field code generation
│           └── expand.rs     # Macro expansion
├── PROGRESS.md               # Development roadmap
└── README.md
```

## Development Status

**Current Phase:** A.0 - Correctness Sprint

**Working:**
- Core derive macro with env var loading
- Error accumulation (all errors at once)
- miette diagnostics with helpful messages
- Secret masking in errors/debug output
- .env.example generation
- File config support (TOML/JSON/YAML)
- CLI argument integration

**Known Issues:**
- Source attribution incomplete for profiles and nested configs
- `from_config()` doesn't honor macro-level defaults

See [PROGRESS.md](PROGRESS.md) for full roadmap.

## License

MIT
