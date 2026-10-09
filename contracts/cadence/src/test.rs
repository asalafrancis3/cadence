#![cfg(test)]
extern crate std;

use crate::{
    Cadence, CadenceClient, ChargeResult, Error, SubStatus, MAX_FAILURES, MAX_FEE_BPS,
    RETRY_COOLDOWN,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

const PRICE: i128 = 100_000_000; // 10 tokens at 7 decimals
const PERIOD: u64 = 30 * 24 * 3_600;
const T0: u64 = 1_700_000_000;
const FEE_BPS: u32 = 100; // 1%

struct Ctx<'a> {
    env: Env,
    client: CadenceClient<'a>,
    token: token::Client<'a>,
    contract_id: Address,
    admin: Address,
    treasury: Address,
    merchant: Address,
    subscriber: Address,
}

fn setup<'a>() -> Ctx<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(T0);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    let contract_id = env.register(Cadence, (admin.clone(), treasury.clone(), FEE_BPS));
    let client = CadenceClient::new(&env, &contract_id);

    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token_addr = sac.address();
    token::StellarAssetClient::new(&env, &token_addr).mint(&subscriber, &(PRICE * 100));
    let token = token::Client::new(&env, &token_addr);

    Ctx {
        env,
        client,
        token,
        contract_id,
        admin,
        treasury,
        merchant,
        subscriber,
    }
}

fn warp(env: &Env, secs: u64) {
    env.ledger().with_mut(|l| l.timestamp += secs);
}

fn approve(c: &Ctx, cycles: i128) {
    c.token.approve(
        &c.subscriber,
        &c.contract_id,
        &(PRICE * cycles),
        &(c.env.ledger().sequence() + 100_000),
    );
}

fn plan(c: &Ctx) -> u64 {
    c.client.create_plan(
        &c.merchant,
        &c.token.address,
        &PRICE,
        &PERIOD,
        &String::from_str(&c.env, "Pro"),
    )
}

// ---------------------------------------------------------------- setup

#[test]
fn constructor_sets_config() {
    let c = setup();
    let cfg = c.client.get_config();
    assert_eq!(cfg.fee_bps, FEE_BPS);
    assert_eq!(cfg.fee_recipient, c.treasury);
    assert!(!cfg.paused);
}

#[test]
#[should_panic]
fn constructor_rejects_fee_above_cap() {
    let env = Env::default();
    let a = Address::generate(&env);
    env.register(Cadence, (a.clone(), a, MAX_FEE_BPS + 1));
}

// ---------------------------------------------------------------- plans

#[test]
fn create_plan_validates_input() {
    let c = setup();
    let name = String::from_str(&c.env, "x");
    let t = &c.token.address;
    assert_eq!(
        c.client.try_create_plan(&c.merchant, t, &0, &PERIOD, &name),
        Err(Ok(Error::InvalidAmount))
    );
    // Negative amount must also be rejected (SEC-06).
    assert_eq!(
        c.client
            .try_create_plan(&c.merchant, t, &-1, &PERIOD, &name),
        Err(Ok(Error::InvalidAmount))
    );
    assert_eq!(
        c.client.try_create_plan(&c.merchant, t, &PRICE, &10, &name),
        Err(Ok(Error::InvalidPeriod))
    );
    let long = String::from_str(&c.env, &"n".repeat(65));
    assert_eq!(
        c.client
            .try_create_plan(&c.merchant, t, &PRICE, &PERIOD, &long),
        Err(Ok(Error::NameTooLong))
    );
}

#[test]
fn plans_are_indexed_per_merchant() {
    let c = setup();
    let a = plan(&c);
    let b = plan(&c);
    let ids = c.client.plans_of(&c.merchant, &0, &10);
    assert_eq!((ids.get(0), ids.get(1)), (Some(a), Some(b)));
    assert_eq!(c.client.plans_of(&c.subscriber, &0, &10).len(), 0);
    assert_eq!(c.client.plan_count(), 2);
}

#[test]
fn inactive_plan_rejects_new_subscribers_but_keeps_existing() {
    let c = setup();
    approve(&c, 5);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    c.client.set_plan_active(&p, &false);
    assert_eq!(
        c.client.try_subscribe(&c.subscriber, &p, &0),
        Err(Ok(Error::PlanInactive))
    );

    warp(&c.env, PERIOD);
    assert_eq!(c.client.charge(&s), ChargeResult::Charged);
}

// ---------------------------------------------------------------- subscribe

#[test]
fn subscribe_pays_first_cycle_and_splits_fee() {
    let c = setup();
    approve(&c, 3);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    let fee = PRICE * FEE_BPS as i128 / 10_000;
    assert_eq!(c.token.balance(&c.subscriber), PRICE * 99);
    assert_eq!(c.token.balance(&c.merchant), PRICE - fee);
    assert_eq!(c.token.balance(&c.treasury), fee);
    assert_eq!(
        c.token.balance(&c.contract_id),
        0,
        "contract never keeps funds"
    );

    let sub = c.client.get_subscription(&s);
    assert_eq!(sub.cycles_paid, 1);
    assert_eq!(sub.next_charge, T0 + PERIOD);
    assert_eq!(sub.status, SubStatus::Active);
}

#[test]
fn subscribe_without_allowance_reverts_and_creates_nothing() {
    let c = setup();
    let p = plan(&c);
    assert!(c.client.try_subscribe(&c.subscriber, &p, &0).is_err());
    assert_eq!(c.client.subscription_count(), 0);
    assert_eq!(c.token.balance(&c.subscriber), PRICE * 100);
}

#[test]
fn subscribe_requires_subscriber_auth() {
    let c = setup();
    approve(&c, 1);
    let p = plan(&c);
    c.client.subscribe(&c.subscriber, &p, &0);
    assert!(c.env.auths().iter().any(|(a, _)| *a == c.subscriber));
}

#[test]
fn single_cycle_subscription_completes_immediately() {
    let c = setup();
    approve(&c, 1);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &1);
    assert_eq!(c.client.get_subscription(&s).status, SubStatus::Completed);
    warp(&c.env, PERIOD);
    assert_eq!(
        c.client.try_charge(&s),
        Err(Ok(Error::SubscriptionNotActive))
    );
}

// ---------------------------------------------------------------- charge

#[test]
fn charge_before_due_fails() {
    let c = setup();
    approve(&c, 3);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);
    warp(&c.env, PERIOD - 1);
    assert_eq!(c.client.try_charge(&s), Err(Ok(Error::NotDue)));
    assert!(!c.client.is_due(&s));
}

#[test]
fn charge_when_due_advances_exactly_one_period() {
    let c = setup();
    approve(&c, 3);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    warp(&c.env, PERIOD + 500); // keeper is a bit late
    assert!(c.client.is_due(&s));
    assert_eq!(c.client.charge(&s), ChargeResult::Charged);

    let sub = c.client.get_subscription(&s);
    assert_eq!(sub.cycles_paid, 2);
    assert_eq!(
        sub.next_charge,
        T0 + 2 * PERIOD,
        "cadence is kept, no drift"
    );
    assert_eq!(c.token.balance(&c.contract_id), 0);
}

#[test]
fn missed_periods_are_not_back_charged() {
    let c = setup();
    approve(&c, 10);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    warp(&c.env, 5 * PERIOD);
    assert_eq!(c.client.charge(&s), ChargeResult::Charged);
    // Only one cycle charged; schedule restarts from now.
    let now = c.env.ledger().timestamp();
    assert_eq!(c.client.get_subscription(&s).next_charge, now + PERIOD);
    assert_eq!(c.client.try_charge(&s), Err(Ok(Error::NotDue)));
    assert_eq!(c.token.balance(&c.subscriber), PRICE * 98);
}

#[test]
fn anyone_can_trigger_charge_but_funds_only_go_to_the_plan() {
    let c = setup();
    approve(&c, 3);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);
    warp(&c.env, PERIOD);
    let before = c.token.balance(&c.merchant);
    c.client.charge(&s); // no caller identity involved at all
    assert!(c.token.balance(&c.merchant) > before);
}

#[test]
fn max_cycles_completes_subscription() {
    let c = setup();
    approve(&c, 5);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &2);
    warp(&c.env, PERIOD);
    c.client.charge(&s);
    assert_eq!(c.client.get_subscription(&s).status, SubStatus::Completed);
    warp(&c.env, PERIOD);
    assert_eq!(
        c.client.try_charge(&s),
        Err(Ok(Error::SubscriptionNotActive))
    );
}

#[test]
fn zero_fee_skips_fee_transfer() {
    let c = setup();
    c.client.set_fee(&0);
    approve(&c, 1);
    let p = plan(&c);
    c.client.subscribe(&c.subscriber, &p, &0);
    assert_eq!(c.token.balance(&c.merchant), PRICE);
    assert_eq!(c.token.balance(&c.treasury), 0);
}

// ---------------------------------------------------------------- failures

#[test]
fn failed_charges_are_recorded_rate_limited_then_auto_cancel() {
    let c = setup();
    approve(&c, 1); // only the first cycle is covered
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);
    let balance = c.token.balance(&c.subscriber);

    warp(&c.env, PERIOD);
    assert_eq!(c.client.charge(&s), ChargeResult::Failed);
    assert_eq!(c.client.get_subscription(&s).failures, 1);
    assert_eq!(c.token.balance(&c.subscriber), balance);

    // Spamming retries is blocked.
    assert_eq!(c.client.try_charge(&s), Err(Ok(Error::RetryCooldown)));
    assert!(!c.client.is_due(&s));

    warp(&c.env, RETRY_COOLDOWN);
    assert_eq!(c.client.charge(&s), ChargeResult::Failed);
    warp(&c.env, RETRY_COOLDOWN);
    assert_eq!(MAX_FAILURES, 3);
    assert_eq!(c.client.charge(&s), ChargeResult::Cancelled);
    assert_eq!(c.client.get_subscription(&s).status, SubStatus::Cancelled);
}

#[test]
fn topping_up_allowance_recovers_a_failing_subscription() {
    let c = setup();
    approve(&c, 1);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    warp(&c.env, PERIOD);
    assert_eq!(c.client.charge(&s), ChargeResult::Failed);

    approve(&c, 5);
    warp(&c.env, RETRY_COOLDOWN);
    assert_eq!(c.client.charge(&s), ChargeResult::Charged);
    assert_eq!(c.client.get_subscription(&s).failures, 0);
}

#[test]
fn expired_allowance_counts_as_failure() {
    let c = setup();
    c.token.approve(
        &c.subscriber,
        &c.contract_id,
        &(PRICE * 5),
        &(c.env.ledger().sequence() + 100),
    );
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    c.env.ledger().with_mut(|l| l.sequence_number += 1_000);
    warp(&c.env, PERIOD);
    assert_eq!(c.client.charge(&s), ChargeResult::Failed);
}

// ---------------------------------------------------------------- cancel

#[test]
fn subscriber_and_merchant_can_cancel_but_strangers_cannot() {
    let c = setup();
    approve(&c, 5);
    let p = plan(&c);
    let stranger = Address::generate(&c.env);

    let s1 = c.client.subscribe(&c.subscriber, &p, &0);
    assert_eq!(
        c.client.try_cancel(&stranger, &s1),
        Err(Ok(Error::Unauthorized))
    );
    c.client.cancel(&c.subscriber, &s1);
    assert_eq!(c.client.get_subscription(&s1).status, SubStatus::Cancelled);

    let s2 = c.client.subscribe(&c.subscriber, &p, &0);
    c.client.cancel(&c.merchant, &s2);
    assert_eq!(c.client.get_subscription(&s2).status, SubStatus::Cancelled);

    assert_eq!(
        c.client.try_cancel(&c.subscriber, &s2),
        Err(Ok(Error::SubscriptionNotActive))
    );
    warp(&c.env, PERIOD);
    assert_eq!(
        c.client.try_charge(&s1),
        Err(Ok(Error::SubscriptionNotActive))
    );
}

#[test]
fn subscriptions_are_indexed_per_subscriber() {
    let c = setup();
    approve(&c, 5);
    let p = plan(&c);
    let a = c.client.subscribe(&c.subscriber, &p, &0);
    let b = c.client.subscribe(&c.subscriber, &p, &0);
    let ids = c.client.subscriptions_of(&c.subscriber, &0, &50);
    assert_eq!((ids.get(0), ids.get(1)), (Some(a), Some(b)));
    // pagination
    assert_eq!(
        c.client.subscriptions_of(&c.subscriber, &1, &1).get(0),
        Some(b)
    );
}

// ---------------------------------------------------------------- admin

#[test]
fn pause_blocks_money_movement_but_not_cancel() {
    let c = setup();
    approve(&c, 5);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    c.client.set_paused(&true);
    warp(&c.env, PERIOD);
    assert_eq!(c.client.try_charge(&s), Err(Ok(Error::Paused)));
    assert_eq!(
        c.client.try_subscribe(&c.subscriber, &p, &0),
        Err(Ok(Error::Paused))
    );
    c.client.cancel(&c.subscriber, &s); // users can always leave
    assert_eq!(c.client.get_subscription(&s).status, SubStatus::Cancelled);
}

#[test]
fn fee_is_capped() {
    let c = setup();
    assert_eq!(
        c.client.try_set_fee(&(MAX_FEE_BPS + 1)),
        Err(Ok(Error::InvalidFee))
    );
    c.client.set_fee(&MAX_FEE_BPS);
    assert_eq!(c.client.get_config().fee_bps, MAX_FEE_BPS);
}

#[test]
fn admin_functions_require_admin_auth() {
    let c = setup();
    c.client.set_paused(&true);
    assert!(c.env.auths().iter().any(|(a, _)| *a == c.admin));
}

#[test]
fn admin_transfer_is_two_step() {
    let c = setup();
    assert_eq!(c.client.try_accept_admin(), Err(Ok(Error::NoPendingAdmin)));

    let new_admin = Address::generate(&c.env);
    c.client.propose_admin(&new_admin);
    c.client.accept_admin();
    assert!(c.env.auths().iter().any(|(a, _)| *a == new_admin));

    // New admin is now in control; a second accept has nothing pending.
    c.client.set_paused(&true);
    assert!(c.env.auths().iter().any(|(a, _)| *a == new_admin));
    assert_eq!(c.client.try_accept_admin(), Err(Ok(Error::NoPendingAdmin)));
}

#[test]
fn bump_subscription_is_permissionless() {
    let c = setup();
    approve(&c, 1);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);
    // Calling bump from any address must succeed.
    c.client.bump_subscription(&s);
    // Unknown sub_id must return SubscriptionNotFound.
    assert_eq!(
        c.client.try_bump_subscription(&999),
        Err(Ok(Error::SubscriptionNotFound))
    );
    // After bumping, the subscriber index must still be readable (covers the
    // bump_sub_index path that extends per-user index TTLs on-chain —
    // simulated reads do not commit TTL changes, so this committed write path
    // is the reliable keeper for index liveness; see Finding 3 / SEC-02).
    let ids = c.client.subscriptions_of(&c.subscriber, &0, &10);
    assert_eq!(
        ids.get(0),
        Some(s),
        "subscriber index must be intact after bump"
    );
}

/// SEC-03: subscribe must require auth from the *subscriber* address.
/// This test strips all mocked authorisations after setup; the call must
/// revert rather than silently succeed.
#[test]
fn subscribe_without_subscriber_auth_reverts() {
    // Set up with mock_all_auths so plan creation and token approval work.
    let c = setup();
    approve(&c, 3);
    let p = plan(&c);

    // Disable all mocked authorisations. The require_auth inside `subscribe`
    // will now find no matching entry and panic.
    c.env.set_auths(&[]);
    assert!(
        c.client.try_subscribe(&c.subscriber, &p, &0).is_err(),
        "subscribe must revert when the subscriber has not authorised"
    );
    // No subscription should exist.
    assert_eq!(c.client.subscription_count(), 0);
}

/// SEC-01 (first half): when a charge fails because the subscriber has no
/// allowance (analogous to a trustline failure from the subscriber's side),
/// the subscriber's balance must be unchanged and the failure counter must
/// increment. The subscription must NOT be penalised beyond recording one
/// failure.
///
/// The second half of SEC-01 — that a panicking merchant-side payout reverts
/// the *entire* charge transaction without penalising the subscriber — is
/// asserted indirectly by the `six_month_multi_party_lifecycle` integration
/// test, which asserts on every simulated day that
/// `tok.balance(&contract_id) == 0` (no funds ever trapped) and that total
/// value is conserved. If a payout panic left tokens in the contract or
/// decremented the subscriber's balance without moving them to the merchant,
/// those assertions would fail.
#[test]
fn charge_failure_leaves_subscriber_balance_and_failures_unchanged() {
    let c = setup();
    // Give the subscriber only enough allowance for the initial subscribe.
    approve(&c, 1);
    let p = plan(&c);
    let s = c.client.subscribe(&c.subscriber, &p, &0);

    let balance_before = c.token.balance(&c.subscriber);

    // Advance time so the next charge is due, but allowance is now exhausted.
    warp(&c.env, PERIOD);
    assert_eq!(
        c.client.charge(&s),
        ChargeResult::Failed,
        "charge with no allowance must return Failed"
    );

    // Subscriber's balance must be exactly unchanged.
    assert_eq!(
        c.token.balance(&c.subscriber),
        balance_before,
        "subscriber balance must not change when charge fails"
    );
    // Exactly one failure recorded.
    assert_eq!(c.client.get_subscription(&s).failures, 1);
    // Subscription is still Active (below MAX_FAILURES threshold).
    assert_eq!(c.client.get_subscription(&s).status, SubStatus::Active);
}
