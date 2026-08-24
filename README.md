# claude-burn

Track what Claude Code actually costs you — per session, per model, per
project, with each account kept separate. It lives in the macOS menu bar, warns
you **before** a session balloons, and can block the next prompt once you've
blown your budget.

**Fully local. The app makes zero network requests** — not to Anthropic's API,
not to anything. It only reads files Claude Code already writes to disk, and it
never touches `.credentials.json`. That matters, because your transcripts
contain your source code.

<!-- SCREENSHOTS -->

## Why

Measuring two months of real usage produced this breakdown:

| Component | Share of spend |
|---|---|
| `cache_read` — re-reading context on every turn | **67%** |
| `cache_write` | 24% |
| `output` — what Claude actually writes | 7.6% |
| fresh `input` | 0.03% |

**Two thirds of the bill isn't work — it's dragging context around.** Existing
tools give you totals. This one shows you *which session* and *which turn* the
money went to, and stops you before it keeps going.

Three things that change the number, which other tools miss:

- **Subagents are billed separately.** Their turns don't live in the session
  transcript but in `<session>/subagents/agent-*.jsonl`. Anything that only
  looks at the top level ignores them. In one real measurement that was 304
  agents and $385.
- **Resuming a session replays earlier lines** into a new file. Without
  deduplicating on `requestId` the spend is counted several times — 33,241
  duplicates across 35,000 turns.
- **A subscription with overage disabled doesn't bill per token.** Adding those
  accounts to the total inflates your bill with money nobody charges you.
  claude-burn marks them with `≈` and excludes them from every budget.

## Install

Requires [Rust](https://rustup.rs), Node 20+ and pnpm. macOS only for now
(the menu bar and notification bits are AppKit-specific; the core is portable).

```bash
pnpm install
pnpm --filter @claude-burn/desktop bundle
open target/release/bundle/dmg/claude-burn_0.1.0_aarch64.dmg
```

The bundle isn't signed, so the first launch needs right click → Open.

The autostart toggle lives under **Alertas**. Without it the app isn't running,
and context alerts are worthless — they only mean anything while the session is
still open.

## Try it without your own data

```bash
pnpm --filter @claude-burn/desktop demo
```

Seeds a database of invented accounts, projects and sessions, then opens the
app against it. A demo database is flagged in `settings` and is never synced,
so it can't get contaminated with your real transcripts.

## Accounts

Any `~/.claude*` directory with a `projects/` folder inside is discovered
automatically. If your config dirs live somewhere else, **Ajustes → Cuentas**
lets you add, hide or remove them. None of that deletes data.

Each account is classified by reading `hasExtraUsageEnabled` from its
`.claude.json`:

| | what it means |
|---|---|
| **overage** | every token past the plan is billed. Real money. |
| **flat rate** | subscription with overage disabled. The `$` is API value, not an invoice. Shown with `≈`. |
| **unknown** | no session signed in for that config dir. Nothing is guessed. |

**Budgets and the headline totals only count overage accounts.** Filtering by a
flat-rate account can't change your bill, so the app says so instead of showing
you a number from a different account.

## Budgets

**Alertas → Presupuesto** sets daily, weekly and monthly caps. The week starts
on Monday and the month is a calendar month, the same way Anthropic bills: a
rolling 30-day window never resets, so it can't work as a cap.

The **Cómo vamos** panel shows all three at once, plus the projected month-end
total and how much you can still spend per day for the days that remain.

## The block

A `UserPromptSubmit` hook that **cuts the turn before it's sent** once you're
over. It's the only part of the system that stops you instead of telling you.

The scripts live in [`hooks/`](hooks/). Copy them into your config dir and wire
them up:

```jsonc
// ~/.claude/settings.json
{
  "statusLine": {
    "type": "command",
    "command": "/Users/YOU/.claude/hooks/statusline.sh",
    "refreshInterval": 30
  },
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "/Users/YOU/.claude/hooks/budget-guard.sh", "timeout": 15 }] }
    ],
    "SessionStart": [
      { "hooks": [{ "type": "command", "command": "/Users/YOU/.claude/hooks/session-start-budget-warn.sh", "timeout": 15 }] }
    ]
  }
}
```

They need `burn-cli` on disk:

```bash
cargo build --release -p burn-core --bin burn-cli
cp target/release/burn-cli ~/.local/bin/
```

You turn it on and pick which caps it enforces from **Ajustes → Bloqueo**, not
by editing shell. That's deliberate: **while the block is active, the message
you'd use to ask Claude to unblock it gets blocked too.** The way out can't be
inside the chat.

It is not a hard cap. It stops the *next* prompt once it notices you're over —
it can't claw back what's already spent.

The hooks read from the database via `burn-cli status` (~17 ms) instead of
shelling out to `npx ccusage` on every prompt. Beyond being 100× faster, it
counts subagents and respects the cap configured in the app.

## The four alerts

| Alert | Fires when |
|---|---|
| Budget | **Billable** spend for the day/week/month crosses 50/75/90/100% of the cap |
| Context bloat | A **live** session passes 250k (warn) or 500k (critical) tokens of context |
| Plan limit | An active limit from `cachedUsageUtilization` passes 75% or 90% |
| Expensive model | A premium-rate model takes more than half the day's spend |

Cooldown is keyed on `(kind, key)`, and the key includes the step reached:
going from 75% to 90% notifies again, sitting at 78% doesn't repeat.

"Expensive" is not a hardcoded model list — it's any model whose output price
exceeds Opus 5's, read from the same pricing table. When a new model ships, the
rule already covers it.

## CLI

Filters compose with any command: `--account <name>` and `--days <n>`.

```bash
burn-cli report          # everything
burn-cli months          # spend by month and account
burn-cli composition     # where the money goes
burn-cli sessions 20     # priciest sessions, with their titles
burn-cli session <id>    # context and cost, turn by turn
burn-cli agents          # what subagents cost and who spawns them
burn-cli context         # requests bucketed by context size
burn-cli plan            # plan limits and live sessions
burn-cli status          # caps and spend as JSON, for the hooks
burn-cli demo            # seed a database of invented data
```

```bash
burn-cli models --account work --days 7
```

`BURN_DB=/path/burn.sqlite` points at a different database; the default lives
in the OS data dir.

History starts where the transcripts start: Claude Code prunes old ones, so
"all time" means the last few weeks, not forever. The app shows the exact range
next to the filter.

## Architecture

```
crates/burn-core/          ingest · dedup · pricing · SQLite · alert rules
  ├── ingest.rs            incremental JSONL reader, offset per file
  ├── record.rs            transcript line shapes (serde)
  ├── pricing.rs           cost engine, pricing.json embedded via include_str!
  ├── store.rs             schema and every query
  ├── profiles.rs          account discovery, plan limits, live sessions
  ├── alerts.rs            pure rules, no I/O
  ├── demo.rs              invented dataset
  └── bin/burn-cli.rs      the same engine on the command line

apps/desktop/
  ├── src-tauri/
  │   ├── commands.rs      the bridge the frontend calls
  │   ├── watcher.rs       notify-based file watching, 900 ms debounce
  │   ├── tray.rs          menu bar icon and popover positioning
  │   ├── alerts.rs        rules → native notifications, with cooldown
  │   └── state.rs         shared state, tray summary
  └── src/                 React: overview, sessions, models, alerts, settings

packages/pricing/          pricing.json — single source of prices
hooks/                     the block and statusline scripts
```

**`burn-core` deliberately does not depend on Tauri.** It compiles as a CLI,
which is how the numbers get verified without opening the GUI — and it's what
makes the whole cost engine testable with 35 unit tests and no windowing.

### Data sources

| Source | What it gives |
|---|---|
| `<config>/projects/**/*.jsonl` | billed turns: model, tokens, context, effort |
| `ai-title` lines | what each session is about |
| `<config>/.claude.json` | account, billing type, `cachedUsageUtilization` (official limits) |
| `<config>/sessions/<pid>.json` | sessions running right now |

Transcripts come in three shapes, and missing the last two underestimates your
spend:

```
<project>/<uuid>.jsonl                                main session
<project>/<uuid>/subagents/agent-<id>.jsonl           subagent
<project>/<uuid>/subagents/workflows/wf_*/*.jsonl     workflow agent
```

### Cost engine

```
cost = in·base + w5m·base·1.25 + w1h·base·2 + read·base·0.1 + out·output_rate
```

Plus the multipliers: `speed: "fast"` bills the premium rate where one exists,
`inference_geo: "us"` is ×1.1, `service_tier: "batch"` is ×0.5, and web
searches are $10 per thousand.

`packages/pricing/pricing.json` is the single source, consumed by TypeScript
and embedded in the Rust binary with `include_str!`. **A model that isn't in
the table costs 0 and is flagged in the UI** — a price is never guessed.

### Staying current

A `notify` watcher tracks `projects/` recursively, plus `sessions/` and
`.claude.json` for each account. 900 ms debounce — Claude writes many lines in
a burst, and syncing on each one reads half-written JSON. The offset only
advances past lines ending in `\n`, so a line being written right now waits for
the next pass instead of getting mangled.

Every 90 s it checks anyway, because plan limits refresh inside `.claude.json`
without touching any transcript.

Cold ingest: **1.1 GB across 435 files in ~1 s**. Incremental: ~10 ms.

Deduplication is a `UNIQUE` constraint on `request_id` with `INSERT OR IGNORE`,
which also encodes the attribution rule: a repeated request belongs to the
first session that produced it.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Monorepo | Turborepo + pnpm | shared pricing package between Rust and TS |
| Shell | Tauri v2 | native tray, ~10 MB bundle, Rust backend |
| Frontend | React 19 · Vite 7 · Tailwind v4 · Recharts 3 | |
| Backend | Rust — `rusqlite` (bundled), `notify`, `memchr`, `walkdir`, `chrono` | `memchr` pre-filters lines before `serde_json` touches them |
| Plugins | `notification`, `autostart`, `single-instance`, `opener` | |
| Lint | oxlint · clippy · rustfmt | |

## Cleanup

**Ajustes → Limpiar transcripts de subagente** deletes by age. It's safe for
the numbers: turns are already deduplicated in SQLite and every query reads
from there. The only thing you lose is `--resume` on those branches.

## Development

```bash
pnpm dev                                  # Tauri in dev mode
pnpm turbo lint typecheck build           # frontend
cargo test && cargo clippy -- -D warnings # Rust
```

Note: code comments and the UI are in Spanish.

## License

MIT
