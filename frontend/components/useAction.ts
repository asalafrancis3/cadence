"use client";
import { useCallback, useState } from "react";
import { friendlyError } from "@/lib/errors";

/** Tracks one in-flight async action plus its last error / success notice. */
export function useAction() {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const run = useCallback(async <T,>(label: string, fn: () => Promise<T>) => {
    setBusy(label);
    setError(null);
    setNotice(null);
    try {
      return await fn();
    } catch (e) {
      setError(friendlyError(e));
      return undefined;
    } finally {
      setBusy(null);
    }
  }, []);

  return { busy, error, notice, setNotice, run };
}
