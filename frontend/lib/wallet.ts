"use client";
/**
 * All wallet access lives in this one file. It wraps Stellar Wallets Kit v2
 * (static API: init / authModal / getAddress / signTransaction / disconnect),
 * which supports Freighter, xBull, Albedo, LOBSTR, Hana and others.
 * If the kit's API changes, this is the only file to touch.
 */
import { StellarWalletsKit } from "@creit-tech/stellar-wallets-kit/sdk";
// Individual module imports avoid the broken @metamask/connect-stellar transitive
// dependency that ships inside defaultModules() in @creit-tech/stellar-wallets-kit@2.x.
// MetaMask is intentionally excluded until the upstream package resolves the missing
// "@creit.tech/stellar-wallets-kit" peer dependency in @metamask/connect-stellar@0.3.x.
// The other major Stellar wallets (Freighter, LOBSTR, Albedo, Hana, xBull, Rabet) are
// all included and work correctly.
import { FreighterModule } from "@creit-tech/stellar-wallets-kit/modules/freighter";
import { LobstrModule } from "@creit-tech/stellar-wallets-kit/modules/lobstr";
import { AlbedoModule } from "@creit-tech/stellar-wallets-kit/modules/albedo";
import { HanaModule } from "@creit-tech/stellar-wallets-kit/modules/hana";
import { xBullModule } from "@creit-tech/stellar-wallets-kit/modules/xbull";
import { RabetModule } from "@creit-tech/stellar-wallets-kit/modules/rabet";
import { config } from "./config";

const MODULES = [
  new FreighterModule(),
  new LobstrModule(),
  new AlbedoModule(),
  new HanaModule(),
  new xBullModule(),
  new RabetModule(),
];

let initialised = false;
function init() {
  if (initialised || typeof window === "undefined") return;
  StellarWalletsKit.init({ modules: MODULES });
  initialised = true;
}

/** Opens the wallet picker and returns the chosen account's public key. */
export async function connectWallet(): Promise<string> {
  init();
  const { address } = await StellarWalletsKit.authModal();
  return address;
}

export async function currentAddress(): Promise<string | null> {
  init();
  try {
    const { address } = await StellarWalletsKit.getAddress();
    return address || null;
  } catch {
    return null;
  }
}

export async function disconnectWallet(): Promise<void> {
  init();
  await StellarWalletsKit.disconnect();
}

/** Ask the connected wallet to sign a transaction envelope (base64 XDR). */
export async function signXdr(xdr: string, address: string): Promise<string> {
  init();
  const { signedTxXdr } = await StellarWalletsKit.signTransaction(xdr, {
    networkPassphrase: config.networkPassphrase,
    address,
  });
  return signedTxXdr;
}
