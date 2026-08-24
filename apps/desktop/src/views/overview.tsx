import { useMemo } from "react";
import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { compositionRows, compositionTotal, type Overview as OverviewData } from "@/lib/api";
import { Badge, Meter, Panel, PanelHead, Stat } from "@/components/ui/primitives";
import { ago, limitTone, money, pct, resetState, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";
import { ChartFrame, fmt, tooltipStyle } from "@/components/ui/chart-frame";

const ACCOUNT_COLORS = ["var(--color-hot)", "var(--color-cool)", "var(--color-ok)"];

export function Overview({ data }: { data: OverviewData }) {
  const accountNames = useMemo(
    () => [...new Set(data.by_day.map((r) => r.account))].toSorted(),
    [data.by_day],
  );

  // Recharts quiere una fila por dia con una columna por cuenta.
  const daily = useMemo(() => {
    const byDay = new Map<string, Record<string, number | string>>();
    for (const r of data.by_day) {
      const row = byDay.get(r.day) ?? { day: r.day };
      row[r.account] = r.cost_usd;
      byDay.set(r.day, row);
    }
    return [...byDay.values()].toSorted((a, b) =>
      String(a.day).localeCompare(String(b.day)),
    );
  }, [data.by_day]);

  const comp = compositionRows(data.composition);
  const total = compositionTotal(data.composition);
  const theoretical = data.today_usd - data.today_billable_usd;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-4 gap-3">
        <Panel>
          <Stat
            label="Hoy · facturado"
            value={money(data.today_billable_usd)}
            sub={
              theoretical > 0.005
                ? `+ ${money(theoretical)} de consumo en tarifa plana`
                : "solo cuentas con overage"
            }
          />
        </Panel>
        <Panel>
          <Stat label="Ultimos 7 dias" value={money(data.week_usd, { compact: true })} />
        </Panel>
        <Panel>
          <Stat label="Ultimos 30 dias" value={money(data.month_usd, { compact: true })} />
        </Panel>
        <Panel>
          <Stat
            label="Releer contexto"
            value={pct(data.composition.cache_read, total)}
            tone={data.composition.cache_read / (total || 1) > 0.5 ? "text-crit" : undefined}
            sub="del gasto historico"
          />
        </Panel>
      </div>

      <div className="grid grid-cols-3 gap-3">
        <Panel className="col-span-2">
          <PanelHead
            title="Gasto por dia"
            right={
              <div className="flex gap-3">
                {accountNames.map((a, i) => (
                  <span key={a} className="flex items-center gap-1.5 text-[10px] text-ink-dim">
                    <i
                      className="size-2 rounded-[2px]"
                      style={{ background: ACCOUNT_COLORS[i % ACCOUNT_COLORS.length] }}
                    />
                    {a}
                  </span>
                ))}
              </div>
            }
          />
          <ChartFrame height={210}>
            <AreaChart data={daily} margin={{ top: 8, right: 8, bottom: 0, left: -18 }}>
              <CartesianGrid stroke="var(--color-line)" vertical={false} />
              <XAxis
                dataKey="day"
                tickFormatter={(d: string) => d.slice(5)}
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
                minTickGap={24}
              />
              <YAxis
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
                tickFormatter={(v: number) => `$${v}`}
              />
              <Tooltip
                {...tooltipStyle}
                formatter={fmt((v, name) => [money(v), name])}
              />
              {accountNames.map((a, i) => (
                <Area
                  key={a}
                  type="monotone"
                  dataKey={a}
                  stackId="1"
                  stroke={ACCOUNT_COLORS[i % ACCOUNT_COLORS.length]}
                  fill={ACCOUNT_COLORS[i % ACCOUNT_COLORS.length]}
                  fillOpacity={0.18}
                  strokeWidth={1.5}
                />
              ))}
            </AreaChart>
          </ChartFrame>
        </Panel>

        <Panel>
          <PanelHead title="En que se va la plata" />
          <ChartFrame height={210}>
            <BarChart
              data={comp}
              layout="vertical"
              margin={{ top: 8, right: 44, bottom: 4, left: 4 }}
            >
              <XAxis type="number" hide />
              <YAxis
                type="category"
                dataKey="label"
                width={104}
                stroke="var(--color-ink-dim)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
              />
              <Tooltip
                {...tooltipStyle}
                cursor={{ fill: "var(--color-panel-2)" }}
                formatter={fmt((v) => [`${money(v)} · ${pct(v, total)}`, "costo"])}
              />
              <Bar dataKey="value" radius={[0, 3, 3, 0]} barSize={14}>
                {comp.map((r) => (
                  <Cell key={r.key} fill={r.color} />
                ))}
              </Bar>
            </BarChart>
          </ChartFrame>
        </Panel>
      </div>

      <div className="grid grid-cols-2 gap-3">
        {data.accounts.map((a) => (
          <Panel key={a.name}>
            <PanelHead
              title={
                <span className="flex items-center gap-2">
                  {a.name}
                  <Badge tone={a.is_billable ? "hot" : "neutral"}>
                    {a.is_billable ? "overage · plata real" : "tarifa plana"}
                  </Badge>
                </span>
              }
              right={
                <span className="text-[10px] text-ink-faint">
                  {a.plan_usage ? ago(a.plan_usage.fetched_at_ms) : "sin cache"}
                </span>
              }
            />
            <div className="space-y-3 px-3.5 py-3">
              <div className="text-[11px] text-ink-dim">
                {a.email} · {a.org}
              </div>

              {(a.plan_usage?.limits ?? []).filter((l) => l.is_active).length === 0 ? (
                <p className="text-[11px] text-ink-faint">
                  Sin datos de limite. Se refresca cuando corras Claude Code en esta cuenta.
                </p>
              ) : (
                a.plan_usage?.limits
                  .filter((l) => l.is_active)
                  .map((l) => {
                    const reset = resetState(l.resets_at);
                    const tone = reset.stale ? "ok" : limitTone(l.percent);
                    return (
                      <div
                        key={l.kind}
                        className={cn("space-y-1", reset.stale && "opacity-45")}
                      >
                        <div className="flex items-baseline justify-between">
                          <span className="text-[11px] text-ink-dim">
                            {l.kind === "session" ? "sesion (5 h)" : "semanal"}
                          </span>
                          <span className={cn("num text-[11px] font-semibold", toneClass[tone])}>
                            {l.percent.toFixed(0)}%
                          </span>
                        </div>
                        <Meter value={reset.stale ? 0 : l.percent} tone={tone} />
                        <div className="text-[10px] text-ink-faint">{reset.label}</div>
                      </div>
                    );
                  })
              )}

              {a.live_sessions.length > 0 ? (
                <div className="border-t border-line pt-2.5">
                  <div className="mb-1.5 text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
                    corriendo ahora
                  </div>
                  {a.live_sessions.map((s) => (
                    <div key={s.pid} className="flex items-center justify-between py-0.5">
                      <span className="truncate text-[11px]">
                        {s.name ?? s.cwd.split("/").pop()}
                      </span>
                      <Badge tone={s.status === "busy" ? "warn" : "neutral"}>{s.status}</Badge>
                    </div>
                  ))}
                </div>
              ) : null}
            </div>
          </Panel>
        ))}
      </div>
    </div>
  );
}
