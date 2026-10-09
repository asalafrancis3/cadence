//! Error types for the keeper bot.
//!
//! Errors are classified into categories that drive retry / skip / exit
//! decisions in the polling loop.

use std::fmt;

/// High-level classification of errors returned by the Cadence contract or the
/// RPC layer. The keeper polling loop uses this to decide what to do next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeeperError {
    /// The subscription is not yet due (`contract error code 10`). Skip
    /// immediately without retry.
    NotDue,

    /// The protocol is paused by the admin (`contract error code 1`). Skip
    /// the subscription and defer the next check.
    PausedContract,

    /// The subscriber's token allowance or balance is exhausted (`charge`
    /// returned `Failed` or `Cancelled`). The subscriber must re-approve;
    /// there is nothing the keeper can do.
    ExhaustedAllowance,

    /// The subscription is not active (cancelled / completed,
    /// `contract error code 9`).
    SubscriptionNotActive,

    /// Retry cooldown has not elapsed (`contract error code 11`).
    RetryCooldown,

    /// Subscription not found (`contract error code 8`).
    SubscriptionNotFound,

    /// Transient RPC or network error — eligible for exponential backoff retry.
    Rpc(String),

    /// The keeper account has insufficient balance to pay transaction fees.
    /// This is a **critical** error and the process should exit.
    InsufficientKeeperBalance,

    /// Any other unrecoverable error.
    Other(String),
}

impl fmt::Display for KeeperError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDue => write!(f, "subscription is not due"),
            Self::PausedContract => write!(f, "contract is paused"),
            Self::ExhaustedAllowance => write!(f, "subscriber allowance/balance exhausted"),
            Self::SubscriptionNotActive => write!(f, "subscription is not active"),
            Self::RetryCooldown => write!(f, "retry cooldown has not elapsed"),
            Self::SubscriptionNotFound => write!(f, "subscription not found"),
            Self::Rpc(msg) => write!(f, "RPC error: {msg}"),
            Self::InsufficientKeeperBalance => write!(f, "keeper account balance too low for fees"),
            Self::Other(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for KeeperError {}

/// Cadence contract error codes (from `contracts/cadence/src/errors.rs`).
/// Used to classify simulate/invoke responses.
pub mod contract_codes {
    pub const PAUSED: u32 = 1;
    pub const SUBSCRIPTION_NOT_FOUND: u32 = 8;
    pub const SUBSCRIPTION_NOT_ACTIVE: u32 = 9;
    pub const NOT_DUE: u32 = 10;
    pub const RETRY_COOLDOWN: u32 = 11;
}

/// Map a Soroban contract error code to a [`KeeperError`].
pub fn classify_contract_error(code: u32) -> KeeperError {
    match code {
        contract_codes::PAUSED => KeeperError::PausedContract,
        contract_codes::NOT_DUE => KeeperError::NotDue,
        contract_codes::SUBSCRIPTION_NOT_ACTIVE => KeeperError::SubscriptionNotActive,
        contract_codes::RETRY_COOLDOWN => KeeperError::RetryCooldown,
        contract_codes::SUBSCRIPTION_NOT_FOUND => KeeperError::SubscriptionNotFound,
        other => KeeperError::Other(format!("contract error code {other}")),
    }
}

/// Returns `true` if the error is transient and should be retried.
pub fn is_transient(err: &KeeperError) -> bool {
    matches!(err, KeeperError::Rpc(_))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_known_codes() {
        assert_eq!(classify_contract_error(1), KeeperError::PausedContract);
        assert_eq!(classify_contract_error(8), KeeperError::SubscriptionNotFound);
        assert_eq!(classify_contract_error(9), KeeperError::SubscriptionNotActive);
        assert_eq!(classify_contract_error(10), KeeperError::NotDue);
        assert_eq!(classify_contract_error(11), KeeperError::RetryCooldown);
    }

    #[test]
    fn classify_unknown_code() {
        let err = classify_contract_error(99);
        assert!(matches!(err, KeeperError::Other(_)));
    }

    #[test]
    fn transient_classification() {
        assert!(is_transient(&KeeperError::Rpc("timeout".into())));
        assert!(!is_transient(&KeeperError::NotDue));
        assert!(!is_transient(&KeeperError::PausedContract));
        assert!(!is_transient(&KeeperError::InsufficientKeeperBalance));
    }
}
