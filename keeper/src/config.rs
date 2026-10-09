//! Configuration for the Cadence keeper bot.
//!
//! All settings can be supplied via CLI flags **or** environment variables
//! (the flag always wins). Load a `.env` file with [`dotenvy`] before
//! parsing to support twelve-factor-style config.

use clap::Parser;

/// Default Soroban Testnet RPC endpoint.
const DEFAULT_RPC_URL: &str = "https://soroban-testnet.stellar.org";

/// Default Stellar Testnet passphrase.
const DEFAULT_PASSPHRASE: &str = "Test SDF Network ; September 2015";

/// Cadence keeper bot — polls `is_due`, submits `charge` + `bump_subscription`.
#[derive(Parser, Debug, Clone)]
#[command(name = "keeper", about = "Cadence subscription keeper bot")]
pub struct Config {
    /// Soroban JSON-RPC endpoint URL.
    #[arg(long, env = "RPC_URL", default_value = DEFAULT_RPC_URL)]
    pub rpc_url: String,

    /// Stellar network passphrase.
    #[arg(long, env = "NETWORK_PASSPHRASE", default_value = DEFAULT_PASSPHRASE)]
    pub network_passphrase: String,

    /// Cadence contract address (C…).
    #[arg(long, env = "CADENCE_CONTRACT_ID")]
    pub contract_id: String,

    /// Keeper account secret key (S…). Optional in `--dry-run` mode.
    #[arg(long, env = "KEEPER_SECRET_KEY")]
    pub keeper_secret: Option<String>,

    /// Comma-separated subscription IDs to monitor.
    #[arg(long, env = "SUB_IDS", value_delimiter = ',')]
    pub sub_ids: Vec<u64>,

    /// Seconds between polling rounds.
    #[arg(long, env = "POLL_INTERVAL_SECS", default_value_t = 60)]
    pub poll_interval_secs: u64,

    /// Maximum retries on transient RPC / network errors.
    #[arg(long, env = "MAX_RETRIES", default_value_t = 5)]
    pub max_retries: u32,

    /// Log intended actions without signing or submitting transactions.
    #[arg(long, env = "DRY_RUN", default_value_t = false)]
    pub dry_run: bool,
}

impl Config {
    /// Parse configuration from CLI args + env, loading `.env` first.
    pub fn load() -> Self {
        // Best-effort: ignore if .env is missing.
        let _ = dotenvy::dotenv();
        Config::parse()
    }

    /// Validate that required fields are present for the chosen mode.
    pub fn validate(&self) -> Result<(), String> {
        if self.contract_id.is_empty() {
            return Err("CADENCE_CONTRACT_ID is required".into());
        }
        if self.sub_ids.is_empty() {
            return Err("SUB_IDS must contain at least one subscription ID".into());
        }
        if !self.dry_run && self.keeper_secret.is_none() {
            return Err(
                "KEEPER_SECRET_KEY is required in live mode (use --dry-run to skip)".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_does_not_require_secret() {
        let cfg = Config {
            rpc_url: DEFAULT_RPC_URL.into(),
            network_passphrase: DEFAULT_PASSPHRASE.into(),
            contract_id: "CABC123".into(),
            keeper_secret: None,
            sub_ids: vec![1],
            poll_interval_secs: 60,
            max_retries: 5,
            dry_run: true,
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn live_mode_requires_secret() {
        let cfg = Config {
            rpc_url: DEFAULT_RPC_URL.into(),
            network_passphrase: DEFAULT_PASSPHRASE.into(),
            contract_id: "CABC123".into(),
            keeper_secret: None,
            sub_ids: vec![1],
            poll_interval_secs: 60,
            max_retries: 5,
            dry_run: false,
        };
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn empty_sub_ids_rejected() {
        let cfg = Config {
            rpc_url: DEFAULT_RPC_URL.into(),
            network_passphrase: DEFAULT_PASSPHRASE.into(),
            contract_id: "CABC123".into(),
            keeper_secret: Some("SXXX".into()),
            sub_ids: vec![],
            poll_interval_secs: 60,
            max_retries: 5,
            dry_run: false,
        };
        assert!(cfg.validate().is_err());
    }
}
