/**
 * Typed client for the Cadence contract and SEP-41 tokens, built on
 * @stellar/stellar-sdk. Reads use simulation (free, no wallet); writes are
 * simulated -> prepared -> signed by the wallet -> submitted -> polled.
 */
import {
  Account,
  Address,
  BASE_FEE,
  Contract,
  TransactionBuilder,
  nativeToScVal,
  rpc,
  scValToNative,
  xdr,
} from "@stellar/stellar-sdk";
import { config } from "./config";
import { signXdr } from "./wallet";

export type SubStatus = "Active" | "Cancelled" | "Completed";
export type Plan = {
  id: bigint;
  merchant: string;
  token: string;
  amount: bigint;
  period: bigint;
  name: string;
  active: boolean;
};
export type Subscription = {
  id: bigint;
  planId: bigint;
  subscriber: string;
  nextCharge: bigint;
  cyclesPaid: number;
  maxCycles: number;
  failures: number;
  lastAttempt: bigint;
  status: SubStatus;
};
export type TokenInfo = { symbol: string; decimals: number };

const server = new rpc.Server(config.rpcUrl);
// Simulation does not need a real account; this is the all-zero ed25519 key.
const READ_ACCOUNT = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";

// ---- ScVal helpers ---------------------------------------------------------
export const sv = {
  addr: (a: string) => new Address(a).toScVal(),
  u64: (n: bigint | number) => nativeToScVal(BigInt(n), { type: "u64" }),
  u32: (n: number) => nativeToScVal(n, { type: "u32" }),
  i128: (n: bigint) => nativeToScVal(n, { type: "i128" }),
  str: (s: string) => nativeToScVal(s, { type: "string" }),
  bool: (b: boolean) => nativeToScVal(b, { type: "bool" }),
};

// ---- low-level read / write -----------------------------------------------
async function read<T>(contractId: string, method: string, args: xdr.ScVal[] = []): Promise<T> {
  const tx = new TransactionBuilder(new Account(READ_ACCOUNT, "0"), {
    fee: BASE_FEE,
    networkPassphrase: config.networkPassphrase,
  })
    .addOperation(new Contract(contractId).call(method, ...args))
    .setTimeout(30)
    .build();
  const sim = await server.simulateTransaction(tx);
  if (rpc.Api.isSimulationError(sim)) throw new Error(sim.error);
  if (!sim.result) throw new Error(`No result from ${method}`);
  return scValToNative(sim.result.retval) as T;
}

async function write<T = unknown>(
  source: string,
  contractId: string,
  method: string,
  args: xdr.ScVal[],
): Promise<{ hash: string; result: T }> {
  const account = await server.getAccount(source);
  const built = new TransactionBuilder(account, {
    fee: BASE_FEE,
    networkPassphrase: config.networkPassphrase,
  })
    .addOperation(new Contract(contractId).call(method, ...args))
    .setTimeout(60)
    .build();

  // Simulates, sets the footprint + resource fee, and attaches auth entries.
  const prepared = await server.prepareTransaction(built);
  const signedXdr = await signXdr(prepared.toXDR(), source);
  const signed = TransactionBuilder.fromXDR(signedXdr, config.networkPassphrase);

  const sent = await server.sendTransaction(signed);
  if (sent.status === "ERROR") throw new Error(`Submission failed: ${sent.status}`);

  const done = await server.pollTransaction(sent.hash, { attempts: 30 });
  if (done.status !== rpc.Api.GetTransactionStatus.SUCCESS) {
    throw new Error(`Transaction ${done.status.toLowerCase()}: ${sent.hash}`);
  }
  const result = done.returnValue ? (scValToNative(done.returnValue) as T) : (undefined as T);
  return { hash: sent.hash, result };
}

// ---- decoding --------------------------------------------------------------
type RawPlan = Omit<Plan, "id">;
type RawSub = {
  plan_id: bigint;
  subscriber: string;
  next_charge: bigint;
  cycles_paid: number;
  max_cycles: number;
  failures: number;
  last_attempt: bigint;
  status: string | string[]; // unit enum variants decode as ["Variant"]
};

const toPlan = (id: bigint, p: RawPlan): Plan => ({ ...p, id });
const toSub = (id: bigint, s: RawSub): Subscription => ({
  id,
  planId: s.plan_id,
  subscriber: s.subscriber,
  nextCharge: s.next_charge,
  cyclesPaid: s.cycles_paid,
  maxCycles: s.max_cycles,
  failures: s.failures,
  lastAttempt: s.last_attempt,
  status: (Array.isArray(s.status) ? s.status[0] : s.status) as SubStatus,
});

const cid = () => {
  if (!config.contractId) throw new Error("NEXT_PUBLIC_CADENCE_CONTRACT_ID is not set. Run scripts/deploy-testnet.sh.");
  return config.contractId;
};

// ---- reads -----------------------------------------------------------------
export async function getPlan(id: bigint): Promise<Plan> {
  return toPlan(id, await read<RawPlan>(cid(), "get_plan", [sv.u64(id)]));
}
export async function getSubscription(id: bigint): Promise<Subscription> {
  return toSub(id, await read<RawSub>(cid(), "get_subscription", [sv.u64(id)]));
}
export async function plansOf(merchant: string): Promise<Plan[]> {
  const ids = await read<bigint[]>(cid(), "plans_of", [sv.addr(merchant), sv.u32(0), sv.u32(50)]);
  return Promise.all(ids.map(getPlan));
}
export async function subscriptionsOf(subscriber: string): Promise<Subscription[]> {
  const ids = await read<bigint[]>(cid(), "subscriptions_of", [sv.addr(subscriber), sv.u32(0), sv.u32(50)]);
  return Promise.all(ids.map(getSubscription));
}
export const isDue = (id: bigint) => read<boolean>(cid(), "is_due", [sv.u64(id)]);

const tokenCache = new Map<string, Promise<TokenInfo>>();
export function tokenInfo(token: string): Promise<TokenInfo> {
  if (!tokenCache.has(token)) {
    tokenCache.set(
      token,
      Promise.all([read<string>(token, "symbol"), read<number>(token, "decimals")]).then(
        ([symbol, decimals]) => ({ symbol, decimals: Number(decimals) }),
      ),
    );
  }
  return tokenCache.get(token)!;
}
export const tokenBalance = (token: string, who: string) =>
  read<bigint>(token, "balance", [sv.addr(who)]);
export const tokenAllowance = (token: string, from: string) =>
  read<bigint>(token, "allowance", [sv.addr(from), sv.addr(cid())]);

// ---- writes ----------------------------------------------------------------
export async function createPlan(
  merchant: string,
  p: { token: string; amount: bigint; period: number; name: string },
) {
  return write<bigint>(merchant, cid(), "create_plan", [
    sv.addr(merchant),
    sv.addr(p.token),
    sv.i128(p.amount),
    sv.u64(p.period),
    sv.str(p.name),
  ]);
}
export const setPlanActive = (merchant: string, planId: bigint, active: boolean) =>
  write(merchant, cid(), "set_plan_active", [sv.u64(planId), sv.bool(active)]);

/** Step 1 of subscribing: let Cadence pull up to `amount` from you until ~30 days from now. */
export async function approveAllowance(subscriber: string, token: string, amount: bigint) {
  const { sequence } = await server.getLatestLedger();
  const thirtyDays = 30 * 17_280;
  return write(subscriber, token, "approve", [
    sv.addr(subscriber),
    sv.addr(cid()),
    sv.i128(amount),
    sv.u32(sequence + thirtyDays),
  ]);
}
/** Step 2: subscribe and pay the first cycle. `maxCycles = 0` means until cancelled. */
export const subscribe = (subscriber: string, planId: bigint, maxCycles: number) =>
  write<bigint>(subscriber, cid(), "subscribe", [sv.addr(subscriber), sv.u64(planId), sv.u32(maxCycles)]);
export const cancel = (caller: string, subId: bigint) =>
  write(caller, cid(), "cancel", [sv.addr(caller), sv.u64(subId)]);
/** Anyone can run this once a cycle is due. */
export const charge = (caller: string, subId: bigint) =>
  write<string | string[]>(caller, cid(), "charge", [sv.u64(subId)]);
