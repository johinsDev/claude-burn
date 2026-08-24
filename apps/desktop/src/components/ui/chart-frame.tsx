import type { ComponentProps, ReactElement } from "react";
import { ResponsiveContainer, Tooltip } from "recharts";

/** Estilo compartido de tooltip: todos los graficos leen igual. */
export const tooltipStyle = {
  contentStyle: {
    background: "var(--color-panel-2)",
    border: "1px solid var(--color-line)",
    borderRadius: 6,
    fontSize: 11,
    padding: "6px 9px",
  },
  labelStyle: { color: "var(--color-ink-dim)", marginBottom: 2 },
  itemStyle: { color: "var(--color-ink)" },
} as const;

type TooltipFormatter = NonNullable<ComponentProps<typeof Tooltip>["formatter"]>;

/**
 * Recharts tipa el valor del tooltip como `ValueType | undefined`. Nuestros
 * graficos siempre grafican numeros, asi que la normalizacion vive aca en vez
 * de repetirse en cada callback.
 */
export function fmt(fn: (value: number, name: string) => [string, string]): TooltipFormatter {
  return (value, name) => fn(Number(value ?? 0), String(name ?? ""));
}

export function ChartFrame({
  height,
  children,
}: {
  height: number;
  children: ReactElement;
}) {
  return (
    <div className="px-1.5 py-2" style={{ height }}>
      <ResponsiveContainer width="100%" height="100%">
        {children}
      </ResponsiveContainer>
    </div>
  );
}
