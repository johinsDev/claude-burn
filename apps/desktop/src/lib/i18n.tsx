import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { api } from "@/lib/api";

export type Lang = "en" | "es";
export const LANGS: { id: Lang; label: string }[] = [
  { id: "en", label: "English" },
  { id: "es", label: "Español" },
];

/**
 * Diccionario espanol, indexado por el texto en ingles.
 *
 * La clave *es* el texto por defecto, no un identificador abstracto. Asi el
 * codigo se sigue leyendo (`t("Billed this month")` en vez de
 * `t("overview.month.billed")`) y una clave sin traducir cae sola al ingles en
 * vez de mostrar el identificador crudo.
 */
const ES: Record<string, string> = {
  // — navegacion y cabecera —
  "Overview": "Resumen",
  "Sessions": "Sesiones",
  "Models": "Modelos",
  "Alerts": "Alertas",
  "Settings": "Ajustes",
  "Refresh": "Actualizar",
  "syncing…": "sincronizando…",
  "Hide": "Ocultar",
  "Open": "Abrir",
  "Close · esc": "Cerrar · esc",
  "loading…": "cargando…",
  "reading transcripts…": "leyendo transcripts…",

  // — filtros —
  "Today": "Hoy",
  "7 days": "7 dias",
  "30 days": "30 dias",
  "All": "Todo",
  "all": "todas",
  "period": "periodo",
  "account": "cuenta",

  // — resumen —
  "Today · billed": "Hoy · facturado",
  "7 days · billed": "7 dias · facturado",
  "30 days · billed": "30 dias · facturado",
  "Re-reading context": "Releer contexto",
  "of all-time spend": "del gasto historico",
  "+ {v} on flat rate": "+ {v} de tarifa plana",
  "overage accounts only": "solo cuentas con overage",
  "Spend per day": "Gasto por dia",
  "Where the money goes": "En que se va la plata",
  "Re-read context": "Releer contexto",
  "Cache 1 h": "Cache 1 h",
  "Cache 5 min": "Cache 5 min",
  "Output": "Output",
  "Fresh input": "Input fresco",
  "Web search": "Busqueda web",

  // — presupuesto —
  "How we're doing": "Como vamos",
  "This week": "Esta semana",
  "Month {m}": "Mes {m}",
  "no cap set": "sin techo definido",
  "cap {v}": "techo {v}",
  "of {v}": "de {v}",
  "{v} free": "{v} libres",
  "{v} over": "{v} pasado",
  "day {d} of {n}": "dia {d} de {n}",
  "day {d} of 7": "dia {d} de 7",
  "cut at {t}": "corte a las {t}",
  "Projected month-end": "Proyeccion al cierre del mes",
  "{p}% of the cap at the current pace": "{p}% del techo al ritmo actual",
  "You can spend per day": "Podes gastar por dia",
  "the monthly cap is already blown": "el techo del mes ya se paso",
  "over the {n} days left": "en los {n} dias que quedan",
  "{a} · flat rate": "{a} · tarifa plana",
  "not counted against the cap": "no entra en el techo",
  "Spent today": "Consumo de hoy",
  "Spent this week": "Consumo de la semana",
  "Spent in {m}": "Consumo de {m}",
  "API value, not billed": "valor de API, no se factura",
  "Month": "Mes",
  "Week": "Semana",
  "{v} billed": "{v} facturado",

  // — subagentes —
  "Subagents": "Subagentes",
  "separate transcripts · billed all the same": "transcripts aparte · se facturan igual",
  "Cost": "Costo",
  "Turns": "Turnos",
  "Agents spawned": "Agentes lanzados",
  "Sessions using them": "Sesiones que los usan",
  "{p}% of the period's spend": "{p}% del gasto del periodo",
  "see the sub column under Sessions": "mira la columna sub en Sesiones",

  // — cuentas —
  "overage · real money": "overage · plata real",
  "flat rate": "tarifa plana",
  "unknown billing": "facturacion desconocida",
  "No session signed in for this config dir.": "Sin sesion iniciada en este config dir.",
  "no cache": "sin cache",
  "No limit data. It refreshes when you run Claude Code in this account.":
    "Sin datos de limite. Se refresca cuando corras Claude Code en esta cuenta.",
  "running now": "corriendo ahora",
  "session (5 h)": "sesion (5 h)",
  "weekly": "semanal",
  "Plan limits": "Limites del plan",
  "Live sessions": "Sesiones vivas",
  "+ {v} consumed on flat-rate accounts — API value, not billed":
    "+ {v} de consumo en cuentas de tarifa plana — valor de API, no se factura",
  "is re-reading context, not new work.": "es releer contexto, no trabajo nuevo.",
  "open this session's detail": "abrir el detalle de esta sesion",
  "None running right now.": "Ninguna corriendo ahora.",
  "already reset · stale": "ya reinicio · dato viejo",
  "resets {t}": "reinicia {t}",

  // — tabla de sesiones —
  "{n} sessions": "{n} sesiones",
  "search by title or project…": "buscar por titulo o proyecto…",
  "cost": "costo",
  "$/turn": "$/turno",
  "turns": "turnos",
  "ctx max": "ctx max",
  "date": "fecha",
  "session": "sesion",
  "models": "modelos",
  "loading sessions…": "cargando sesiones…",
  "no title, showing the first prompt": "sin titulo, muestro el primer prompt",
  "billed as overage": "facturado como overage",
  "API-priced consumption — this account is flat rate":
    "consumo a precio de API — esta cuenta es de tarifa plana",
  "Costs marked ≈ come from flat-rate accounts: it's the API value you consumed, not money you're charged. Useful for comparing sessions against each other — and because the same habit on an overage account does get billed.":
    "Los costos con ≈ son de cuentas de tarifa plana: es el valor de API que consumiste, no plata que te cobran. Sirve para comparar sesiones entre si — y porque el mismo habito en una cuenta con overage si se factura.",
  "{n} compact": "{n} compact",
  "{n} sub": "{n} sub",

  // — detalle de sesion —
  "started with:": "arranco con:",
  "$ per turn": "$ por turno",
  "Max context": "Contexto max",
  "Compactions": "Compactaciones",
  "Context and cost per turn": "Contexto y costo por turno",
  "context": "contexto",
  "cost/turn": "costo/turno",
  "turn": "turno",
  "no turns recorded": "sin turnos registrados",
  "The average turn reads {r} to write {o} — a ratio of {x}:1.":
    "El turno promedio lee {r} para escribir {o} — una proporcion de {x}:1.",
  "read": "lectura",
  "average turn": "turno promedio",
  "reads": "lee",
  "writes": "escribe",
  "reads {n}× what it writes": "lee {n}× lo que escribe",
  "{a} subagents · {n} of {total} turns": "{a} subagentes · {n} de {total} turnos",
  "turn {n}": "turno {n}",
  "Priciest model": "Modelo mas caro",
  "If Fable had been Opus 5": "Si Fable hubiera sido Opus 5",
  "Requests above 200k context": "Requests sobre 200k de contexto",
  "Breakdown by account and model": "Detalle por cuenta y modelo",
  "write": "escritura",

  // — modelos —
  "Spend by model": "Gasto por modelo",
  "Requests by context size": "Requests por tamano de contexto",
  "above 200k": "sobre 200k",
  "{a} of {b}": "{a} de {b}",
  "requests": "requests",
  "model": "modelo",
  "If this had run on Opus 5": "Si esto hubiera corrido en Opus 5",
  "would have been saved": "se habria ahorrado",

  // — alertas / ajustes —
  "Budget": "Presupuesto",
  "Only counts spend from overage accounts — the money actually billed. Warns at 50, 75, 90, 100%.":
    "Solo cuenta el gasto de cuentas con overage — la plata que realmente se factura. Avisa al 50, 75, 90, 100%.",
  "per day": "por dia",
  "per week": "por semana",
  "per month": "por mes",
  "no limit": "sin limite",
  "Context bloat": "Contexto inflado",
  "Warns when a live session passes these sizes. This is the alert that goes after 67% of the spend: past a certain point, almost all of a turn's cost is re-reading the same thing.":
    "Avisa cuando una sesion viva pasa estos tamanos. Es la alerta que ataca el 67% del gasto: pasado cierto punto, casi todo el costo del turno es releer lo mismo.",
  "WARN": "AVISO",
  "CRITICAL": "CRITICO",
  "DON'T REPEAT WITHIN": "NO REPETIR ANTES DE",
  "Startup": "Arranque",
  "Enable": "Activar",
  "Disable": "Desactivar",
  "Starts with the system. The meter counts from the first turn of the day.":
    "Arranca con el sistema. El medidor cuenta desde el primer turno del dia.",
  "Alerts fired": "Alertas disparadas",
  "loading settings…": "cargando ajustes…",
  "budget": "presupuesto",
  "plan limit": "limite del plan",
  "expensive model": "modelo caro",
  "alert:context": "contexto",

  // — ajustes: cuentas, bloqueo, limpieza —
  "Accounts": "Cuentas",
  "{n} config dirs": "{n} config dirs",
  "Any {glob} with a {sub} folder inside is found automatically. Add the ones that live elsewhere, and hide the ones you don't want measured.":
    "Se descubren solos los {glob} que tengan un {sub} adentro. Agrega los que esten en otro lado y oculta los que no quieras medir.",
  "Show": "Mostrar",
  "Hide account": "Ocultar",
  "Remove": "Quitar",
  "Restore": "Restaurar",
  "manual": "manual",
  "{n} transcripts": "{n} transcripts",
  "Add": "Agregar",
  "remove from the list — deletes no data, can be undone":
    "sacar de la lista — no borra datos, se puede deshacer",
  "{n} config dir removed from discovery": "{n} config dir quitado del escaneo",
  "{n} config dirs removed from discovery": "{n} config dirs quitados del escaneo",
  "reading config dirs…": "leyendo config dirs…",
  "Block": "Bloqueo",
  "Active": "Activo",
  "Off": "Apagado",
  "which caps it enforces": "que techos hace cumplir",
  "daily": "diario",
  "monthly": "mensual",
  "blocks": "bloquea",
  "doesn't block": "no bloquea",
  "A cap with no amount set never blocks, ticked or not.":
    "Un techo sin monto definido no bloquea nunca, este tildado o no.",
  "Clean up subagent transcripts": "Limpiar transcripts de subagente",
  "older than {n} days": "mas de {n} dias",
  "Nothing older than {n} days.": "No hay nada mas viejo que {n} dias.",
  "{f} files · {s} older than {n} days.": "{f} archivos · {s} con mas de {n} dias.",
  "Delete": "Borrar",
  "Cancel": "Cancelar",
  "Deleted {f} files · {s} freed.": "Borrados {f} archivos · {s} liberados.",
  "Language": "Idioma",
  "overage": "overage",
  "unknown": "desconocida",
  "Subagents leave one {ext} per agent under {dir}, and they're the bulk of the disk use. Deleting them changes no numbers: the turns are already in the database. The only thing you lose is {resume} on those branches.":
    "Los subagentes dejan un {ext} por agente en {dir}, y son lo que mas ocupa. Borrarlos no cambia ningun numero: los turnos ya estan en la base. Lo unico que se pierde es poder hacer {resume} de esas ramas.",
  "Cuts the turn before it's sent once you're over. It's the only thing that stops you instead of warning you. Needs the {hook} hook wired into the account's {file}.":
    "Corta el turno antes de mandarlo cuando ya te pasaste. Es lo unico que frena en vez de avisar. Necesita el hook {hook} conectado en el {file} de la cuenta.",
  "saved": "guardado",
  "warn": "aviso",
  "critical": "critico",
  "don't repeat within {n} min": "no repetir antes de {n} min",
  "Only counts spend from overage accounts — the money actually billed. Warns at {steps}%.":
    "Solo cuenta el gasto de cuentas con overage — la plata que realmente se factura. Avisa al {steps}%.",
  "Won't start on its own. With the app closed there are no alerts — context ones only matter while the session is still open.":
    "No arranca solo. Si la app esta cerrada no hay alertas: las de contexto solo sirven mientras la sesion sigue abierta.",
  "None fired yet. Good sign — or the budget isn't set up.":
    "Todavia no se disparo ninguna. Es buena senal — o el presupuesto todavia no esta configurado.",

  // — formato —
  "just now": "recien",
  "{n} min ago": "hace {n} min",
  "{n} h ago": "hace {n} h",
  "{n} d ago": "hace {n} d",
  "no data": "sin dato",
  "resets in {n} d": "reinicia en {n} d",
  "resets in {h} h {m} min": "reinicia en {h} h {m} min",
  "resets in {m} min": "reinicia en {m} min",
  "data from {a} to {b}": "datos del {a} al {b}",
};

const DICTS: Record<Lang, Record<string, string>> = { en: {}, es: ES };

/** Rellena `{clave}` con los valores dados. */
function interpolate(text: string, vars?: Record<string, string | number>): string {
  if (!vars) return text;
  return text.replace(/\{(\w+)\}/g, (whole, key: string) =>
    key in vars ? String(vars[key]) : whole,
  );
}

export type T = (text: string, vars?: Record<string, string | number>) => string;

const Ctx = createContext<{ lang: Lang; setLang: (l: Lang) => void; t: T }>({
  lang: "en",
  setLang: () => {},
  t: (text, vars) => interpolate(text, vars),
});

export function I18nProvider({ children }: { children: ReactNode }) {
  // Arranca en ingles y corrige al leer lo guardado. La alternativa —no pintar
  // nada hasta que llegue el idioma— deja la ventana en blanco en cada
  // arranque por un ajuste que casi siempre es el default.
  const [lang, setLangState] = useState<Lang>("en");

  useEffect(() => {
    let live = true;
    void api.lang().then((l) => {
      if (live && l && l !== lang) setLangState(l as Lang);
    });
    return () => {
      live = false;
    };
    // Solo al montar: despues manda `setLang`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const setLang = (l: Lang) => {
    setLangState(l);
    void api.setLang(l);
  };

  const value = useMemo(() => {
    const dict = DICTS[lang];
    const t: T = (text, vars) => interpolate(dict[text] ?? text, vars);
    return { lang, setLang, t };
    // `setLang` es estable: solo lee `lang` a traves del setter.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lang]);

  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useI18n() {
  return useContext(Ctx);
}

/** Atajo para los componentes que solo traducen. */
export function useT(): T {
  return useContext(Ctx).t;
}
