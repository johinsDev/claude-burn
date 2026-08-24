import { useEffect, useMemo } from "react";
import {
  Area,
  CartesianGrid,
  ComposedChart,
  Line,
  ReferenceLine,
  Tooltip,
  XAxis,
  YAxis,
} from "recharts";
import { api, type SessionRow, type TurnPoint } from "@/lib/api";
import { useAsyncData } from "@/hooks/use-async-data";
import { Badge, Button, Empty, Stat } from "@/components/ui/primitives";
import { ChartFrame, fmt, tooltipStyle } from "@/components/ui/chart-frame";
import { contextTone, money, projectName, tokens, toneClass } from "@/lib/format";
import { cn } from "@/lib/utils";

/**
 * El grafico que contesta la pregunta original: donde se dio garra esta sesion.
 *
 * Contexto y costo comparten el eje X (numero de turno). Cuando la linea de
 * contexto sube y se queda arriba, cada turno siguiente cuesta mas sin hacer
 * mas trabajo — eso es el 67% de la factura.
 */
export function SessionDetail({
  session,
  onClose,
}: {
  session: SessionRow;
  onClose: () => void;
}) {
  const { data: points, loading } = useAsyncData(
    () => api.sessionTimeline(session.session_id),
    [session.session_id],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const data = useMemo(
    () =>
      (points ?? []).map((p, i) => ({
        turn: i + 1,
        ctx: p.ctx_tok,
        cost: p.cost_usd,
        model: p.model,
        ts: p.ts,
        effort: p.effort,
      })),
    [points],
  );

  /**
   * De donde sale el $ por turno: cuanto se fue en releer y cuanto en escribir.
   *
   * Es la respuesta a "que es este numero". En una sesion inflada el turno
   * promedio lee cientos de miles de tokens para escribir unos cientos, y la
   * factura sigue esa proporcion.
   */
  const perTurn = useMemo(() => {
    if (!points?.length) return null;
    const n = points.length;
    const sum = (f: (p: TurnPoint) => number) => points.reduce((a, p) => a + f(p), 0) / n;
    const read = sum((p) => p.read_tok);
    const out = sum((p) => p.out_tok);
    return {
      readTok: read,
      outTok: out,
      readCost: sum((p) => p.cost_read),
      outCost: sum((p) => p.cost_output),
      ratio: out > 0 ? read / out : 0,
      agents: new Set(points.filter((p) => p.agent_id).map((p) => p.agent_id)).size,
      agentTurns: points.filter((p) => p.agent_id).length,
    };
  }, [points]);

  // Cuanto habria costado la sesion si el contexto no hubiera crecido mas alla
  // del umbral de aviso. Es una cota gruesa, pero ordena la magnitud.
  const wasted = useMemo(() => {
    if (!points?.length) return 0;
    const CAP = 250_000;
    return points.reduce((acc, p) => {
      if (p.ctx_tok <= CAP) return acc;
      return acc + p.cost_usd * (1 - CAP / p.ctx_tok);
    }, 0);
  }, [points]);

  const tone = contextTone(session.max_ctx);

  return (
    <div
      className="fixed inset-0 z-50 flex items-end justify-center bg-black/55 backdrop-blur-[2px]"
      onClick={onClose}
    >
      <div
        className="h-[86vh] w-full max-w-5xl rounded-t-xl border border-line bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <header className="flex items-start justify-between border-b border-line px-5 py-3.5">
          <div className="min-w-0">
            <h2 className="truncate text-sm font-semibold">
              {projectName(session.project)}
            </h2>
            <p className="num mt-0.5 truncate text-[10.5px] text-ink-faint">
              {session.account} · {session.session_id}
            </p>
          </div>
          <Button onClick={onClose}>Cerrar · esc</Button>
        </header>

        <div className="grid grid-cols-5 divide-x divide-line border-b border-line">
          <Stat label="Costo" value={money(session.cost_usd)} />
          <Stat label="$ por turno" value={money(session.cost_per_turn)} />
          <Stat label="Turnos" value={session.turns} />
          <Stat
            label="Contexto max"
            value={tokens(session.max_ctx)}
            tone={toneClass[tone]}
            sub={`promedio ${tokens(session.avg_ctx)}`}
          />
          <Stat
            label="Compactaciones"
            value={session.compactions}
            tone={session.compactions === 0 ? "text-crit" : undefined}
            sub={session.compactions === 0 ? "nunca se limpio" : undefined}
          />
        </div>

        {perTurn ? (
          <div className="flex flex-wrap items-center gap-x-5 gap-y-1 border-b border-line px-5 py-2.5 text-[11px]">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-ink-faint">
              turno promedio
            </span>
            <span className="text-ink-dim">
              lee{" "}
              <span className="num font-semibold text-crit">{tokens(perTurn.readTok)}</span>{" "}
              → {money(perTurn.readCost)}
            </span>
            <span className="text-ink-dim">
              escribe{" "}
              <span className="num font-semibold text-ok">{tokens(perTurn.outTok)}</span> →{" "}
              {money(perTurn.outCost)}
            </span>
            {perTurn.ratio > 1 ? (
              <span className={cn(perTurn.ratio > 100 ? toneClass.crit : "text-ink-dim")}>
                lee {Math.round(perTurn.ratio)}× lo que escribe
              </span>
            ) : null}
            {perTurn.agents > 0 ? (
              <span className="ml-auto text-ink-faint">
                {perTurn.agents} subagentes · {perTurn.agentTurns} de {points?.length} turnos
              </span>
            ) : null}
          </div>
        ) : null}

        {loading ? (
          <Empty>cargando turnos…</Empty>
        ) : !points || points.length === 0 ? (
          <Empty>sin turnos registrados</Empty>
        ) : (
          <>
            <div className="px-2 pt-3">
              <ChartFrame height={300}>
                <ComposedChart data={data} margin={{ top: 8, right: 14, bottom: 4, left: -6 }}>
                  <CartesianGrid stroke="var(--color-line)" vertical={false} />
                  <XAxis
                    dataKey="turn"
                    stroke="var(--color-ink-faint)"
                    fontSize={10}
                    tickLine={false}
                    axisLine={false}
                    minTickGap={30}
                  />
                  <YAxis
                    yAxisId="ctx"
                    stroke="var(--color-ink-faint)"
                    fontSize={10}
                    tickLine={false}
                    axisLine={false}
                    tickFormatter={(v: number) => tokens(v)}
                  />
                  <YAxis
                    yAxisId="cost"
                    orientation="right"
                    stroke="var(--color-ink-faint)"
                    fontSize={10}
                    tickLine={false}
                    axisLine={false}
                    tickFormatter={(v: number) => `$${v.toFixed(2)}`}
                  />
                  {/* Los umbrales que disparan la alerta de contexto inflado. */}
                  <ReferenceLine
                    yAxisId="ctx"
                    y={250_000}
                    stroke="var(--color-hot)"
                    strokeDasharray="3 4"
                    strokeOpacity={0.6}
                    label={{ value: "250k", fill: "var(--color-hot)", fontSize: 9, position: "right" }}
                  />
                  <ReferenceLine
                    yAxisId="ctx"
                    y={500_000}
                    stroke="var(--color-crit)"
                    strokeDasharray="3 4"
                    strokeOpacity={0.6}
                    label={{ value: "500k", fill: "var(--color-crit)", fontSize: 9, position: "right" }}
                  />
                  <Tooltip
                    {...tooltipStyle}
                    labelFormatter={(v) => `turno ${v}`}
                    formatter={fmt((v, name) =>
                      name === "ctx" ? [tokens(v), "contexto"] : [money(v), "costo"],
                    )}
                  />
                  <Area
                    yAxisId="ctx"
                    type="monotone"
                    dataKey="ctx"
                    stroke="var(--color-cool)"
                    fill="var(--color-cool)"
                    fillOpacity={0.14}
                    strokeWidth={1.5}
                    dot={false}
                  />
                  <Line
                    yAxisId="cost"
                    type="monotone"
                    dataKey="cost"
                    stroke="var(--color-hot)"
                    strokeWidth={1.2}
                    dot={false}
                  />
                </ComposedChart>
              </ChartFrame>
            </div>

            <div className="flex items-center gap-4 px-5 pb-4 text-[11px]">
              <span className="flex items-center gap-1.5 text-ink-dim">
                <i className="h-0.5 w-4 rounded bg-cool" /> contexto
              </span>
              <span className="flex items-center gap-1.5 text-ink-dim">
                <i className="h-0.5 w-4 rounded bg-hot" /> costo del turno
              </span>
              {wasted > 0.5 ? (
                <span className={cn("ml-auto", toneClass.crit)}>
                  ~{money(wasted)} atribuible a contexto por encima de 250k
                </span>
              ) : null}
            </div>

            <div className="flex flex-wrap gap-1.5 border-t border-line px-5 py-3">
              {[...new Set(data.map((d) => d.model))]
                .filter((m) => m !== "<synthetic>")
                .map((m) => (
                  <Badge key={m} tone={m.includes("fable") ? "crit" : "neutral"}>
                    {m.replace("claude-", "")}
                  </Badge>
                ))}
              {[...new Set(data.map((d) => d.effort).filter(Boolean))].map((e) => (
                <Badge key={e} tone={e === "high" ? "warn" : "neutral"}>
                  effort {e}
                </Badge>
              ))}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
