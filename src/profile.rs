//! Built-in profiling (dashq.md D20): one record per measured operation in a bounded local file, a report that
//! ranks operations worst first. Records hold operation names, numbers, outcomes and error classes only.

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// Measured operations (D20): name, p95 budget in ms, error budget as a failure rate.
pub const OPS: [(&str, u64, f64); 12] = [
    ("http.page", 3000, 0.01),
    ("http.data", 3000, 0.01),
    ("http.enroll", 500, 0.01),
    ("http.answer", 15000, 0.05),
    ("read.langfuse", 10000, 0.05),
    ("read.board", 3000, 0.05),
    ("read.release", 3000, 0.05),
    ("read.taskq", 10000, 0.05),
    ("read.serverq", 50, 0.01),
    ("update.check", 15000, 0.05),
    ("update", 120000, 0.05),
    ("restart", 30000, 0.05),
];
/// Store bound: past `MAX` bytes the file is cut to its newest half.
const MAX: u64 = 4 << 20;
const FILE: &str = "profile.jsonl";

static STORE: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

pub fn trace() -> bool {
    std::env::var_os("DASHQ_TRACE").is_some_and(|v| v == "1")
}

/// Turns on recording into `dir`; without it (selftest, tests) operations are only traced.
pub fn open(dir: &Path) {
    let _ = STORE.set(dir.join(FILE));
    if trace() {
        eprintln!("profile: trace on");
    }
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Short stable error class: the reason before any `:` detail, plain characters, at most 40.
fn class(reason: &str) -> String {
    reason.split(':').next().unwrap_or("").chars().filter(|c| c.is_ascii_alphanumeric() || " ._-".contains(*c)).take(40).collect::<String>().trim().to_string()
}

/// Records one finished operation; a failed write never fails the operation.
pub fn record(op: &str, start: Instant, error: Option<&str>) {
    let ms = start.elapsed().as_millis() as u64;
    let error = error.map(class);
    if trace() {
        eprintln!("trace {op} {ms} ms {}", error.as_deref().unwrap_or("ok"));
    }
    let Some(p) = STORE.get() else { return };
    let line = json!({"at": crate::source::now(), "op": op, "ms": ms, "outcome": if error.is_some() { "failed" } else { "ok" }, "error": error}).to_string() + "\n";
    let _g = LOCK.lock();
    let _ = append(p, &line, MAX);
}

// ponytail: the lock covers this process only; a concurrent `dashq update` prune may drop a line of the service
fn append(p: &Path, line: &str, max: u64) -> std::io::Result<()> {
    let mut o = std::fs::OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    let mut f = o.open(p)?;
    f.write_all(line.as_bytes())?;
    if f.metadata()?.len() <= max {
        return Ok(());
    }
    let s = std::fs::read(p)?;
    let cut = s.len().saturating_sub(max as usize / 2);
    let start = s[cut..].iter().position(|b| *b == b'\n').map_or(s.len(), |i| cut + i + 1);
    let tmp = p.with_extension("tmp");
    std::fs::write(&tmp, &s[start..])?;
    std::fs::rename(tmp, p)
}

fn pct(sorted: &[u64], p: usize) -> u64 {
    sorted[(sorted.len() * p).div_ceil(100).saturating_sub(1)]
}

/// The profile report of the window ending at `now`; `text` is the store, `None` when it is missing.
pub fn report(text: Option<&str>, now: u64, window: u64) -> Value {
    let from = now.saturating_sub(window);
    let (mut by, mut corrupt) = (BTreeMap::<String, Vec<(u64, String, Option<String>)>>::new(), 0);
    for line in text.unwrap_or("").lines().filter(|l| !l.trim().is_empty()) {
        let r: Value = serde_json::from_str(line).unwrap_or_default();
        let (Some(at), Some(op), Some(ms), Some(out)) = (r["at"].as_u64(), r["op"].as_str(), r["ms"].as_u64(), r["outcome"].as_str()) else {
            corrupt += 1;
            continue;
        };
        if !matches!(out, "ok" | "failed" | "cancelled" | "retried") || op.is_empty() || !op.bytes().all(|b| b.is_ascii_lowercase() || b == b'.') {
            corrupt += 1;
            continue;
        }
        if at >= from && at <= now {
            by.entry(op.into()).or_default().push((ms, out.into(), r["error"].as_str().map(String::from)));
        }
    }
    let unknown: Vec<Value> = OPS
        .iter()
        .filter(|(op, ..)| !by.contains_key(*op))
        .map(|(op, ..)| json!({"op": op, "reason": if text.is_none() { "no profile store" } else { "no samples in window" }}))
        .collect();
    let mut rows: Vec<(f64, f64, Value)> = by
        .iter()
        .map(|(op, s)| {
            let (budget, errors) = OPS.iter().find(|(o, ..)| o == op).map_or((None, 0.01), |(_, b, e)| (Some(*b), *e));
            let mut ms: Vec<u64> = s.iter().map(|x| x.0).collect();
            ms.sort_unstable();
            let n = ms.len() as f64;
            let mean = ms.iter().sum::<u64>() as f64 / n;
            let cv = if mean > 0.0 { (ms.iter().map(|x| (*x as f64 - mean).powi(2)).sum::<f64>() / n).sqrt() / mean } else { 0.0 };
            let count = |o: &str| s.iter().filter(|x| x.1 == o).count();
            let failure_rate = count("failed") as f64 / n;
            let p95 = pct(&ms, 95);
            let score = (budget.map_or(0.0, |b| p95 as f64 / b as f64)).max(failure_rate / errors);
            let mut classes = BTreeMap::new();
            for e in s.iter().filter_map(|x| x.2.as_deref()) {
                *classes.entry(e.to_string()).or_insert(0) += 1;
            }
            let r = (score * 100.0).round() / 100.0;
            let row = json!({
                "op": op, "count": ms.len(), "p50_ms": pct(&ms, 50), "p95_ms": p95, "max_ms": ms[ms.len() - 1],
                "budget_p95_ms": budget, "error_budget": errors, "failed": count("failed"), "retried": count("retried"), "cancelled": count("cancelled"),
                "failure_rate": (failure_rate * 1000.0).round() / 1000.0, "cv": (cv * 100.0).round() / 100.0, "errors": classes,
                "score": r, "over_budget": score > 1.0,
            });
            (score, cv, row)
        })
        .collect();
    // worst first: slowness against budget or failure rate against the error budget, then instability
    rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.total_cmp(&a.1)));
    let samples: usize = by.values().map(Vec::len).sum();
    let store = match (text, corrupt) {
        (None, _) => json!({"state": "unknown", "reason": "no profile store", "corrupt": 0}),
        (_, 0) => json!({"state": "ok", "reason": null, "corrupt": 0}),
        (_, n) => json!({"state": "incomplete", "reason": format!("{n} corrupt records skipped"), "corrupt": n}),
    };
    json!({
        "schema": 1, "generated_at": now, "window_s": window, "from": from, "to": now, "trace": if trace() { "on" } else { "off" },
        "store": store, "samples": samples, "operations": rows.into_iter().map(|r| r.2).collect::<Vec<_>>(), "unknown": unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // chain: measured operations reach a ranked profile report (D20); missing data stays unknown, never zero
    #[test]
    fn report_ranks_and_keeps_unknowns() {
        let none = report(None, 1000, 100);
        assert_eq!((none["store"]["state"].as_str(), none["samples"].as_u64()), (Some("unknown"), Some(0)));
        assert_eq!(none["unknown"].as_array().unwrap().len(), OPS.len());
        assert_eq!(none["unknown"][0]["reason"], "no profile store");
        let empty = report(Some(""), 1000, 100);
        assert_eq!((empty["store"]["state"].as_str(), empty["operations"].as_array().unwrap().len()), (Some("ok"), 0));
        assert_eq!(empty["unknown"][0]["reason"], "no samples in window");
        let rec = |at: u64, op: &str, ms: u64, err: Option<&str>| json!({"at": at, "op": op, "ms": ms, "outcome": if err.is_some() { "failed" } else { "ok" }, "error": err}).to_string();
        let mut lines = vec![
            rec(950, "read.serverq", 2, None),
            rec(960, "read.serverq", 3, None),
            "{\"at\":970,\"op\":\"http.page\"".into(),
            "not json".into(),
            rec(10, "http.page", 99999, None),
            rec(980, "read.board", 900, Some("timeout")),
            rec(990, "read.board", 800, None),
        ];
        lines.extend((0..20).map(|i| rec(900 + i, "http.page", 1000 + i, None)));
        lines.push(rec(999, "http.page", 6000, None));
        let r = report(Some(&lines.join("\n")), 1000, 100);
        assert_eq!(r["store"], json!({"state": "incomplete", "reason": "2 corrupt records skipped", "corrupt": 2}));
        let ops: Vec<&str> = r["operations"].as_array().unwrap().iter().map(|o| o["op"].as_str().unwrap()).collect();
        assert_eq!(ops, ["read.board", "http.page", "read.serverq"], "failing, then slow, then healthy");
        let b = &r["operations"][0];
        assert_eq!((b["failure_rate"].as_f64(), b["errors"]["timeout"].as_u64(), b["over_budget"].as_bool()), (Some(0.5), Some(1), Some(true)));
        let p = &r["operations"][1];
        assert_eq!((p["count"].as_u64(), p["p50_ms"].as_u64(), p["max_ms"].as_u64()), (Some(21), Some(1010), Some(6000)), "the record outside the window is not counted");
        assert!(p["cv"].as_f64().unwrap() > 0.5, "one 6 s outlier marks the page unstable");
        assert!(!r["unknown"].as_array().unwrap().iter().any(|u| u["op"] == "http.page"));
    }

    // chain: the store stays bounded and keeps whole newest records; no content, only classes
    #[test]
    fn store_is_bounded() {
        let p = std::env::temp_dir().join(format!("dashq-profile-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&p);
        for i in 0..200 {
            append(&p, &format!("{{\"at\":{i}}}\n"), 1000).unwrap();
        }
        let s = std::fs::read_to_string(&p).unwrap();
        assert!(s.len() <= 1000 && s.starts_with("{\"at\":") && s.ends_with("{\"at\":199}\n"));
        let _ = std::fs::remove_file(&p);
        assert_eq!(class("taskq incomplete: HTTP 401 /home/x"), "taskq incomplete");
        assert_eq!(class("HTTP 500"), "HTTP 500");
    }
}
