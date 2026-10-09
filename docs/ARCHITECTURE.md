# Architecture

## Components

* **Cadence contract**: plans, subscriptions, fee split, admin. One contract, many
  merchants and tokens.
* **SEP-41 token**: holds balances and the allowance Cadence spends. Any Stellar
  Asset Contract (XLM, USDC, ...) or compliant custom token works.
* **Keepers**: bots (or users) that call `charge`. They need no permission and gain
  nothing from calling it, so many independent keepers can coexist.
* **Frontend**: reads by simulation (no wallet needed), writes through Stellar
  Wallets Kit.

## Why pull payments via allowance

`approve(from, spender, amount, expiration_ledger)` lets the subscriber give
Cadence a capped, expiring right to call `transfer_from`. Cadence cannot exceed
that cap, cannot outlive the expiry, and the user can set the allowance to 0 in
their wallet without Cadence's cooperation. This is strictly safer than
custodial escrow, and strictly more convenient than signing each renewal.

## Money flow inside one charge

1. `transfer_from(subscriber -> Cadence, amount)`, a single all-or-nothing pull.
2. `transfer(Cadence -> merchant, amount - fee)` and `transfer(Cadence -> treasury, fee)`.

The pull and the payout are separate steps on purpose:

* If the **pull** fails (allowance or balance), that is the subscriber's problem:
  it is caught with `try_transfer_from`, recorded as a failure, and the call
  returns `Failed` without reverting.
* If a **payout** fails (merchant lacks a trustline, token is frozen), that is not
  the subscriber's fault: the whole transaction reverts, the subscriber is not
  penalised, and the keeper simply sees an error.

The contract balance is zero at the end of every call (asserted in the
integration test on every simulated day).

## Scheduling

`next_charge` advances by exactly one `period` per charge so billing dates do not
drift. If `now >= next_charge + period` (the keeper skipped a whole cycle), the
schedule restarts at `now + period`. There is no back-billing.

## Storage layout and TTL

| Key | Storage | Notes |
| --- | --- | --- |
| `Admin`, `PendingAdmin`, `Config`, `PlanCount`, `SubCount` | instance | small, shared lifetime |
| `Plan(id)`, `Sub(id)` | persistent | TTL extended on every read and write |
| `SubOf(addr, n)`, `SubOfCount(addr)`, `PlanOf(addr, n)`, `PlanOfCount(addr)` | persistent | paginated per-user indexes, no unbounded `Vec` |

Entries are bumped to about 30 days whenever they are touched by an **on-chain
committed transaction**. Frontend calls that only simulate (`get_subscription`,
`subscriptions_of`, etc.) do not commit state and therefore do not extend TTLs.

The reliable way to keep index entries alive for subscribers who do not frequently
perform writes (subscribe / cancel / charge) is to call the permissionless
`bump_subscription`. It explicitly bumps both the subscription record, the plan
record, and all entries in the subscriber's per-user index. Keepers should call
`bump_subscription` alongside `charge` for long-period plans (≥ 30 days). If an
entry is archived anyway, it can be restored with `stellar contract restore`.

## Events

Typed `#[contractevent]` structs: `PlanCreated`, `PlanStatusChanged`, `Subscribed`,
`Charged`, `ChargeFailed`, `SubscriptionEnded`, `ConfigChanged`, `AdminProposed`,
`AdminChanged`. Topics are chosen so an indexer can filter by `sub_id`, `plan_id`,
`merchant` or `subscriber`.

## Frontend data flow

```
read:  build tx -> simulateTransaction -> scValToNative(result)
write: build tx -> prepareTransaction (simulate + footprint + auth)
       -> wallet signs XDR -> sendTransaction -> pollTransaction
```

Subscribing is two transactions (Soroban allows one invoke-host-function operation
per transaction): `approve` on the token, then `subscribe` on Cadence. The UI
presents them as two steps and disables step 2 until the allowance covers one cycle.
