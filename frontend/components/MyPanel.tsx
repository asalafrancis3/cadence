"use client";
import { useCallback, useEffect, useState } from "react";
import {
  cancel,
  charge,
  getPlan,
  isDue,
  subscriptionsOf,
  tokenInfo,
  type Plan,
  type Subscription,
  type TokenInfo,
} from "@/lib/cadence";
import { formatAmount, formatDate, formatPeriod } from "@/lib/format";
import { CycleStrip } from "./CycleStrip";
import { Status } from "./Status";
import { useAction } from "./useAction";

type Row = { sub: Subscription; plan: Plan; info: TokenInfo; due: boolean };

export function MyPanel({ address }: { address: string | null }) {
  const [rows, setRows] = useState<Row[]>([]);
  const [loading, setLoading] = useState(false);
  const { busy, error, notice, setNotice, run } = useAction();

  const load = useCallback(async () => {
    if (!address) return setRows([]);
    setLoading(true);
    try {
      const subs = await subscriptionsOf(address);
      setRows(
        await Promise.all(
          subs.map(async (sub) => {
            const plan = await getPlan(sub.planId);
            const [info, due] = await Promise.all([tokenInfo(plan.token), isDue(sub.id)]);
            return { sub, plan, info, due };
          }),
        ),
      );
    } finally {
      setLoading(false);
    }
  }, [address]);

  useEffect(() => {
    load().catch(() => setRows([]));
  }, [load]);

  if (!address) return <p className="empty">Connect a wallet to see your subscriptions.</p>;

  return (
    <div className="stack">
      <div className="row row-between">
        <h2>Your subscriptions</h2>
        <button className="btn btn-quiet" onClick={() => void load()} disabled={loading}>
          Refresh
        </button>
      </div>
      <Status busy={busy} error={error} notice={notice} />
      {rows.length === 0 && !loading && <p className="empty">Nothing here yet. Subscribe to a plan and it will show up.</p>}
      {rows.map(({ sub, plan, info, due }) => {
        const active = sub.status === "Active";
        return (
          <article key={String(sub.id)} className="card">
            <div className="row row-between">
              <div>
                <strong>{plan.name || `Plan #${plan.id}`}</strong>
                <div className="muted">
                  {formatAmount(plan.amount, info.decimals)} {info.symbol} {formatPeriod(plan.period)}
                </div>
              </div>
              <span className={`pill pill-${sub.status.toLowerCase()}`}>
                {sub.failures > 0 && active ? "Payment failing" : sub.status}
              </span>
            </div>
            <CycleStrip
              paid={sub.cyclesPaid}
              total={sub.maxCycles}
              state={!active ? "ended" : sub.failures > 0 ? "failing" : "active"}
            />
            <p className="muted">
              {active ? `Next payment ${formatDate(sub.nextCharge)}` : "No further payments"} · {sub.cyclesPaid} paid
              {sub.maxCycles > 0 ? ` of ${sub.maxCycles}` : ""}
            </p>
            {active && (
              <div className="row">
                {due && (
                  <button
                    className="btn"
                    disabled={!!busy}
                    onClick={async () => {
                      const r = await run("Charging", () => charge(address, sub.id));
                      if (r) setNotice(`Charge result: ${Array.isArray(r.result) ? r.result[0] : r.result}.`);
                      await load();
                    }}
                  >
                    Run due payment
                  </button>
                )}
                <button
                  className="btn btn-quiet"
                  disabled={!!busy}
                  onClick={async () => {
                    const r = await run("Cancelling", () => cancel(address, sub.id));
                    if (r) setNotice("Subscription cancelled. No further payments will be taken.");
                    await load();
                  }}
                >
                  Cancel subscription
                </button>
              </div>
            )}
          </article>
        );
      })}
    </div>
  );
}
