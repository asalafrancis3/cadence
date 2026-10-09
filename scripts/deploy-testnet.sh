#!/usr/bin/env bash
# Deploy Cadence to Stellar Testnet and write frontend/.env.local.
#
# Env overrides:
#   SOURCE         stellar-cli identity name (default: cadence-deployer; created + funded if missing)
#   FEE_RECIPIENT  address receiving protocol fees (default: the deployer)
#   FEE_BPS        protocol fee in basis points, max 500 (default: 50 = 0.5%)
set -euo pipefail
cd "$(dirname "$0")/.."

NETWORK=testnet
SOURCE="${SOURCE:-cadence-deployer}"
FEE_BPS="${FEE_BPS:-50}"

if ! stellar keys address "$SOURCE" >/dev/null 2>&1; then
  echo "Creating and funding identity '$SOURCE' on $NETWORK..."
  stellar keys generate "$SOURCE" --network "$NETWORK" --fund
fi
ADMIN="$(stellar keys address "$SOURCE")"
FEE_RECIPIENT="${FEE_RECIPIENT:-$ADMIN}"

./scripts/build.sh
WASM=target/wasm32v1-none/release/cadence.wasm

echo "Deploying (constructor args are applied atomically)..."
CONTRACT_ID="$(stellar contract deploy \
  --wasm "$WASM" \
  --source "$SOURCE" \
  --network "$NETWORK" \
  --alias cadence \
  -- \
  --admin "$ADMIN" \
  --fee_recipient "$FEE_RECIPIENT" \
  --fee_bps "$FEE_BPS")"

# Native XLM wrapped as a Stellar Asset Contract: a ready-made SEP-41 token for demos.
XLM_SAC="$(stellar contract id asset --asset native --network "$NETWORK")"

mkdir -p deployments
cat > deployments/testnet.json <<JSON
{
  "network": "$NETWORK",
  "contractId": "$CONTRACT_ID",
  "admin": "$ADMIN",
  "feeRecipient": "$FEE_RECIPIENT",
  "feeBps": $FEE_BPS,
  "demoToken": "$XLM_SAC"
}
JSON

cat > frontend/.env.local <<ENV
NEXT_PUBLIC_NETWORK=testnet
NEXT_PUBLIC_RPC_URL=https://soroban-testnet.stellar.org
NEXT_PUBLIC_NETWORK_PASSPHRASE=Test SDF Network ; September 2015
NEXT_PUBLIC_CADENCE_CONTRACT_ID=$CONTRACT_ID
NEXT_PUBLIC_DEMO_TOKEN_ID=$XLM_SAC
ENV

echo
echo "Cadence deployed: $CONTRACT_ID"
echo "Explorer: https://stellar.expert/explorer/testnet/contract/$CONTRACT_ID"
echo "Wrote deployments/testnet.json and frontend/.env.local"
