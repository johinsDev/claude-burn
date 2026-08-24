import { useState } from "react";
import { api, type CleanupPreview, type ProfileEntry } from "@/lib/api";
import { useAsyncData } from "@/hooks/use-async-data";
import { Badge, Button, Empty, Panel, PanelHead } from "@/components/ui/primitives";
import { count } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * Ajustes de cuentas y mantenimiento.
 *
 * El auto-descubrimiento (`~/.claude*`) cubre el caso normal, pero no todos
 * tienen sus config dirs ahi. Esta pantalla existe para que la app sirva en
 * maquinas que no son la del autor.
 */
export function Settings() {
  const { data: profiles, loading } = useAsyncData(() => api.profilesList(), []);
  const [rows, setRows] = useState<ProfileEntry[] | null>(null);
  const list = rows ?? profiles;

  if (loading && !list) return <Empty>leyendo config dirs…</Empty>;

  return (
    <div className="grid grid-cols-2 gap-3">
      <AccountsPanel rows={list ?? []} onChange={setRows} />
      <CleanupPanel />
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
        title="Cuentas"
        right={<span className="text-[10px] text-ink-faint">{rows.length} config dirs</span>}
      />
      <div className="space-y-2 px-3.5 py-3">
        <p className="text-[11px] leading-snug text-ink-faint">
          Se descubren solos los <code className="text-ink-dim">~/.claude*</code> que
          tengan un <code className="text-ink-dim">projects/</code> adentro. Agrega
          los que esten en otro lado y oculta los que no quieras medir.
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
                    ? "overage"
                    : p.billing === "flat"
                      ? "tarifa plana"
                      : "desconocida"}
                </Badge>
                {p.discovered ? null : <Badge tone="neutral">manual</Badge>}
              </span>
              <span className="flex shrink-0 gap-1">
                <Button
                  disabled={busy}
                  onClick={() => void run(() => api.profileSetHidden(p.name, !p.hidden))}
                >
                  {p.hidden ? "Mostrar" : "Ocultar"}
                </Button>
                <Button
                  disabled={busy}
                  onClick={() => void run(() => api.profileForget(p.config_dir))}
                  title="sacar de la lista — no borra datos, se puede deshacer"
                >
                  Quitar
                </Button>
              </span>
            </div>
            <div className="mt-0.5 truncate text-[10px] text-ink-faint">
              {p.config_dir} · {count(p.transcripts)} transcripts
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
            placeholder="~/.claude-trabajo"
            className="min-w-0 flex-1 rounded border border-line bg-panel-2 px-2 py-1 text-[11px] text-ink outline-none placeholder:text-ink-faint focus:border-ink-faint"
          />
          <Button
            variant="solid"
            disabled={busy || !dir.trim()}
            onClick={() => void run(() => api.profileAdd(dir))}
          >
            Agregar
          </Button>
        </div>
        {error ? <p className="text-[10.5px] text-crit">{error}</p> : null}
        {ignored > 0 ? (
          <p className="flex items-center justify-between gap-2 text-[10.5px] text-ink-faint">
            <span>
              {count(ignored)} config {ignored === 1 ? "dir quitado" : "dirs quitados"} del
              escaneo
            </span>
            <Button disabled={busy} onClick={() => void run(api.profilesRestore)}>
              Restaurar
            </Button>
          </p>
        ) : null}
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
      <PanelHead title="Limpiar transcripts de subagente" />
      <div className="space-y-2.5 px-3.5 py-3">
        <p className="text-[11px] leading-snug text-ink-faint">
          Los subagentes dejan un <code className="text-ink-dim">.jsonl</code> por
          agente en <code className="text-ink-dim">&lt;sesion&gt;/subagents/</code>, y
          son lo que mas ocupa. Borrarlos{" "}
          <span className="text-ink-dim">no cambia ningun numero</span>: los turnos ya
          estan en la base. Lo unico que se pierde es poder hacer{" "}
          <code className="text-ink-dim">--resume</code> de esas ramas.
        </p>

        <div className="flex gap-1">
          {CLEANUP_DAYS.map((d) => (
            <Button key={d} disabled={busy} onClick={() => void look(d)}>
              mas de {d} dias
            </Button>
          ))}
        </div>

        {preview ? (
          preview.files === 0 ? (
            <p className="text-[11px] text-ink-dim">
              No hay nada mas viejo que {preview.older_than_days} dias.
            </p>
          ) : (
            <div className="space-y-2 rounded border border-warn/40 bg-panel-2/50 px-2.5 py-2">
              <p className="text-[11.5px]">
                <span className="num font-semibold">{count(preview.files)}</span> archivos ·{" "}
                <span className="num font-semibold">{mb(preview.bytes)}</span> con mas de{" "}
                {preview.older_than_days} dias.
              </p>
              <div className="flex gap-1.5">
                <Button variant="solid" disabled={busy} onClick={() => void wipe()}>
                  Borrar
                </Button>
                <Button disabled={busy} onClick={() => setPreview(null)}>
                  Cancelar
                </Button>
              </div>
            </div>
          )
        ) : null}

        {done ? (
          <p className="text-[11.5px] text-ok">
            Borrados {count(done.files)} archivos · {mb(done.bytes)} liberados.
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
