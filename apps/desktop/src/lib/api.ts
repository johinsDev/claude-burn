import { invoke } from "@tauri-apps/api/core";

export type Billing = "flat" | "overage" | "unknown";

export type PlanLimit = {
  kind: string;
  group: string;
  percent: number;
  severity: string;
  resets_at: string | null;
  scope: string | null;
  is_active: boolean;
};

export type PlanUsage = {
  fetched_at_ms: number | null;
  limits: PlanLimit[];
  extra_usage_spent: number | null;
  extra_usage_limit: number | null;
  extra_usage_enabled: boolean;
};

export type LiveSession = {
  pid: number;
  session_id: string;
  cwd: string;
  name: string | null;
  status: string | null;
  started_at_ms: number | null;
  version: string | null;
  account: string;
};

export type AccountInfo = {
  name: string;
  email: string | null;
  org: string | null;
  plan: string | null;
  billing: Billing;
  is_billable: boolean;
  plan_usage: PlanUsage | null;
  live_sessions: LiveSession[];
};

export type DayRow = { day: string; account: string; cost_usd: number; turns: number };
export type MonthRow = { month: string; account: string; cost_usd: number; turns: number };
export type ModelRow = {
  account: string;
  model: string;
  cost_usd: number;
  turns: number;
  out_tok: number;
};

export type Composition = {
  fresh_input: number;
  cache_write_5m: number;
  cache_write_1h: number;
  cache_read: number;
  output: number;
  web_search: number;
};

export type TraySummary = {
  today_usd: number;
  today_billable_usd: number;
  worst_limit_pct: number | null;
  worst_limit_kind: string | null;
  live_sessions: number;
  max_live_ctx: number | null;
};

export type Overview = {
  accounts: AccountInfo[];
  today_usd: number;
  today_billable_usd: number;
  week_usd: number;
  /// La parte de `week_usd` que de verdad se factura (cuentas con overage).
  week_billable_usd: number;
  month_usd: number;
  month_billable_usd: number;
  subagents: SubagentSplit;
  by_day: DayRow[];
  by_month: MonthRow[];
  composition: Composition;
  tray: TraySummary;
  known_accounts: string[];
  data_from: string | null;
  data_to: string | null;
};

/** Cuanto del gasto se lo llevaron los subagentes, que se facturan aparte. */
export type SubagentSplit = {
  cost_usd: number;
  total_usd: number;
  turns: number;
  agents: number;
  sessions: number;
};

export type SessionRow = {
  session_id: string;
  account: string;
  project: string;
  first_ts: string;
  last_ts: string;
  turns: number;
  cost_usd: number;
  cost_per_turn: number;
  max_ctx: number;
  avg_ctx: number;
  compactions: number;
  models: string;
  /// Del costo total, lo que se fue en subagentes de esta sesion.
  agent_usd: number;
  agents: number;
  /// Titulo que Claude Code le genero a la sesion, si alcanzo a hacerlo.
  title: string | null;
  /// Primer prompt de la sesion: el respaldo cuando no hay titulo.
  prompt: string | null;
  /// `false` cuando la cuenta es de tarifa plana: el $ es consumo, no factura.
  is_billable: boolean;
};

export type TurnPoint = {
  ts: string;
  ctx_tok: number;
  cost_usd: number;
  model: string;
  out_tok: number;
  effort: string | null;
  read_tok: number;
  cost_read: number;
  cost_output: number;
  agent_id: string | null;
};

export type AlertConfig = {
  budget_daily_usd: number | null;
  budget_weekly_usd: number | null;
  budget_monthly_usd: number | null;
  budget_steps: number[];
  limit_steps: number[];
  context_warn_tokens: number;
  context_critical_tokens: number;
  expensive_share: number;
  expensive_min_usd: number;
  cooldown_minutes: number;
};

export type FiredAlert = {
  kind: string;
  fired_at_ms: number;
  alert: {
    kind: string;
    key: string;
    title: string;
    body: string;
    severity: "info" | "warn" | "critical";
    account: string | null;
    project: string | null;
    session_id: string | null;
  };
};

/** Recorte que comparten Resumen, Sesiones y Modelos. */
export type Filter = { account: string | null; since: string | null };

export const PERIODS = [
  { id: "1", label: "Hoy", days: 1 },
  { id: "7", label: "7 dias", days: 7 },
  { id: "30", label: "30 dias", days: 30 },
  { id: "all", label: "Todo", days: null },
] as const;

export type PeriodId = (typeof PERIODS)[number]["id"];

/** Convierte periodo + cuenta en el filtro que espera el backend. */
export function buildFilter(period: PeriodId, account: string | null): Filter {
  const days = PERIODS.find((p) => p.id === period)?.days ?? null;
  return {
    account,
    since:
      days === null
        ? null
        : new Date(Date.now() - days * 86_400_000).toISOString().replace("Z", "Z"),
  };
}

export const api = {
  overview: (filter?: Filter) => invoke<Overview>("overview", { filter }),
  syncNow: () => invoke<number>("sync_now"),
  sessions: (filter?: Filter, limit?: number) =>
    invoke<SessionRow[]>("sessions", { filter, limit }),
  sessionTimeline: (sessionId: string) =>
    invoke<TurnPoint[]>("session_timeline", { sessionId }),
  models: (filter?: Filter) => invoke<ModelRow[]>("models", { filter }),
  contextHistogram: (filter?: Filter) =>
    invoke<[number, number][]>("context_histogram", { filter }),
  budgets: () => invoke<[string, string, number][]>("budgets"),
  setBudget: (scope: string, period: string, limitUsd: number) =>
    invoke<void>("set_budget", { scope, period, limitUsd }),
  alertConfig: () => invoke<AlertConfig>("alert_config"),
  setAlertConfig: (config: AlertConfig) => invoke<void>("set_alert_config", { config }),
  recentAlerts: (limit?: number) => invoke<FiredAlert[]>("recent_alerts", { limit }),
};

/** Desglosa la composicion en filas ordenadas, la forma que consumen los graficos. */
export function compositionRows(c: Composition) {
  const rows = [
    { key: "cache_read", label: "Releer contexto", value: c.cache_read, color: "var(--color-crit)" },
    { key: "cache_write_1h", label: "Cache 1 h", value: c.cache_write_1h, color: "var(--color-hot)" },
    { key: "cache_write_5m", label: "Cache 5 min", value: c.cache_write_5m, color: "var(--color-warn)" },
    { key: "output", label: "Output", value: c.output, color: "var(--color-ok)" },
    { key: "fresh_input", label: "Input fresco", value: c.fresh_input, color: "var(--color-cool)" },
    { key: "web_search", label: "Busqueda web", value: c.web_search, color: "var(--color-ink-faint)" },
  ].filter((r) => r.value > 0);
  return rows.toSorted((a, b) => b.value - a.value);
}

export function compositionTotal(c: Composition) {
  return (
    c.fresh_input + c.cache_write_5m + c.cache_write_1h + c.cache_read + c.output + c.web_search
  );
}
