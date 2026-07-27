//! A consumer-prescribed set of deployment environments.
//!
//! The `env` base field parses into `config::Env` out of the box — `local`,
//! `development`, or `production`. Services with a different set of
//! environments derive [`config::Environment`] on their own enum and select
//! it with the struct-level `#[config(env = ...)]` attribute. Unknown names
//! still fail at startup, and the generated schema and template list the
//! custom variants.
//!
//! Run with:
//!
//! ```sh
//! cargo run -p demo --example custom_env              # local, the #[default]
//! ENV=staging cargo run -p demo --example custom_env  # not a config::Env variant
//! ENV=qa cargo run -p demo --example custom_env       # fails: unknown environment
//! curl localhost:8080/env
//! ```

use tinkr_framework::{
    Server,
    config::{self, Configurable, Environment},
    routing::get,
};

/// The environments this service deploys to.
///
/// `Default` marks the variant used when neither `ENV` nor the `env` key in
/// `config.toml` is set; names resolve from the snake_cased variant names,
/// case-insensitively.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Environment)]
enum DemoEnv {
    #[default]
    Local,
    Development,
    Staging,
    Production,
}

/// Configuration with a custom environment type instead of `config::Env`.
#[derive(Debug, Configurable)]
#[config(env = DemoEnv)]
struct CustomEnvConfig {}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = tinkr_framework::init!(CustomEnvConfig)?;

    // `cfg.env` is a `DemoEnv`, so environment checks are exhaustive: adding
    // a variant later makes the compiler point at every place to revisit.
    match cfg.env {
        DemoEnv::Local => tracing::info!("running locally"),
        DemoEnv::Development | DemoEnv::Staging => {
            tracing::info!(env = %cfg.env, "pre-production deployment")
        }
        DemoEnv::Production => tracing::warn!("this demo is not meant to run in production"),
    }

    Server::new().route("/env", get(env_name)).serve().await?;
    Ok(())
}

/// Reports the resolved deployment environment.
async fn env_name() -> String {
    format!("{}\n", config::get::<CustomEnvConfig>().env)
}
