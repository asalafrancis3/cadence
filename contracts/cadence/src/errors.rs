use soroban_sdk::contracterror;

/// All errors the contract can return. Codes are part of the public ABI:
/// never renumber an existing variant, only append.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The protocol is paused by the admin (cancellations still work).
    Paused = 1,
    /// Caller is not allowed to perform this action.
    Unauthorized = 2,
    /// Plan amount must be strictly positive.
    InvalidAmount = 3,
    /// Billing period is outside `[MIN_PERIOD, MAX_PERIOD]`.
    InvalidPeriod = 4,
    /// Fee is above `MAX_FEE_BPS`.
    InvalidFee = 5,
    PlanNotFound = 6,
    /// Plan is closed to new subscribers.
    PlanInactive = 7,
    SubscriptionNotFound = 8,
    /// Subscription is cancelled, completed or auto-cancelled.
    SubscriptionNotActive = 9,
    /// The next billing time has not been reached yet.
    NotDue = 10,
    /// A failed charge was attempted too recently; wait for the cooldown.
    RetryCooldown = 11,
    NoPendingAdmin = 12,
    NameTooLong = 13,
    Overflow = 14,
}
