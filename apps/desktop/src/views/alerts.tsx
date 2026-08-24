import { useState } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, type AlertConfig, type FiredAlert } from "@/lib/api";
import { Badge, Button, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import { useAsyncData } from "@/hooks/use-async-data";
import { ago, money, tokens } from "@/lib/format";
import { cn } from "@/lib/utils";

const SEVERITY_TONE = {
  critical: "crit",
  warn: "hot",
  info: "neutral",
} as const;

const KIND_LABEL: Record<string, string> = {
  budget: "presupuesto",
  context: "contexto",
  plan_limit: "limite del plan",
  expensive_model: "modelo caro",
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
            title="Presupuesto"
            right={
              saved ? <span className="text-[10px] text-ok">guardado</span> : null
            }
          />
          <div className="space-y-3 px-3.5 py-3">
            <p className="text-[11px] leading-snug text-ink-dim">
              Solo cuenta el gasto de cuentas con overage — la plata que
              realmente se factura. Avisa al {cfg.budget_steps.join(", ")}%.
            </p>
            {(
              [
                ["budget_daily_usd", "por dia"],
                ["budget_weekly_usd", "por semana"],
                ["budget_monthly_usd", "por mes"],
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
          <PanelHead title="Contexto inflado" />
          <div className="space-y-3 px-3.5 py-3">
            <p className="text-[11px] leading-snug text-ink-dim">
              Avisa cuando una sesion viva pasa estos tamanos. Es la alerta que
              ataca el 67% del gasto: pasado cierto punto, casi todo el costo
              del turno es releer lo mismo.
            </p>
            <Field label={`aviso · ${tokens(cfg.context_warn_tokens)}`}>
              <TokenSlider
                value={cfg.context_warn_tokens}
                onCommit={(v) => void update({ context_warn_tokens: v })}
              />
            </Field>
            <Field label={`critico · ${tokens(cfg.context_critical_tokens)}`}>
              <TokenSlider
                value={cfg.context_critical_tokens}
                onCommit={(v) => void update({ context_critical_tokens: v })}
              />
            </Field>
            <Field label={`no repetir antes de ${cfg.cooldown_minutes} min`}>
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
  const { data: enabled, reload } = useAsyncData(() => isEnabled(), []);

  const toggle = async () => {
    await (enabled ? disable() : enable());
    reload();
  };

  return (
    <Panel>
      <PanelHead
        title="Arranque"
        right={
          <Button variant="solid" onClick={() => void toggle()}>
            {enabled ? "Desactivar" : "Activar"}
          </Button>
        }
      />
      <p className="px-3.5 py-3 text-[11px] leading-snug text-ink-dim">
        {enabled
          ? "Arranca con el sistema. El medidor cuenta desde el primer turno del dia."
          : "No arranca solo. Si la app esta cerrada no hay alertas: las de contexto solo sirven mientras la sesion sigue abierta."}
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
  return (
    <Panel className="overflow-hidden">
      <PanelHead
        title={`Alertas disparadas${fired?.length ? ` · ${fired.length}` : ""}`}
        right={<Button onClick={onReload}>Actualizar</Button>}
      />
      {!fired || fired.length === 0 ? (
        <Empty>
          Todavia no se disparo ninguna. Es buena senal — o el presupuesto
          todavia no esta configurado.
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
  const tone = SEVERITY_TONE[fired.alert.severity] ?? "neutral";
  return (
    <li className="px-3.5 py-2.5">
      <div className="flex items-start justify-between gap-2">
        <span className="text-[12px] font-medium">{fired.alert.title}</span>
        <Badge tone={tone}>{KIND_LABEL[fired.kind] ?? fired.kind}</Badge>
      </div>
      {fired.alert.body ? (
        <p className="mt-0.5 text-[11px] leading-snug text-ink-dim">{fired.alert.body}</p>
      ) : null}
      <p className="mt-1 text-[10px] text-ink-faint">{ago(fired.fired_at_ms)}</p>
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
          placeholder="sin limite"
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
