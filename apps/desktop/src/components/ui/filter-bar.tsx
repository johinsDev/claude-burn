import { PERIODS, type PeriodId } from "@/lib/api";
import { cn } from "@/lib/utils";
import { useT } from "@/lib/i18n";

export type Scope = { period: PeriodId; account: string | null };

/**
 * Recorte compartido por Resumen, Sesiones y Modelos.
 *
 * Vive en el header y no dentro de cada vista a proposito: si cada pestana
 * tuviera su propio periodo, comparar un numero contra otro dejaria de tener
 * sentido sin darse cuenta.
 */
export function FilterBar({
  scope,
  onChange,
  accounts,
  dataFrom,
  dataTo,
}: {
  scope: Scope;
  onChange: (next: Scope) => void;
  accounts: string[];
  dataFrom?: string | null;
  dataTo?: string | null;
}) {
  const t = useT();
  const showingAll = scope.period === "all";
  return (
    <div className="flex items-center gap-2">
      <Group>
        {PERIODS.map((p) => (
          <Chip
            key={p.id}
            active={scope.period === p.id}
            onClick={() => onChange({ ...scope, period: p.id })}
          >
            {t(p.label)}
          </Chip>
        ))}
      </Group>

      <Group>
        <Chip active={scope.account === null} onClick={() => onChange({ ...scope, account: null })}>
          {t("all")}
        </Chip>
        {accounts.map((a) => (
          <Chip
            key={a}
            active={scope.account === a}
            onClick={() => onChange({ ...scope, account: a })}
          >
            {a}
          </Chip>
        ))}
      </Group>

      {/* Claude Code poda transcripts viejos: decir donde arranca el historico
          evita leer "todo" como si fuera desde siempre. */}
      {showingAll && dataFrom ? (
        <span className="num text-[10px] text-ink-faint">
          {dataFrom} → {dataTo}
        </span>
      ) : null}
    </div>
  );
}

function Group({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex gap-0.5 rounded-md border border-line bg-panel p-0.5">{children}</div>
  );
}

function Chip({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className={cn(
        "rounded px-2 py-1 text-[10.5px] font-medium transition-colors",
        active ? "bg-panel-2 text-ink" : "text-ink-faint hover:text-ink-dim",
      )}
    >
      {children}
    </button>
  );
}
