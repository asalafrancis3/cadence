# Security model

Status: **unaudited**. Treat as reference code until reviewed.

## Assets and trust

* Users trust the **token** they subscribe with, and Cadence's code.
* Cadence holds no user funds between transactions.
* The **admin** can change the fee (max 5%), pause new activity, rotate the fee
  recipient and upgrade the WASM. Use a multisig or governance account for admin
  on Mainnet. An upgrade is the largest trust assumption.

## Threats and mitigations

| Threat | Mitigation |
| --- | --- |
| Front-running initialisation | Admin and config are set in `__constructor`, atomically at deploy |
| Someone charges a subscriber early or repeatedly | `charge` requires `now >= next_charge`; one cycle per call; no back-billing |
| Griefing by repeatedly triggering failed charges to force auto-cancel | 6 hour retry cooldown, 3 failures needed, subscriber can top up allowance |
| Merchant drains more than agreed | Amount and period are fixed in the plan; subscriber allowance is the hard cap |
| Plan terms changed after subscribing | Plans are immutable except `active`, which only gates new subscribers |
| Fee raised to steal value | Hard cap `MAX_FEE_BPS = 500` enforced in constructor and `set_fee`; fee comes out of the merchant's side, subscriber price is fixed |
| Admin key loss or typo on transfer | Two-step `propose_admin` / `accept_admin` |
| Malicious token contract | A token can only affect users who approved it. Soroban forbids contract re-entrancy, so a hostile token cannot re-enter Cadence mid-call. Front ends should show an allow-list or warn on unknown tokens |
| Arithmetic overflow | `checked_*` ops and `overflow-checks = true` in the release profile |
| Missing auth | `require_auth` on every state-changing user action: merchant (`create_plan`, `set_plan_active`), subscriber (`subscribe`), caller (`cancel`), admin (all admin calls). Tests assert the expected address appears in `env.auths()` |
| Pause traps user funds | `cancel` is never blocked by `paused`; users can always leave and revoke allowance in their wallet |
| State archival breaks long subscriptions | TTL extended on every touch; permissionless `bump_subscription`; restore possible |

## Known limitations and non-goals

* Price is fixed per plan. Variable or usage-based billing is out of scope.
* The first charge uses `transfer_from` and requires a prior `approve`. This is
  two transactions by design.
* Tokens with transfer fees or rebasing balances are not supported (the merchant
  would be paid less than `amount - fee`).
* **Simulation does not commit TTL extensions.** The frontend reads `subscriptions_of`
  and `plans_of` via `simulateTransaction`, which does not write state on-chain.
  Therefore, TTL bumps that happen inside `read_index` during simulation are
  discarded. The on-chain path that reliably keeps indexes alive is the
  `bump_subscription` call (which now explicitly calls `bump_sub_index`). Keepers
  should call `bump_subscription` alongside `charge` for any long-period plan.
  Subscribers who only read but never write should call `bump_subscription` at
  least once every 30 days, or restore archived entries with `stellar contract restore`.
* `max_cycles` and allowance limits are enforced exclusively on-chain: `charge`
  checks `sub.max_cycles > 0 && sub.cycles_paid >= sub.max_cycles` and stops;
  `try_transfer_from` fails when the allowance is depleted. No frontend
  validation is required for spending-limit enforcement.
* Per-user indexes are not removed when a subscription is cancelled or completed
  (the id stays in the index but the subscription record will show the terminal
  status). Consumers of `subscriptions_of` should check `sub.status`.
* A deactivated plan keeps billing existing subscribers until they or the merchant
  cancel. This is intentional: merchants cannot silently alter a customer's terms
  mid-subscription.
* Fee rounding uses truncated (floor) division:
  `fee = amount * fee_bps / 10_000`. The remainder is at most 1 stroop per charge
  and always goes to the merchant. Subscribers pay exactly `amount` per cycle.

## Recommended before Mainnet

1. Independent audit and a public bug bounty.
2. Fuzz the charge scheduler (`proptest` / `cargo fuzz` with `soroban-sdk` arbitrary).
3. Admin behind a multisig; consider removing `upgrade` or time-locking it.
4. Formal checks on the invariants: contract balance is zero after every call; a
   subscription's lifetime payments never exceed `cycles x amount`.

## Reporting

Please report vulnerabilities privately to the maintainers (add a contact in
`SECURITY.md` of your fork) rather than opening a public issue.
