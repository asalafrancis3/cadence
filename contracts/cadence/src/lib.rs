//! # Cadence
//!
//! Non-custodial recurring payments for Soroban tokens.
//!
//! Stellar has no native "pull payment". Cadence builds one from the SEP-41
//! allowance primitive: a subscriber `approve`s this contract as a spender for a
//! bounded amount, and **anyone** (a keeper bot, the merchant, the subscriber)
//! can trigger `charge` once a cycle is due. Funds never rest in the contract
//! between transactions, and the subscriber can cancel or revoke the allowance
//! at any time.
//!
//! See `docs/ARCHITECTURE.md` and `docs/SECURITY.md`.
#![no_std]

mod errors;
mod events;
mod storage;
mod types;

#[cfg(test)]
mod test;

pub use errors::Error;
pub use types::{ChargeResult, Config, Plan, SubStatus, Subscription};

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, BytesN, Env, String, Vec,
};

/// Hard ceiling on the protocol fee (5%).
pub const MAX_FEE_BPS: u32 = 500;
const BPS_DENOMINATOR: i128 = 10_000;
/// Shortest allowed billing period (1 hour).
pub const MIN_PERIOD: u64 = 3_600;
/// Longest allowed billing period (366 days).
pub const MAX_PERIOD: u64 = 366 * 24 * 3_600;
/// Consecutive failed charges before a subscription is auto-cancelled.
pub const MAX_FAILURES: u32 = 3;
/// Minimum wait between a failed attempt and the next one (6 hours).
pub const RETRY_COOLDOWN: u64 = 6 * 3_600;
pub const MAX_NAME_LEN: u32 = 64;

#[contract]
pub struct Cadence;

#[contractimpl]
impl Cadence {
    /// Runs atomically at deploy time, so there is no window in which an
    /// attacker could front-run `initialize`.
    pub fn __constructor(env: Env, admin: Address, fee_recipient: Address, fee_bps: u32) {
        if fee_bps > MAX_FEE_BPS {
            panic_with_error!(&env, Error::InvalidFee);
        }
        storage::set_admin(&env, &admin);
        storage::set_config(
            &env,
            &Config {
                fee_bps,
                fee_recipient,
                paused: false,
            },
        );
        storage::extend_instance(&env);
    }

    // ------------------------------------------------------------------
    // Merchants
    // ------------------------------------------------------------------

    /// Publish a plan. Returns its id.
    pub fn create_plan(
        env: Env,
        merchant: Address,
        token: Address,
        amount: i128,
        period: u64,
        name: String,
    ) -> Result<u64, Error> {
        storage::extend_instance(&env);
        require_not_paused(&env)?;
        merchant.require_auth();

        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }
        if !(MIN_PERIOD..=MAX_PERIOD).contains(&period) {
            return Err(Error::InvalidPeriod);
        }
        if name.len() > MAX_NAME_LEN {
            return Err(Error::NameTooLong);
        }

        let plan_id = storage::next_plan_id(&env);
        storage::put_plan(
            &env,
            plan_id,
            &Plan {
                merchant: merchant.clone(),
                token: token.clone(),
                amount,
                period,
                name,
                active: true,
            },
        );
        storage::push_plan_of(&env, &merchant, plan_id);

        events::PlanCreated {
            plan_id,
            merchant,
            token,
            amount,
            period,
        }
        .publish(&env);
        Ok(plan_id)
    }

    /// Open or close a plan to *new* subscribers. Existing subscriptions are untouched.
    pub fn set_plan_active(env: Env, plan_id: u64, active: bool) -> Result<(), Error> {
        storage::extend_instance(&env);
        let mut plan = storage::get_plan(&env, plan_id)?;
        plan.merchant.require_auth();
        plan.active = active;
        storage::put_plan(&env, plan_id, &plan);
        events::PlanStatusChanged { plan_id, active }.publish(&env);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Subscribers
    // ------------------------------------------------------------------

    /// Subscribe and pay the first cycle immediately.
    ///
    /// The subscriber must have called `token.approve(subscriber, <this contract>,
    /// amount * cycles, expiration_ledger)` beforehand. If the first payment
    /// fails, the whole call reverts: there is never a subscription without a
    /// paid first cycle. `max_cycles = 0` means "until cancelled".
    pub fn subscribe(
        env: Env,
        subscriber: Address,
        plan_id: u64,
        max_cycles: u32,
    ) -> Result<u64, Error> {
        storage::extend_instance(&env);
        let cfg = storage::get_config(&env);
        if cfg.paused {
            return Err(Error::Paused);
        }
        subscriber.require_auth();

        let plan = storage::get_plan(&env, plan_id)?;
        if !plan.active {
            return Err(Error::PlanInactive);
        }

        let now = env.ledger().timestamp();
        let me = env.current_contract_address();
        let tok = token::Client::new(&env, &plan.token);
        tok.transfer_from(&me, &subscriber, &me, &plan.amount);
        let fee = distribute(&env, &cfg, &plan, &tok)?;

        let sub_id = storage::next_sub_id(&env);
        let sub = Subscription {
            plan_id,
            subscriber: subscriber.clone(),
            next_charge: now.checked_add(plan.period).ok_or(Error::Overflow)?,
            cycles_paid: 1,
            max_cycles,
            failures: 0,
            last_attempt: 0,
            status: if max_cycles == 1 {
                SubStatus::Completed
            } else {
                SubStatus::Active
            },
        };
        storage::put_sub(&env, sub_id, &sub);
        storage::push_sub_of(&env, &subscriber, sub_id);

        events::Subscribed {
            sub_id,
            plan_id,
            subscriber,
        }
        .publish(&env);
        events::Charged {
            sub_id,
            amount: plan.amount,
            fee,
            cycles_paid: 1,
        }
        .publish(&env);
        Ok(sub_id)
    }

    /// Cancel a subscription. Callable by the subscriber or the plan's merchant,
    /// and deliberately **not** blocked by `paused`.
    pub fn cancel(env: Env, caller: Address, sub_id: u64) -> Result<(), Error> {
        storage::extend_instance(&env);
        caller.require_auth();

        let mut sub = storage::get_sub(&env, sub_id)?;
        let plan = storage::get_plan(&env, sub.plan_id)?;
        if caller != sub.subscriber && caller != plan.merchant {
            return Err(Error::Unauthorized);
        }
        if sub.status != SubStatus::Active {
            return Err(Error::SubscriptionNotActive);
        }
        sub.status = SubStatus::Cancelled;
        storage::put_sub(&env, sub_id, &sub);
        events::SubscriptionEnded {
            sub_id,
            status: SubStatus::Cancelled,
        }
        .publish(&env);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Keepers (permissionless)
    // ------------------------------------------------------------------

    /// Execute one due cycle. Anyone may call this; the caller gains nothing
    /// and can only move funds along the path the subscriber already approved.
    ///
    /// * Success: pulls `amount` from the subscriber, pays the merchant (minus
    ///   fee), advances `next_charge` by one period. Missed periods are **not**
    ///   back-charged: if the keeper was late by more than one period the
    ///   schedule restarts from now.
    /// * Insufficient allowance/balance: records a failure and returns
    ///   `Failed` (state is persisted, the transaction does not revert). After
    ///   `MAX_FAILURES` consecutive failures the subscription is cancelled.
    pub fn charge(env: Env, sub_id: u64) -> Result<ChargeResult, Error> {
        storage::extend_instance(&env);
        let cfg = storage::get_config(&env);
        if cfg.paused {
            return Err(Error::Paused);
        }

        let mut sub = storage::get_sub(&env, sub_id)?;
        if sub.status != SubStatus::Active {
            return Err(Error::SubscriptionNotActive);
        }
        let plan = storage::get_plan(&env, sub.plan_id)?;

        let now = env.ledger().timestamp();
        if now < sub.next_charge {
            return Err(Error::NotDue);
        }
        if sub.failures > 0 && now < sub.last_attempt.saturating_add(RETRY_COOLDOWN) {
            return Err(Error::RetryCooldown);
        }

        // Pull the full amount into the contract in ONE call so the attempt is
        // all-or-nothing, then split it. If the pull fails (allowance/balance),
        // the failure is recorded; if the payout fails (e.g. merchant lacks a
        // trustline) the whole transaction reverts and the subscriber is not
        // penalised.
        //
        // Double-execution safety: Soroban processes ledger transactions
        // sequentially. If two keepers submit competing `charge` transactions
        // for the same subscription in the same ledger, the second one will
        // observe the already-advanced `next_charge` written by the first and
        // return `Err(NotDue)`. No double-billing is possible.
        let me = env.current_contract_address();
        let tok = token::Client::new(&env, &plan.token);
        let pulled = tok.try_transfer_from(&me, &sub.subscriber, &me, &plan.amount);
        if !matches!(pulled, Ok(Ok(()))) {
            sub.failures += 1;
            sub.last_attempt = now;
            if sub.failures >= MAX_FAILURES {
                sub.status = SubStatus::Cancelled;
                storage::put_sub(&env, sub_id, &sub);
                events::SubscriptionEnded {
                    sub_id,
                    status: SubStatus::Cancelled,
                }
                .publish(&env);
                return Ok(ChargeResult::Cancelled);
            }
            storage::put_sub(&env, sub_id, &sub);
            events::ChargeFailed {
                sub_id,
                failures: sub.failures,
            }
            .publish(&env);
            return Ok(ChargeResult::Failed);
        }

        let fee = distribute(&env, &cfg, &plan, &tok)?;

        let on_schedule = sub
            .next_charge
            .checked_add(plan.period)
            .ok_or(Error::Overflow)?;
        sub.next_charge = if now >= on_schedule {
            now.checked_add(plan.period).ok_or(Error::Overflow)?
        } else {
            on_schedule
        };
        sub.cycles_paid = sub.cycles_paid.checked_add(1).ok_or(Error::Overflow)?;
        sub.failures = 0;
        sub.last_attempt = now;
        let completed = sub.max_cycles > 0 && sub.cycles_paid >= sub.max_cycles;
        if completed {
            sub.status = SubStatus::Completed;
        }
        storage::put_sub(&env, sub_id, &sub);

        events::Charged {
            sub_id,
            amount: plan.amount,
            fee,
            cycles_paid: sub.cycles_paid,
        }
        .publish(&env);
        if completed {
            events::SubscriptionEnded {
                sub_id,
                status: SubStatus::Completed,
            }
            .publish(&env);
        }
        Ok(ChargeResult::Charged)
    }

    /// Permissionless TTL extension for a long-period subscription (e.g. yearly)
    /// so its ledger entries are not archived between charges.
    ///
    /// This also bumps the subscriber's per-user index entries. Keepers should
    /// call this alongside `charge`. Subscribers who only read their data via
    /// simulation (which does not commit state) should call this periodically
    /// to prevent index archival.
    pub fn bump_subscription(env: Env, sub_id: u64) -> Result<(), Error> {
        storage::extend_instance(&env);
        let sub = storage::get_sub(&env, sub_id)?;
        storage::get_plan(&env, sub.plan_id)?;
        // Explicitly bump the subscriber's index so read-only users who never
        // trigger a write transaction don't risk archival of their index entries
        // (Finding 3 / SEC-02: simulated reads don't commit TTL extensions).
        storage::bump_sub_index(&env, &sub.subscriber);
        Ok(())
    }

    // ------------------------------------------------------------------
    // Views
    // ------------------------------------------------------------------

    pub fn get_config(env: Env) -> Config {
        storage::get_config(&env)
    }
    pub fn get_plan(env: Env, plan_id: u64) -> Result<Plan, Error> {
        storage::get_plan(&env, plan_id)
    }
    pub fn get_subscription(env: Env, sub_id: u64) -> Result<Subscription, Error> {
        storage::get_sub(&env, sub_id)
    }
    pub fn plan_count(env: Env) -> u64 {
        storage::plan_count(&env)
    }
    pub fn subscription_count(env: Env) -> u64 {
        storage::sub_count(&env)
    }
    /// Page through a merchant's plan ids (`limit` is capped at 50).
    pub fn plans_of(env: Env, merchant: Address, start: u32, limit: u32) -> Vec<u64> {
        storage::plans_of(&env, &merchant, start, limit)
    }
    /// Page through a subscriber's subscription ids (`limit` is capped at 50).
    pub fn subscriptions_of(env: Env, subscriber: Address, start: u32, limit: u32) -> Vec<u64> {
        storage::subs_of(&env, &subscriber, start, limit)
    }
    /// Cheap check for keeper bots: would `charge` pass its timing/status checks now?
    pub fn is_due(env: Env, sub_id: u64) -> Result<bool, Error> {
        let sub = storage::get_sub(&env, sub_id)?;
        let now = env.ledger().timestamp();
        let cooled = sub.failures == 0 || now >= sub.last_attempt.saturating_add(RETRY_COOLDOWN);
        Ok(sub.status == SubStatus::Active && now >= sub.next_charge && cooled)
    }

    // ------------------------------------------------------------------
    // Admin
    // ------------------------------------------------------------------

    pub fn set_fee(env: Env, fee_bps: u32) -> Result<(), Error> {
        require_admin(&env);
        if fee_bps > MAX_FEE_BPS {
            return Err(Error::InvalidFee);
        }
        let mut cfg = storage::get_config(&env);
        cfg.fee_bps = fee_bps;
        save_config(&env, &cfg);
        Ok(())
    }

    pub fn set_fee_recipient(env: Env, recipient: Address) {
        require_admin(&env);
        let mut cfg = storage::get_config(&env);
        cfg.fee_recipient = recipient;
        save_config(&env, &cfg);
    }

    /// Emergency stop for `create_plan`, `subscribe` and `charge`. `cancel` keeps working.
    pub fn set_paused(env: Env, paused: bool) {
        require_admin(&env);
        let mut cfg = storage::get_config(&env);
        cfg.paused = paused;
        save_config(&env, &cfg);
    }

    /// Step 1 of a two-step admin handover (prevents transferring to a wrong address).
    pub fn propose_admin(env: Env, new_admin: Address) {
        require_admin(&env);
        storage::set_pending_admin(&env, &new_admin);
        events::AdminProposed { new_admin }.publish(&env);
    }

    /// Step 2: the proposed admin proves control of its key.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        storage::extend_instance(&env);
        let pending = storage::get_pending_admin(&env).ok_or(Error::NoPendingAdmin)?;
        pending.require_auth();
        storage::set_admin(&env, &pending);
        storage::clear_pending_admin(&env);
        events::AdminChanged { new_admin: pending }.publish(&env);
        Ok(())
    }

    /// Replace the contract WASM. Use a multisig or governance account as admin.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        require_admin(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);
    }
}

// ----------------------------------------------------------------------
// Internal helpers
// ----------------------------------------------------------------------

fn require_not_paused(env: &Env) -> Result<(), Error> {
    if storage::get_config(env).paused {
        return Err(Error::Paused);
    }
    Ok(())
}

fn require_admin(env: &Env) {
    storage::extend_instance(env);
    storage::get_admin(env).require_auth();
}

fn save_config(env: &Env, cfg: &Config) {
    storage::set_config(env, cfg);
    events::ConfigChanged {
        fee_bps: cfg.fee_bps,
        fee_recipient: cfg.fee_recipient.clone(),
        paused: cfg.paused,
    }
    .publish(env);
}

/// Split the amount the contract just pulled between merchant and fee
/// recipient. Returns the fee. The contract ends every call with a zero balance.
fn distribute(env: &Env, cfg: &Config, plan: &Plan, tok: &token::Client) -> Result<i128, Error> {
    let fee = plan
        .amount
        .checked_mul(cfg.fee_bps as i128)
        .ok_or(Error::Overflow)?
        / BPS_DENOMINATOR;
    let net = plan.amount - fee;
    let me = env.current_contract_address();
    tok.transfer(&me, &plan.merchant, &net);
    if fee > 0 {
        tok.transfer(&me, &cfg.fee_recipient, &fee);
    }
    Ok(fee)
}
