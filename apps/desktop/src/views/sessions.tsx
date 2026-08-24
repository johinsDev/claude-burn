import { useEffect, useMemo, useState } from "react";
import { api, buildFilter, type SessionRow } from "@/lib/api";
import type { Scope } from "@/components/ui/filter-bar";
import { Badge, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import {
  contextTone,
  count,
  money,
  projectName,
  sessionTitle,
  shortDate,
  tokens,
  toneClass,
} from "@/lib/format";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";
import { SessionDetail } from "@/views/session-detail";
import { useAsyncData, useLatest } from "@/hooks/use-async-data";

type SortKey = "cost_usd" | "cost_per_turn" | "max_ctx" | "turns" | "last_ts";

const COLUMNS: { key: SortKey; label: string; width: string }[] = [
  { key: "cost_usd", label: "cost", width: "w-20" },
  { key: "cost_per_turn", label: "$/turn", width: "w-20" },
  { key: "turns", label: "turns", width: "w-16" },
  { key: "max_ctx", label: "ctx max", width: "w-20" },
  { key: "last_ts", label: "date", width: "w-24" },
];

export function Sessions({
  scope,
  focusSessionId,
  onFocusHandled,
}: {
  scope: Scope;
  /// Sesion que el popover pidio abrir; puede no estar en el recorte actual.
  focusSessionId?: string | null;
  onFocusHandled?: () => void;
}) {
  const t = useT();
  const [sort, setSort] = useState<SortKey>("cost_usd");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SessionRow | null>(null);
  const { data: rows, loading } = useAsyncData(
    () => api.sessions(buildFilter(scope.period, scope.account)),
    [scope.period, scope.account],
  );

  // La sesion pedida se busca por id y no en `rows`: una sesion viva puede
  // quedar fuera del filtro de periodo y el click del popover fallaria justo
  // cuando mas sirve.
  //
  // El callback va por ref y no en las deps: es una funcion nueva en cada
  // render del padre, asi que estaba en las deps el efecto se cancelaba a si
  // mismo cada vez que llegaba el refresco de datos, y el detalle no abria.
  const handled = useLatest(onFocusHandled);
  useEffect(() => {
    if (!focusSessionId) return;
    let live = true;
    void api.sessionRow(focusSessionId).then((row) => {
      if (!live) return;
      if (row) setSelected(row);
      handled.current?.();
    });
    return () => {
      live = false;
    };
  }, [focusSessionId, handled]);

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    const matches = (r: SessionRow) =>
      !q ||
      sessionTitle(r).text.toLowerCase().includes(q) ||
      r.project.toLowerCase().includes(q) ||
      r.account.toLowerCase().includes(q);
    return (rows ?? [])
      .filter(matches)
      .toSorted((a, b) =>
        sort === "last_ts" ? b.last_ts.localeCompare(a.last_ts) : b[sort] - a[sort],
      );
  }, [rows, sort, query]);

  const hasFlatRate = visible.some((r) => !r.is_billable);

  if (loading) return <Empty>{t("loading sessions…")}</Empty>;

  return (
    <>
      <Panel className="overflow-hidden">
        <PanelHead
          title={t("{n} sessions", { n: visible.length })}
          right={
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("search by title or project…")}
              className="w-56 rounded border border-line bg-panel-2 px-2 py-1 text-[11px] text-ink outline-none placeholder:text-ink-faint focus:border-ink-faint"
            />
          }
        />
        {hasFlatRate ? (
          <p className="border-b border-line bg-panel-2/40 px-3.5 py-2 text-[10.5px] leading-snug text-ink-faint">
            {t(
              "Costs marked ≈ come from flat-rate accounts: it's the API value you consumed, not money you're charged. Useful for comparing sessions against each other — and because the same habit on an overage account does get billed.",
            )}
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
                      {t(c.label)}
                      {sort === c.key ? " ↓" : ""}
                    </button>
                  </th>
                ))}
                <th className="px-2 py-2 text-left font-medium">{t("session")}</th>
                <th className="px-2 py-2 text-left font-medium">{t("models")}</th>
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
                          ? t("billed as overage")
                          : t("API-priced consumption — this account is flat rate")
                      }
                    >
                      {r.is_billable ? "" : "≈"}
                      {money(r.cost_usd)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-faint">
                      {money(r.cost_per_turn)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-dim">{count(r.turns)}</td>
                    <td className={cn("num px-2 py-1.5 text-right", toneClass[tone])}>
                      {tokens(r.max_ctx)}
                    </td>
                    <td className="num px-2 py-1.5 text-right text-ink-faint">
                      {shortDate(r.last_ts)}
                    </td>
                    <td className="max-w-[22rem] px-2 py-1.5">
                      <SessionLabel row={r} />
                    </td>
                    <td className="px-2 py-1.5">
                      <div className="flex flex-wrap gap-1">
                        {modelBadges(r.models).map((m) => (
                          <Badge key={m} tone={m.includes("fable") ? "crit" : "neutral"}>
                            {m}
                          </Badge>
                        ))}
                        {r.compactions > 0 ? (
                          <Badge tone="ok">{t("{n} compact", { n: r.compactions })}</Badge>
                        ) : null}
                        {r.agents > 0 ? (
                          <Badge
                            tone={r.agent_usd / (r.cost_usd || 1) > 0.2 ? "warn" : "neutral"}
                          >
                            {t("{n} sub", { n: r.agents })} · {money(r.agent_usd)}
                          </Badge>
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

/**
 * Que se ve de una sesion en la tabla: de que trata arriba, donde corrio
 * abajo. El titulo es lo que permite reconocerla; el proyecto solo, no.
 */
function SessionLabel({ row }: { row: SessionRow }) {
  const t = useT();
  const { text, kind } = sessionTitle(row);
  return (
    <div className="min-w-0">
      <div
        className={cn("truncate", kind === "project" && "italic text-ink-faint")}
        title={text}
      >
        {text}
      </div>
      <div className="truncate text-[10px] text-ink-faint">
        {row.account} · {projectName(row.project)}
        {kind === "prompt" ? ` · ${t("no title, showing the first prompt")}` : ""}
      </div>
    </div>
  );
}

/** `models` viene como lista separada por comas desde SQL. */
function modelBadges(models: string): string[] {
  return models
    .split(",")
    .filter((m) => m && m !== "<synthetic>")
    .map((m) => m.replace("claude-", ""));
}
