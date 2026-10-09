# Cadence Keeper Bot

A lightweight, reference keeper bot for the [Cadence](../README.md) non-custodial subscription protocol on Soroban. It polls `is_due` for configured subscriptions, submits `charge` when a cycle is due, and calls `bump_subscription` to extend storage TTLs.

## Features

- **Permissionless** — anyone can run a keeper; `charge` requires no special authorisation
- **Dry-run mode** — logs intended actions without signing or submitting transactions
- **Exponential backoff with jitter** — robust handling of transient RPC/network errors
- **Error classification** — distinct handling for `NotDue`, `PausedContract`, `ExhaustedAllowance`, insufficient keeper balance, and RPC errors
- **Structured logging** — machine-parseable tracing output with `RUST_LOG` control

## Quick Start

### Prerequisites

- Rust (stable, 1.80+)
- A funded Stellar Testnet account (for live mode)
- A deployed Cadence contract on Testnet

### Configuration

Copy the environment template and fill in your values:

```bash
cp keeper/.env.example keeper/.env
# Edit keeper/.env with your contract ID, secret key, and subscription IDs
```

All settings can also be passed as CLI flags. Run `--help` to see all options:

```bash
cargo run -p keeper -- --help
```

### Dry-Run Mode (no secret key required)

```bash
cargo run -p keeper -- \
  --contract-id CABC123... \
  --sub-ids 1,2,3 \
  --dry-run
```

This will poll `is_due` for subscriptions 1, 2, and 3 every 60 seconds and log what it *would* do — without signing or submitting any transactions.

### Live Mode

```bash
cargo run -p keeper -- \
  --contract-id CABC123... \
  --sub-ids 1,2,3 \
  --keeper-secret SXXX...
```

### Docker

```bash
# Build
docker build -f keeper/Dockerfile -t cadence-keeper .

# Run
docker run --env-file keeper/.env cadence-keeper
```

## Architecture

```
keeper/
├── Cargo.toml          crate manifest
├── .env.example        env var template
├── Dockerfile          multi-stage production build
└── src/
    ├── main.rs         entry point, tracing init, polling loop
    ├── config.rs       clap + env configuration
    ├── bot.rs          polling engine (poll_round, process_subscription)
    ├── client.rs       Soroban JSON-RPC wrappers (is_due, charge, bump)
    ├── errors.rs       error classification matrix
    └── backoff.rs      exponential backoff with full jitter
```

## Error Handling Matrix

| Error | Action | Log Level |
|---|---|---|
| `NotDue` | Skip, no retry | `DEBUG` |
| `PausedContract` | Skip, defer next check | `WARN` |
| `ExhaustedAllowance` | Skip, subscriber must re-approve | `ERROR` |
| `SubscriptionNotActive` | Skip | `INFO` |
| `RetryCooldown` | Skip | `DEBUG` |
| RPC / Network Error | Exponential backoff (up to `MAX_RETRIES`) | `WARN` |
| Insufficient Keeper Balance | **Exit with code 1** | `ERROR` |

## Tests

```bash
cargo test -p keeper
```

## Configuration Reference

| Key | CLI Flag | Default | Description |
|---|---|---|---|
| `RPC_URL` | `--rpc-url` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `NETWORK_PASSPHRASE` | `--network-passphrase` | `Test SDF Network ; September 2015` | Stellar network passphrase |
| `CADENCE_CONTRACT_ID` | `--contract-id` | *(required)* | Cadence contract address |
| `KEEPER_SECRET_KEY` | `--keeper-secret` | *(required in live mode)* | Keeper signing key |
| `SUB_IDS` | `--sub-ids` | *(required)* | Comma-separated subscription IDs |
| `POLL_INTERVAL_SECS` | `--poll-interval-secs` | `60` | Seconds between polls |
| `MAX_RETRIES` | `--max-retries` | `5` | Max retries for transient errors |
| `DRY_RUN` | `--dry-run` | `false` | Log actions without submitting |
| `RUST_LOG` | — | `info` | Tracing filter (e.g. `debug`, `keeper=trace`) |
