/** Mirrors contracts/cadence/src/errors.rs. Append-only, like the contract. */
const CONTRACT_ERRORS: Record<number, string> = {
  1: "Cadence is paused right now. You can still cancel subscriptions.",
  2: "Your account is not allowed to do that.",
  3: "The price must be greater than zero.",
  4: "The billing period must be between 1 hour and 366 days.",
  5: "That fee is above the 5% cap.",
  6: "That plan does not exist.",
  7: "This plan is closed to new subscribers.",
  8: "That subscription does not exist.",
  9: "That subscription is no longer active.",
  10: "The next payment is not due yet.",
  11: "A recent payment attempt failed. Try again in a few hours.",
  12: "No admin handover is pending.",
  13: "The plan name is too long (64 characters max).",
  14: "Numeric overflow.",
};

/** Turn simulator / wallet / RPC failures into a sentence a person can act on. */
export function friendlyError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  const code = raw.match(/Error\(Contract, #(\d+)\)/);
  if (code) return CONTRACT_ERRORS[Number(code[1])] ?? `Contract error #${code[1]}.`;
  if (/allowance/i.test(raw) || /insufficient/i.test(raw))
    return "Not enough allowance or balance. Approve the amount first and check your balance.";
  if (/user (declined|rejected)|rejected|denied/i.test(raw)) return "You cancelled the request in your wallet.";
  if (/account not found|op_no_account|not funded/i.test(raw))
    return "This account is not funded on Testnet yet. Fund it with Friendbot.";
  return raw.length > 220 ? `${raw.slice(0, 220)}…` : raw;
}
