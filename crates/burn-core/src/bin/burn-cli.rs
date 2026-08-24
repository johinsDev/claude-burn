//! CLI de verificacion. Los mismos numeros que consume la GUI, en texto plano,
//! para poder contrastarlos contra `ccusage` sin abrir la app.
//!
//!   burn-cli sync        sincroniza y muestra el resumen de la ingesta
//!   burn-cli months      gasto por mes y cuenta
//!   burn-cli days [n]    gasto por dia
//!   burn-cli models      gasto por modelo
//!   burn-cli composition en que se va la plata
//!   burn-cli sessions    las sesiones mas caras
//!   burn-cli session ID  contexto y costo turno a turno
//!   burn-cli context     distribucion de requests por tamano de contexto
//!   burn-cli plan        limites del plan y sesiones vivas
//!   burn-cli report      todo lo anterior de una

use anyhow::Result;
use burn_core::{profiles, store::Store, sync_default};
use std::collections::BTreeMap;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("report");

    let db_path = std::env::var("BURN_DB")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| burn_core::default_db_path());
    let mut db = Store::open(&db_path)?;

    let (profs, rep) = sync_default(&mut db)?;
    if matches!(cmd, "sync" | "report") {
        println!("== ingesta ==");
        println!(
            "  {} archivos ({} con cambios) en {} ms",
            rep.files_scanned, rep.files_changed, rep.elapsed_ms
        );
        println!(
            "  {} turnos nuevos, {} duplicados descartados, {} compactaciones",
            rep.turns_new, rep.turns_duplicate, rep.compactions
        );
        if !rep.unknown_models.is_empty() {
            println!(
                "  modelos sin precio (costo 0): {}",
                rep.unknown_models.join(", ")
            );
        }
        println!("  base: {}", db_path.display());
        println!("  total acumulado: {} turnos", db.turn_count()?);
        println!();
    }

    match cmd {
        "sync" => {}
        "months" | "report" => {
            print_months(&db, &profs)?;
            if cmd == "report" {
                println!();
                print_composition(&db)?;
                println!();
                print_models(&db)?;
                println!();
                print_sessions(&db, 12)?;
                println!();
                print_context(&db)?;
                println!();
                print_plan(&profs)?;
            }
        }
        "days" => {
            let n: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
            for row in db.by_day(None, n)? {
                println!(
                    "{}  {:<12} ${:>9.2}  {:>5} turnos",
                    row.day, row.account, row.cost_usd, row.turns
                );
            }
        }
        "models" => print_models(&db)?,
        "composition" => print_composition(&db)?,
        "sessions" => {
            let n: i64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
            print_sessions(&db, n)?;
        }
        "session" => {
            let Some(id) = args.get(1) else {
                anyhow::bail!("uso: burn-cli session <session-id>");
            };
            print_timeline(&db, id)?;
        }
        "context" => print_context(&db)?,
        "plan" => print_plan(&profs)?,
        other => anyhow::bail!("comando desconocido: {other}"),
    }
    Ok(())
}

fn print_months(db: &Store, profs: &[profiles::Profile]) -> Result<()> {
    let billing: BTreeMap<&str, profiles::Billing> =
        profs.iter().map(|p| (p.name.as_str(), p.billing)).collect();
    let rows = db.by_month()?;
    let mut totals: BTreeMap<String, f64> = BTreeMap::new();
    for r in &rows {
        *totals.entry(r.month.clone()).or_default() += r.cost_usd;
    }
    println!("== gasto por mes (precio de lista) ==");
    let mut current = String::new();
    for r in &rows {
        if r.month != current {
            current = r.month.clone();
            println!("{}  TOTAL ${:>9.0}", current, totals[&current]);
        }
        let tag = match billing.get(r.account.as_str()) {
            Some(profiles::Billing::Flat) => "  (tarifa plana, no facturado)",
            Some(profiles::Billing::Overage) => "  (overage, plata real)",
            _ => "",
        };
        println!(
            "    {:<14} ${:>9.0}  {:>6} turnos{}",
            r.account, r.cost_usd, r.turns, tag
        );
    }
    Ok(())
}

fn print_composition(db: &Store) -> Result<()> {
    let c = db.composition()?;
    let t = c.total();
    println!("== en que se va la plata ==");
    let mut rows = [
        ("cache_read (releer contexto)", c.cache_read),
        ("cache_write_1h", c.cache_write_1h),
        ("cache_write_5m", c.cache_write_5m),
        ("output (lo que Claude escribe)", c.output),
        ("input fresco", c.fresh_input),
        ("busqueda web", c.web_search),
    ];
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (label, v) in rows {
        let pct = if t > 0.0 { v / t * 100.0 } else { 0.0 };
        println!("  {:<32} ${:>9.0}  {:>5.1}%", label, v, pct);
    }
    println!("  {:<32} ${:>9.0}", "TOTAL", t);
    Ok(())
}

fn print_models(db: &Store) -> Result<()> {
    println!("== gasto por modelo ==");
    for r in db.by_model()? {
        println!(
            "  {:<12} {:<22} ${:>9.0}  {:>6} turnos",
            r.account, r.model, r.cost_usd, r.turns
        );
    }
    Ok(())
}

fn print_sessions(db: &Store, n: i64) -> Result<()> {
    println!("== sesiones mas caras ==");
    println!(
        "  {:>9}  {:>7}  {:<12} {:>6} {:>9} {:>8} {:>5}  proyecto",
        "costo", "$/turno", "cuenta", "turnos", "ctx max", "ctx prom", "comp"
    );
    for s in db.top_sessions(n)? {
        println!(
            "  ${:>8.0}  ${:>6.2}  {:<12} {:>6} {:>9} {:>8} {:>5}  {}",
            s.cost_usd,
            s.cost_per_turn,
            s.account,
            s.turns,
            fmt_k(s.max_ctx),
            fmt_k(s.avg_ctx),
            s.compactions,
            short_project(&s.project),
        );
        println!(
            "      {}  {}  {}",
            &s.first_ts[..10.min(s.first_ts.len())],
            s.session_id,
            s.models
        );
    }
    Ok(())
}

fn print_timeline(db: &Store, session_id: &str) -> Result<()> {
    let points = db.session_timeline(session_id)?;
    if points.is_empty() {
        println!("sin turnos para la sesion {session_id}");
        return Ok(());
    }
    println!("== {} — {} turnos ==", session_id, points.len());
    println!(
        "  {:<20} {:>9} {:>9} {:>8}  modelo",
        "hora", "contexto", "$ turno", "output"
    );
    let mut acc = 0.0;
    for p in &points {
        acc += p.cost_usd;
        let bar_len = ((p.ctx_tok as f64 / 1_000_000.0) * 24.0).round() as usize;
        println!(
            "  {:<20} {:>9} {:>9.3} {:>8}  {:<18} {}",
            p.ts.get(..19).unwrap_or(&p.ts),
            fmt_k(p.ctx_tok),
            p.cost_usd,
            p.out_tok,
            p.model,
            "#".repeat(bar_len.min(24)),
        );
    }
    println!("  acumulado: ${acc:.2}");
    Ok(())
}

fn print_context(db: &Store) -> Result<()> {
    println!("== requests por tamano de contexto ==");
    let rows = db.context_histogram()?;
    let total: i64 = rows.iter().map(|(_, c)| c).sum();
    for (bucket, count) in rows {
        let pct = if total > 0 {
            count as f64 / total as f64 * 100.0
        } else {
            0.0
        };
        let label = if bucket >= 10 {
            ">1000k".to_string()
        } else {
            format!("{}k-{}k", bucket * 100, (bucket + 1) * 100)
        };
        println!(
            "  {:<12} {:>8}  {:>5.1}%  {}",
            label,
            count,
            pct,
            "#".repeat((pct / 2.0) as usize)
        );
    }
    Ok(())
}

fn print_plan(profs: &[profiles::Profile]) -> Result<()> {
    println!("== cuentas, limites del plan y sesiones vivas ==");
    for p in profs {
        println!(
            "  {} — {} ({:?}) {}",
            p.name,
            p.email.as_deref().unwrap_or("?"),
            p.billing,
            p.plan.as_deref().unwrap_or("")
        );
        match profiles::read_plan_usage(p) {
            Some(u) => {
                let age = u
                    .fetched_at_ms
                    .map(|ms| format!("{} min", (burn_core::now_ms() - ms) / 60_000))
                    .unwrap_or_else(|| "?".into());
                println!("      cache de Anthropic, antiguedad {age}");
                for l in u.limits {
                    println!(
                        "      {:<14} {:>5.0}%  {}{}",
                        l.kind,
                        l.percent,
                        l.resets_at.as_deref().unwrap_or("-"),
                        if l.is_active { "  (activo)" } else { "" }
                    );
                }
                if let Some(s) = u.extra_usage_spent {
                    println!("      overage gastado: ${s:.2}");
                }
            }
            None => println!("      sin cache de uso en .claude.json"),
        }
        for s in profiles::read_live_sessions(p) {
            println!(
                "      VIVA pid={} {} [{}] {}",
                s.pid,
                s.name.as_deref().unwrap_or("-"),
                s.status.as_deref().unwrap_or("?"),
                s.cwd
            );
        }
    }
    Ok(())
}

fn fmt_k(n: i64) -> String {
    if n >= 1000 {
        format!("{}k", n / 1000)
    } else {
        n.to_string()
    }
}

fn short_project(p: &str) -> String {
    p.rsplit('-')
        .take(3)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("-")
}
