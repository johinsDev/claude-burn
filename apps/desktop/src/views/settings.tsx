import { useState } from "react";
import { api, type AlertConfig, type CleanupPreview, type ProfileEntry } from "@/lib/api";
import { useAsyncData } from "@/hooks/use-async-data";
import { Badge, Button, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import { count, money } from "@/lib/format";
import { cn } from "@/lib/utils";
import { useI18n, useT, LANGS, type Lang } from "@/lib/i18n";

/**
 * Ajustes de cuentas y mantenimiento.
 *
 * El auto-descubrimiento (`~/.claude*`) cubre el caso normal, pero no todos
 * tienen sus config dirs ahi. Esta pantalla existe para que la app sirva en
 * maquinas que no son la del autor.
 */
export function Settings() {
  const t = useT();
  const { data: profiles, loading } = useAsyncData(() => api.profilesList(), []);
  const [rows, setRows] = useState<ProfileEntry[] | null>(null);
  const list = rows ?? profiles;

  if (loading && !list) return <Empty>{t("reading config dirs…")}</Empty>;

  return (
    <div className="grid grid-cols-2 gap-3">
      <AccountsPanel rows={list ?? []} onChange={setRows} />
      <div className="space-y-3">
        <LanguagePanel />
        <GuardPanel />
        <CleanupPanel />
      </div>
    </div>
  );
}

function AccountsPanel({
  rows,
  onChange,
}: {
  rows: ProfileEntry[];
  onChange: (rows: ProfileEntry[]) => void;
}) {
  const t = useT();
  const [dir, setDir] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const { data: ignoredCount, reload: reloadIgnored } = useAsyncData(
    () => api.profilesIgnoredCount(),
    [],
  );
  const ignored = ignoredCount ?? 0;

  const run = async (fn: () => Promise<ProfileEntry[]>) => {
    setBusy(true);
    setError(null);
    try {
      onChange(await fn());
      setDir("");
      reloadIgnored();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel>
      <PanelHead
        title={t("Accounts")}
        right={<span className="text-[10px] text-ink-faint">{t("{n} config dirs", { n: rows.length })}</span>}
      />
      <div className="space-y-2 px-3.5 py-3">
        <p className="text-[11px] leading-snug text-ink-faint">
          {t(
            "Any {glob} with a {sub} folder inside is found automatically. Add the ones that live elsewhere, and hide the ones you don't want measured.",
            { glob: "~/.claude*", sub: "projects/" },
          )}
        </p>

        {rows.map((p) => (
          <div
            key={p.config_dir}
            className={cn(
              "rounded border border-line bg-panel-2/50 px-2.5 py-2",
              p.hidden && "opacity-45",
            )}
          >
            <div className="flex items-center justify-between gap-2">
              <span className="flex min-w-0 items-center gap-2">
                <span className="truncate text-[12px] font-medium">{p.name}</span>
                <Badge tone={p.billing === "overage" ? "hot" : "neutral"}>
                  {p.billing === "overage"
                    ? t("overage")
                    : p.billing === "flat"
                      ? t("flat rate")
                      : t("unknown")}
                </Badge>
                {p.discovered ? null : <Badge tone="neutral">{t("manual")}</Badge>}
              </span>
              <span className="flex shrink-0 gap-1">
                <Button
                  disabled={busy}
                  onClick={() => void run(() => api.profileSetHidden(p.name, !p.hidden))}
                >
                  {p.hidden ? t("Show") : t("Hide account")}
                </Button>
                <Button
                  disabled={busy}
                  onClick={() => void run(() => api.profileForget(p.config_dir))}
                  title={t("remove from the list — deletes no data, can be undone")}
                >
                  {t("Remove")}
                </Button>
              </span>
            </div>
            <div className="mt-0.5 truncate text-[10px] text-ink-faint">
              {p.config_dir} · {t("{n} transcripts", { n: count(p.transcripts) })}
              {p.email ? ` · ${p.email}` : ""}
            </div>
          </div>
        ))}

        <div className="flex gap-1.5 pt-1">
          <input
            value={dir}
            onChange={(e) => setDir(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && dir.trim()) void run(() => api.profileAdd(dir));
            }}
            placeholder="~/.claude-work"
            className="min-w-0 flex-1 rounded border border-line bg-panel-2 px-2 py-1 text-[11px] text-ink outline-none placeholder:text-ink-faint focus:border-ink-faint"
          />
          <Button
            variant="solid"
            disabled={busy || !dir.trim()}
            onClick={() => void run(() => api.profileAdd(dir))}
          >
            {t("Add")}
          </Button>
        </div>
        {error ? <p className="text-[10.5px] text-crit">{error}</p> : null}
        {ignored > 0 ? (
          <p className="flex items-center justify-between gap-2 text-[10.5px] text-ink-faint">
            <span>
              {ignored === 1
                ? t("{n} config dir removed from discovery", { n: count(ignored) })
                : t("{n} config dirs removed from discovery", { n: count(ignored) })}
            </span>
            <Button disabled={busy} onClick={() => void run(api.profilesRestore)}>
              {t("Restore")}
            </Button>
          </p>
        ) : null}
      </div>
    </Panel>
  );
}

const GUARD_PERIODS: { id: string; label: string }[] = [
  { id: "daily", label: "daily" },
  { id: "weekly", label: "weekly" },
  { id: "monthly", label: "monthly" },
];

/**
 * El bloqueo: lo unico del sistema que frena en vez de avisar.
 *
 * Se configura aca y no en el shell a proposito. Cuando el bloqueo esta
 * activo, el mensaje con el que pedirias desbloquearlo tambien queda
 * bloqueado: tiene que haber una salida fuera del chat.
 */
function GuardPanel() {
  const t = useT();
  const { data: loaded } = useAsyncData(() => api.alertConfig(), []);
  const [cfg, setCfg] = useState<AlertConfig | null>(null);
  const current = cfg ?? loaded;

  if (!current) return null;

  const save = (next: AlertConfig) => {
    setCfg(next);
    void api.setAlertConfig(next);
  };

  const togglePeriod = (id: string) => {
    const has = current.guard_periods.includes(id);
    save({
      ...current,
      guard_periods: has
        ? current.guard_periods.filter((p) => p !== id)
        : [...current.guard_periods, id],
    });
  };

  const capFor = (id: string) =>
    id === "daily"
      ? current.budget_daily_usd
      : id === "weekly"
        ? current.budget_weekly_usd
        : current.budget_monthly_usd;

  return (
    <Panel>
      <PanelHead
        title={t("Block")}
        right={
          <Button
            variant={current.guard_enabled ? "solid" : undefined}
            onClick={() => save({ ...current, guard_enabled: !current.guard_enabled })}
          >
            {current.guard_enabled ? t("Active") : t("Off")}
          </Button>
        }
      />
      <div className="space-y-2.5 px-3.5 py-3">
        <p className="text-[11px] leading-snug text-ink-faint">
          {t(
            "Cuts the turn before it's sent once you're over. It's the only thing that stops you instead of warning you. Needs the {hook} hook wired into the account's {file}.",
            { hook: "budget-guard.sh", file: "settings.json" },
          )}
        </p>

        <div
          className={cn(
            "space-y-1.5",
            current.guard_enabled ? "" : "pointer-events-none opacity-40",
          )}
        >
          <div className="text-[10px] uppercase tracking-wider text-ink-faint">
            {t("which caps it enforces")}
          </div>
          {GUARD_PERIODS.map((p) => {
            const on = current.guard_periods.includes(p.id);
            const cap = capFor(p.id);
            return (
              <button
                key={p.id}
                type="button"
                onClick={() => togglePeriod(p.id)}
                className={cn(
                  "flex w-full items-center justify-between rounded border px-2.5 py-1.5 text-left transition-colors",
                  on
                    ? "border-hot/50 bg-panel-2"
                    : "border-line bg-panel-2/40 hover:border-ink-faint",
                )}
              >
                <span className="text-[11.5px]">
                  {t(p.label)}
                  <span className="text-ink-faint">
                    {" "}
                    · {cap ? money(cap) : t("no cap set")}
                  </span>
                </span>
                <span className={cn("text-[10px]", on ? "text-hot" : "text-ink-faint")}>
                  {on ? t("blocks") : t("doesn't block")}
                </span>
              </button>
            );
          })}
        </div>

        <p className="text-[10px] leading-snug text-ink-faint">
          {t("A cap with no amount set never blocks, ticked or not.")}
        </p>
      </div>
    </Panel>
  );
}

const CLEANUP_DAYS = [7, 30, 90];

/**
 * Borrar transcripts de subagente viejos.
 *
 * Es seguro para los numeros: los turnos ya estan deduplicados en SQLite y
 * todas las consultas salen de ahi. Lo unico que se pierde es el `--resume` de
 * esas ramas. Aun asi va en dos pasos, porque borra archivos.
 */
function CleanupPanel() {
  const t = useT();
  const [days, setDays] = useState(30);
  const [preview, setPreview] = useState<CleanupPreview | null>(null);
  const [done, setDone] = useState<CleanupPreview | null>(null);
  const [busy, setBusy] = useState(false);

  const look = async (d: number) => {
    setDays(d);
    setDone(null);
    setBusy(true);
    try {
      setPreview(await api.cleanupPreview(d));
    } finally {
      setBusy(false);
    }
  };

  const wipe = async () => {
    setBusy(true);
    try {
      setDone(await api.cleanupSubagents(days));
      setPreview(null);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Panel>
      <PanelHead title={t("Clean up subagent transcripts")} />
      <div className="space-y-2.5 px-3.5 py-3">
        <p className="text-[11px] leading-snug text-ink-faint">
          {t(
            "Subagents leave one {ext} per agent under {dir}, and they're the bulk of the disk use. Deleting them changes no numbers: the turns are already in the database. The only thing you lose is {resume} on those branches.",
            { ext: ".jsonl", dir: "<session>/subagents/", resume: "--resume" },
          )}
        </p>

        <div className="flex gap-1">
          {CLEANUP_DAYS.map((d) => (
            <Button key={d} disabled={busy} onClick={() => void look(d)}>
              {t("older than {n} days", { n: d })}
            </Button>
          ))}
        </div>

        {preview ? (
          preview.files === 0 ? (
            <p className="text-[11px] text-ink-dim">
              {t("Nothing older than {n} days.", { n: preview.older_than_days })}
            </p>
          ) : (
            <div className="space-y-2 rounded border border-warn/40 bg-panel-2/50 px-2.5 py-2">
              <p className="text-[11.5px]">
                {t("{f} files · {s} older than {n} days.", {
                  f: count(preview.files),
                  s: mb(preview.bytes),
                  n: preview.older_than_days,
                })}
              </p>
              <div className="flex gap-1.5">
                <Button variant="solid" disabled={busy} onClick={() => void wipe()}>
                  {t("Delete")}
                </Button>
                <Button disabled={busy} onClick={() => setPreview(null)}>
                  {t("Cancel")}
                </Button>
              </div>
            </div>
          )
        ) : null}

        {done ? (
          <p className="text-[11.5px] text-ok">
            {t("Deleted {f} files · {s} freed.", { f: count(done.files), s: mb(done.bytes) })}
          </p>
        ) : null}
      </div>
    </Panel>
  );
}

function mb(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
  return `${Math.round(bytes / 1024 ** 2)} MB`;
}

/**
 * Selector de idioma.
 *
 * El valor se guarda en la base y no en el navegador porque las alertas
 * nativas las arma Rust: una notificacion en otro idioma que la ventana se
 * lee como un bug.
 */
function LanguagePanel() {
  const { lang, setLang, t } = useI18n();
  return (
    <Panel>
      <PanelHead title={t("Language")} />
      <div className="flex gap-1 px-3.5 py-3">
        {LANGS.map((l) => (
          <Button
            key={l.id}
            variant={lang === l.id ? "solid" : undefined}
            onClick={() => setLang(l.id as Lang)}
          >
            {l.label}
          </Button>
        ))}
      </div>
    </Panel>
  );
}
