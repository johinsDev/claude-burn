import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { api, compositionRows, compositionTotal } from "@/lib/api";
import { useAsyncData, useInterval } from "@/hooks/use-async-data";
import { Badge, Button, Meter } from "@/components/ui/primitives";
import { ago, limitTone, money, pct, resetState, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * El popover de la barra de menu. Responde tres preguntas en un vistazo:
 * cuanto llevo hoy, que tan cerca del limite estoy, y si alguna sesion viva
 * se esta inflando.
 */
export function TrayPopover() {
  const [busy, setBusy] = useState(false);
  const { data, reload } = useAsyncData(() => api.overview(), []);

  useInterval(reload, 20_000);

  useEffect(() => {
    // Al abrirse vuelve a pedir datos: el usuario acaba de mirarlo.
    const un = listen("burn://popover-open", reload);
    return () => void un.then((f) => f());
  }, [reload]);

  const refresh = async () => {
    setBusy(true);
    try {
      await api.syncNow();
      reload();
    } finally {
      setBusy(false);
    }
  };

  if (!data) {
    return (
      <div className="popover-root flex items-center justify-center text-ink-faint">
        leyendo transcripts…
      </div>
    );
  }

  const live = data.accounts.flatMap((a) =>
    a.live_sessions.map((s) => ({ ...s, billable: a.is_billable })),
  );
  const limits = data.accounts.flatMap((a) =>
    (a.plan_usage?.limits ?? [])
      .filter((l) => l.is_active)
      .map((l) => ({ ...l, account: a.name, fetched: a.plan_usage?.fetched_at_ms })),
  );
  const comp = compositionRows(data.composition);
  const total = compositionTotal(data.composition);
  const cacheShare = data.composition.cache_read / (total || 1);
  const theoretical = data.today_usd - data.today_billable_usd;

  return (
    <div className="popover-root flex flex-col">
      <header
        className="flex items-center justify-between border-b border-line px-3.5 py-2.5"
        data-tauri-drag-region
      >
        <div className="flex items-baseline gap-2">
          <span className="num text-lg font-semibold">{money(data.today_billable_usd)}</span>
          <span className="text-[11px] text-ink-faint">hoy</span>
        </div>
        <div className="flex items-center gap-1">
          <Button onClick={() => void refresh()} disabled={busy}>
            {busy ? "…" : "Actualizar"}
          </Button>
          <Button
            variant="solid"
            onClick={() => void invoke("show_main_window")}
          >
            Abrir
          </Button>
        </div>
      </header>

      <div className="flex-1 space-y-3 overflow-y-auto p-3">
        {theoretical > 0.005 ? (
          <p className="text-[10.5px] leading-snug text-ink-faint">
            + {money(theoretical)} de consumo en cuentas de tarifa plana — valor
            de API, no se factura
          </p>
        ) : null}

        {data.month.budget_usd ? (
          <div className="space-y-1 rounded border border-line bg-panel-2/40 p-2">
            <div className="flex items-baseline justify-between">
              <span className="text-[11px] text-ink-dim">
                Mes · techo {money(data.month.budget_usd)}
              </span>
              <span
                className={cn(
                  "num text-[11px] font-semibold",
                  toneClass[monthTone(data.month.spent_usd, data.month.budget_usd)],
                )}
              >
                {((data.month.spent_usd / data.month.budget_usd) * 100).toFixed(0)}%
              </span>
            </div>
            <Meter
              value={Math.min((data.month.spent_usd / data.month.budget_usd) * 100, 100)}
              tone={monthTone(data.month.spent_usd, data.month.budget_usd)}
            />
            <div className="flex justify-between text-[10px] text-ink-faint">
              <span>{money(data.month.spent_usd)} facturado</span>
              <span>
                {data.month.daily_allowance_usd === 0
                  ? "techo pasado"
                  : `${money(data.month.daily_allowance_usd ?? 0)} por dia`}
              </span>
            </div>
          </div>
        ) : null}

        {/* Facturado, igual que el titular. Sumar la tarifa plana aca hacia
            que estos dos numeros no significaran nada. */}
        <div className="grid grid-cols-2 gap-2">
          <MiniStat
            label="7 dias · facturado"
            value={money(data.week_billable_usd, { compact: true })}
          />
          <MiniStat
            label="30 dias · facturado"
            value={money(data.month_billable_usd, { compact: true })}
          />
        </div>

        {limits.length > 0 ? (
          <Section title="Limites del plan">
            {limits.map((l) => {
              const reset = resetState(l.resets_at);
              // Un limite ya reiniciado muestra un porcentaje muerto: se
              // atenua para que no se lea como consumo de ahora.
              const tone = reset.stale ? "ok" : limitTone(l.percent);
              return (
                <div
                  key={`${l.account}-${l.kind}`}
                  className={cn("space-y-1", reset.stale && "opacity-45")}
                >
                  <div className="flex items-baseline justify-between gap-2">
                    <span className="truncate text-[11px] text-ink-dim">
                      {l.account} · {limitLabel(l.kind)}
                    </span>
                    <span className={cn("num text-[11px] font-semibold", toneClass[tone])}>
                      {l.percent.toFixed(0)}%
                    </span>
                  </div>
                  <Meter value={reset.stale ? 0 : l.percent} tone={tone} />
                  <div className="flex justify-between text-[10px] text-ink-faint">
                    <span>{reset.label}</span>
                    <span>{ago(l.fetched)}</span>
                  </div>
                </div>
              );
            })}
          </Section>
        ) : null}

        <Section
          title={`Sesiones vivas${live.length ? ` · ${live.length}` : ""}`}
        >
          {live.length === 0 ? (
            <p className="text-[11px] text-ink-faint">Ninguna corriendo ahora.</p>
          ) : (
            live.map((s) => <LiveRow key={s.pid} session={s} />)
          )}
        </Section>

        {total > 0 ? (
          <Section title="En que se va la plata">
            <div className="flex h-2 w-full overflow-hidden rounded-full">
              {comp.map((r) => (
                <div
                  key={r.key}
                  style={{ width: `${(r.value / total) * 100}%`, background: r.color }}
                  title={`${r.label} ${pct(r.value, total)}`}
                />
              ))}
            </div>
            <p className="text-[11px] leading-snug text-ink-dim">
              <span className={cn("num font-semibold", cacheShare > 0.5 ? "text-crit" : "")}>
                {pct(data.composition.cache_read, total)}
              </span>{" "}
              es releer contexto, no trabajo nuevo.
            </p>
          </Section>
        ) : null}
      </div>
    </div>
  );
}

function MiniStat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-line bg-panel-2/60 px-2.5 py-2">
      <div className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</div>
      <div className="num mt-0.5 text-sm font-semibold">{value}</div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="space-y-2">
      <h3 className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
        {title}
      </h3>
      <div className="space-y-2">{children}</div>
    </section>
  );
}

function LiveRow({
  session,
}: {
  session: {
    session_id: string;
    name: string | null;
    cwd: string;
    status: string | null;
    account: string;
  };
}) {
  return (
    <button
      type="button"
      onClick={() => void api.openSession(session.session_id)}
      title="abrir el detalle de esta sesion"
      className="w-full rounded-md border border-line bg-panel-2/60 px-2.5 py-2 text-left transition-colors hover:border-ink-faint hover:bg-panel-2"
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-[11px] font-medium">
          {session.name ?? session.cwd.split("/").pop()}
        </span>
        <Badge tone={session.status === "busy" ? "warn" : "neutral"}>
          {session.status ?? "?"}
        </Badge>
      </div>
      <div className="mt-0.5 truncate text-[10px] text-ink-faint">
        {session.account} · {session.cwd.split("/").slice(-2).join("/")}
      </div>
    </button>
  );
}

/** Nombres legibles para los `kind` que devuelve Anthropic. */
function limitLabel(kind: string): string {
  switch (kind) {
    case "session":
      return "sesion (5 h)";
    case "weekly_all":
      return "semanal";
    case "weekly_scoped":
      return "semanal por modelo";
    default:
      return kind;
  }
}

/** El techo mensual pintado como el resto de los medidores. */
function monthTone(spent: number, budget: number): "ok" | "warn" | "hot" | "crit" {
  return limitTone((spent / budget) * 100);
}
