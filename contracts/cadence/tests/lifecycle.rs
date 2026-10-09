//! Integration test: several merchants and subscribers over six months, with a
//! keeper bot charging whatever is due. Asserts the key invariants:
//!   1. value is conserved (no tokens created or destroyed),
//!   2. the contract never holds a balance between calls,
//!   3. merchants are paid exactly (cycles x price - fee).
use cadence::{Cadence, CadenceClient, ChargeResult, SubStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String,
};

const DAY: u64 = 86_400;
const FEE_BPS: u32 = 250; // 2.5%

#[test]
fn six_month_multi_party_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_700_000_000);

    let admin = Address::generate(&env);
    let treasury = Address::generate(&env);
    let id = env.register(Cadence, (admin, treasury.clone(), FEE_BPS));
    let cadence = CadenceClient::new(&env, &id);

    let token_addr = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let tok = token::Client::new(&env, &token_addr);
    let mint = token::StellarAssetClient::new(&env, &token_addr);

    let news = Address::generate(&env);
    let vpn = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);
    let everyone = [&news, &vpn, &alice, &bob, &carol, &treasury];

    for who in [&alice, &bob, &carol] {
        mint.mint(who, &10_000_000_000);
    }
    let supply: i128 = everyone.iter().map(|a| tok.balance(a)).sum();

    let monthly = 50_000_000_i128;
    let weekly = 10_000_000_i128;
    let news_plan = cadence.create_plan(
        &news,
        &token_addr,
        &monthly,
        &(30 * DAY),
        &String::from_str(&env, "News"),
    );
    let vpn_plan = cadence.create_plan(
        &vpn,
        &token_addr,
        &weekly,
        &(7 * DAY),
        &String::from_str(&env, "VPN"),
    );

    let approve = |who: &Address, amount: i128| {
        tok.approve(who, &id, &amount, &(env.ledger().sequence() + 200_000));
    };
    approve(&alice, monthly * 6);
    approve(&bob, weekly * 4 + monthly); // runs dry on the VPN plan
    approve(&carol, monthly * 3);

    let a_news = cadence.subscribe(&alice, &news_plan, &0);
    let b_vpn = cadence.subscribe(&bob, &vpn_plan, &0);
    let c_news = cadence.subscribe(&carol, &news_plan, &3); // capped at 3 cycles
    let ids = [a_news, b_vpn, c_news];

    // Keeper bot: once a day for 180 days, charge anything that is due.
    let mut charged = 0u32;
    for _ in 0..180 {
        env.ledger().with_mut(|l| l.timestamp += DAY);
        for sid in ids {
            if cadence.is_due(&sid) && cadence.charge(&sid) == ChargeResult::Charged {
                charged += 1;
            }
        }
        assert_eq!(tok.balance(&id), 0, "contract must never hold funds");
        let now: i128 = everyone.iter().map(|a| tok.balance(a)).sum();
        assert_eq!(now, supply, "value must be conserved");
    }

    // Carol's capped plan completed after exactly 3 cycles.
    let carol_sub = cadence.get_subscription(&c_news);
    assert_eq!(carol_sub.status, SubStatus::Completed);
    assert_eq!(carol_sub.cycles_paid, 3);

    // Bob's allowance ran out -> auto-cancelled after repeated failures.
    assert_eq!(
        cadence.get_subscription(&b_vpn).status,
        SubStatus::Cancelled
    );
    assert!(cadence.get_subscription(&b_vpn).cycles_paid >= 4);

    // Alice paid all six monthly cycles available in her allowance.
    assert_eq!(cadence.get_subscription(&a_news).cycles_paid, 6);

    // Merchant revenue is exactly cycles x price minus the 2.5% fee.
    let net =
        |amount: i128, cycles: i128| amount * cycles - (amount * FEE_BPS as i128 / 10_000) * cycles;
    let news_cycles = 6 + 3;
    assert_eq!(tok.balance(&news), net(monthly, news_cycles));
    let vpn_cycles = cadence.get_subscription(&b_vpn).cycles_paid as i128;
    assert_eq!(tok.balance(&vpn), net(weekly, vpn_cycles));
    assert!(charged > 0);
}
