// NEXT_PUBLIC_* must be referenced literally so Next.js can inline them at build time.
export const config = {
  rpcUrl: process.env.NEXT_PUBLIC_RPC_URL ?? "https://soroban-testnet.stellar.org",
  networkPassphrase:
    process.env.NEXT_PUBLIC_NETWORK_PASSPHRASE ?? "Test SDF Network ; September 2015",
  contractId: process.env.NEXT_PUBLIC_CADENCE_CONTRACT_ID ?? "",
  demoTokenId: process.env.NEXT_PUBLIC_DEMO_TOKEN_ID ?? "",
  explorer: "https://stellar.expert/explorer/testnet",
} as const;
