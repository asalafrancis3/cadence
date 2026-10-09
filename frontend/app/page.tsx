"use client";
import { useCallback, useEffect, useState } from "react";
import { CycleStrip } from "@/components/CycleStrip";
import { MyPanel } from "@/components/MyPanel";
import { SellPanel } from "@/components/SellPanel";
import { SubscribePanel } from "@/components/SubscribePanel";
import { WalletButton } from "@/components/WalletButton";
import { config } from "@/lib/config";
import { friendlyError } from "@/lib/errors";
import { connectWallet, currentAddress, disconnectWallet } from "@/lib/wallet";

const TABS = [
  { id: "subscribe", label: "Subscribe" },
  { id: "mine", label: "My subscriptions" },
  { id: "sell", label: "Sell" },
] as const;
type Tab = (typeof TABS)[number]["id"];

export default function Home() {
  const [address, setAddress] = useState<string | null>(null);
  const [walletError, setWalletError] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>("subscribe");
  const [planParam, setPlanParam] = useState("");

  useEffect(() => {
    const plan = new URLSearchParams(window.location.search).get("plan");
    if (plan && /^\d+$/.test(plan)) setPlanParam(plan);
    void currentAddress().then(setAddress);
  }, []);

  const connect = useCallback(async () => {
    setWalletError(null);
    try {
      setAddress(await connectWallet());
    } catch (e) {
      setWalletError(friendlyError(e));
    }
  }, []);

  const disconnect = useCallback(async () => {
    await disconnectWallet();
    setAddress(null);
  }, []);

  return (
    <>
      <header className="bar">
        <span className="wordmark">Cadence</span>
        <span className="net">Testnet</span>
        <WalletButton address={address} onConnect={connect} onDisconnect={disconnect} />
      </header>

      <main className="page">
        <section className="hero">
          <h1>Subscriptions that never leave your wallet.</h1>
          <p className="lede">
            Approve a spending limit, get billed on schedule, cancel whenever you like. Merchants get paid in
            stablecoins on Stellar. Nobody holds your funds in between.
          </p>
          <div className="hero-strip" aria-hidden="true">
            <CycleStrip paid={7} total={12} />
          </div>
          {walletError && <p className="status-error">{walletError}</p>}
        </section>

        <nav className="tabs" role="tablist" aria-label="Cadence sections">
          {TABS.map((t) => (
            <button
              key={t.id}
              role="tab"
              aria-selected={tab === t.id}
              className={`tab ${tab === t.id ? "tab-on" : ""}`}
              onClick={() => setTab(t.id)}
            >
              {t.label}
            </button>
          ))}
        </nav>

        <section role="tabpanel">
          {tab === "subscribe" && <SubscribePanel address={address} initialPlanId={planParam} />}
          {tab === "mine" && <MyPanel address={address} />}
          {tab === "sell" && <SellPanel address={address} />}
        </section>
      </main>

      <footer className="foot">
        {config.contractId ? (
          <a href={`${config.explorer}/contract/${config.contractId}`} target="_blank" rel="noreferrer">
            View contract on Stellar Expert
          </a>
        ) : (
          <span>Contract not deployed yet. Run scripts/deploy-testnet.sh.</span>
        )}
      </footer>
    </>
  );
}
