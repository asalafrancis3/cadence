"use client";
import { shortAddr } from "@/lib/format";

type Props = {
  address: string | null;
  onConnect: () => void;
  onDisconnect: () => void;
};

export function WalletButton({ address, onConnect, onDisconnect }: Props) {
  if (!address) {
    return (
      <button className="btn btn-primary" onClick={onConnect}>
        Connect wallet
      </button>
    );
  }
  return (
    <div className="wallet">
      <span className="wallet-addr" title={address}>
        {shortAddr(address)}
      </span>
      <button className="btn btn-quiet" onClick={onDisconnect}>
        Disconnect
      </button>
    </div>
  );
}
