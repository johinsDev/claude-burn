# hooks

Scripts that wire claude-burn into Claude Code. Copy them into your config dir
(e.g. `~/.claude/hooks/`) and reference them from `settings.json` — see the
main README.

| script | what it does |
|---|---|
| `burn.sh` | reads `burn-cli status`. The other three build on it. |
| `budget-guard.sh` | `UserPromptSubmit`: **cuts the turn** if you're already over a cap |
| `session-start-budget-warn.sh` | `SessionStart`: warns if you're past 80% of the day, or the month is over its cap |
| `statusline.sh` | `statusLine`: `🤖 Opus 5 \| 💰 $22.38/$33 today (68%)` |

None of them hold configuration. The caps and the block's on/off switch come
from the app (**Ajustes → Bloqueo**, **Alertas → Presupuesto**). That's
deliberate — while the block is active, the message you'd use to ask for it to
be lifted gets blocked too, so the way out can't live in the chat.

They need `jq`, and `burn-cli` on `~/.local/bin` (or set `BURN_CLI=/some/path`).
If `burn-cli` isn't there the hooks exit quietly instead of breaking the
session.
