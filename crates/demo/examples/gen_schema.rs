//! Regenerates `config.schema.json` from [`demo::config::AppConfig`] so
//! editors can validate and complete `config.toml`.
//!
//! Run `just schema` (or, from `crates/demo`, `cargo run --example
//! gen_schema`) after changing the configuration structs and commit the
//! result — CI fails when the committed schema drifts.

use demo::config::AppConfig;
use tinkr_framework::config;

fn main() -> std::io::Result<()> {
    config::write_schema::<AppConfig>("config.schema.json")?;
    println!("wrote config.schema.json");
    Ok(())
}
