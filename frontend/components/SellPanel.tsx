"use client";
import { useCallback, useEffect, useState } from "react";
import { createPlan, plansOf, setPlanActive, tokenInfo, type Plan, type TokenInfo } from "@/lib/cadence";
import { config } from "@/lib/config";
import { PERIOD_PRESETS, formatAmount, formatPeriod, parseAmount } from "@/lib/format";
import { Status } from "./Status";
import { useAction } from "./useAction";

export function SellPanel({ address }: { address: string | null }) {
  const [name, setName] = useState("");
  const [token, setToken] = useState<string>(config.demoTokenId);
  const [price, setPrice] = useState("");
  const [period, setPeriod] = useState(PERIOD_PRESETS[3].seconds);
  const [plans, setPlans] = useState<(Plan & { info: TokenInfo })[]>([]);
  const { busy, error, notice, setNotice, run } = useAction();

  const load = useCallback(async () => {
    if (!address) return setPlans([]);
    const list = await plansOf(address);
    setPlans(await Promise.all(list.map(async (p) => ({ ...p, info: await tokenInfo(p.token) }))));
  }, [address]);

  useEffect(() => {
    load().catch(() => setPlans([]));
  }, [load]);

  if (!address) return <p className="empty">Connect a wallet to create a plan.</p>;

  async function submit(e: React.SyntheticEvent) {
    e.preventDefault();
    const res = await run("Creating plan", async () => {
      const info = await tokenInfo(token);
      return createPlan(address!, {
        token,
        amount: parseAmount(price, info.decimals),
        period,
        name: name.trim(),
      });
    });
    if (res) {
      setNotice(`Plan #${res.result} is live. Share ${window.location.origin}/?plan=${res.result}`);
      setName("");
      setPrice("");
      await load();
    }
  }

  return (
    <div className="stack">
      <form onSubmit={submit} className="card form">
        <h2>New plan</h2>
        <label>
          Plan name
          <input value={name} onChange={(e) => setName(e.target.value)} maxLength={64} required placeholder="Studio membership" />
        </label>
        <label>
          Token contract
          <input value={token} onChange={(e) => setToken(e.target.value.trim())} required placeholder="C…" />
        </label>
        <div className="row">
          <label>
            Price per cycle
            <input value={price} onChange={(e) => setPrice(e.target.value)} inputMode="decimal" required placeholder="10" />
          </label>
          <label>
            Billed
            <select value={period} onChange={(e) => setPeriod(Number(e.target.value))}>
              {PERIOD_PRESETS.map((p) => (
                <option key={p.seconds} value={p.seconds}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <button className="btn btn-primary" disabled={!!busy}>
          Create plan
        </button>
        <Status busy={busy} error={error} notice={notice} />
      </form>

      <section className="card">
        <h2>Your plans</h2>
        {plans.length === 0 ? (
          <p className="empty">No plans yet. Create one above and share its link.</p>
        ) : (
          <ul className="list">
            {plans.map((p) => (
              <li key={String(p.id)} className="list-item">
                <div>
                  <strong>
                    #{String(p.id)} {p.name}
                  </strong>
                  <div className="muted">
                    {formatAmount(p.amount, p.info.decimals)} {p.info.symbol} {formatPeriod(p.period)}
                    {p.active ? "" : " · closed to new subscribers"}
                  </div>
                </div>
                <button
                  className="btn btn-quiet"
                  disabled={!!busy}
                  onClick={async () => {
                    await run(p.active ? "Closing plan" : "Reopening plan", () => setPlanActive(address, p.id, !p.active));
                    await load();
                  }}
                >
                  {p.active ? "Close plan" : "Reopen plan"}
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </div>
  );
}
