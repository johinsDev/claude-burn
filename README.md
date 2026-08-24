# claude-burn

Mide el gasto de Claude Code: por sesión, por modelo, por proyecto, con las
cuentas separadas. Vive en la barra de menú de macOS y avisa **antes** de que
una sesión se infle, no después de que llegue la factura.

**Todo local. La app no hace ninguna petición de red** — ni a la API de
Anthropic ni a nada. Solo lee archivos que Claude Code ya escribe en disco, y
nunca toca `.credentials.json`. Esto importa porque los transcripts contienen
tu código fuente.

<!-- CAPTURAS -->

## Por qué

Medir el gasto real de dos meses de uso dio esto:

| Componente | % del gasto |
|---|---|
| `cache_read` — releer el contexto en cada turno | **67%** |
| `cache_write` | 24% |
| `output` — lo que Claude realmente escribe | 7,6% |
| `input` fresco | 0,03% |

**Dos tercios de la factura no son trabajo, son arrastrar contexto.** Las
herramientas existentes dan totales; esta muestra *en qué sesión* y *en qué
turno* se fue la plata, y te corta antes de que siga.

Tres detalles que cambian el número y que otras herramientas no ven:

- **Los subagentes se facturan aparte.** Sus turnos no están en el transcript
  de la sesión sino en `<sesión>/subagents/agent-*.jsonl`. Una herramienta que
  mira solo el nivel de arriba los ignora. En una medición real eran 304
  agentes y $385.
- **Reanudar una sesión reescribe las líneas previas** en un archivo nuevo. Sin
  deduplicar por `requestId` el gasto se cuenta varias veces — eran 33.241
  duplicados sobre 35.000 turnos.
- **Una suscripción con overage deshabilitado no te cobra por token.** Sumar
  esas cuentas al total infla la factura con plata que nadie te cobra.
  claude-burn las marca con `≈` y las excluye de los techos.

## Instalar

Requiere [Rust](https://rustup.rs), Node 20+ y pnpm.

```bash
pnpm install
pnpm --filter @claude-burn/desktop bundle
open target/release/bundle/dmg/claude-burn_0.1.0_aarch64.dmg
```

El bundle no está firmado: la primera vez, click derecho → Abrir.

El toggle de arranque automático está en **Alertas**. Sin él la app no corre, y
las alertas de contexto no sirven de nada porque solo tienen sentido mientras
la sesión sigue abierta.

## Cuentas

Se descubren solas las carpetas `~/.claude*` que tengan un `projects/` adentro.
Si tenés tus config dirs en otro lado, **Ajustes → Cuentas** deja agregarlos a
mano, ocultarlos o quitarlos del escaneo. Nada de eso borra datos.

Cada cuenta se clasifica leyendo `hasExtraUsageEnabled` de su `.claude.json`:

| | qué significa |
|---|---|
| **overage** | cada token por encima del plan se factura. Es plata real. |
| **tarifa plana** | suscripción con overage deshabilitado. El `$` es valor de API, no factura. Se muestra con `≈`. |
| **desconocida** | no hay sesión iniciada en ese config dir. No se adivina. |

**Los techos y los totales de la portada solo cuentan las cuentas con
overage.** Filtrar por una cuenta de tarifa plana no puede cambiar tu factura,
así que la app lo dice en vez de mostrar un número de otra cuenta.

## Techos

**Alertas → Presupuesto** define los techos por día, semana y mes. La semana
arranca el lunes y el mes es calendario, igual que factura Anthropic: una
ventana móvil de 30 días nunca vuelve a cero, así que no sirve como techo.

El panel **Cómo vamos** muestra los tres a la vez, más la proyección al cierre
del mes y cuánto podés gastar por día en los que quedan.

## Bloqueo

Un hook `UserPromptSubmit` que **corta el turno antes de mandarlo** cuando ya
te pasaste. Es lo único del sistema que frena en vez de avisar.

Los scripts están en [`hooks/`](hooks/). Copialos a tu config dir y conectalos:

```jsonc
// ~/.claude/settings.json
{
  "statusLine": {
    "type": "command",
    "command": "/Users/TU-USUARIO/.claude/hooks/statusline.sh",
    "refreshInterval": 30
  },
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "/Users/TU-USUARIO/.claude/hooks/budget-guard.sh", "timeout": 15 }] }
    ],
    "SessionStart": [
      { "hooks": [{ "type": "command", "command": "/Users/TU-USUARIO/.claude/hooks/session-start-budget-warn.sh", "timeout": 15 }] }
    ]
  }
}
```

Necesitan `burn-cli` en `~/.local/bin`:

```bash
cargo build --release -p burn-core --bin burn-cli
cp target/release/burn-cli ~/.local/bin/
```

Se prende y se elige qué techos hace cumplir desde **Ajustes → Bloqueo**, no
editando shell. Eso es a propósito: **cuando el bloqueo está activo, el mensaje
con el que le pedirías a Claude que lo desbloquee también queda bloqueado.**
Tiene que haber una salida fuera del chat.

No es un tope a prueba de balas: corta el *próximo* prompt una vez que detecta
que ya te pasaste. No puede devolver lo que ya se gastó.

Los hooks leen de la base con `burn-cli status` (~17 ms) en vez de llamar a
`npx ccusage` en cada prompt. Además de ser 100× más rápido, cuenta los
subagentes y respeta el techo configurado en la app.

## Las cuatro alertas

| Alerta | Cuándo |
|---|---|
| Presupuesto | El gasto **facturable** del día/semana/mes cruza 50/75/90/100% del techo |
| Contexto inflado | Una sesión **viva** pasa 250k (aviso) o 500k (crítico) de contexto |
| Límite del plan | Un límite activo de `cachedUsageUtilization` pasa 75% o 90% |
| Modelo caro | Un modelo de tarifa premium se lleva más de la mitad del día |

El cooldown va por `(tipo, clave)` y la clave incluye el escalón alcanzado:
subir de 75% a 90% notifica de nuevo, pero quedarse en 78% no repite.

"Caro" no es una lista fija de modelos: es cualquiera cuyo precio de salida
supere al de Opus 5, leído de la misma tabla de precios. Cuando salga un modelo
nuevo, la regla ya lo cubre.

## CLI

Filtros combinables con cualquier comando: `--account <nombre>` y `--days <n>`.

```bash
burn-cli report          # todo
burn-cli months          # gasto por mes y cuenta
burn-cli composition     # en qué se va la plata
burn-cli sessions 20     # sesiones más caras, con su título
burn-cli session <id>    # contexto y costo turno a turno
burn-cli agents          # cuánto se van los subagentes y quién los lanza
burn-cli context         # requests por tamaño de contexto
burn-cli plan            # límites del plan y sesiones vivas
burn-cli status          # techos y gasto en JSON, para los hooks
```

```bash
burn-cli models --account trabajo --days 7
```

`BURN_DB=/ruta/burn.sqlite` cambia la base; por defecto va al data dir del SO.

El histórico arranca donde arrancan los transcripts: Claude Code poda los
viejos, así que "todo" son las últimas semanas, no desde siempre. La app
muestra el rango exacto al lado del filtro.

## De dónde salen los datos

| Fuente | Qué aporta |
|---|---|
| `<config>/projects/**/*.jsonl` | turnos facturados: modelo, tokens, contexto, effort |
| `<config>/projects/**/ai-title` | de qué trata cada sesión |
| `<config>/.claude.json` | cuenta, tipo de facturación, `cachedUsageUtilization` (límites oficiales) |
| `<config>/sessions/<pid>.json` | sesiones corriendo ahora mismo |

Los transcripts vienen en tres formas, y perderse las dos últimas subestima el
gasto:

```
<proyecto>/<uuid>.jsonl                               sesión principal
<proyecto>/<uuid>/subagents/agent-<id>.jsonl          subagente
<proyecto>/<uuid>/subagents/workflows/wf_*/*.jsonl    agente de workflow
```

## Motor de costo

```
costo = in·base + w5m·base·1,25 + w1h·base·2 + read·base·0,1 + out·salida
```

Más los multiplicadores: `speed: "fast"` cobra tarifa premium donde exista,
`inference_geo: "us"` es ×1,1, `service_tier: "batch"` es ×0,5, y las búsquedas
web van a $10 por mil.

`packages/pricing/pricing.json` es la fuente única, consumida por TypeScript y
embebida en el binario de Rust con `include_str!`. **Un modelo que no esté en
la tabla cuenta 0 y se marca en la UI** — nunca se adivina un precio.

## Cómo se mantiene al día

Un watcher (`notify`) vigila `projects/` en recursivo, `sessions/` y
`.claude.json` de cada cuenta. Debounce de 900 ms — Claude escribe muchas
líneas seguidas y sincronizar en cada una lee JSON a medio escribir. El offset
solo avanza sobre líneas terminadas en `\n`, así que una línea a medio escribir
espera a la próxima pasada en vez de perderse.

Cada 90 s revisa igual aunque nada se mueva, porque los límites del plan se
refrescan en `.claude.json` sin tocar ningún transcript.

Ingesta en frío: **1,1 GB en 435 archivos, ~1 s**. Incremental: ~10 ms.

## Limpieza

**Ajustes → Limpiar transcripts de subagente** borra por antigüedad. Es seguro
para los números: los turnos ya están deduplicados en SQLite y todas las
consultas salen de ahí. Lo único que se pierde es poder hacer `--resume` de
esas ramas.

## Arquitectura

```
crates/burn-core/     ingesta · dedup · precios · SQLite · alertas   (sin Tauri)
  └── bin/burn-cli    el mismo motor por línea de comandos
apps/desktop/
  ├── src-tauri/      tray, ventana, watcher, notificaciones
  └── src/            React 19 + Tailwind v4 + Recharts
packages/pricing/     pricing.json — fuente única de precios
hooks/                los scripts del bloqueo y el statusline
```

`burn-core` no depende de Tauri a propósito: se compila como CLI, que es como
se verifican los números sin abrir la GUI.

## Desarrollo

```bash
pnpm dev                                  # Tauri en modo dev
pnpm turbo lint typecheck build           # frontend
cargo test && cargo clippy -- -D warnings # Rust
```

## Licencia

MIT
