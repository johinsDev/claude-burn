import { useMemo, useState } from "react";
import { api, buildFilter, type SessionRow } from "@/lib/api";
import type { Scope } from "@/components/ui/filter-bar";
import { Badge, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import { contextTone, money, projectName, shortDate, tokens, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";
import { SessionDetail } from "@/views/session-detail";
import { useAsyncData } from "@/hooks/use-async-data";

type SortKey = "cost_usd" | "cost_per_turn" | "max_ctx" | "turns" | "last_ts";

const COLUMNS: { key: SortKey; label: string; width: string }[] = [
  { key: "cost_usd", label: "costo", width: "w-20" },
  { key: "cost_per_turn", label: "$/turno", width: "w-20" },
  { key: "turns", label: "turnos", width: "w-16" },
  { key: "max_ctx", label: "ctx max", width: "w-20" },
  { key: "last_ts", label: "fecha", width: "w-24" },
];

export function Sessions({ scope }: { scope: Scope }) {
  const [sort, setSort] = useState<SortKey>("cost_usd");
  const [selected, setSelected] = useState<SessionRow | null>(null);
  const { data: rows, loading } = useAsyncData(
    () => api.sessions(buildFilter(scope.period, scope.account)),
    [scope.period, scope.account],
  );

  const visible = useMemo(
    () =>
      (rows ?? []).toSorted((a, b) =>
        sort === "last_ts" ? b.last_ts.localeCompare(a.last_ts) : b[sort] - a[sort],
      ),
    [rows, sort],
  );

  const hasFlatRate = visible.some((r) => !r.is_billable);

  if (loading) return <Empty>cargando sesiones…</Empty>;

  return (
    <>
      <Panel className="overflow-hidden">
        <PanelHead title={`${visible.length} sesiones`} />
        {hasFlatRate ? (
          <p className="border-b border-line bg-panel-2/40 px-3.5 py-2 text-[10.5px] leading-snug text-ink-faint">
            Los costos con <span className="text-ink-dim">≈</span> son de cuentas
            de tarifa plana: es el valor de API que consumiste, no plata que te
            cobran. Sirve para comparar sesiones entre si — y porque el mismo
            habito en una cuenta con overage si se factura.
          </p>
        ) : null}
        <div className="max-h-[calc(100vh-190px)] overflow-y-auto">
          <table className="w-full text-[11.5px]">
            <thead className="sticky top-0 z-10 bg-panel">
              <tr className="border-b border-line text-[10px] uppercase tracking-wider text-ink-faint">
                {COLUMNS.map((c) => (
                  <th
                    key={c.key}
                    className={cn("px-2 py-2 text-right font-medium", c.width)}
                  >
                    <button
                      onClick={() => setSort(c.key)}
                      className={cn(
                        "transition-colors hover:text-ink",
                        sort === c.key && "text-ink",
                      )}
                    >
                      {c.label}
                      {sort === c.key ? " ↓" : ""}
                    </button>
                  </th>
                ))}
                <th className="px-2 py-2 text-left font-medium">proyecto</th>
                <th className="px-2 py-2 text-left font-medium">modelos</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((r) => {
                const tone = contextTone(r.max_ctx);
                return (
                  <tr
                    key={r.session_id}
                    onClick={() => setSelected(r)}
                    className="cursor-pointer border-b border-line/50 transition-colors hover:bg-panel-2"
                  >
                    <td
                      className={cn(
                        "num px-2 py-1.5 text-right font-semibold",
                        !r.is_billable && "font-normal text-ink-dim",
                      )}
                      title={
                        r.is_billable
                          ? "facturado como overage"
                          : "consumo a precio de API — esta cuenta es de tarifa plana"
                      }
                    >
                      {r.is_billable ? "" : "≈"}
                      {money(r.cost_usd)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-faint">
                      {money(r.cost_per_turn)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-dim">{r.turns}</td>
                    <td className={cn("num px-2 py-1.5 text-right", toneClass[tone])}>
                      {tokens(r.max_ctx)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-faint">
                      {shortDate(r.last_ts)}
                    </td>
                    <td className="max-w-56 truncate px-2 py-1.5">
                      <span className="text-ink-dim">{r.account}</span>{" "}
                      {projectName(r.project)}
                    </td>
                    <td className="px-2 py-1.5">
                      <div className="flex flex-wrap gap-1">
                        {modelBadges(r.models).map((m) => (
                          <Badge key={m} tone={m.includes("fable") ? "crit" : "neutral"}>
                            {m}
                          </Badge>
                        ))}
                        {r.compactions > 0 ? (
                          <Badge tone="ok">{r.compactions} compact</Badge>
                        ) : null}
                      </div>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </Panel>

      {selected ? (
        <SessionDetail session={selected} onClose={() => setSelected(null)} />
      ) : null}
    </>
  );
}

/** `models` viene como lista separada por comas desde SQL. */
function modelBadges(models: string): string[] {
  return models
    .split(",")
    .filter((m) => m && m !== "<synthetic>")
    .map((m) => m.replace("claude-", ""));
}
