import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { api, compositionRows, compositionTotal } from "@/lib/api";
import { useAsyncData, useInterval } from "@/hooks/use-async-data";
import { Badge, Button, Meter } from "@/components/ui/primitives";
import { ago, limitTone, money, pct, resetState, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

/**
 * El popover de la barra de menu. Responde tres preguntas en un vistazo:
 * cuanto llevo hoy, que tan cerca del limite estoy, y si alguna sesion viva
 * se esta inflando.
 */
export function TrayPopover() {
  const t = useT();
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
        {t("reading transcripts…")}
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
            {busy ? "…" : t("Refresh")}
          </Button>
          <Button
            variant="solid"
            onClick={() => void invoke("show_main_window")}
          >
            {t("Open")}
          </Button>
        </div>
      </header>

      <div className="flex-1 space-y-3 overflow-y-auto p-3">
        {theoretical > 0.005 ? (
          <p className="text-[10.5px] leading-snug text-ink-faint">
            {t("+ {v} consumed on flat-rate accounts — API value, not billed", {
              v: money(theoretical),
            })}
          </p>
        ) : null}

        {data.month.scoped_flat_account ? null : (
          <div className="space-y-2 rounded border border-line bg-panel-2/40 p-2">
            <PopoverMeter
              label={t("Today")}
              spent={data.month.today.spent_usd}
              budget={data.month.today.budget_usd}
            />
            <PopoverMeter
              label={t("Week")}
              spent={data.month.week.spent_usd}
              budget={data.month.week.budget_usd}
              foot={data.month.week.elapsed_label}
            />
            <PopoverMeter
              label={t("Month")}
              spent={data.month.spent_usd}
              budget={data.month.budget_usd}
              foot={t("day {d} of {n}", { d: data.month.day, n: data.month.days_in_month })}
            />
          </div>
        )}

        {limits.length > 0 ? (
          <Section title={t("Plan limits")}>
            {limits.map((l) => {
              const reset = resetState(l.resets_at, t);
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
                    <span>{ago(l.fetched, t)}</span>
                  </div>
                </div>
              );
            })}
          </Section>
        ) : null}

        <Section
          title={`${t("Live sessions")}${live.length ? ` · ${live.length}` : ""}`}
        >
          {live.length === 0 ? (
            <p className="text-[11px] text-ink-faint">{t("None running right now.")}</p>
          ) : (
            live.map((s) => <LiveRow key={s.pid} session={s} />)
          )}
        </Section>

        {total > 0 ? (
          <Section title={t("Where the money goes")}>
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
              {t("is re-reading context, not new work.")}
            </p>
          </Section>
        ) : null}
      </div>
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
  const t = useT();
  return (
    <button
      type="button"
      onClick={() => void api.openSession(session.session_id)}
      title={t("open this session's detail")}
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
      return "session (5 h)";
    case "weekly_all":
      return "weekly";
    case "weekly_scoped":
      return "semanal por modelo";
    default:
      return kind;
  }
}


/**
 * Un periodo contra su techo, en la version angosta del popover: etiqueta y
 * porcentaje arriba, la barra, y el pie con lo que queda.
 */
function PopoverMeter({
  label,
  spent,
  budget,
  foot,
}: {
  label: string;
  spent: number;
  budget: number | null;
  foot?: string;
}) {
  const t = useT();
  if (!budget) {
    return (
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-[11px] text-ink-dim">{label}</span>
        <span className="num text-[11px] font-semibold">{money(spent)}</span>
      </div>
    );
  }
  const share = spent / budget;
  const tone = limitTone(share * 100);
  const left = budget - spent;
  return (
    <div className="space-y-1">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-[11px] text-ink-dim">
          {label} · {t("cap {v}", { v: money(budget) })}
        </span>
        <span className={cn("num text-[11px] font-semibold", toneClass[tone])}>
          {(share * 100).toFixed(0)}%
        </span>
      </div>
      <Meter value={Math.min(share * 100, 100)} tone={tone} />
      <div className="flex justify-between text-[10px] text-ink-faint">
        <span>{t("{v} billed", { v: money(spent) })}</span>
        <span className={cn(left < 0 && toneClass.crit)}>
          {left >= 0 ? t("{v} free", { v: money(left) }) : t("{v} over", { v: money(-left) })}
          {foot ? ` · ${foot}` : ""}
        </span>
      </div>
    </div>
  );
}
