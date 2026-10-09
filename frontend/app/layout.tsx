import type { Metadata } from "next";
import { Bricolage_Grotesque } from "next/font/google";
import "./globals.css";

const font = Bricolage_Grotesque({ subsets: ["latin"], display: "swap", variable: "--font-ui" });

export const metadata: Metadata = {
  title: "Cadence · subscriptions that stay in your wallet",
  description:
    "Non-custodial recurring payments on Stellar. Approve a limit, pay on schedule, cancel any time.",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en" className={font.variable}>
      <body>{children}</body>
    </html>
  );
}
