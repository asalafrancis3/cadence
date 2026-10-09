# Starter issues for contributors

Each item is scoped to be finished in one pull request. Labels in brackets.

## Bug fixes and hardening
1. **[bug][good first issue]** `plans_of` / `subscriptions_of` skip holes silently; return a typed result or document the behaviour and add a test.
2. **[bug]** Frontend: `parseAmount("1.")` and `parseAmount(".5")` are rejected; decide on and implement the expected behaviour with tests.
3. **[security]** Add a property test (`proptest`) asserting `cycles_paid * amount` never exceeds total pulled for random schedules and keeper delays.
4. **[security]** Add a `min_amount` guard (dust threshold) to stop spam plans, behind a new admin-set config value.

## Features
5. **[feature]** `extend_allowance_hint(sub_id)` view returning how many more cycles the current allowance covers.
6. **[feature]** Trial periods: `trial_secs` on a plan, first charge deferred.
7. **[feature]** Pause/resume by subscriber without cancelling.
8. **[feature]** Keeper reference bot (Node or Rust) that polls `is_due` and calls `charge` + `bump_subscription`.
9. **[feature]** Frontend: token allow-list with symbol, icon and a warning for unknown tokens.
10. **[feature]** Frontend: show real allowance remaining and a "top up" button on a failing subscription.

## Docs
11. **[docs][good first issue]** Add a "Build a paywall with Cadence" guide using generated TypeScript bindings.
12. **[docs]** Write a Mainnet deployment checklist (admin multisig, fee recipient, monitoring).
13. **[docs]** Record a short walkthrough GIF for the README.

## Testing
14. **[test][good first issue]** Add a test for a token that rejects `transfer` to the merchant (missing trustline) and assert the whole `charge` reverts without penalising the subscriber.
15. **[test]** Add snapshot tests for emitted events.
16. **[test]** Playwright smoke test for the Subscribe flow with a mocked wallet module.
