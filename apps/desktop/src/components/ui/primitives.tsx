import { cn } from "@/lib/utils";
import type { ReactNode } from "react";

export function Panel({
  className,
  children,
  ...rest
}: React.HTMLAttributes<HTMLDivElement>) {
  return (
    <div
      className={cn("rounded-lg border border-line bg-panel", className)}
      {...rest}
    >
      {children}
    </div>
  );
}

export function PanelHead({ title, right }: { title: ReactNode; right?: ReactNode }) {
  return (
    <div className="flex items-center justify-between border-b border-line px-3.5 py-2.5">
      <h2 className="text-[11px] font-semibold uppercase tracking-wider text-ink-dim">
        {title}
      </h2>
      {right}
    </div>
  );
}

/** Numero grande con su etiqueta. La unidad de lectura del dashboard. */
export function Stat({
  label,
  value,
  sub,
  tone,
  className,
}: {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  tone?: string;
  className?: string;
}) {
  return (
    <div className={cn("px-3.5 py-3", className)}>
      <div className="text-[10px] font-medium uppercase tracking-wider text-ink-faint">
        {label}
      </div>
      <div className={cn("num mt-1 text-2xl leading-none font-semibold", tone)}>{value}</div>
      {sub ? <div className="mt-1.5 text-[11px] text-ink-dim">{sub}</div> : null}
    </div>
  );
}

export function Badge({
  children,
  tone = "neutral",
  className,
}: {
  children: ReactNode;
  tone?: "neutral" | "ok" | "warn" | "hot" | "crit";
  className?: string;
}) {
  const tones = {
    neutral: "border-line text-ink-dim",
    ok: "border-ok/35 text-ok",
    warn: "border-warn/35 text-warn",
    hot: "border-hot/35 text-hot",
    crit: "border-crit/40 text-crit",
  };
  return (
    <span
      className={cn(
        "inline-flex items-center gap-1 rounded border px-1.5 py-0.5 text-[10px] font-medium whitespace-nowrap",
        tones[tone],
        className,
      )}
    >
      {children}
    </span>
  );
}

/** Barra de progreso que colorea segun que tan cerca del limite esta. */
export function Meter({
  value,
  tone = "ok",
  className,
}: {
  value: number;
  tone?: string;
  className?: string;
}) {
  const bg: Record<string, string> = {
    ok: "bg-ok",
    warn: "bg-warn",
    hot: "bg-hot",
    crit: "bg-crit",
  };
  return (
    <div className={cn("h-1.5 w-full overflow-hidden rounded-full bg-line", className)}>
      <div
        className={cn("h-full rounded-full transition-[width] duration-500", bg[tone] ?? bg.ok)}
        style={{ width: `${Math.min(100, Math.max(0, value))}%` }}
      />
    </div>
  );
}

export function Button({
  className,
  variant = "ghost",
  ...rest
}: React.ButtonHTMLAttributes<HTMLButtonElement> & { variant?: "ghost" | "solid" }) {
  return (
    <button
      className={cn(
        "inline-flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-[11px] font-medium transition-colors",
        "disabled:cursor-not-allowed disabled:opacity-40",
        variant === "solid"
          ? "bg-panel-2 text-ink hover:bg-line"
          : "text-ink-dim hover:bg-panel-2 hover:text-ink",
        className,
      )}
      {...rest}
    />
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="flex h-full min-h-32 items-center justify-center px-6 text-center text-[12px] text-ink-faint">
      {children}
    </div>
  );
}
