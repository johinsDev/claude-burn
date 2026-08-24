/**
 * Separador de miles. Se fija es-ES a proposito en vez de dejar el locale del
 * sistema: los numeros de la app se leen al lado de codigo y de rutas, y que
 * cambien de forma segun la maquina hace imposible compararlos de memoria.
 */
const GROUPED = new Intl.NumberFormat("es-ES", {
  maximumFractionDigits: 0,
  useGrouping: "always",
});

function group(n: number): string {
  return GROUPED.format(n);
}

/** Formatea dolares con la precision que corresponde a la magnitud. */
export function money(n: number, opts: { compact?: boolean } = {}): string {
  const abs = Math.abs(n);
  const sign = n < 0 ? "-" : "";
  if (opts.compact && abs >= 1000) return `${sign}$${(abs / 1000).toFixed(1)}k`;
  if (abs >= 1000) return `${sign}$${group(abs)}`;
  if (abs >= 100) return `${sign}$${abs.toFixed(0)}`;
  if (abs >= 1) return `${sign}$${abs.toFixed(2)}`;
  if (abs === 0) return "$0";
  return `${sign}$${abs.toFixed(3)}`;
}

/** Enteros con separador de miles: turnos, agentes, sesiones. */
export function count(n: number): string {
  return group(n);
}

export function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${group(Math.round(n / 1000))}k`;
  return String(n);
}

export function pct(part: number, whole: number): string {
  if (!whole) return "0%";
  return `${((part / whole) * 100).toFixed(1)}%`;
}

/**
 * Color segun que tan cargado esta el contexto. Los umbrales son los mismos
 * que disparan la alerta: 250K avisa, 500K es critico.
 */
export function contextTone(ctx: number): "ok" | "warn" | "hot" | "crit" {
  if (ctx >= 500_000) return "crit";
  if (ctx >= 250_000) return "hot";
  if (ctx >= 120_000) return "warn";
  return "ok";
}

export function limitTone(percent: number): "ok" | "warn" | "hot" | "crit" {
  if (percent >= 90) return "crit";
  if (percent >= 75) return "hot";
  if (percent >= 50) return "warn";
  return "ok";
}

export const toneClass: Record<string, string> = {
  ok: "text-ok",
  warn: "text-warn",
  hot: "text-hot",
  crit: "text-crit",
};

export const toneBg: Record<string, string> = {
  ok: "bg-ok",
  warn: "bg-warn",
  hot: "bg-hot",
  crit: "bg-crit",
};

/**
 * "3 min ago" a partir de un epoch en ms.
 *
 * Recibe `t` en vez de importarlo: `format.ts` no es un componente y no puede
 * usar el hook, y pasar el traductor es mas barato que volverlo un contexto.
 */
export function ago(ms: number | null | undefined, t: Translate = identity): string {
  if (!ms) return t("no data");
  const min = Math.floor((Date.now() - ms) / 60_000);
  if (min < 1) return t("just now");
  if (min < 60) return t("{n} min ago", { n: min });
  const h = Math.floor(min / 60);
  if (h < 24) return t("{n} h ago", { n: h });
  return t("{n} d ago", { n: Math.floor(h / 24) });
}

/** La firma de `t` de i18n, sin importar el modulo y crear un ciclo. */
export type Translate = (text: string, vars?: Record<string, string | number>) => string;
const identity: Translate = (text) => text;

/**
 * Estado de un limite del plan segun su fecha de reinicio.
 *
 * Cuando `resets_at` ya paso, el porcentaje del cache es de *antes* del
 * reinicio: no es tu consumo actual sino un numero muerto. Decirlo importa —
 * mostrar "15%" sin aclararlo hace creer que es el dato de hoy.
 */
export function resetState(
  iso: string | null | undefined,
  t: Translate = identity,
): { stale: boolean; label: string } {
  if (!iso) return { stale: false, label: "" };
  const diff = new Date(iso).getTime() - Date.now();
  if (diff <= 0) {
    return { stale: true, label: t("already reset · stale") };
  }
  const h = Math.floor(diff / 3_600_000);
  const m = Math.floor((diff % 3_600_000) / 60_000);
  if (h >= 24) return { stale: false, label: t("resets in {n} d", { n: Math.floor(h / 24) }) };
  return {
    stale: false,
    label: h > 0 ? t("resets in {h} h {m} min", { h, m }) : t("resets in {m} min", { m }),
  };
}

/** El slug de proyecto es la ruta con guiones; queda el nombre final. */
export function projectName(slug: string): string {
  const cleaned = slug.replace(/^-+/, "").replace(/^Users-[^-]+-/, "");
  const parts = cleaned.split("--claude-worktrees-");
  const base = parts[0]?.split("-").slice(-2).join("-") ?? cleaned;
  return parts.length > 1 ? `${base} · ${parts[1]}` : base;
}

export function shortDate(iso: string): string {
  return iso.slice(0, 10);
}

/**
 * De que trata una sesion. Claude Code le pone titulo a casi todas
 * (`ai-title`); cuando no llego a hacerlo, el primer prompt dice lo mismo con
 * mas ruido, y recien despues cae al proyecto.
 */
export function sessionTitle(s: {
  title: string | null;
  prompt: string | null;
  project: string;
}): { text: string; kind: "title" | "prompt" | "project" } {
  if (s.title?.trim()) return { text: s.title.trim(), kind: "title" };
  const prompt = s.prompt?.replace(/\s+/g, " ").trim();
  if (prompt) return { text: prompt, kind: "prompt" };
  return { text: projectName(s.project), kind: "project" };
}
