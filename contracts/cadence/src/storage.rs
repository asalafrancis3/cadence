//! Thin storage layer: every read of a persistent entry also extends its TTL,
//! so active subscriptions never get archived while they are being used.

use soroban_sdk::{Address, Env, Vec};

use crate::{
    errors::Error,
    types::{Config, DataKey, Plan, Subscription},
};

/// ~1 day in ledgers (5s per ledger).
pub const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_THRESHOLD: u32 = 7 * DAY_IN_LEDGERS;
const INSTANCE_BUMP: u32 = 30 * DAY_IN_LEDGERS;
const PERSISTENT_THRESHOLD: u32 = 15 * DAY_IN_LEDGERS;
const PERSISTENT_BUMP: u32 = 30 * DAY_IN_LEDGERS;

pub const MAX_PAGE: u32 = 50;

pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

fn extend_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
}

// ---- instance: admin & config & counters -------------------------------

pub fn get_admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}
pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
}
pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

pub fn get_config(env: &Env) -> Config {
    env.storage().instance().get(&DataKey::Config).unwrap()
}
pub fn set_config(env: &Env, config: &Config) {
    env.storage().instance().set(&DataKey::Config, config);
}

fn counter(env: &Env, key: &DataKey) -> u64 {
    env.storage().instance().get(key).unwrap_or(0)
}
fn next_id(env: &Env, key: DataKey) -> u64 {
    let id = counter(env, &key) + 1;
    env.storage().instance().set(&key, &id);
    id
}
pub fn next_plan_id(env: &Env) -> u64 {
    next_id(env, DataKey::PlanCount)
}
pub fn next_sub_id(env: &Env) -> u64 {
    next_id(env, DataKey::SubCount)
}
pub fn plan_count(env: &Env) -> u64 {
    counter(env, &DataKey::PlanCount)
}
pub fn sub_count(env: &Env) -> u64 {
    counter(env, &DataKey::SubCount)
}

// ---- persistent: plans & subscriptions ---------------------------------

pub fn get_plan(env: &Env, id: u64) -> Result<Plan, Error> {
    let key = DataKey::Plan(id);
    let plan = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::PlanNotFound)?;
    extend_persistent(env, &key);
    Ok(plan)
}
pub fn put_plan(env: &Env, id: u64, plan: &Plan) {
    let key = DataKey::Plan(id);
    env.storage().persistent().set(&key, plan);
    extend_persistent(env, &key);
}

pub fn get_sub(env: &Env, id: u64) -> Result<Subscription, Error> {
    let key = DataKey::Sub(id);
    let sub = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::SubscriptionNotFound)?;
    extend_persistent(env, &key);
    Ok(sub)
}
pub fn put_sub(env: &Env, id: u64, sub: &Subscription) {
    let key = DataKey::Sub(id);
    env.storage().persistent().set(&key, sub);
    extend_persistent(env, &key);
}

// ---- persistent: per-user indexes (paginated, no unbounded Vec) ---------

fn push_index(env: &Env, count_key: DataKey, item_key: impl Fn(u32) -> DataKey, id: u64) {
    let n: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
    let item = item_key(n);
    env.storage().persistent().set(&item, &id);
    extend_persistent(env, &item);
    env.storage().persistent().set(&count_key, &(n + 1));
    extend_persistent(env, &count_key);
}

fn read_index(
    env: &Env,
    count_key: DataKey,
    item_key: impl Fn(u32) -> DataKey,
    start: u32,
    limit: u32,
) -> Vec<u64> {
    let n: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
    // Bump the count key so it does not get archived between activity bursts
    // (SEC-02: per-user indexes were only bumped on write, not on read).
    if n > 0 {
        extend_persistent(env, &count_key);
    }
    let end = n.min(start.saturating_add(limit.min(MAX_PAGE)));
    let mut out = Vec::new(env);
    let mut i = start;
    while i < end {
        let key = item_key(i);
        if let Some(id) = env.storage().persistent().get::<_, u64>(&key) {
            extend_persistent(env, &key);
            out.push_back(id);
        }
        i += 1;
    }
    out
}

/// Extend TTL on every entry in a subscriber's subscription index.
/// Called from `bump_subscription` so keepers can keep indexes alive
/// for subscribers who never perform on-chain writes of their own.
/// Simulated frontend reads do not commit TTL changes; this committed
/// on-chain bump is the reliable path to prevent index archival.
pub fn bump_sub_index(env: &Env, who: &Address) {
    let count_key = DataKey::SubOfCount(who.clone());
    let n: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
    if n == 0 {
        return;
    }
    extend_persistent(env, &count_key);
    for i in 0..n {
        let key = DataKey::SubOf(who.clone(), i);
        if env.storage().persistent().has(&key) {
            extend_persistent(env, &key);
        }
    }
}

pub fn push_sub_of(env: &Env, who: &Address, id: u64) {
    let w = who.clone();
    push_index(
        env,
        DataKey::SubOfCount(who.clone()),
        move |n| DataKey::SubOf(w.clone(), n),
        id,
    );
}
pub fn push_plan_of(env: &Env, who: &Address, id: u64) {
    let w = who.clone();
    push_index(
        env,
        DataKey::PlanOfCount(who.clone()),
        move |n| DataKey::PlanOf(w.clone(), n),
        id,
    );
}
pub fn subs_of(env: &Env, who: &Address, start: u32, limit: u32) -> Vec<u64> {
    let w = who.clone();
    read_index(
        env,
        DataKey::SubOfCount(who.clone()),
        move |n| DataKey::SubOf(w.clone(), n),
        start,
        limit,
    )
}
pub fn plans_of(env: &Env, who: &Address, start: u32, limit: u32) -> Vec<u64> {
    let w = who.clone();
    read_index(
        env,
        DataKey::PlanOfCount(who.clone()),
        move |n| DataKey::PlanOf(w.clone(), n),
        start,
        limit,
    )
}
