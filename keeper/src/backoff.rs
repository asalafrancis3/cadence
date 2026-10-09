//! Exponential backoff with full jitter for transient error retries.
//!
//! Formula: `delay = min(max_delay, min_delay × factor^attempt) × rand(0.0, 1.0)`

use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing::warn;

/// Configuration for exponential backoff.
#[derive(Debug, Clone)]
pub struct BackoffConfig {
    /// Base delay (before exponentiation).
    pub min_delay: Duration,
    /// Ceiling — the delay will never exceed this.
    pub max_delay: Duration,
    /// Multiplicative factor per attempt.
    pub factor: f64,
    /// How many times to retry (does **not** count the initial attempt).
    pub max_retries: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            min_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(120),
            factor: 2.0,
            max_retries: 5,
        }
    }
}

/// Compute the jittered delay for a given `attempt` (0-indexed).
///
/// `delay = min(max_delay, min_delay × factor^attempt) × rand(0.0, 1.0)`
pub fn jittered_delay(cfg: &BackoffConfig, attempt: u32) -> Duration {
    let base = cfg.min_delay.as_secs_f64() * cfg.factor.powi(attempt as i32);
    let capped = base.min(cfg.max_delay.as_secs_f64());
    let jitter: f64 = rand::thread_rng().gen_range(0.0..=1.0);
    Duration::from_secs_f64(capped * jitter)
}

/// Retry an async operation with exponential backoff + full jitter.
///
/// Returns `Ok(T)` on the first successful attempt, or the last `Err(E)` if
/// all `max_retries + 1` attempts fail.
pub async fn retry_with_backoff<F, Fut, T, E>(cfg: &BackoffConfig, label: &str, mut f: F) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last_err: Option<E> = None;

    for attempt in 0..=cfg.max_retries {
        match f().await {
            Ok(val) => return Ok(val),
            Err(e) => {
                if attempt < cfg.max_retries {
                    let delay = jittered_delay(cfg, attempt);
                    warn!(
                        %label,
                        attempt = attempt + 1,
                        max_retries = cfg.max_retries,
                        delay_ms = delay.as_millis() as u64,
                        error = %e,
                        "transient error, retrying after backoff"
                    );
                    sleep(delay).await;
                }
                last_err = Some(e);
            }
        }
    }

    Err(last_err.expect("loop ran at least once"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jittered_delay_respects_bounds() {
        let cfg = BackoffConfig {
            min_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            factor: 2.0,
            max_retries: 10,
        };

        for attempt in 0..10 {
            let delay = jittered_delay(&cfg, attempt);
            // Must be non-negative.
            assert!(delay >= Duration::ZERO);
            // Must never exceed max_delay.
            assert!(
                delay <= cfg.max_delay,
                "attempt {attempt}: {delay:?} exceeded max {:?}",
                cfg.max_delay
            );
        }
    }

    #[test]
    fn jittered_delay_grows_with_attempts() {
        let cfg = BackoffConfig {
            min_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(3600),
            factor: 2.0,
            max_retries: 10,
        };

        // The *cap* (before jitter) should grow; with many samples the mean
        // should be higher for later attempts.
        let samples = 200;
        let mean = |attempt: u32| -> f64 {
            (0..samples)
                .map(|_| jittered_delay(&cfg, attempt).as_secs_f64())
                .sum::<f64>()
                / samples as f64
        };

        let m0 = mean(0);
        let m5 = mean(5);
        // 2^5 = 32× difference in cap, so the mean should be at least 4× higher.
        assert!(
            m5 > m0 * 2.0,
            "expected mean@5 ({m5:.2}) >> mean@0 ({m0:.2})"
        );
    }

    #[test]
    fn delay_caps_at_max() {
        let cfg = BackoffConfig {
            min_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(30),
            factor: 100.0,
            max_retries: 5,
        };

        for _ in 0..50 {
            let d = jittered_delay(&cfg, 4);
            assert!(d <= cfg.max_delay);
        }
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let cfg = BackoffConfig {
            max_retries: 3,
            ..Default::default()
        };
        let result: Result<i32, String> =
            retry_with_backoff(&cfg, "test", || async { Ok(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_returns_last_error_after_exhaustion() {
        let cfg = BackoffConfig {
            min_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
            factor: 1.0,
            max_retries: 2,
        };
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = counter.clone();

        let result: Result<(), String> = retry_with_backoff(&cfg, "test", || {
            let c = c.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("boom".to_string())
            }
        })
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "boom");
        // initial + 2 retries = 3 total
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let cfg = BackoffConfig {
            min_delay: Duration::from_millis(1),
            max_delay: Duration::from_millis(5),
            factor: 1.0,
            max_retries: 5,
        };
        let counter = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let c = counter.clone();

        let result: Result<&str, String> = retry_with_backoff(&cfg, "test", || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                if n < 2 {
                    Err("transient".to_string())
                } else {
                    Ok("ok")
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), "ok");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 3);
    }
}
