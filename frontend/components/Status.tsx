type Props = { busy: string | null; error: string | null; notice: string | null };

/** One live region for progress, errors and confirmations. */
export function Status({ busy, error, notice }: Props) {
  return (
    <div className="status" role="status" aria-live="polite">
      {busy && <p className="status-busy">{busy}…</p>}
      {error && <p className="status-error">{error}</p>}
      {notice && <p className="status-ok">{notice}</p>}
    </div>
  );
}
