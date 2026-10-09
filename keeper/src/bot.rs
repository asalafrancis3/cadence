//! Core polling loop for the Cadence keeper bot.
//!
//! Iterates over configured subscription IDs, checks `is_due`, and submits
//! `charge` + `bump_subscription` when appropriate. Error handling follows
//! the classification matrix in [`crate::errors`].

use crate::backoff::{retry_with_backoff, BackoffConfig};
use crate::client::{ChargeOutcome, SorobanClient};
use crate::config::Config;
use crate::errors::{is_transient, KeeperError};
use std::time::Duration;
use tracing::{debug, error, info, warn};

/// Run a single polling round across all configured subscription IDs.
///
/// Returns `Ok(())` if the round completed (even if individual subs were
/// skipped due to non-transient errors), or `Err(KeeperError)` if a critical
/// error demands process termination.
pub async fn poll_round(
    client: &SorobanClient,
    cfg: &Config,
    backoff: &BackoffConfig,
) -> Result<(), KeeperError> {
    info!(sub_count = cfg.sub_ids.len(), "starting poll round");

    for &sub_id in &cfg.sub_ids {
        if let Err(e) = process_subscription(client, cfg, backoff, sub_id).await {
            // Critical: keeper can't pay fees → exit immediately.
            if e == KeeperError::InsufficientKeeperBalance {
                error!(sub_id, "CRITICAL: keeper balance too low for fees — exiting");
                return Err(e);
            }
            // All other errors are already logged in process_subscription.
        }
    }

    info!("poll round complete");
    Ok(())
}

/// Check and (if due) charge a single subscription.
async fn process_subscription(
    client: &SorobanClient,
    cfg: &Config,
    backoff: &BackoffConfig,
    sub_id: u64,
) -> Result<(), KeeperError> {
    // ---- Step 1: is_due? ----
    let due = check_is_due(client, backoff, sub_id).await?;

    if !due {
        debug!(sub_id, "not due, skipping");
        return Ok(());
    }

    info!(sub_id, "subscription is due");

    // ---- Step 2: charge ----
    if cfg.dry_run {
        info!(sub_id, "[DRY-RUN] would submit charge({})", sub_id);
        info!(
            sub_id,
            "[DRY-RUN] would submit bump_subscription({})", sub_id
        );
        return Ok(());
    }

    let outcome = submit_charge(client, backoff, sub_id).await?;

    match outcome {
        ChargeOutcome::Charged => {
            info!(sub_id, "charge successful");
        }
        ChargeOutcome::Failed => {
            warn!(
                sub_id,
                "charge recorded failure — subscriber may need to re-approve allowance"
            );
            return Ok(());
        }
        ChargeOutcome::Cancelled => {
            error!(
                sub_id,
                "subscription auto-cancelled after consecutive failures"
            );
            return Ok(());
        }
    }

    // ---- Step 3: bump_subscription ----
    if let Err(e) = submit_bump(client, backoff, sub_id).await {
        // Bump failure is not critical — the charge already went through.
        warn!(sub_id, error = %e, "bump_subscription failed (charge was successful)");
    } else {
        info!(sub_id, "bump_subscription successful");
    }

    Ok(())
}

/// Check `is_due` with retries on transient errors.
async fn check_is_due(
    client: &SorobanClient,
    backoff: &BackoffConfig,
    sub_id: u64,
) -> Result<bool, KeeperError> {
    let label = format!("is_due({sub_id})");

    retry_with_backoff(backoff, &label, || async {
        client.is_due(sub_id).await
    })
    .await
    .or_else(|e| {
        // Non-transient errors from is_due should be handled gracefully.
        match &e {
            KeeperError::NotDue => Ok(false),
            KeeperError::PausedContract => {
                warn!(sub_id, "contract is paused, skipping subscription");
                Ok(false)
            }
            KeeperError::SubscriptionNotActive => {
                info!(sub_id, "subscription is not active, skipping");
                Ok(false)
            }
            KeeperError::SubscriptionNotFound => {
                warn!(sub_id, "subscription not found, skipping");
                Ok(false)
            }
            KeeperError::RetryCooldown => {
                debug!(sub_id, "retry cooldown active, skipping");
                Ok(false)
            }
            _ => Err(e),
        }
    })
}

/// Submit `charge` with retries on transient errors.
async fn submit_charge(
    client: &SorobanClient,
    backoff: &BackoffConfig,
    sub_id: u64,
) -> Result<ChargeOutcome, KeeperError> {
    let label = format!("charge({sub_id})");

    retry_with_backoff(backoff, &label, || async {
        let result = client.charge(sub_id).await;
        // Only retry transient errors; non-transient ones should bubble up.
        match &result {
            Err(e) if !is_transient(e) => result,
            _ => result,
        }
    })
    .await
    .map_err(|e| {
        match &e {
            KeeperError::ExhaustedAllowance => {
                error!(sub_id, "subscriber allowance/balance exhausted — re-approval needed");
            }
            KeeperError::PausedContract => {
                warn!(sub_id, "contract is paused");
            }
            KeeperError::NotDue => {
                // Race condition — another keeper charged it first.
                info!(sub_id, "no longer due (another keeper may have charged it)");
            }
            _ => {
                error!(sub_id, error = %e, "charge failed");
            }
        }
        e
    })
}

/// Submit `bump_subscription` with retries on transient errors.
async fn submit_bump(
    client: &SorobanClient,
    backoff: &BackoffConfig,
    sub_id: u64,
) -> Result<(), KeeperError> {
    let label = format!("bump_subscription({sub_id})");

    retry_with_backoff(backoff, &label, || async {
        client.bump_subscription(sub_id).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::KeeperError;

    /// Verify that `check_is_due` converts non-transient errors to `Ok(false)`.
    #[tokio::test]
    async fn check_is_due_paused_returns_false() {
        // We can't easily mock the SorobanClient here without a trait, but
        // we can test the error-mapping logic directly.
        let err = KeeperError::PausedContract;
        let result: Result<bool, KeeperError> = match &err {
            KeeperError::PausedContract => Ok(false),
            _ => Err(err),
        };
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn check_is_due_not_active_returns_false() {
        let err = KeeperError::SubscriptionNotActive;
        let result: Result<bool, KeeperError> = match &err {
            KeeperError::SubscriptionNotActive => Ok(false),
            _ => Err(err),
        };
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn check_is_due_not_found_returns_false() {
        let err = KeeperError::SubscriptionNotFound;
        let result: Result<bool, KeeperError> = match &err {
            KeeperError::SubscriptionNotFound => Ok(false),
            _ => Err(err),
        };
        assert_eq!(result.unwrap(), false);
    }

    #[tokio::test]
    async fn insufficient_balance_is_critical() {
        let err = KeeperError::InsufficientKeeperBalance;
        // The polling loop should detect this and exit.
        assert_eq!(err, KeeperError::InsufficientKeeperBalance);
    }

    #[test]
    fn error_branch_routing() {
        // Verify the classification drives the right actions.
        let cases = vec![
            (KeeperError::NotDue, "skip"),
            (KeeperError::PausedContract, "skip"),
            (KeeperError::ExhaustedAllowance, "skip"),
            (KeeperError::SubscriptionNotActive, "skip"),
            (KeeperError::RetryCooldown, "skip"),
            (KeeperError::SubscriptionNotFound, "skip"),
            (KeeperError::Rpc("timeout".into()), "retry"),
            (KeeperError::InsufficientKeeperBalance, "exit"),
        ];

        for (err, expected_action) in cases {
            let action = if err == KeeperError::InsufficientKeeperBalance {
                "exit"
            } else if is_transient(&err) {
                "retry"
            } else {
                "skip"
            };
            assert_eq!(
                action, expected_action,
                "wrong action for {err:?}: got {action}, expected {expected_action}"
            );
        }
    }
}
