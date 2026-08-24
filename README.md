# claude-burn

Mide el gasto diario de Claude Code: por sesion, por modelo, por proyecto, con
las cuentas separadas. Todo local — no hace ninguna peticion de red.

## Por que

El 67% del gasto no es trabajo, es arrastrar contexto: `cache_read`, o sea
releer el mismo prompt gigante en cada turno. Las herramientas existentes dan
totales; esta muestra *en que sesion* y *en que turno* se fue la plata.

## Estado

- [x] **M1** — `burn-core` + CLI: ingesta incremental, deduplicacion, motor de costo, SQLite
- [x] **M2** — Shell Tauri: tray icon, popover, Resumen
- [x] **M3** — Drill-down: tabla de sesiones, detalle de sesion, modelos
- [x] **M4** — Daemon: watcher, sesiones vivas, limites del plan, las 4 alertas
- [ ] **M5** — Empaquetado: autostart, firma, `.dmg`

## Las cuatro alertas

| Alerta | Cuando |
|---|---|
| Presupuesto | El gasto **facturable** del dia/semana/mes cruza 50/75/90/100% del techo |
| Contexto inflado | Una sesion **viva** pasa 250k (aviso) o 500k (critico) de contexto |
| Limite del plan | Un limite activo de `cachedUsageUtilization` pasa 75% o 90% |
| Modelo caro | Un modelo de tarifa premium se lleva mas de la mitad del dia |

El cooldown va por `(tipo, clave)` y la clave incluye el escalon alcanzado:
subir de 75% a 90% notifica de nuevo, pero quedarse en 78% no repite.

"Caro" no es una lista fija de modelos: es cualquiera cuyo precio de salida
supere al de Opus 5, leido de la misma tabla de precios. Cuando salga un modelo
nuevo, la regla ya lo cubre.

## CLI

```
cargo run --release --bin burn-cli -- report        # todo
cargo run --release --bin burn-cli -- months        # gasto por mes y cuenta
cargo run --release --bin burn-cli -- composition   # en que se va la plata
cargo run --release --bin burn-cli -- sessions 20   # sesiones mas caras
cargo run --release --bin burn-cli -- session <id>  # contexto y costo turno a turno
cargo run --release --bin burn-cli -- context       # requests por tamano de contexto
cargo run --release --bin burn-cli -- plan          # limites del plan y sesiones vivas
```

`BURN_DB=/ruta/burn.sqlite` cambia la base; por defecto va al data dir del SO.

## Como se mantiene al dia

Un watcher (`notify`) vigila `projects/` en recursivo, `sessions/` y
`.claude.json` de cada cuenta. Debounce de 900 ms — Claude escribe muchas
lineas seguidas y sincronizar en cada una lee JSON a medio escribir. Cada
90 s revisa igual aunque nada se mueva, porque los limites del plan se
refrescan en `.claude.json` sin tocar ningun transcript.

## De donde salen los datos

| Fuente | Que aporta |
|---|---|
| `<config>/projects/**/*.jsonl` | turnos facturados: modelo, tokens, contexto, effort |
| `<config>/.claude.json` | cuenta, tipo de facturacion, `cachedUsageUtilization` (limites oficiales) |
| `<config>/sessions/<pid>.json` | sesiones corriendo ahora mismo |

Los transcripts vienen en tres formas, y perderse las dos ultimas subestima el gasto:

```
<proyecto>/<uuid>.jsonl                               sesion principal
<proyecto>/<uuid>/subagents/agent-<id>.jsonl          subagente
<proyecto>/<uuid>/subagents/workflows/wf_*/*.jsonl    agente de workflow
```

## Precios

`packages/pricing/pricing.json` es la fuente unica, consumida por TypeScript y
embebida en el binario de Rust. Un modelo que no este en la tabla cuenta 0 y se
marca en la UI — nunca se adivina un precio.
