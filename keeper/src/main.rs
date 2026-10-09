//! # Cadence Keeper Bot
//!
//! A lightweight reference keeper that monitors Cadence subscriptions,
//! calls `charge` when due, and extends storage TTLs via `bump_subscription`.
//!
//! ```text
//! cargo run -p keeper -- --contract-id CABC... --sub-ids 1,2,3 --dry-run
//! ```

mod backoff;
mod bot;
mod client;
mod config;
mod errors;

use backoff::BackoffConfig;
use client::SorobanClient;
use config::Config;
use errors::KeeperError;
use std::time::Duration;
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialise structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(true)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();

    // Load and validate configuration.
    let cfg = Config::load();
    if let Err(e) = cfg.validate() {
        error!("{e}");
        std::process::exit(1);
    }

    info!(
        contract = %cfg.contract_id,
        sub_ids = ?cfg.sub_ids,
        poll_interval = cfg.poll_interval_secs,
        dry_run = cfg.dry_run,
        max_retries = cfg.max_retries,
        "starting Cadence keeper bot"
    );

    // Build the Soroban RPC client.
    let client = SorobanClient::new(
        cfg.rpc_url.clone(),
        cfg.contract_id.clone(),
        cfg.network_passphrase.clone(),
        cfg.keeper_secret.clone(),
    );

    // Backoff configuration for transient errors.
    let backoff = BackoffConfig {
        min_delay: Duration::from_secs(1),
        max_delay: Duration::from_secs(120),
        factor: 2.0,
        max_retries: cfg.max_retries,
    };

    // Main polling loop.
    loop {
        match bot::poll_round(&client, &cfg, &backoff).await {
            Ok(()) => {}
            Err(KeeperError::InsufficientKeeperBalance) => {
                error!("CRITICAL: keeper account cannot pay transaction fees — shutting down");
                std::process::exit(1);
            }
            Err(e) => {
                error!(error = %e, "unexpected error in poll round");
            }
        }

        info!(
            interval_secs = cfg.poll_interval_secs,
            "sleeping until next poll"
        );
        tokio::time::sleep(Duration::from_secs(cfg.poll_interval_secs)).await;
    }
}
