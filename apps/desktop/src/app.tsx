import { lazy, Suspense, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { api, buildFilter, type PeriodId } from "@/lib/api";
import { FilterBar, type Scope } from "@/components/ui/filter-bar";
import { useAsyncData, useInterval } from "@/hooks/use-async-data";
import { Button, Empty } from "@/components/ui/primitives";
import { TrayPopover } from "@/views/tray-popover";

// Las vistas con graficos cargan aparte: el popover de la barra de menu se
// monta sin tocar recharts, que es dos tercios del bundle.
const Overview = lazy(() => import("@/views/overview").then((m) => ({ default: m.Overview })));
const Sessions = lazy(() => import("@/views/sessions").then((m) => ({ default: m.Sessions })));
const Models = lazy(() => import("@/views/models").then((m) => ({ default: m.Models })));
const Alerts = lazy(() => import("@/views/alerts").then((m) => ({ default: m.Alerts })));
import { cn } from "@/lib/utils";

const TABS = [
  { id: "overview", label: "Resumen" },
  { id: "sessions", label: "Sesiones" },
  { id: "models", label: "Modelos" },
  { id: "alerts", label: "Alertas" },
] as const;

type Tab = (typeof TABS)[number]["id"];

export function App() {
  // Las dos ventanas comparten bundle; el hash decide cual se monta.
  if (window.location.hash.startsWith("#/tray")) return <TrayPopover />;
  return <MainWindow />;
}

function MainWindow() {
  const [tab, setTab] = useState<Tab>("overview");
  const [busy, setBusy] = useState(false);
  const [scope, setScope] = useState<Scope>({ period: "30" as PeriodId, account: null });
  const filter = buildFilter(scope.period, scope.account);
  const { data, reload } = useAsyncData(
    () => api.overview(filter),
    [scope.period, scope.account],
  );

  useInterval(reload, 30_000);

  useEffect(() => {
    const un = listen("burn://refreshed", reload);
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

  return (
    <div className="flex h-full flex-col">
      <header
        className="flex shrink-0 items-center gap-1 border-b border-line px-4 pt-7 pb-0"
        data-tauri-drag-region
      >
        <nav className="flex gap-0.5">
          {TABS.map((t) => (
            <button
              key={t.id}
              onClick={() => setTab(t.id)}
              className={cn(
                "-mb-px border-b-2 px-3 py-2 text-[12px] font-medium transition-colors",
                tab === t.id
                  ? "border-hot text-ink"
                  : "border-transparent text-ink-faint hover:text-ink-dim",
              )}
            >
              {t.label}
            </button>
          ))}
        </nav>
        <div className="ml-auto flex items-center gap-2 pb-1.5">
          {tab === "alerts" ? null : (
            <FilterBar
              scope={scope}
              onChange={setScope}
              accounts={data?.known_accounts ?? []}
              dataFrom={data?.data_from}
              dataTo={data?.data_to}
            />
          )}
          <Button onClick={() => void refresh()} disabled={busy}>
            {busy ? "sincronizando…" : "Actualizar"}
          </Button>
          <Button onClick={() => void invoke("hide_main_window")}>Ocultar</Button>
        </div>
      </header>

      <main className="flex-1 overflow-y-auto p-3">
        <Suspense fallback={<Empty>cargando…</Empty>}>
          {tab === "overview" ? (
            data ? (
              <Overview data={data} />
            ) : (
              <Empty>leyendo transcripts…</Empty>
            )
          ) : tab === "sessions" ? (
            <Sessions scope={scope} />
          ) : tab === "models" ? (
            <Models scope={scope} />
          ) : (
            <Alerts />
          )}
        </Suspense>
      </main>
    </div>
  );
}
