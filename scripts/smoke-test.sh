#!/usr/bin/env bash
# End-to-end check against the deployed Testnet contract using stellar-cli only.
# Flow: create plan -> approve allowance -> subscribe -> (not due) -> cancel.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v jq >/dev/null || { echo "jq is required"; exit 1; }

NETWORK=testnet
CONTRACT_ID="$(jq -r .contractId deployments/testnet.json)"
TOKEN="$(jq -r .demoToken deployments/testnet.json)"
MERCHANT_KEY=cadence-merchant
SUBSCRIBER_KEY=cadence-subscriber

for k in $MERCHANT_KEY $SUBSCRIBER_KEY; do
  stellar keys address "$k" >/dev/null 2>&1 || stellar keys generate "$k" --network $NETWORK --fund
done
MERCHANT="$(stellar keys address $MERCHANT_KEY)"
SUBSCRIBER="$(stellar keys address $SUBSCRIBER_KEY)"

invoke() { stellar contract invoke --network $NETWORK --id "$1" --source "$2" -- "${@:3}"; }

LEDGER="$(curl -s -X POST https://soroban-testnet.stellar.org \
  -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"getLatestLedger"}' | jq .result.sequence)"

echo "1/5 create plan (1 XLM every hour)"
PLAN_ID="$(invoke "$CONTRACT_ID" $MERCHANT_KEY create_plan \
  --merchant "$MERCHANT" --token "$TOKEN" --amount 10000000 --period 3600 --name '"Smoke plan"' | tr -d '"')"

echo "2/5 approve allowance for 3 cycles"
invoke "$TOKEN" $SUBSCRIBER_KEY approve \
  --from "$SUBSCRIBER" --spender "$CONTRACT_ID" --amount 30000000 \
  --expiration_ledger $((LEDGER + 100000))

echo "3/5 subscribe (pays cycle 1 immediately)"
SUB_ID="$(invoke "$CONTRACT_ID" $SUBSCRIBER_KEY subscribe \
  --subscriber "$SUBSCRIBER" --plan_id "$PLAN_ID" --max_cycles 3 | tr -d '"')"
invoke "$CONTRACT_ID" $SUBSCRIBER_KEY get_subscription --sub_id "$SUB_ID"

echo "4/5 charging now must fail with NotDue (#10)"
if invoke "$CONTRACT_ID" $MERCHANT_KEY charge --sub_id "$SUB_ID" 2>&1 | grep -q "#10"; then
  echo "   OK: NotDue"
else
  echo "   UNEXPECTED: charge did not return NotDue"; exit 1
fi

echo "5/5 cancel"
invoke "$CONTRACT_ID" $SUBSCRIBER_KEY cancel --caller "$SUBSCRIBER" --sub_id "$SUB_ID"
echo "Smoke test passed."
