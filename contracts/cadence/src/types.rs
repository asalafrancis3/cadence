use soroban_sdk::{contracttype, Address, String};

/// Global, admin-controlled settings.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// Protocol fee in basis points, taken from each charge. Capped by `MAX_FEE_BPS`.
    pub fee_bps: u32,
    pub fee_recipient: Address,
    pub paused: bool,
}

/// A merchant's offer: "pay `amount` of `token` every `period` seconds".
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Plan {
    pub merchant: Address,
    /// SEP-41 token (any Stellar Asset Contract or custom token implementing `transfer_from`).
    pub token: Address,
    /// Price per cycle, in the token's smallest unit.
    pub amount: i128,
    /// Seconds between charges.
    pub period: u64,
    pub name: String,
    /// Only gates *new* subscriptions. Existing subscribers keep being billed
    /// until they or the merchant cancel.
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SubStatus {
    Active,
    /// Cancelled by the subscriber, the merchant, or after too many failed charges.
    Cancelled,
    /// `max_cycles` reached.
    Completed,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub plan_id: u64,
    pub subscriber: Address,
    /// Ledger timestamp at/after which the next charge may be executed.
    pub next_charge: u64,
    pub cycles_paid: u32,
    /// 0 = unlimited.
    pub max_cycles: u32,
    /// Consecutive failed charge attempts.
    pub failures: u32,
    pub last_attempt: u64,
    pub status: SubStatus,
}

/// Outcome of a `charge` call that did not revert.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChargeResult {
    /// Funds moved; the subscription advanced one cycle.
    Charged,
    /// Allowance or balance was insufficient; failure recorded, retry later.
    Failed,
    /// Too many consecutive failures; the subscription was cancelled.
    Cancelled,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    // instance storage
    Admin,
    PendingAdmin,
    Config,
    PlanCount,
    SubCount,
    // persistent storage
    Plan(u64),
    Sub(u64),
    /// Per-user index: item `n` of the user's subscriptions / plans.
    SubOf(Address, u32),
    SubOfCount(Address),
    PlanOf(Address, u32),
    PlanOfCount(Address),
}
