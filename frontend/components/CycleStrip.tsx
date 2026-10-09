type Props = {
  paid: number;
  /** Total cycles for capped subscriptions; omit or 0 for "until cancelled". */
  total?: number;
  state?: "active" | "ended" | "failing";
};

const TICK = 12;
const GAP = 5;

/**
 * One tick per billing cycle: filled = paid, amber outline = next to be paid.
 * It is the product's central metaphor: a subscription is a rhythm you can see.
 */
export function CycleStrip({ paid, total = 0, state = "active" }: Props) {
  const slots = total > 0 ? Math.min(total, 24) : Math.min(Math.max(12, paid + 2), 24);
  const width = slots * (TICK + GAP) - GAP;
  const label =
    total > 0 ? `${paid} of ${total} cycles paid` : `${paid} cycles paid, runs until cancelled`;

  return (
    <svg
      className="strip"
      viewBox={`0 0 ${width} 30`}
      preserveAspectRatio="xMinYMid meet"
      role="img"
      aria-label={label}
    >
      {Array.from({ length: slots }, (_, i) => {
        const x = i * (TICK + GAP);
        const isPaid = i < paid;
        const isNext = i === paid && state !== "ended";
        const fill = isPaid ? (state === "ended" ? "var(--muted)" : "var(--cobalt)") : "none";
        const stroke = isNext ? (state === "failing" ? "var(--danger)" : "var(--amber)") : "var(--rule)";
        return (
          <rect
            key={i}
            x={x + 1}
            y={1}
            width={TICK - 2}
            height={28}
            rx={3}
            fill={fill}
            stroke={isPaid ? "none" : stroke}
            strokeWidth={isNext ? 2.5 : 1.5}
          />
        );
      })}
    </svg>
  );
}
