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
import {
  compositionRows,
  compositionTotal,
  type Billing,
  type MonthPace,
  type Overview as OverviewData,
  type SubagentSplit,
} from "@/lib/api";
import { Badge, Meter, Panel, PanelHead, Stat } from "@/components/ui/primitives";
import { ago, count, limitTone, money, pct, resetState, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";
import type { Translate } from "@/lib/format";
import { ChartFrame, fmt, tooltipStyle } from "@/components/ui/chart-frame";

const ACCOUNT_COLORS = ["var(--color-hot)", "var(--color-cool)", "var(--color-ok)"];

export function Overview({ data }: { data: OverviewData }) {
  const t = useT();
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
            label={t("Today · billed")}
            value={money(data.today_billable_usd)}
            sub={
              theoretical > 0.005
                ? t("+ {v} on flat rate", { v: money(theoretical) })
                : t("overage accounts only")
            }
          />
        </Panel>
        <Panel>
          <Stat
            label={t("7 days · billed")}
            value={money(data.week_billable_usd, { compact: true })}
            sub={flatSub(data.week_usd - data.week_billable_usd, t)}
          />
        </Panel>
        <Panel>
          <Stat
            label={t("30 days · billed")}
            value={money(data.month_billable_usd, { compact: true })}
            sub={flatSub(data.month_usd - data.month_billable_usd, t)}
          />
        </Panel>
        <Panel>
          <Stat
            label={t("Re-reading context")}
            value={pct(data.composition.cache_read, total)}
            tone={data.composition.cache_read / (total || 1) > 0.5 ? "text-crit" : undefined}
            sub={t("of all-time spend")}
          />
        </Panel>
      </div>

      <div className="grid grid-cols-3 gap-3">
        <Panel className="col-span-2">
          <PanelHead
            title={t("Spend per day")}
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
          <PanelHead title={t("Where the money goes")} />
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

      <MonthBudgetPanel pace={data.month} />

      <SubagentPanel split={data.subagents} />

      <div className="grid grid-cols-2 gap-3">
        {data.accounts.map((a) => (
          <Panel key={a.name}>
            <PanelHead
              title={
                <span className="flex items-center gap-2">
                  {a.name}
                  <BillingBadge billing={a.billing} />
                </span>
              }
              right={
                <span className="text-[10px] text-ink-faint">
                  {a.plan_usage ? ago(a.plan_usage.fetched_at_ms, t) : t("no cache")}
                </span>
              }
            />
            <div className="space-y-3 px-3.5 py-3">
              <div className="text-[11px] text-ink-dim">
                {[a.email, a.org].filter(Boolean).join(" · ") || (
                  <span className="text-ink-faint">
                    Sin sesion iniciada en este config dir.
                  </span>
                )}
              </div>

              {(a.plan_usage?.limits ?? []).filter((l) => l.is_active).length === 0 ? (
                <p className="text-[11px] text-ink-faint">
                  {t("No limit data. It refreshes when you run Claude Code in this account.")}
                </p>
              ) : (
                a.plan_usage?.limits
                  .filter((l) => l.is_active)
                  .map((l) => {
                    const reset = resetState(l.resets_at, t);
                    const tone = reset.stale ? "ok" : limitTone(l.percent);
                    return (
                      <div
                        key={l.kind}
                        className={cn("space-y-1", reset.stale && "opacity-45")}
                      >
                        <div className="flex items-baseline justify-between">
                          <span className="text-[11px] text-ink-dim">
                            {l.kind === "session" ? t("session (5 h)") : t("weekly")}
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
                    {t("running now")}
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

/**
 * El mes contra el techo. Es la pantalla que contesta "no quiero pasarme de
 * X al mes": cuanto va, a donde llega si sigo asi, y cuanto puedo gastar por
 * dia de aca al cierre.
 */
function MonthBudgetPanel({ pace }: { pace: MonthPace }) {
  const t = useT();
  // Filtrar por una cuenta de tarifa plana no puede cambiar tu factura. Antes
  // el panel seguia mostrando el techo de la cuenta facturable y parecia que el filtro
  // no funcionaba; decir por que es mas util que fingir un numero.
  if (pace.scoped_flat_account) {
    return (
      <Panel>
        <PanelHead
          title={t("{a} · flat rate", { a: pace.scoped_flat_account })}
          right={
            <span className="text-[10px] text-ink-faint">
              {t("not counted against the cap")}
            </span>
          }
        />
        <div className="grid grid-cols-3 divide-x divide-line">
          <Stat label={t("Spent today")} value={money(pace.today.spent_usd)} />
          <Stat label={t("Spent this week")} value={money(pace.week.spent_usd)} />
          <Stat
            label={t("Spent in {m}", { m: pace.month })}
            value={money(pace.spent_usd)}
            sub={t("API value, not billed")}
          />
        </div>
      </Panel>
    );
  }

  const { budget_usd: budget, spent_usd: spent, projected_usd: projected } = pace;
  const daysLeft = Math.max(pace.days_in_month - pace.day, 1);

  return (
    <Panel>
      <PanelHead
        title={t("How we're doing")}
        right={<span className="text-[10px] text-ink-faint">{t("overage accounts only")}</span>}
      />
      <div className="grid grid-cols-3 divide-x divide-line">
        <BudgetMeter
          label={t("Today")}
          spent={pace.today.spent_usd}
          budget={pace.today.budget_usd}
          foot={`corte a las ${pace.today.elapsed_label}`}
        />
        <BudgetMeter
          label={t("This week")}
          spent={pace.week.spent_usd}
          budget={pace.week.budget_usd}
          foot={pace.week.elapsed_label}
        />
        <BudgetMeter
          label={t("Month {m}", { m: pace.month })}
          spent={spent}
          budget={budget}
          foot={t("day {d} of {n}", { d: pace.day, n: pace.days_in_month })}
        />
      </div>
      {budget ? (
        <div className="grid grid-cols-2 divide-x divide-line border-t border-line">
          <Stat
            label={t("Projected month-end")}
            value={money(projected, { compact: true })}
            tone={projected >= budget ? "text-crit" : undefined}
            sub={t("{p}% of the cap at the current pace", { p: ((projected / budget) * 100).toFixed(0) })}
          />
          <Stat
            label={t("You can spend per day")}
            value={money(pace.daily_allowance_usd ?? 0)}
            tone={pace.daily_allowance_usd === 0 ? "text-crit" : undefined}
            sub={
              pace.daily_allowance_usd === 0
                ? t("the monthly cap is already blown")
                : t("over the {n} days left", { n: daysLeft })
            }
          />
        </div>
      ) : null}
    </Panel>
  );
}

/** Un periodo contra su techo: cuanto va, cuanto falta y de que color. */
function BudgetMeter({
  label,
  spent,
  budget,
  foot,
}: {
  label: string;
  spent: number;
  budget: number | null;
  foot: string;
}) {
  const t = useT();
  if (!budget) {
    return (
      <div className="px-3.5 py-3">
        <div className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</div>
        <div className="num mt-1 text-xl font-semibold">{money(spent)}</div>
        <div className="mt-1 text-[10px] text-ink-faint">{t("no cap set")}</div>
      </div>
    );
  }
  const share = spent / budget;
  const tone = limitTone(share * 100);
  const left = budget - spent;
  return (
    <div className="space-y-1.5 px-3.5 py-3">
      <div className="flex items-baseline justify-between">
        <span className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</span>
        <span className={cn("num text-[11px] font-semibold", toneClass[tone])}>
          {(share * 100).toFixed(0)}%
        </span>
      </div>
      <div className="num text-xl font-semibold">
        {money(spent)}
        <span className="text-[11px] font-normal text-ink-faint"> de {money(budget)}</span>
      </div>
      <Meter value={Math.min(share * 100, 100)} tone={tone} />
      <div className="flex justify-between text-[10px] text-ink-faint">
        <span>{foot}</span>
        <span className={cn(left < 0 && toneClass.crit)}>
          {left >= 0 ? t("{v} free", { v: money(left) }) : t("{v} over", { v: money(-left) })}
        </span>
      </div>
    </div>
  );
}

/**
 * El gasto de los subagentes, que es el mas facil de no ver.
 *
 * Sus turnos no viven en el transcript de la sesion sino en archivos aparte
 * (`<sesion>/subagents/agent-*.jsonl`). Una herramienta que mira solo el nivel
 * de arriba no los cuenta — pero Anthropic si los factura.
 */
function SubagentPanel({ split }: { split: SubagentSplit }) {
  const t = useT();
  if (split.turns === 0) return null;
  const share = split.cost_usd / (split.total_usd || 1);
  return (
    <Panel>
      <PanelHead
        title={t("Subagents")}
        right={
          <span className="text-[10px] text-ink-faint">
            {t("separate transcripts · billed all the same")}
          </span>
        }
      />
      <div className="grid grid-cols-4 divide-x divide-line">
        <Stat
          label={t("Cost")}
          value={money(split.cost_usd)}
          tone={share > 0.1 ? "text-warn" : undefined}
          sub={t("{p}% of the period's spend", { p: (share * 100).toFixed(1) })}
        />
        <Stat label={t("Turns")} value={count(split.turns)} />
        <Stat label={t("Agents spawned")} value={count(split.agents)} />
        <Stat
          label={t("Sessions using them")}
          value={count(split.sessions)}
          sub={t("see the sub column under Sessions")}
        />
      </div>
    </Panel>
  );
}

/** El pie de los totales: lo que se consumio pero no se factura. */
function flatSub(theoretical: number, t: Translate): string {
  return theoretical > 0.005
    ? t("+ {v} on flat rate", { v: money(theoretical, { compact: true }) })
    : t("overage accounts only");
}

/**
 * Tres estados, no dos. Un config dir sin sesion iniciada no es "tarifa
 * plana": es que no sabemos como se factura, y decir lo primero es afirmar
 * algo que no leimos en ningun lado.
 */
function BillingBadge({ billing }: { billing: Billing }) {
  const t = useT();
  if (billing === "overage") return <Badge tone="hot">{t("overage · real money")}</Badge>;
  if (billing === "flat") return <Badge tone="neutral">{t("flat rate")}</Badge>;
  return <Badge tone="neutral">{t("unknown billing")}</Badge>;
}
