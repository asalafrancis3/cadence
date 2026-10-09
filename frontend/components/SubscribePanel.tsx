"use client";
import { useEffect, useState } from "react";
import {
  approveAllowance,
  getPlan,
  subscribe,
  tokenAllowance,
  tokenBalance,
  tokenInfo,
  type Plan,
  type TokenInfo,
} from "@/lib/cadence";
import { config } from "@/lib/config";
import { formatAmount, formatPeriod, shortAddr } from "@/lib/format";
import { Status } from "./Status";
import { useAction } from "./useAction";

type Loaded = { plan: Plan; info: TokenInfo; allowance: bigint; balance: bigint };

export function SubscribePanel({ address, initialPlanId }: { address: string | null; initialPlanId: string }) {
  const [planId, setPlanId] = useState(initialPlanId);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [cycles, setCycles] = useState("3");
  const [stopAfter, setStopAfter] = useState("");
  const { busy, error, notice, setNotice, run } = useAction();

  async function lookup(idText = planId) {
    const l = await run("Looking up plan", async () => {
      const plan = await getPlan(BigInt(idText));
      const info = await tokenInfo(plan.token);
      const [allowance, balance] = address
        ? await Promise.all([tokenAllowance(plan.token, address), tokenBalance(plan.token, address)])
        : [0n, 0n];
      return { plan, info, allowance, balance } satisfies Loaded;
    });
    setLoaded(l ?? null);
  }

  useEffect(() => {
    if (initialPlanId) void lookup(initialPlanId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [address]);

  const prepay = BigInt(Math.max(1, Number(cycles) || 1));
  const need = loaded ? loaded.plan.amount * prepay : 0n;
  const approved = !!loaded && loaded.allowance >= loaded.plan.amount;
  const fmt = (v: bigint) => (loaded ? `${formatAmount(v, loaded.info.decimals)} ${loaded.info.symbol}` : "");

  async function approve() {
    if (!loaded || !address) return;
    await run("Approving allowance", () => approveAllowance(address, loaded.plan.token, need));
    setNotice("Allowance set. Now subscribe.");
    await lookup();
  }

  async function doSubscribe() {
    if (!loaded || !address) return;
    const res = await run("Subscribing", () => subscribe(address, loaded.plan.id, Number(stopAfter) || 0));
    if (res) {
      setNotice(`Subscribed. Subscription #${res.result} is active and the first payment is done.`);
      await lookup();
    }
  }

  return (
    <div className="stack">
      <form
        className="card form"
        onSubmit={(e) => {
          e.preventDefault();
          void lookup();
        }}
      >
        <h2>Find a plan</h2>
        <div className="row row-end">
          <label>
            Plan number
            <input value={planId} onChange={(e) => setPlanId(e.target.value.replace(/\D/g, ""))} inputMode="numeric" placeholder="1" required />
          </label>
          <button className="btn btn-primary" disabled={!!busy}>
            Look up
          </button>
        </div>
        {!config.contractId && <p className="status-error">Contract ID is not configured. Run scripts/deploy-testnet.sh.</p>}
      </form>

      {loaded && (
        <section className="card form">
          <h2>{loaded.plan.name || `Plan #${loaded.plan.id}`}</h2>
          <p className="price">
            {fmt(loaded.plan.amount)} <span className="muted">{formatPeriod(loaded.plan.period)}</span>
          </p>
          <p className="muted">
            Paid to {shortAddr(loaded.plan.merchant)} · your balance {fmt(loaded.balance)} · current allowance {fmt(loaded.allowance)}
          </p>

          {!address ? (
            <p className="empty">Connect a wallet to subscribe.</p>
          ) : !loaded.plan.active ? (
            <p className="status-error">This plan is closed to new subscribers.</p>
          ) : (
            <ol className="steps">
              <li>
                <h3>Allow payments</h3>
                <p className="muted">
                  Cadence can only take what you approve here, and you can revoke it from your wallet at any time.
                </p>
                <div className="row row-end">
                  <label>
                    Cycles to cover
                    <input value={cycles} onChange={(e) => setCycles(e.target.value.replace(/\D/g, ""))} inputMode="numeric" />
                  </label>
                  <button className="btn" disabled={!!busy} onClick={approve}>
                    Approve {fmt(need)}
                  </button>
                </div>
              </li>
              <li>
                <h3>Subscribe and pay the first cycle</h3>
                <label>
                  Stop after (cycles, blank = until I cancel)
                  <input value={stopAfter} onChange={(e) => setStopAfter(e.target.value.replace(/\D/g, ""))} inputMode="numeric" placeholder="∞" />
                </label>
                <button className="btn btn-primary" disabled={!!busy || !approved} onClick={doSubscribe}>
                  Subscribe for {fmt(loaded.plan.amount)}
                </button>
                {!approved && <p className="muted">Approve at least one cycle first.</p>}
              </li>
            </ol>
          )}
          <Status busy={busy} error={error} notice={notice} />
        </section>
      )}
      {!loaded && <Status busy={busy} error={error} notice={notice} />}
    </div>
  );
}
