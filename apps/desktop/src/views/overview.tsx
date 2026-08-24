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
          <Stat
            label="7 dias · facturado"
            value={money(data.week_billable_usd, { compact: true })}
            sub={flatSub(data.week_usd - data.week_billable_usd)}
          />
        </Panel>
        <Panel>
          <Stat
            label="30 dias · facturado"
            value={money(data.month_billable_usd, { compact: true })}
            sub={flatSub(data.month_usd - data.month_billable_usd)}
          />
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
                  {a.plan_usage ? ago(a.plan_usage.fetched_at_ms) : "sin cache"}
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

/**
 * El mes contra el techo. Es la pantalla que contesta "no quiero pasarme de
 * X al mes": cuanto va, a donde llega si sigo asi, y cuanto puedo gastar por
 * dia de aca al cierre.
 */
function MonthBudgetPanel({ pace }: { pace: MonthPace }) {
  // Filtrar por una cuenta de tarifa plana no puede cambiar tu factura. Antes
  // el panel seguia mostrando el techo de la cuenta facturable y parecia que el filtro
  // no funcionaba; decir por que es mas util que fingir un numero.
  if (pace.scoped_flat_account) {
    return (
      <Panel>
        <PanelHead
          title={`${pace.scoped_flat_account} · tarifa plana`}
          right={
            <span className="text-[10px] text-ink-faint">
              no entra en el techo
            </span>
          }
        />
        <div className="grid grid-cols-3 divide-x divide-line">
          <Stat label="Consumo de hoy" value={money(pace.today.spent_usd)} />
          <Stat label="Consumo de la semana" value={money(pace.week.spent_usd)} />
          <Stat
            label={`Consumo de ${pace.month}`}
            value={money(pace.spent_usd)}
            sub="valor de API, no se factura"
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
        title="Como vamos"
        right={<span className="text-[10px] text-ink-faint">solo cuentas con overage</span>}
      />
      <div className="grid grid-cols-3 divide-x divide-line">
        <BudgetMeter
          label="Hoy"
          spent={pace.today.spent_usd}
          budget={pace.today.budget_usd}
          foot={`corte a las ${pace.today.elapsed_label}`}
        />
        <BudgetMeter
          label="Esta semana"
          spent={pace.week.spent_usd}
          budget={pace.week.budget_usd}
          foot={pace.week.elapsed_label}
        />
        <BudgetMeter
          label={`Mes ${pace.month}`}
          spent={spent}
          budget={budget}
          foot={`dia ${pace.day} de ${pace.days_in_month}`}
        />
      </div>
      {budget ? (
        <div className="grid grid-cols-2 divide-x divide-line border-t border-line">
          <Stat
            label="Proyeccion al cierre del mes"
            value={money(projected, { compact: true })}
            tone={projected >= budget ? "text-crit" : undefined}
            sub={`${((projected / budget) * 100).toFixed(0)}% del techo al ritmo actual`}
          />
          <Stat
            label="Podes gastar por dia"
            value={money(pace.daily_allowance_usd ?? 0)}
            tone={pace.daily_allowance_usd === 0 ? "text-crit" : undefined}
            sub={
              pace.daily_allowance_usd === 0
                ? "el techo del mes ya se paso"
                : `en los ${daysLeft} dias que quedan`
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
  if (!budget) {
    return (
      <div className="px-3.5 py-3">
        <div className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</div>
        <div className="num mt-1 text-xl font-semibold">{money(spent)}</div>
        <div className="mt-1 text-[10px] text-ink-faint">sin techo definido</div>
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
          {left >= 0 ? `${money(left)} libres` : `${money(-left)} pasado`}
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
  if (split.turns === 0) return null;
  const share = split.cost_usd / (split.total_usd || 1);
  return (
    <Panel>
      <PanelHead
        title="Subagentes"
        right={
          <span className="text-[10px] text-ink-faint">
            transcripts aparte · se facturan igual
          </span>
        }
      />
      <div className="grid grid-cols-4 divide-x divide-line">
        <Stat
          label="Costo"
          value={money(split.cost_usd)}
          tone={share > 0.1 ? "text-warn" : undefined}
          sub={`${(share * 100).toFixed(1)}% del gasto del periodo`}
        />
        <Stat label="Turnos" value={count(split.turns)} />
        <Stat label="Agentes lanzados" value={count(split.agents)} />
        <Stat
          label="Sesiones que los usan"
          value={count(split.sessions)}
          sub="mira la columna sub en Sesiones"
        />
      </div>
    </Panel>
  );
}

/** El pie de los totales: lo que se consumio pero no se factura. */
function flatSub(theoretical: number): string {
  return theoretical > 0.005
    ? `+ ${money(theoretical, { compact: true })} de tarifa plana`
    : "solo cuentas con overage";
}

/**
 * Tres estados, no dos. Un config dir sin sesion iniciada no es "tarifa
 * plana": es que no sabemos como se factura, y decir lo primero es afirmar
 * algo que no leimos en ningun lado.
 */
function BillingBadge({ billing }: { billing: Billing }) {
  if (billing === "overage") return <Badge tone="hot">overage · plata real</Badge>;
  if (billing === "flat") return <Badge tone="neutral">tarifa plana</Badge>;
  return <Badge tone="neutral">facturacion desconocida</Badge>;
}
