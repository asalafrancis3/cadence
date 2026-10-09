//! Soroban JSON-RPC client for the Cadence contract.
//!
//! Wraps the three keeper-relevant contract functions (`is_due`, `charge`,
//! `bump_subscription`) behind a clean async interface. Errors are classified
//! into [`KeeperError`] variants for the polling loop.

use crate::errors::{classify_contract_error, KeeperError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, trace};

// ---------------------------------------------------------------------------
// Soroban XDR helpers – minimal inline encoding
// ---------------------------------------------------------------------------

/// Encode a `u64` as a Soroban XDR SCVal JSON representation.
fn scval_u64(v: u64) -> Value {
    json!({ "type": "u64", "value": v.to_string() })
}

// ---------------------------------------------------------------------------
// JSON-RPC request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    result: Option<Value>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: Option<i64>,
    message: Option<String>,
    data: Option<Value>,
}

// ---------------------------------------------------------------------------
// Public client
// ---------------------------------------------------------------------------

/// Soroban RPC client tailored for Cadence keeper operations.
#[derive(Clone)]
pub struct SorobanClient {
    http: Client,
    rpc_url: String,
    contract_id: String,
    network_passphrase: String,
    /// Keeper signing key (hex-encoded secret seed). `None` in dry-run mode.
    keeper_secret: Option<String>,
    req_id: std::sync::atomic::AtomicU64,
}

impl SorobanClient {
    pub fn new(
        rpc_url: String,
        contract_id: String,
        network_passphrase: String,
        keeper_secret: Option<String>,
    ) -> Self {
        Self {
            http: Client::new(),
            rpc_url,
            contract_id,
            network_passphrase,
            keeper_secret,
            req_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        self.req_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }

    // ------------------------------------------------------------------
    // Low-level RPC helpers
    // ------------------------------------------------------------------

    /// Send a raw JSON-RPC request and return the parsed response.
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, KeeperError> {
        let req = RpcRequest {
            jsonrpc: "2.0",
            id: self.next_id(),
            method: method.to_string(),
            params,
        };

        trace!(method, "sending RPC request");

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&req)
            .send()
            .await
            .map_err(|e| KeeperError::Rpc(format!("HTTP error: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(KeeperError::Rpc(format!(
                "HTTP {status}: {body}"
            )));
        }

        let rpc_resp: RpcResponse = resp
            .json()
            .await
            .map_err(|e| KeeperError::Rpc(format!("failed to parse RPC response: {e}")))?;

        if let Some(err) = rpc_resp.error {
            let msg = err.message.unwrap_or_default();

            // Try to extract a Soroban contract error code from the message
            // or data. Common patterns:
            //   "HostError: Error(Contract, #10)"
            //   data.diagnosticEvents containing contract error codes
            if let Some(code) = extract_contract_error_code(&msg, &err.data) {
                return Err(classify_contract_error(code));
            }

            // Check for insufficient balance indicators
            if msg.contains("insufficient balance")
                || msg.contains("txINSUFFICIENT_BALANCE")
                || msg.contains("tx_insufficient_balance")
            {
                return Err(KeeperError::InsufficientKeeperBalance);
            }

            return Err(KeeperError::Rpc(msg));
        }

        rpc_resp
            .result
            .ok_or_else(|| KeeperError::Rpc("empty RPC result".into()))
    }

    /// Simulate a contract invocation (read-only, no fees).
    async fn simulate_invoke(
        &self,
        function_name: &str,
        args: Vec<Value>,
    ) -> Result<Value, KeeperError> {
        let params = json!({
            "sourceAccount": self.source_account_id(),
            "transaction": self.build_invoke_transaction_xdr(function_name, &args),
        });

        self.rpc_call("simulateTransaction", params).await
    }

    /// Build a minimal placeholder for the source account in simulation.
    /// In dry-run mode we use a well-known throwaway public key.
    fn source_account_id(&self) -> &str {
        // A zero-filled public key is fine for simulation only.
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF"
    }

    /// Build a base64-encoded transaction XDR for `simulateTransaction`.
    ///
    /// NOTE: This is a *simplified* representation. A production keeper would
    /// use a proper Stellar SDK to build real transaction envelopes. For this
    /// reference implementation we send the invoke parameters via the
    /// `simulateTransaction` RPC method which accepts a simplified format.
    fn build_invoke_transaction_xdr(&self, function_name: &str, args: &[Value]) -> String {
        // The Soroban RPC `simulateTransaction` endpoint can accept the
        // invocation directly in newer versions. We use the JSON body format.
        let _ = (function_name, args);
        // Placeholder — the actual XDR building is handled in the params
        // structure of each method below.
        String::new()
    }

    // ------------------------------------------------------------------
    // Contract method wrappers
    // ------------------------------------------------------------------

    /// Check whether a subscription is due for charging.
    ///
    /// Calls `is_due(sub_id)` via `simulateTransaction` (read-only).
    pub async fn is_due(&self, sub_id: u64) -> Result<bool, KeeperError> {
        debug!(sub_id, "checking is_due");

        let params = json!({
            "sourceAccount": self.source_account_id(),
            "invokeContractArgs": {
                "contractId": self.contract_id,
                "functionName": "is_due",
                "args": [scval_u64(sub_id)]
            }
        });

        let result = self.rpc_call("simulateTransaction", params).await?;

        // Parse the simulation result to extract the boolean return value.
        // The result is in `results[0].xdr` or `results[0].retval`.
        parse_bool_result(&result)
    }

    /// Submit `charge(sub_id)` to the network.
    ///
    /// In dry-run mode this should **not** be called; the bot logs instead.
    pub async fn charge(&self, sub_id: u64) -> Result<ChargeOutcome, KeeperError> {
        debug!(sub_id, "submitting charge");

        if self.keeper_secret.is_none() {
            return Err(KeeperError::Other(
                "cannot submit charge without KEEPER_SECRET_KEY".into(),
            ));
        }

        let params = json!({
            "sourceAccount": self.source_account_id(),
            "invokeContractArgs": {
                "contractId": self.contract_id,
                "functionName": "charge",
                "args": [scval_u64(sub_id)]
            },
            "networkPassphrase": self.network_passphrase,
            "secretKey": self.keeper_secret,
        });

        let result = self.rpc_call("sendTransaction", params).await?;

        parse_charge_result(&result)
    }

    /// Submit `bump_subscription(sub_id)` to the network.
    pub async fn bump_subscription(&self, sub_id: u64) -> Result<(), KeeperError> {
        debug!(sub_id, "submitting bump_subscription");

        if self.keeper_secret.is_none() {
            return Err(KeeperError::Other(
                "cannot submit bump_subscription without KEEPER_SECRET_KEY".into(),
            ));
        }

        let params = json!({
            "sourceAccount": self.source_account_id(),
            "invokeContractArgs": {
                "contractId": self.contract_id,
                "functionName": "bump_subscription",
                "args": [scval_u64(sub_id)]
            },
            "networkPassphrase": self.network_passphrase,
            "secretKey": self.keeper_secret,
        });

        let _result = self.rpc_call("sendTransaction", params).await?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Result parsing helpers
// ---------------------------------------------------------------------------

/// Outcome of a successful `charge` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeOutcome {
    /// Funds moved, subscription advanced.
    Charged,
    /// Allowance/balance insufficient — failure recorded on-chain.
    Failed,
    /// Too many failures — subscription auto-cancelled.
    Cancelled,
}

/// Try to extract a contract error code from an RPC error message or data.
///
/// Soroban RPC errors typically contain patterns like:
///   `"HostError: Error(Contract, #10)"`
fn extract_contract_error_code(message: &str, data: &Option<Value>) -> Option<u32> {
    // Pattern: "Error(Contract, #<code>)"
    if let Some(pos) = message.find("Error(Contract, #") {
        let rest = &message[pos + 17..];
        if let Some(end) = rest.find(')') {
            if let Ok(code) = rest[..end].parse::<u32>() {
                return Some(code);
            }
        }
    }

    // Also check the data field for diagnostic events
    if let Some(data) = data {
        let data_str = data.to_string();
        if let Some(pos) = data_str.find("Error(Contract, #") {
            let rest = &data_str[pos + 17..];
            if let Some(end) = rest.find(')') {
                if let Ok(code) = rest[..end].parse::<u32>() {
                    return Some(code);
                }
            }
        }
    }

    None
}

/// Parse a boolean result from a `simulateTransaction` response.
fn parse_bool_result(result: &Value) -> Result<bool, KeeperError> {
    // The Soroban RPC returns results in various nested formats.
    // We try several common paths.

    // Path 1: result.results[0].xdr — decode from XDR
    // Path 2: result.results[0].retval.value (bool)
    // Path 3: result.retval

    if let Some(results) = result.get("results").and_then(|r| r.as_array()) {
        if let Some(first) = results.first() {
            // Check retval
            if let Some(retval) = first.get("retval") {
                if let Some(val) = retval.get("value") {
                    if let Some(b) = val.as_bool() {
                        return Ok(b);
                    }
                    // SCVal true is sometimes encoded as { "type": "bool", "value": true }
                    if let Some(s) = val.as_str() {
                        return Ok(s == "true");
                    }
                }
                // Direct boolean in retval
                if let Some(b) = retval.as_bool() {
                    return Ok(b);
                }
            }
        }
    }

    // Fallback: look at the top-level result for error indicators
    if let Some(error) = result.get("error") {
        let msg = error.as_str().unwrap_or("unknown simulation error");
        if let Some(code) = extract_contract_error_code(msg, &None) {
            return Err(classify_contract_error(code));
        }
        return Err(KeeperError::Rpc(msg.to_string()));
    }

    // If we can't parse the result, assume not due (safe default)
    debug!(?result, "could not parse is_due result, assuming false");
    Ok(false)
}

/// Parse the outcome of a `charge` call from a `sendTransaction` response.
fn parse_charge_result(result: &Value) -> Result<ChargeOutcome, KeeperError> {
    // Check transaction status
    let status = result
        .get("status")
        .and_then(|s| s.as_str())
        .unwrap_or("");

    match status {
        "SUCCESS" | "PENDING" => {
            // Try to read the return value to distinguish Charged / Failed / Cancelled
            if let Some(return_value) = result.get("returnValue") {
                let val_str = return_value.to_string();
                if val_str.contains("Charged") {
                    return Ok(ChargeOutcome::Charged);
                } else if val_str.contains("Cancelled") {
                    return Ok(ChargeOutcome::Cancelled);
                } else if val_str.contains("Failed") {
                    return Ok(ChargeOutcome::Failed);
                }
            }
            // Default to Charged if status is success
            Ok(ChargeOutcome::Charged)
        }
        "ERROR" | "FAILED" => {
            let msg = result
                .get("errorResultXdr")
                .or_else(|| result.get("error"))
                .and_then(|v| v.as_str())
                .unwrap_or("transaction failed");

            if let Some(code) = extract_contract_error_code(msg, &None) {
                return Err(classify_contract_error(code));
            }
            Err(KeeperError::Rpc(msg.to_string()))
        }
        _ => {
            // Unknown status
            debug!(?result, "unknown transaction status");
            Ok(ChargeOutcome::Charged)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_error_code_from_message() {
        let msg = "HostError: Error(Contract, #10)";
        assert_eq!(extract_contract_error_code(msg, &None), Some(10));
    }

    #[test]
    fn extract_error_code_from_data() {
        let data = json!({"diagnostic": "Error(Contract, #1)"});
        assert_eq!(
            extract_contract_error_code("something else", &Some(data)),
            Some(1)
        );
    }

    #[test]
    fn extract_error_code_missing() {
        assert_eq!(extract_contract_error_code("no error here", &None), None);
    }

    #[test]
    fn parse_bool_result_true() {
        let result = json!({
            "results": [{"retval": {"type": "bool", "value": true}}]
        });
        assert!(parse_bool_result(&result).unwrap());
    }

    #[test]
    fn parse_bool_result_false() {
        let result = json!({
            "results": [{"retval": {"type": "bool", "value": false}}]
        });
        assert!(!parse_bool_result(&result).unwrap());
    }

    #[test]
    fn parse_bool_result_with_string_value() {
        let result = json!({
            "results": [{"retval": {"type": "bool", "value": "true"}}]
        });
        assert!(parse_bool_result(&result).unwrap());
    }

    #[test]
    fn parse_bool_result_with_error() {
        let result = json!({
            "error": "HostError: Error(Contract, #10)"
        });
        let err = parse_bool_result(&result).unwrap_err();
        assert_eq!(err, KeeperError::NotDue);
    }

    #[test]
    fn parse_charge_success() {
        let result = json!({
            "status": "SUCCESS",
            "returnValue": {"type": "enum", "value": "Charged"}
        });
        assert_eq!(parse_charge_result(&result).unwrap(), ChargeOutcome::Charged);
    }

    #[test]
    fn parse_charge_failed() {
        let result = json!({
            "status": "SUCCESS",
            "returnValue": {"type": "enum", "value": "Failed"}
        });
        assert_eq!(parse_charge_result(&result).unwrap(), ChargeOutcome::Failed);
    }

    #[test]
    fn parse_charge_error() {
        let result = json!({
            "status": "ERROR",
            "error": "HostError: Error(Contract, #1)"
        });
        let err = parse_charge_result(&result).unwrap_err();
        assert_eq!(err, KeeperError::PausedContract);
    }
}
