use soroban_sdk::{contractevent, Address};

use crate::types::SubStatus;

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanCreated {
    #[topic]
    pub plan_id: u64,
    #[topic]
    pub merchant: Address,
    pub token: Address,
    pub amount: i128,
    pub period: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlanStatusChanged {
    #[topic]
    pub plan_id: u64,
    pub active: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscribed {
    #[topic]
    pub sub_id: u64,
    #[topic]
    pub plan_id: u64,
    #[topic]
    pub subscriber: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Charged {
    #[topic]
    pub sub_id: u64,
    pub amount: i128,
    pub fee: i128,
    pub cycles_paid: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChargeFailed {
    #[topic]
    pub sub_id: u64,
    pub failures: u32,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionEnded {
    #[topic]
    pub sub_id: u64,
    pub status: SubStatus,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigChanged {
    pub fee_bps: u32,
    pub fee_recipient: Address,
    pub paused: bool,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposed {
    #[topic]
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminChanged {
    #[topic]
    pub new_admin: Address,
}
