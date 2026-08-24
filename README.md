# claude-burn

Mide el gasto diario de Claude Code: por sesion, por modelo, por proyecto, con
las cuentas separadas. Todo local — no hace ninguna peticion de red.

## Por que

El 67% del gasto no es trabajo, es arrastrar contexto: `cache_read`, o sea
releer el mismo prompt gigante en cada turno. Las herramientas existentes dan
totales; esta muestra *en que sesion* y *en que turno* se fue la plata.

## Estado

- [x] **M1** — `burn-core` + CLI: ingesta incremental, deduplicacion, motor de costo, SQLite
- [ ] **M2** — Shell Tauri: tray icon, popover, Overview
- [ ] **M3** — Drill-down: tabla de sesiones, detalle de sesion, modelos
- [ ] **M4** — Daemon: watcher, sesiones vivas, limites del plan, alertas
- [ ] **M5** — Empaquetado: ajustes, presupuestos, autostart, `.dmg`

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
