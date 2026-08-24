import { useMemo } from "react";
import { Bar, BarChart, CartesianGrid, Cell, Tooltip, XAxis, YAxis } from "recharts";
import pricingTable from "@claude-burn/pricing/pricing.json" with { type: "json" };
import { api, buildFilter } from "@/lib/api";
import type { Scope } from "@/components/ui/filter-bar";
import { useAsyncData } from "@/hooks/use-async-data";
import { Empty, Panel, PanelHead, Stat } from "@/components/ui/primitives";
import { ChartFrame, fmt, tooltipStyle } from "@/components/ui/chart-frame";
import { count, money, pct, tokens } from "@/lib/format";
import { cn } from "@/lib/utils";

type Rate = { id: string; label: string; input: number; output: number };
const RATES = new Map((pricingTable.models as Rate[]).map((m) => [m.id, m]));

export function Models({ scope }: { scope: Scope }) {
  const filter = buildFilter(scope.period, scope.account);
  const deps = [scope.period, scope.account];
  const { data: rows, loading } = useAsyncData(() => api.models(filter), deps);
  const { data: hist } = useAsyncData(() => api.contextHistogram(filter), deps);

  const byModel = useMemo(() => {
    const acc = new Map<string, number>();
    for (const r of rows ?? []) acc.set(r.model, (acc.get(r.model) ?? 0) + r.cost_usd);
    return [...acc.entries()]
      .filter(([m, v]) => v > 0 && m !== "<synthetic>")
      .map(([model, cost]) => ({ model, cost, label: model.replace("claude-", "") }))
      .toSorted((a, b) => b.cost - a.cost);
  }, [rows]);

  const total = byModel.reduce((s, r) => s + r.cost, 0);

  // Contrafactual: que pasaria si lo que corrio en Fable 5 hubiera corrido en
  // Opus 5. Los dos son de la familia mas capaz; Fable cuesta exactamente el
  // doble por token, asi que la cuenta es directa.
  const fable = byModel.find((m) => m.model === "claude-fable-5");
  const fableRate = RATES.get("claude-fable-5");
  const opusRate = RATES.get("claude-opus-5");
  const savings =
    fable && fableRate && opusRate ? fable.cost * (1 - opusRate.output / fableRate.output) : 0;

  const histData = useMemo(
    () =>
      (hist ?? []).map(([bucket, requests]) => ({
        label: bucket >= 10 ? ">1M" : `${bucket * 100}k`,
        count: requests,
        bucket,
      })),
    [hist],
  );
  const histTotal = histData.reduce((s, r) => s + r.count, 0);
  const heavy = histData.filter((r) => r.bucket >= 2).reduce((s, r) => s + r.count, 0);

  if (loading || !rows) return <Empty>cargando…</Empty>;

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-3 gap-3">
        <Panel>
          <Stat
            label="Modelo mas caro"
            value={byModel[0]?.label ?? "—"}
            sub={byModel[0] ? `${money(byModel[0].cost)} · ${pct(byModel[0].cost, total)}` : ""}
          />
        </Panel>
        <Panel>
          <Stat
            label="Si Fable hubiera sido Opus 5"
            value={savings > 0 ? `− ${money(savings)}` : "—"}
            tone={savings > 0 ? "text-ok" : undefined}
            sub="Fable cuesta el doble por token"
          />
        </Panel>
        <Panel>
          <Stat
            label="Requests sobre 200k de contexto"
            value={pct(heavy, histTotal)}
            tone={heavy / (histTotal || 1) > 0.5 ? "text-crit" : "text-warn"}
            sub={`${count(heavy)} de ${count(histTotal)}`}
          />
        </Panel>
      </div>

      <div className="grid grid-cols-2 gap-3">
        <Panel>
          <PanelHead title="Gasto por modelo" />
          <ChartFrame height={230}>
            <BarChart data={byModel} margin={{ top: 8, right: 10, bottom: 4, left: -14 }}>
              <CartesianGrid stroke="var(--color-line)" vertical={false} />
              <XAxis
                dataKey="label"
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
                tickFormatter={(v: number) => (v >= 1000 ? `$${v / 1000}k` : `$${v}`)}
              />
              <Tooltip
                {...tooltipStyle}
                cursor={{ fill: "var(--color-panel-2)" }}
                formatter={fmt((v) => [money(v), "costo"])}
              />
              <Bar dataKey="cost" radius={[3, 3, 0, 0]} barSize={34}>
                {byModel.map((m) => (
                  <Cell
                    key={m.model}
                    fill={m.model.includes("fable") ? "var(--color-crit)" : "var(--color-cool)"}
                  />
                ))}
              </Bar>
            </BarChart>
          </ChartFrame>
        </Panel>

        <Panel>
          <PanelHead title="Requests por tamano de contexto" />
          <ChartFrame height={230}>
            <BarChart data={histData} margin={{ top: 8, right: 10, bottom: 4, left: -14 }}>
              <CartesianGrid stroke="var(--color-line)" vertical={false} />
              <XAxis
                dataKey="label"
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                stroke="var(--color-ink-faint)"
                fontSize={10}
                tickLine={false}
                axisLine={false}
                tickFormatter={(v: number) => (v >= 1000 ? `${v / 1000}k` : String(v))}
              />
              <Tooltip
                {...tooltipStyle}
                cursor={{ fill: "var(--color-panel-2)" }}
                formatter={fmt((v) => [count(v), "requests"])}
              />
              <Bar dataKey="count" radius={[3, 3, 0, 0]}>
                {histData.map((r) => (
                  <Cell
                    key={r.label}
                    fill={
                      r.bucket >= 5
                        ? "var(--color-crit)"
                        : r.bucket >= 2
                          ? "var(--color-hot)"
                          : "var(--color-ok)"
                    }
                  />
                ))}
              </Bar>
            </BarChart>
          </ChartFrame>
        </Panel>
      </div>

      <Panel className="overflow-hidden">
        <PanelHead title="Detalle por cuenta y modelo" />
        <table className="w-full text-[11.5px]">
          <thead>
            <tr className="border-b border-line text-[10px] uppercase tracking-wider text-ink-faint">
              <th className="px-3 py-2 text-left font-medium">cuenta</th>
              <th className="px-3 py-2 text-left font-medium">modelo</th>
              <th className="px-3 py-2 text-right font-medium">$/MTok in · out</th>
              <th className="px-3 py-2 text-right font-medium">turnos</th>
              <th className="px-3 py-2 text-right font-medium">output</th>
              <th className="px-3 py-2 text-right font-medium">costo</th>
            </tr>
          </thead>
          <tbody>
            {rows
              .filter((r) => r.cost_usd > 0)
              .map((r) => {
                const rate = RATES.get(r.model);
                return (
                  <tr key={`${r.account}-${r.model}`} className="border-b border-line/50">
                    <td className="px-3 py-1.5 text-ink-dim">{r.account}</td>
                    <td
                      className={cn(
                        "px-3 py-1.5",
                        r.model.includes("fable") && "text-crit",
                      )}
                    >
                      {rate?.label ?? r.model}
                    </td>
                    <td className="num px-3 py-1.5 text-right text-ink-faint">
                      {rate ? `${rate.input} · ${rate.output}` : "—"}
                    </td>
                    <td className="num px-3 py-1.5 text-right text-ink-dim">
                      {count(r.turns)}
                    </td>
                    <td className="num px-3 py-1.5 text-right text-ink-dim">
                      {tokens(r.out_tok)}
                    </td>
                    <td className="num px-3 py-1.5 text-right font-semibold">
                      {money(r.cost_usd)}
                    </td>
                  </tr>
                );
              })}
          </tbody>
        </table>
      </Panel>
    </div>
  );
}
