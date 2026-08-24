import raw from "./pricing.json" with { type: "json" };

export type ModelRate = {
  id: string;
  label: string;
  input: number;
  output: number;
  fast?: { input: number; output: number };
};

export type PricingTable = {
  updated: string;
  source: string;
  multipliers: {
    cache_write_5m: number;
    cache_write_1h: number;
    cache_read: number;
    inference_geo_us: number;
    batch: number;
  };
  web_search_per_1k_requests: number;
  models: ModelRate[];
};

export const pricing = raw as unknown as PricingTable;

const byId = new Map(pricing.models.map((m) => [m.id, m]));

/**
 * Normaliza un model id de un transcript al id canonico de la tabla.
 * Quita el sufijo de ventana `[1m]` y cualquier sufijo de fecha (`-20251114`).
 * Devuelve null si el modelo no esta en la tabla — el caller debe mostrar
 * un badge de "modelo desconocido", nunca inventar un precio.
 */
export function normalizeModelId(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const stripped = raw.replace(/\[1m\]$/, "");
  if (byId.has(stripped)) return stripped;
  const undated = stripped.replace(/-\d{8}$/, "");
  if (byId.has(undated)) return undated;
  // prefijo mas largo que coincida (cubre variantes no vistas)
  let best: string | null = null;
  for (const id of byId.keys()) {
    if (undated.startsWith(id) && (best === null || id.length > best.length)) best = id;
  }
  return best;
}

export function rateFor(modelId: string): ModelRate | undefined {
  return byId.get(modelId);
}

export function labelFor(raw: string | null | undefined): string {
  const id = normalizeModelId(raw);
  return id ? (byId.get(id)?.label ?? id) : (raw ?? "desconocido");
}
