# hooks

Scripts para conectar claude-burn a Claude Code. Copialos a tu config dir
(por ejemplo `~/.claude/hooks/`) y conectalos en `settings.json` — ver el
README principal.

| script | qué hace |
|---|---|
| `burn.sh` | lee `burn-cli status`. Los otros tres se apoyan en este. |
| `budget-guard.sh` | `UserPromptSubmit`: **corta el turno** si ya pasaste un techo |
| `session-start-budget-warn.sh` | `SessionStart`: avisa si vas >80% del día o el mes está pasado |
| `statusline.sh` | `statusLine`: `🤖 Opus 5 \| 💰 $22.38/$33 hoy (68%)` |

Ninguno tiene configuración adentro: los techos y el interruptor del bloqueo
salen de la app (**Ajustes → Bloqueo**, **Alertas → Presupuesto**). Es a
propósito — cuando el bloqueo está activo, el mensaje con el que pedirías
desbloquearlo también queda bloqueado, así que la salida no puede estar en el
chat.

Requieren `jq` y `burn-cli` en `~/.local/bin` (o `BURN_CLI=/otra/ruta`).
Si `burn-cli` no está, los hooks salen sin hacer nada en vez de romper la
sesión.
