//! Agent work (dashq.md D21): taskq attempt stage records (taskq.md § 4 Profiling, `taskq-stage` lines of the
//! local `taskq.log`) per host and runtime. Records hold fixed codes and numbers only; missing data stays unknown.

use crate::page::esc;
use crate::source::clock;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const PREFIX: &str = "taskq-stage ";
pub const STAGES: [&str; 7] = ["claim", "start", "build", "checks", "review", "ask", "close"];
const RUNTIMES: [&str; 4] = ["claude", "codex", "codex-app", "other"];
const OUTCOMES: [&str; 6] = ["ok", "ask", "retried", "failed", "cancelled", "unknown"];
const ERRORS: [&str; 11] = ["none", "http4xx", "http5xx", "timeout", "permission", "invalid", "other", "panic", "exit", "left-board", "unreadable"];
const UNKNOWN: [&str; 5] = ["no-model", "no-usage", "log-unreadable", "app-turn", "adopted"];
/// A log read older than this is stale (D6); the service rereads it every 60 s.
const LIMIT: u64 = 300;

/// Reads the stage records of one `taskq.log`; other lines are taskq's own and never leave this function.
pub fn read(path: &str) -> Result<(Value, bool), crate::source::Fail> {
    let text = std::fs::read(path).map_err(|_| ("log missing".to_string(), None))?;
    let text = String::from_utf8_lossy(&text);
    let (mut records, mut corrupt) = (vec![], 0);
    for l in text.lines().filter_map(|l| l.strip_prefix(PREFIX)) {
        match serde_json::from_str::<Value>(l) {
            Ok(v) if valid(&v) => records.push(v),
            _ => corrupt += 1,
        }
    }
    Ok((json!({"records": records, "corrupt": corrupt}), false))
}

fn valid(r: &Value) -> bool {
    let is = |k: &str, set: &[&str]| r[k].as_str().is_some_and(|s| set.contains(&s));
    is("stage", &STAGES) && is("rt", &RUNTIMES) && is("outcome", &OUTCOMES) && is("error", &ERRORS)
        && (r["unknown"].is_null() || is("unknown", &UNKNOWN)) && r["task"].is_u64() && r["attempt"].is_u64() && r["at_ms"].is_u64()
        && (r["us"].is_null() || r["us"].is_u64()) && ["calls", "tokens"].iter().all(|k| r[*k].is_null() || r[*k].is_u64())
}

fn pct(sorted: &[u64], p: usize) -> u64 {
    sorted[(sorted.len() * p).div_ceil(100) - 1]
}

fn ms(us: u64) -> Value {
    json!((us as f64 / 100.0).round() / 10.0)
}

/// Sum of `field` over `v`: `sum` only from known values, `known` of `of`, reasons of the rest (taskq's codes).
fn usage(v: &[&Value], field: &str) -> Value {
    let known: Vec<u64> = v.iter().filter_map(|r| r[field].as_u64()).collect();
    let reasons: BTreeSet<&str> = v.iter().filter(|r| r[field].is_null()).map(|r| r["unknown"].as_str().unwrap_or("not-reported")).collect();
    json!({"sum": (!known.is_empty()).then(|| known.iter().sum::<u64>()), "known": known.len(), "of": v.len(), "reasons": reasons})
}

fn counts<'a>(it: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, u64> {
    let mut m = BTreeMap::new();
    for k in it {
        *m.entry(k).or_insert(0) += 1;
    }
    m
}

/// `/data.agent_work` from `inp.agent_work` `{host: snap}` (absent: not configured), the period and host filter.
pub fn build(inp: &Value) -> Value {
    let (now, period) = (inp["now"].as_u64().unwrap_or(0), inp["period"].as_u64().unwrap_or(1800));
    let from = now.saturating_sub(period);
    let Some(logs) = inp["agent_work"].as_object() else {
        return json!({"source": crate::data::source(&Value::Null, now, LIMIT, None), "window_s": period, "from": from, "to": now, "hosts": [], "rows": []});
    };
    let (mut hosts, mut rows) = (vec![], vec![]);
    for (host, snap) in logs.iter().filter(|(h, _)| inp["filters"]["host"].as_str().is_none_or(|x| x == h.as_str())) {
        let mut src = crate::data::source(snap, now, LIMIT, None);
        let all: Vec<&Value> = snap["value"]["records"].as_array().into_iter().flatten().collect();
        let corrupt = snap["value"]["corrupt"].as_u64().unwrap_or(0);
        if corrupt > 0 && src["state"] == "ok" {
            src["state"] = json!("incomplete");
            src["reason"] = json!(format!("{corrupt} unreadable stage records skipped"));
        }
        let retained = all.iter().filter_map(|r| r["at_ms"].as_u64()).min().map(|m| m / 1000);
        let win: Vec<&Value> = all.iter().copied().filter(|r| r["at_ms"].as_u64().is_some_and(|m| (from..=now).contains(&(m / 1000)))).collect();
        // the log is bounded (taskq.md § 4 Data): records before its oldest kept one are unknown, never absent
        let coverage = match retained {
            _ if snap["value"].is_null() => Value::Null,
            None => json!("no stage records in log"),
            Some(r) if r > from => json!(format!("log retains stages from {}; earlier unknown", clock(r, now))),
            Some(_) => Value::Null,
        };
        hosts.push(json!({"host": host, "source": src, "records": (!snap["value"].is_null()).then_some(win.len()), "retained_from": retained, "coverage": coverage}));
        for rt in RUNTIMES {
            let mine: Vec<&Value> = win.iter().copied().filter(|r| r["rt"] == rt).collect();
            if !mine.is_empty() {
                rows.push(row(host, rt, &mine));
            }
        }
    }
    // worst first: the slowest stage p95 of each row; untimed rows last
    let worst = |r: &Value| r["stages"][0]["p95_ms"].as_f64().unwrap_or(-1.0);
    rows.sort_by(|a, b| worst(b).total_cmp(&worst(a)));
    // the worst host names the block's state; one missing log never reads ok
    let bad = ["unknown", "unconfirmed", "stale", "incomplete"].iter().find_map(|st| hosts.iter().find(|h| h["source"]["state"] == *st));
    let src = match (hosts.first(), bad) {
        (None, _) => json!({"state": "unknown", "at": null, "age_s": null, "reason": "no log for this host filter", "incomplete": false}),
        (_, Some(h)) => {
            let mut s = h["source"].clone();
            s["reason"] = json!(format!("{}: {}", h["host"].as_str().unwrap_or(""), s["reason"].as_str().unwrap_or("")));
            s
        }
        (Some(h), None) => h["source"].clone(),
    };
    json!({"source": src, "window_s": period, "from": from, "to": now, "hosts": hosts, "rows": rows})
}

fn row(host: &str, rt: &str, v: &[&Value]) -> Value {
    let attempts: BTreeSet<u64> = v.iter().filter_map(|r| r["attempt"].as_u64()).filter(|a| *a > 0).collect();
    let tasks: BTreeSet<u64> = v.iter().filter_map(|r| r["task"].as_u64()).collect();
    let mut stages: Vec<Value> = STAGES
        .iter()
        .filter_map(|s| {
            let st: Vec<&Value> = v.iter().copied().filter(|r| r["stage"] == *s).collect();
            if st.is_empty() {
                return None;
            }
            let mut us: Vec<u64> = st.iter().filter_map(|r| r["us"].as_u64()).collect();
            us.sort_unstable();
            let timed = !us.is_empty();
            Some(json!({
                "stage": s, "count": st.len(), "timed": us.len(),
                "p50_ms": timed.then(|| ms(pct(&us, 50))), "p95_ms": timed.then(|| ms(pct(&us, 95))), "max_ms": us.last().map(|u| ms(*u)),
                "total_ms": timed.then(|| ms(us.iter().sum())),
                "outcomes": counts(st.iter().filter_map(|r| r["outcome"].as_str())),
                "errors": counts(st.iter().filter_map(|r| r["error"].as_str()).filter(|e| *e != "none")),
                "calls": usage(&st, "calls"), "tokens": usage(&st, "tokens"),
            }))
        })
        .collect();
    stages.sort_by(|a, b| b["p95_ms"].as_f64().unwrap_or(-1.0).total_cmp(&a["p95_ms"].as_f64().unwrap_or(-1.0)));
    let models: Vec<&Value> = v.iter().copied().filter(|r| matches!(r["stage"].as_str(), Some("build" | "review"))).collect();
    let total: Vec<f64> = stages.iter().filter_map(|s| s["total_ms"].as_f64()).collect();
    json!({
        "host": host, "runtime": rt, "attempts": attempts.len(), "tasks": tasks.len(),
        // the attempt's hand-in outcome lives on its build/review stage (taskq.md § 4 Profiling)
        "outcomes": counts(models.iter().filter_map(|r| r["outcome"].as_str())),
        "time_ms": (!total.is_empty()).then(|| (total.iter().sum::<f64>() * 10.0).round() / 10.0),
        "untimed": v.iter().filter(|r| r["us"].is_null()).count(),
        "tokens": usage(&models, "tokens"), "calls": usage(&models, "calls"),
        "stages": stages,
    })
}

fn n(v: &Value) -> String {
    match v.as_f64() {
        Some(f) if f >= 60_000.0 => format!("{:.1} min", f / 60_000.0),
        Some(f) if f >= 1000.0 => format!("{:.1} s", f / 1000.0),
        Some(f) => format!("{f} ms"),
        None => "<span class=\"unknown\" title=\"unknown: adopted across a service restart\">—</span>".into(),
    }
}

/// Digits grouped by thin spaces: 15 210 000.
fn group(n: u64) -> String {
    let d = n.to_string();
    d.chars().enumerate().fold(String::new(), |mut o, (i, c)| {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            o.push('\u{2009}');
        }
        o.push(c);
        o
    })
}

fn plural(n: &Value, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// A usage cell: a sum only when every record reported it, else `k of n known` with taskq's reasons; never 0 for unknown.
fn use_cell(u: &Value) -> String {
    let reasons: Vec<&str> = u["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let why = esc(&reasons.join(", "));
    match (u["sum"].as_u64(), u["known"] == u["of"]) {
        (Some(s), true) => group(s),
        (Some(s), false) => format!("{} <small>({} of {} known; {why})</small>", group(s), u["known"], u["of"]),
        (None, _) if reasons == ["no-model"] => "none (no model)".into(),
        (None, _) => format!("<span class=\"unknown\" title=\"unknown: {why}\">—</span> <small>unknown: {why}</small>"),
    }
}

fn map(v: &Value) -> String {
    let m: Vec<String> = v.as_object().into_iter().flatten().map(|(k, n)| format!("{} {n}", esc(k))).collect();
    if m.is_empty() { "—".into() } else { m.join(" · ") }
}

pub fn render(w: &mut String, doc: &Value) {
    let a = &doc["agent_work"];
    if a["hosts"].as_array().is_none_or(|h| h.is_empty()) && a["source"]["reason"] == "not configured" {
        return;
    }
    let now = doc["generated_at"].as_u64().unwrap_or(0);
    let _ = writeln!(w, "<section id=\"agentwork\">\n<h2>Agent work</h2>\n<p class=\"note\">taskq attempt stages ended in the period, per host and runtime; slowest stage (p95) first. Budgets unknown. Model use counts build and review only.</p>");
    for h in a["hosts"].as_array().into_iter().flatten() {
        let s = &h["source"];
        let state = s["state"].as_str().unwrap_or("unknown");
        let mut notes: Vec<String> = s["reason"].as_str().map(esc).into_iter().collect();
        notes.extend(h["coverage"].as_str().map(esc));
        let _ = writeln!(w, "<p class=\"note\">{} {} · {} stage records{}{}</p>", esc(h["host"].as_str().unwrap_or("")), crate::page::badge(state, state), h["records"].as_u64().map_or("—".into(), |r| r.to_string()),
            s["at"].as_u64().map(|t| format!(" · read {}", clock(t, now))).unwrap_or_default(), if notes.is_empty() { String::new() } else { format!(" · {}", notes.join(" · ")) });
    }
    for r in a["rows"].as_array().into_iter().flatten() {
        let untimed = r["untimed"].as_u64().filter(|u| *u > 0).map(|u| format!(" ({u} untimed)")).unwrap_or_default();
        let _ = writeln!(w, "<h3>{} · {}</h3>\n<p>{} · {} · outcomes {} · time {}{untimed} · context tokens {} · model calls {}</p>",
            esc(r["host"].as_str().unwrap_or("")), esc(r["runtime"].as_str().unwrap_or("")), plural(&r["attempts"], "attempt"), plural(&r["tasks"], "task"), map(&r["outcomes"]), n(&r["time_ms"]), use_cell(&r["tokens"]), use_cell(&r["calls"]));
        let _ = writeln!(w, "<table>\n<tr><th>Stage</th><th>Count</th><th>p50 · p95 · max</th><th>Total time</th><th>Outcomes</th><th>Errors</th><th>Model calls</th><th>Context tokens</th></tr>");
        for s in r["stages"].as_array().into_iter().flatten() {
            let timed = if s["timed"] == s["count"] { String::new() } else { format!(" <small>({} of {} timed)</small>", s["timed"], s["count"]) };
            let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{} · {} · {}{timed}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(s["stage"].as_str().unwrap_or("")), s["count"], n(&s["p50_ms"]), n(&s["p95_ms"]), n(&s["max_ms"]), n(&s["total_ms"]), map(&s["outcomes"]), map(&s["errors"]), use_cell(&s["calls"]), use_cell(&s["tokens"]));
        }
        let _ = writeln!(w, "</table>");
    }
    if a["rows"].as_array().is_none_or(|r| r.is_empty()) {
        let _ = writeln!(w, "<p class=\"note\">No attempt stages in this period.</p>");
    }
    let _ = writeln!(w, "</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    // chain: taskq-stage lines in a local taskq.log -> /data.agent_work per host and runtime -> page; missing,
    // adopted, unreported and corrupt data stays unknown with a reason, never zero (D21)
    #[test]
    fn stage_records_to_agent_work() {
        let dir = std::env::temp_dir().join(format!("dashq-agentwork-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("taskq.log");
        let rec = |stage: &str, task: u64, att: u64, rt: &str, us: Option<u64>, out: &str, at: u64, calls: Option<u64>, tokens: Option<u64>, unk: Option<&str>| {
            let mut r = json!({"stage": stage, "task": task, "attempt": att, "rt": rt, "us": us, "outcome": out, "error": if out == "failed" { "exit" } else { "none" }, "at_ms": at * 1000});
            for (k, v) in [("calls", calls.map(|c| json!(c))), ("tokens", tokens.map(|c| json!(c))), ("unknown", unk.map(|c| json!(c)))] {
                if let Some(v) = v {
                    r[k] = v;
                }
            }
            format!("{PREFIX}{r}")
        };
        let lines = [
            "2026-10-13T07:00:00Z service pass".to_string(),
            rec("build", 1, 10, "claude", Some(600_000_000), "ok", 1000, Some(40), Some(900_000), None),
            rec("build", 2, 11, "claude", Some(60_000_000), "failed", 1500, Some(3), None, Some("no-usage")),
            rec("claim", 2, 11, "claude", Some(1_500), "ok", 1500, None, None, Some("no-model")),
            rec("review", 1, 12, "codex", None, "ok", 1600, None, None, Some("adopted")),
            rec("build", 3, 13, "codex", Some(5_000_000), "ok", 10, Some(1), Some(5), None),
            format!("{PREFIX}{{\"stage\":\"lunch\"}}"),
        ];
        std::fs::write(&log, lines.join("\n")).unwrap();
        let (value, _) = read(log.to_str().unwrap()).unwrap();
        assert_eq!((value["records"].as_array().unwrap().len(), value["corrupt"].as_u64()), (5, Some(1)), "only stage lines leave the log");
        let snap = json!({"value": value, "at": 2990, "error": null, "incomplete": false});
        let doc = build(&json!({"now": 3000, "period": 1800, "filters": {}, "agent_work": {"mac": snap, "fra": {"value": null, "at": null, "error": "log missing", "incomplete": false}}}));
        let _ = std::fs::remove_dir_all(&dir);
        let hosts = doc["hosts"].as_array().unwrap();
        assert_eq!((hosts[1]["host"].as_str(), hosts[1]["source"]["state"].as_str(), hosts[1]["records"].clone()), (Some("fra"), Some("unknown"), Value::Null));
        assert_eq!(hosts[0]["source"]["reason"], "1 unreadable stage records skipped");
        assert_eq!(doc["source"]["state"], "unknown", "a missing log is not ok");
        let rows = doc["rows"].as_array().unwrap();
        assert_eq!(rows.iter().map(|r| r["runtime"].as_str().unwrap()).collect::<Vec<_>>(), ["claude", "codex"], "slowest first");
        let c = &rows[0];
        assert_eq!((c["attempts"].as_u64(), c["tasks"].as_u64(), c["outcomes"]["failed"].as_u64()), (Some(1), Some(1), Some(1)), "record at 1000 s is outside the window");
        assert_eq!(c["stages"][0]["stage"], "build");
        assert_eq!(c["tokens"], json!({"sum": null, "known": 0, "of": 1, "reasons": ["no-usage"]}), "unreported tokens never read 0");
        let claim = c["stages"].as_array().unwrap().iter().find(|s| s["stage"] == "claim").unwrap();
        assert_eq!((claim["p95_ms"].as_f64(), claim["tokens"]["reasons"][0].as_str()), (Some(1.5), Some("no-model")));
        let x = &rows[1];
        assert_eq!((x["time_ms"].clone(), x["untimed"].as_u64(), x["stages"][0]["p95_ms"].clone()), (Value::Null, Some(1), Value::Null), "adopted stage has no duration");
        let mut html = String::new();
        render(&mut html, &json!({"agent_work": doc, "generated_at": 3000}));
        assert!(html.contains("unknown: no-usage") && html.contains("none (no model)") && html.contains("1.0 min") && html.contains("log missing"), "{html}");
        assert!(!html.contains(">0<"), "{html}");
        let mut none = String::new();
        render(&mut none, &json!({"agent_work": build(&json!({"now": 1, "period": 1800}))}));
        assert!(none.is_empty(), "unconfigured: no empty panel (D19)");
    }
}
