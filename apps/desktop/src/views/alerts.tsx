import { useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, type AlertConfig, type FiredAlert } from "@/lib/api";
import { Badge, Button, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import { useAsyncData } from "@/hooks/use-async-data";
import { ago, money, tokens } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

const SEVERITY_TONE = {
  critical: "crit",
  warn: "hot",
  info: "neutral",
} as const;

const KIND_LABEL: Record<string, string> = {
  budget: "budget",
  context: "alert:context",
  plan_limit: "plan limit",
  expensive_model: "expensive model",
};

export function Alerts() {
  const { data: loaded, loading } = useAsyncData(() => api.alertConfig(), []);
  const { data: fired, reload: reloadFired } = useAsyncData(() => api.recentAlerts(80), []);

  if (loading || !loaded) return <Empty>cargando ajustes…</Empty>;

  return (
    <div className="grid grid-cols-2 gap-3">
      {/* El formulario se siembra una vez con lo guardado y despues es la
          fuente de verdad; montarlo recien aca evita sincronizarlo a mano. */}
      <AlertSettings initial={loaded} />
      <div className="space-y-3">
        <StartupPanel />
        <FiredList fired={fired} onReload={reloadFired} />
      </div>
    </div>
  );
}

function AlertSettings({ initial }: { initial: AlertConfig }) {
  const t = useT();
  const [cfg, setCfg] = useState(initial);
  const [saved, setSaved] = useState(false);

  const update = async (patch: Partial<AlertConfig>) => {
    const next = { ...cfg, ...patch };
    setCfg(next);
    await api.setAlertConfig(next);
    setSaved(true);
    setTimeout(() => setSaved(false), 1400);
  };

  return (
    <div className="space-y-3">
        <Panel>
          <PanelHead
            title={t("Budget")}
            right={
              saved ? <span className="text-[10px] text-ok">{t("saved")}</span> : null
            }
          />
          <div className="space-y-3 px-3.5 py-3">
            <p className="text-[11px] leading-snug text-ink-dim">
              {t(
                "Only counts spend from overage accounts — the money actually billed. Warns at {steps}%.",
                { steps: cfg.budget_steps.join(", ") },
              )}
            </p>
            {(
              [
                ["budget_daily_usd", "per day"],
                ["budget_weekly_usd", "per week"],
                ["budget_monthly_usd", "per month"],
              ] as const
            ).map(([key, label]) => (
              <Field key={key} label={label}>
                {/* La key incluye el valor confirmado: al persistir, el campo
                    se remonta con el valor normalizado sin un efecto extra. */}
                <MoneyInput
                  key={`${key}:${cfg[key]}`}
                  value={cfg[key]}
                  onCommit={(v) => void update({ [key]: v } as Partial<AlertConfig>)}
                />
              </Field>
            ))}
          </div>
        </Panel>

        <Panel>
          <PanelHead title={t("Context bloat")} />
          <div className="space-y-3 px-3.5 py-3">
            <p className="text-[11px] leading-snug text-ink-dim">
              {t(
                "Warns when a live session passes these sizes. This is the alert that goes after 67% of the spend: past a certain point, almost all of a turn's cost is re-reading the same thing.",
              )}
            </p>
            <Field label={`${t("warn")} · ${tokens(cfg.context_warn_tokens)}`}>
              <TokenSlider
                value={cfg.context_warn_tokens}
                onCommit={(v) => void update({ context_warn_tokens: v })}
              />
            </Field>
            <Field label={`${t("critical")} · ${tokens(cfg.context_critical_tokens)}`}>
              <TokenSlider
                value={cfg.context_critical_tokens}
                onCommit={(v) => void update({ context_critical_tokens: v })}
              />
            </Field>
            <Field label={t("don't repeat within {n} min", { n: cfg.cooldown_minutes })}>
              <input
                type="range"
                min={5}
                max={240}
                step={5}
                value={cfg.cooldown_minutes}
                onChange={(e) => void update({ cooldown_minutes: Number(e.target.value) })}
                className="w-full accent-hot"
              />
            </Field>
          </div>
        </Panel>
    </div>
  );
}

function StartupPanel() {
  const t = useT();
  const { data: enabled, reload } = useAsyncData(() => isEnabled(), []);

  const toggle = async () => {
    await (enabled ? disable() : enable());
    reload();
  };

  return (
    <Panel>
      <PanelHead
        title={t("Startup")}
        right={
          <Button variant="solid" onClick={() => void toggle()}>
            {enabled ? t("Disable") : t("Enable")}
          </Button>
        }
      />
      <p className="px-3.5 py-3 text-[11px] leading-snug text-ink-dim">
        {enabled
          ? t("Starts with the system. The meter counts from the first turn of the day.")
          : t("Won't start on its own. With the app closed there are no alerts — context ones only matter while the session is still open.")}
      </p>
    </Panel>
  );
}

function FiredList({
  fired,
  onReload,
}: {
  fired: FiredAlert[] | null;
  onReload: () => void;
}) {
  const t = useT();
  return (
    <Panel className="overflow-hidden">
      <PanelHead
        title={`${t("Alerts fired")}${fired?.length ? ` · ${fired.length}` : ""}`}
        right={<Button onClick={onReload}>{t("Refresh")}</Button>}
      />
      {!fired || fired.length === 0 ? (
        <Empty>
          {t("None fired yet. Good sign — or the budget isn't set up.")}
        </Empty>
      ) : (
        <ul className="max-h-[calc(100vh-190px)] divide-y divide-line/60 overflow-y-auto">
          {fired.map((f) => (
            <AlertRow key={`${f.kind}-${f.alert.key}-${f.fired_at_ms}`} fired={f} />
          ))}
        </ul>
      )}
    </Panel>
  );
}

function AlertRow({ fired }: { fired: FiredAlert }) {
  const t = useT();
  const { alert } = fired;
  const tone = SEVERITY_TONE[alert.severity] ?? "neutral";
  // El titulo trae el nombre de la sesion, que no alcanza para ubicarla: hay
  // sesiones con el mismo nombre en cuentas distintas.
  const origin = [alert.account, alert.project?.split("/").slice(-2).join("/")]
    .filter(Boolean)
    .join(" · ");
  return (
    <li className="px-3.5 py-2.5">
      <div className="flex items-start justify-between gap-2">
        <span className="text-[12px] font-medium">{alert.title}</span>
        <Badge tone={tone}>{t(KIND_LABEL[fired.kind] ?? fired.kind)}</Badge>
      </div>
      {origin ? (
        <p className="mt-0.5 truncate text-[10.5px] text-ink-dim">{origin}</p>
      ) : null}
      {alert.body ? (
        <p className="mt-0.5 text-[11px] leading-snug text-ink-dim">{alert.body}</p>
      ) : null}
      <div className="mt-1 flex items-baseline justify-between gap-2">
        <span className="text-[10px] text-ink-faint">{ago(fired.fired_at_ms)}</span>
        {alert.session_id ? (
          <span className="num truncate text-[9.5px] text-ink-faint/70">
            {alert.session_id.slice(0, 8)}
          </span>
        ) : null}
      </div>
    </li>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-[10px] uppercase tracking-wider text-ink-faint">{label}</span>
      {children}
    </label>
  );
}

/** Deja escribir libremente y solo persiste al salir del campo o con Enter. */
function MoneyInput({
  value,
  onCommit,
}: {
  value: number | null;
  onCommit: (v: number | null) => void;
}) {
  const t = useT();
  const [draft, setDraft] = useState(value === null ? "" : String(value));

  const commit = () => {
    const trimmed = draft.trim();
    if (trimmed === "") return onCommit(null);
    const n = Number(trimmed);
    onCommit(Number.isFinite(n) && n > 0 ? n : null);
  };

  return (
    <div className="flex items-center gap-2">
      <div className="flex flex-1 items-center rounded-md border border-line bg-panel-2 px-2">
        <span className="text-ink-faint">$</span>
        <input
          value={draft}
          inputMode="decimal"
          placeholder={t("no limit")}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => e.key === "Enter" && commit()}
          className={cn(
            "num w-full bg-transparent px-1.5 py-1.5 text-[12px] outline-none",
            "placeholder:font-sans placeholder:text-ink-faint",
          )}
        />
      </div>
      {value !== null ? (
        <span className="num w-16 text-right text-[11px] text-ink-faint">{money(value)}</span>
      ) : null}
    </div>
  );
}

function TokenSlider({
  value,
  onCommit,
}: {
  value: number;
  onCommit: (v: number) => void;
}) {
  return (
    <input
      type="range"
      min={50_000}
      max={900_000}
      step={25_000}
      value={value}
      onChange={(e) => onCommit(Number(e.target.value))}
      className="w-full accent-hot"
    />
  );
}
