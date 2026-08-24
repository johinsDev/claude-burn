//! Persistencia en SQLite.
//!
//! La regla que sostiene todos los numeros: `turns.request_id` es PRIMARY KEY.
//! Reanudar una sesion copia las lineas previas a un archivo nuevo — en el
//! escaneo completo aparecieron 33.241 requestId duplicados. Sin esta clave el
//! gasto se infla varias veces.

use crate::record::Turn;
use anyhow::Result;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Recorte que aplican todas las consultas de agregacion.
///
/// `since` se compara lexicograficamente contra `ts`: los transcripts guardan
/// la hora en ISO-8601 UTC, donde el orden alfabetico es el cronologico.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Filter {
    pub account: Option<String>,
    pub since: Option<String>,
}

/// Predicado comun. Los parametros van siempre en el mismo orden — `?1`
/// cuenta, `?2` desde — para que ninguna consulta los cruce.
const SCOPE: &str = "(?1 IS NULL OR account = ?1) AND (?2 IS NULL OR ts >= ?2)";

pub struct Store {
    conn: Connection,
}

/// Posicion de lectura de un transcript, tal como se persiste en `files`.
#[derive(Debug, Clone, Copy)]
pub struct FileCursor<'a> {
    pub path: &'a str,
    pub account: &'a str,
    pub project: &'a str,
    pub session_id: &'a str,
    pub size: u64,
    pub mtime_ms: i64,
    pub offset: u64,
    /// Compactaciones vistas en esta pasada, que se suman a las ya conocidas.
    pub compactions_delta: u32,
    /// `true` cuando esta pasada ya cubrio el archivo entero buscando titulo.
    pub meta_scanned: bool,
}

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;

CREATE TABLE IF NOT EXISTS files (
    path        TEXT PRIMARY KEY,
    account     TEXT NOT NULL,
    project     TEXT NOT NULL,
    session_id  TEXT NOT NULL,
    size        INTEGER NOT NULL DEFAULT 0,
    mtime_ms    INTEGER NOT NULL DEFAULT 0,
    offset      INTEGER NOT NULL DEFAULT 0,
    compactions INTEGER NOT NULL DEFAULT 0
);

-- Titulo y primer prompt de cada sesion. Va aparte de `turns` porque una
-- sesion vive en varios archivos cuando se retoma, y el titulo es de la
-- sesion, no del archivo.
CREATE TABLE IF NOT EXISTS session_meta (
    session_id TEXT PRIMARY KEY,
    title      TEXT,
    prompt     TEXT
);

CREATE TABLE IF NOT EXISTS turns (
    request_id     TEXT PRIMARY KEY,
    session_id     TEXT NOT NULL,
    account        TEXT NOT NULL,
    project        TEXT NOT NULL,
    cwd            TEXT,
    git_branch     TEXT,
    ts             TEXT NOT NULL,
    day            TEXT NOT NULL,
    month          TEXT NOT NULL,
    model          TEXT,
    raw_model      TEXT NOT NULL,
    effort         TEXT,
    in_tok         INTEGER NOT NULL,
    w5m_tok        INTEGER NOT NULL,
    w1h_tok        INTEGER NOT NULL,
    read_tok       INTEGER NOT NULL,
    out_tok        INTEGER NOT NULL,
    thinking_tok   INTEGER NOT NULL,
    web_searches   INTEGER NOT NULL,
    ctx_tok        INTEGER NOT NULL,
    is_sidechain   INTEGER NOT NULL,
    agent_id       TEXT,
    cost_input     REAL NOT NULL,
    cost_w5m       REAL NOT NULL,
    cost_w1h       REAL NOT NULL,
    cost_read      REAL NOT NULL,
    cost_output    REAL NOT NULL,
    cost_websearch REAL NOT NULL,
    cost_usd       REAL NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_turns_day     ON turns(day);
CREATE INDEX IF NOT EXISTS idx_turns_month   ON turns(month);
CREATE INDEX IF NOT EXISTS idx_turns_session ON turns(session_id, ts);
CREATE INDEX IF NOT EXISTS idx_turns_acct    ON turns(account, ts);
CREATE INDEX IF NOT EXISTS idx_turns_model   ON turns(model);

CREATE TABLE IF NOT EXISTS budgets (
    scope     TEXT NOT NULL,
    period    TEXT NOT NULL,
    limit_usd REAL NOT NULL,
    PRIMARY KEY (scope, period)
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS alerts (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    kind         TEXT NOT NULL,
    key          TEXT NOT NULL,
    fired_at_ms  INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    dismissed    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_alerts_key ON alerts(kind, key, fired_at_ms);
"#;

/// Marca de "ya le busque titulo a este archivo". Las bases creadas antes de
/// que existiera `session_meta` tienen los offsets al final, asi que el barrido
/// incremental nunca volveria a ver sus lineas `ai-title`: necesitan una pasada
/// completa, pero una sola vez.
const MIGRATIONS: &[&str] =
    &["ALTER TABLE files ADD COLUMN meta_scanned INTEGER NOT NULL DEFAULT 0"];

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        Self::prepare(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::prepare(Connection::open_in_memory()?)
    }

    fn prepare(conn: Connection) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        for m in MIGRATIONS {
            // Una columna que ya existe da error; es el caso normal tras la
            // primera vez, no una falla.
            let _ = conn.execute(m, []);
        }
        Ok(Self { conn })
    }

    /// Archivos a los que todavia no se les busco titulo. Requieren una lectura
    /// completa porque su offset ya esta al final.
    pub fn needs_meta_scan(&self, path: &str) -> Result<bool> {
        let scanned: Option<i64> = self
            .conn
            .query_row(
                "SELECT meta_scanned FROM files WHERE path = ?1",
                params![path],
                |r| r.get(0),
            )
            .optional()?;
        Ok(scanned.unwrap_or(0) == 0)
    }

    /// Guarda titulo y primer prompt. El titulo se pisa (Claude re-titula), el
    /// prompt no: interesa el que abrio la sesion.
    pub fn save_session_meta(
        &self,
        session_id: &str,
        title: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<()> {
        if title.is_none() && prompt.is_none() {
            return Ok(());
        }
        self.conn.execute(
            "INSERT INTO session_meta (session_id, title, prompt) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET
                title  = COALESCE(?2, session_meta.title),
                prompt = COALESCE(session_meta.prompt, ?3)",
            params![session_id, title, prompt],
        )?;
        Ok(())
    }

    pub fn cursor_for(&self, path: &str) -> Result<(u64, u64)> {
        let row: Option<(i64, i64)> = self
            .conn
            .query_row(
                "SELECT offset, size FROM files WHERE path = ?1",
                params![path],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(row.map_or((0, 0), |(o, s)| (o as u64, s as u64)))
    }

    pub fn save_cursor(&self, c: &FileCursor<'_>) -> Result<()> {
        let FileCursor {
            path,
            account,
            project,
            session_id,
            size,
            mtime_ms,
            offset,
            compactions_delta,
            meta_scanned,
        } = *c;
        self.conn.execute(
            "INSERT INTO files (path, account, project, session_id, size, mtime_ms, offset, compactions, meta_scanned)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
                size = ?5, mtime_ms = ?6, offset = ?7,
                compactions = CASE WHEN ?7 <= files.offset THEN ?8 ELSE files.compactions + ?8 END,
                meta_scanned = MAX(files.meta_scanned, ?9)",
            params![
                path,
                account,
                project,
                session_id,
                size as i64,
                mtime_ms,
                offset as i64,
                compactions_delta,
                i64::from(meta_scanned)
            ],
        )?;
        Ok(())
    }

    /// Inserta turnos deduplicando por `request_id`. Devuelve cuantos eran nuevos.
    ///
    /// `INSERT OR IGNORE` implementa la regla de atribucion: un requestId
    /// repetido se queda con la primera sesion que lo produjo.
    pub fn insert_turns(&mut self, turns: &[Turn]) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut inserted = 0;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO turns (
                    request_id, session_id, account, project, cwd, git_branch, ts, day, month,
                    model, raw_model, effort, in_tok, w5m_tok, w1h_tok, read_tok, out_tok,
                    thinking_tok, web_searches, ctx_tok, is_sidechain, agent_id,
                    cost_input, cost_w5m, cost_w1h, cost_read, cost_output, cost_websearch, cost_usd
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29)",
            )?;
            for t in turns {
                let day = t.ts.get(..10).unwrap_or_default();
                let month = t.ts.get(..7).unwrap_or_default();
                inserted += stmt.execute(params![
                    t.request_id,
                    t.session_id,
                    t.account,
                    t.project,
                    t.cwd,
                    t.git_branch,
                    t.ts,
                    day,
                    month,
                    t.model,
                    t.raw_model,
                    t.effort,
                    t.usage.input as i64,
                    t.usage.cache_write_5m as i64,
                    t.usage.cache_write_1h as i64,
                    t.usage.cache_read as i64,
                    t.usage.output as i64,
                    t.thinking_tokens as i64,
                    t.usage.web_searches as i64,
                    t.context_tokens as i64,
                    t.is_sidechain as i64,
                    t.agent_id,
                    t.cost.fresh_input,
                    t.cost.cache_write_5m,
                    t.cost.cache_write_1h,
                    t.cost.cache_read,
                    t.cost.output,
                    t.cost.web_search,
                    t.cost.total(),
                ])?;
            }
        }
        tx.commit()?;
        Ok(inserted)
    }

    pub fn turn_count(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))?)
    }

    /// Primer y ultimo dia con datos.
    pub fn data_range(&self) -> Result<(Option<String>, Option<String>)> {
        Ok(self.conn.query_row(
            "SELECT MIN(day), MAX(day) FROM turns WHERE day != ''",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?)
    }

    pub fn by_month(&self) -> Result<Vec<MonthRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT month, account, SUM(cost_usd), COUNT(*)
             FROM turns WHERE month != '' GROUP BY month, account ORDER BY month, account",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(MonthRow {
                    month: r.get(0)?,
                    account: r.get(1)?,
                    cost_usd: r.get(2)?,
                    turns: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn by_day(&self, f: &Filter, limit: i64) -> Result<Vec<DayRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT day, account, SUM(cost_usd), COUNT(*)
             FROM turns WHERE day != '' AND {SCOPE}
             GROUP BY day, account ORDER BY day DESC LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(params![f.account, f.since, limit], |r| {
                Ok(DayRow {
                    day: r.get(0)?,
                    account: r.get(1)?,
                    cost_usd: r.get(2)?,
                    turns: r.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn by_model(&self, f: &Filter) -> Result<Vec<ModelRow>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT account, COALESCE(model, raw_model), SUM(cost_usd), COUNT(*), SUM(out_tok)
             FROM turns WHERE {SCOPE}
             GROUP BY account, COALESCE(model, raw_model) ORDER BY SUM(cost_usd) DESC"
        ))?;
        let rows = stmt
            .query_map(params![f.account, f.since], |r| {
                Ok(ModelRow {
                    account: r.get(0)?,
                    model: r.get(1)?,
                    cost_usd: r.get(2)?,
                    turns: r.get(3)?,
                    out_tok: r.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Composicion del gasto. Es la consulta que explica el problema:
    /// cuanto de la factura es trabajo y cuanto es arrastrar contexto.
    pub fn composition(&self, f: &Filter) -> Result<Composition> {
        Ok(self.conn.query_row(
            &format!(
                "SELECT SUM(cost_input), SUM(cost_w5m), SUM(cost_w1h), SUM(cost_read),
                        SUM(cost_output), SUM(cost_websearch) FROM turns WHERE {SCOPE}"
            ),
            params![f.account, f.since],
            |r| {
                Ok(Composition {
                    fresh_input: r.get::<_, Option<f64>>(0)?.unwrap_or(0.0),
                    cache_write_5m: r.get::<_, Option<f64>>(1)?.unwrap_or(0.0),
                    cache_write_1h: r.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                    cache_read: r.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                    output: r.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
                    web_search: r.get::<_, Option<f64>>(5)?.unwrap_or(0.0),
                })
            },
        )?)
    }

    pub fn top_sessions(&self, f: &Filter, limit: i64) -> Result<Vec<SessionRow>> {
        // El recorte va en el WHERE, asi que una sesion que cruza el borde del
        // periodo aparece con el costo que tuvo *dentro* del periodo.
        let mut stmt = self.conn.prepare(&format!(
            "SELECT t.session_id, t.account, t.project, MIN(t.ts), MAX(t.ts), COUNT(*),
                    SUM(t.cost_usd), MAX(t.ctx_tok),
                    CAST(AVG(t.ctx_tok) AS INTEGER),
                    COALESCE((SELECT SUM(fl.compactions) FROM files fl
                              WHERE fl.session_id = t.session_id), 0),
                    GROUP_CONCAT(DISTINCT COALESCE(t.model, t.raw_model)),
                    m.title, m.prompt,
                    COALESCE(SUM(CASE WHEN t.agent_id IS NOT NULL THEN t.cost_usd END), 0),
                    COUNT(DISTINCT t.agent_id)
             FROM turns t
             LEFT JOIN session_meta m ON m.session_id = t.session_id
             WHERE {SCOPE}
             GROUP BY t.session_id ORDER BY SUM(t.cost_usd) DESC LIMIT ?3"
        ))?;
        let rows = stmt
            .query_map(params![f.account, f.since, limit], |r| {
                let turns: i64 = r.get(5)?;
                let cost: f64 = r.get(6)?;
                Ok(SessionRow {
                    session_id: r.get(0)?,
                    account: r.get(1)?,
                    project: r.get(2)?,
                    first_ts: r.get(3)?,
                    last_ts: r.get(4)?,
                    turns,
                    cost_usd: cost,
                    max_ctx: r.get(7)?,
                    avg_ctx: r.get(8)?,
                    compactions: r.get(9)?,
                    models: r.get::<_, Option<String>>(10)?.unwrap_or_default(),
                    title: r.get(11)?,
                    prompt: r.get(12)?,
                    agent_usd: r.get(13)?,
                    agents: r.get(14)?,
                    cost_per_turn: if turns > 0 { cost / turns as f64 } else { 0.0 },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Serie por turno de una sesion: contexto y costo en el mismo eje.
    /// Es el grafico que contesta "en que sesiones me doy garra".
    pub fn session_timeline(&self, session_id: &str) -> Result<Vec<TurnPoint>> {
        let mut stmt = self.conn.prepare(
            "SELECT ts, ctx_tok, cost_usd, COALESCE(model, raw_model), out_tok, effort,
                    read_tok, cost_read, cost_output, agent_id
             FROM turns WHERE session_id = ?1 ORDER BY ts",
        )?;
        let rows = stmt
            .query_map(params![session_id], |r| {
                Ok(TurnPoint {
                    ts: r.get(0)?,
                    ctx_tok: r.get(1)?,
                    cost_usd: r.get(2)?,
                    model: r.get(3)?,
                    out_tok: r.get(4)?,
                    effort: r.get(5)?,
                    read_tok: r.get(6)?,
                    cost_read: r.get(7)?,
                    cost_output: r.get(8)?,
                    agent_id: r.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Gasto acumulado de un dia. Alimenta la alerta de presupuesto y el tray.
    pub fn cost_on_day(&self, day: &str, account: Option<&str>) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM turns
             WHERE day = ?1 AND (?2 IS NULL OR account = ?2)",
            params![day, account],
            |r| r.get(0),
        )?)
    }

    pub fn cost_since(&self, ts: &str, account: Option<&str>) -> Result<f64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM turns
             WHERE ts >= ?1 AND (?2 IS NULL OR account = ?2)",
            params![ts, account],
            |r| r.get(0),
        )?)
    }

    /// Cuanto del gasto se lo llevaron los subagentes.
    ///
    /// Sus turnos viven en transcripts aparte
    /// (`<sesion>/subagents/agent-*.jsonl`) que ninguna herramienta que mira
    /// solo el nivel de arriba cuenta. Se facturan igual.
    pub fn subagent_split(&self, f: &Filter) -> Result<SubagentSplit> {
        Ok(self.conn.query_row(
            &format!(
                "SELECT COALESCE(SUM(CASE WHEN agent_id IS NOT NULL THEN cost_usd END), 0),
                        COALESCE(SUM(cost_usd), 0),
                        COUNT(CASE WHEN agent_id IS NOT NULL THEN 1 END),
                        COUNT(DISTINCT agent_id),
                        COUNT(DISTINCT CASE WHEN agent_id IS NOT NULL THEN session_id END)
                 FROM turns WHERE {SCOPE}"
            ),
            params![f.account, f.since],
            |r| {
                Ok(SubagentSplit {
                    cost_usd: r.get(0)?,
                    total_usd: r.get(1)?,
                    turns: r.get(2)?,
                    agents: r.get(3)?,
                    sessions: r.get(4)?,
                })
            },
        )?)
    }

    /// Distribucion de requests por tamano de contexto, en tramos de 100K.
    pub fn context_histogram(&self, f: &Filter) -> Result<Vec<(i64, i64)>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT MIN(ctx_tok / 100000, 10) AS bucket, COUNT(*)
             FROM turns WHERE {SCOPE} GROUP BY bucket ORDER BY bucket"
        ))?;
        let rows = stmt
            .query_map(params![f.account, f.since], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn last_alert_ms(&self, kind: &str, key: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT MAX(fired_at_ms) FROM alerts WHERE kind = ?1 AND key = ?2",
                params![kind, key],
                |r| r.get::<_, Option<i64>>(0),
            )
            .optional()?
            .flatten())
    }

    pub fn record_alert(
        &self,
        kind: &str,
        key: &str,
        fired_at_ms: i64,
        payload: &str,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO alerts (kind, key, fired_at_ms, payload_json) VALUES (?1, ?2, ?3, ?4)",
            params![kind, key, fired_at_ms, payload],
        )?;
        Ok(())
    }

    /// Gasto de hoy por modelo. Alimenta la alerta de modelo caro.
    pub fn cost_by_model_on_day(&self, day: &str) -> Result<Vec<(String, f64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(model, raw_model), SUM(cost_usd) FROM turns
             WHERE day = ?1 GROUP BY COALESCE(model, raw_model) ORDER BY 2 DESC",
        )?;
        let rows = stmt
            .query_map(params![day], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Ultimo tamano de contexto visto en una sesion. Para una sesion viva es
    /// su contexto actual.
    pub fn latest_context(&self, session_id: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT ctx_tok FROM turns WHERE session_id = ?1 ORDER BY ts DESC LIMIT 1",
                params![session_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                params![key],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Historial de alertas disparadas, mas recientes primero.
    pub fn recent_alerts(&self, limit: i64) -> Result<Vec<(String, i64, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT kind, fired_at_ms, payload_json FROM alerts
             ORDER BY fired_at_ms DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn set_budget(&self, scope: &str, period: &str, limit_usd: f64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO budgets (scope, period, limit_usd) VALUES (?1, ?2, ?3)
             ON CONFLICT(scope, period) DO UPDATE SET limit_usd = ?3",
            params![scope, period, limit_usd],
        )?;
        Ok(())
    }

    pub fn budgets(&self) -> Result<Vec<(String, String, f64)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT scope, period, limit_usd FROM budgets")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

#[derive(Debug, Serialize)]
pub struct MonthRow {
    pub month: String,
    pub account: String,
    pub cost_usd: f64,
    pub turns: i64,
}

#[derive(Debug, Serialize)]
pub struct DayRow {
    pub day: String,
    pub account: String,
    pub cost_usd: f64,
    pub turns: i64,
}

#[derive(Debug, Serialize)]
pub struct ModelRow {
    pub account: String,
    pub model: String,
    pub cost_usd: f64,
    pub turns: i64,
    pub out_tok: i64,
}

#[derive(Debug, Serialize, Default)]
pub struct Composition {
    pub fresh_input: f64,
    pub cache_write_5m: f64,
    pub cache_write_1h: f64,
    pub cache_read: f64,
    pub output: f64,
    pub web_search: f64,
}

impl Composition {
    pub fn total(&self) -> f64 {
        self.fresh_input
            + self.cache_write_5m
            + self.cache_write_1h
            + self.cache_read
            + self.output
            + self.web_search
    }
}

#[derive(Debug, Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub account: String,
    pub project: String,
    pub first_ts: String,
    pub last_ts: String,
    pub turns: i64,
    pub cost_usd: f64,
    pub cost_per_turn: f64,
    pub max_ctx: i64,
    pub avg_ctx: i64,
    pub compactions: i64,
    pub models: String,
    /// Costo que se fue en subagentes de esta sesion.
    pub agent_usd: f64,
    /// Subagentes distintos que lanzo la sesion.
    pub agents: i64,
    /// Titulo que Claude Code le puso a la sesion, si lo alcanzo a generar.
    pub title: Option<String>,
    /// Primer prompt de la sesion: el respaldo cuando no hay titulo.
    pub prompt: Option<String>,
}

/// Reparto entre lo que gasto la sesion principal y lo que gastaron sus
/// subagentes.
#[derive(Debug, Serialize)]
pub struct SubagentSplit {
    pub cost_usd: f64,
    pub total_usd: f64,
    pub turns: i64,
    pub agents: i64,
    /// Sesiones que llegaron a lanzar al menos un subagente.
    pub sessions: i64,
}

#[derive(Debug, Serialize)]
pub struct TurnPoint {
    pub ts: String,
    pub ctx_tok: i64,
    pub cost_usd: f64,
    pub model: String,
    pub out_tok: i64,
    pub effort: Option<String>,
    /// Tokens leidos del cache en este turno.
    pub read_tok: i64,
    /// El costo partido en sus dos mitades: lo que costo releer y lo que costo
    /// escribir. Es el desglose que explica de donde sale el $ por turno.
    pub cost_read: f64,
    pub cost_output: f64,
    /// `Some(id)` si el turno lo genero un subagente y no la sesion principal.
    pub agent_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::{Cost, Usage};
    use crate::record::Turn;

    fn turn(request_id: &str, session: &str, cost: f64) -> Turn {
        Turn {
            request_id: request_id.into(),
            session_id: session.into(),
            account: "cruisebound".into(),
            project: "proj".into(),
            cwd: None,
            git_branch: None,
            ts: "2026-08-24T10:00:00.000Z".into(),
            model: Some("claude-opus-5"),
            raw_model: "claude-opus-5".into(),
            effort: Some("high".into()),
            usage: Usage {
                output: 1000,
                ..Default::default()
            },
            thinking_tokens: 0,
            context_tokens: 300_000,
            is_sidechain: false,
            agent_id: None,
            cost: Cost {
                output: cost,
                ..Default::default()
            },
        }
    }

    #[test]
    fn deduplica_por_request_id() {
        let mut s = Store::open_in_memory().unwrap();
        // el mismo requestId visto en dos sesiones distintas (sesion reanudada)
        assert_eq!(s.insert_turns(&[turn("req_1", "s1", 1.0)]).unwrap(), 1);
        assert_eq!(s.insert_turns(&[turn("req_1", "s2", 1.0)]).unwrap(), 0);
        assert_eq!(s.turn_count().unwrap(), 1);
        // y se queda con la primera sesion que lo produjo
        let sessions = s.top_sessions(&Filter::default(), 10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, "s1");
    }

    #[test]
    fn agrega_por_mes_y_por_dia() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_turns(&[turn("a", "s1", 2.0), turn("b", "s1", 3.0)])
            .unwrap();
        let months = s.by_month().unwrap();
        assert_eq!(months.len(), 1);
        assert_eq!(months[0].month, "2026-08");
        assert!((months[0].cost_usd - 5.0).abs() < 1e-9);
        assert!((s.cost_on_day("2026-08-24", None).unwrap() - 5.0).abs() < 1e-9);
        assert!((s.cost_on_day("2026-08-23", None).unwrap()).abs() < 1e-9);
    }

    #[test]
    fn costo_por_turno_y_contexto_maximo() {
        let mut s = Store::open_in_memory().unwrap();
        s.insert_turns(&[turn("a", "s1", 2.0), turn("b", "s1", 4.0)])
            .unwrap();
        let row = &s.top_sessions(&Filter::default(), 1).unwrap()[0];
        assert_eq!(row.turns, 2);
        assert!((row.cost_per_turn - 3.0).abs() < 1e-9);
        assert_eq!(row.max_ctx, 300_000);
    }

    #[test]
    fn cursor_persiste_el_offset() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(s.cursor_for("/x.jsonl").unwrap(), (0, 0));
        let cur = |size: u64, mtime_ms: i64, offset: u64| FileCursor {
            path: "/x.jsonl",
            account: "personal",
            project: "p",
            session_id: "s",
            size,
            mtime_ms,
            offset,
            compactions_delta: 1,
            meta_scanned: false,
        };
        s.save_cursor(&cur(100, 1, 100)).unwrap();
        assert_eq!(s.cursor_for("/x.jsonl").unwrap(), (100, 100));
        s.save_cursor(&cur(200, 2, 200)).unwrap();
        let (off, size) = s.cursor_for("/x.jsonl").unwrap();
        assert_eq!((off, size), (200, 200));
    }

    #[test]
    fn releer_desde_cero_no_duplica_compactaciones() {
        let s = Store::open_in_memory().unwrap();
        let cur = |offset: u64, delta: u32| FileCursor {
            path: "/x.jsonl",
            account: "personal",
            project: "p",
            session_id: "s",
            size: 100,
            mtime_ms: 1,
            offset,
            compactions_delta: delta,
            meta_scanned: false,
        };
        let count = || -> i64 {
            s.conn
                .query_row(
                    "SELECT compactions FROM files WHERE path = '/x.jsonl'",
                    [],
                    |r| r.get(0),
                )
                .unwrap()
        };
        s.save_cursor(&cur(100, 3)).unwrap();
        assert_eq!(count(), 3);
        // Segunda pasada leyendo el archivo entero: mismo offset, mismo total.
        s.save_cursor(&cur(100, 3)).unwrap();
        assert_eq!(count(), 3, "releer entero recuenta, no suma");
        // Crecimiento normal: si suma.
        s.save_cursor(&cur(180, 1)).unwrap();
        assert_eq!(count(), 4);
    }

    #[test]
    fn el_titulo_se_pisa_y_el_prompt_no() {
        let s = Store::open_in_memory().unwrap();
        s.save_session_meta("s1", Some("Primer tema"), Some("hola"))
            .unwrap();
        s.save_session_meta("s1", Some("Segundo tema"), Some("otro prompt"))
            .unwrap();
        let (t, p): (Option<String>, Option<String>) = s
            .conn
            .query_row(
                "SELECT title, prompt FROM session_meta WHERE session_id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(t.as_deref(), Some("Segundo tema"));
        assert_eq!(p.as_deref(), Some("hola"));

        // Un scan sin titulo no borra el que ya habia.
        s.save_session_meta("s1", None, None).unwrap();
        let t2: Option<String> = s
            .conn
            .query_row(
                "SELECT title FROM session_meta WHERE session_id = 's1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t2.as_deref(), Some("Segundo tema"));
    }
}
