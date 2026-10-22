//! Agent work (dashq.md D21): taskq attempt stage records (taskq.md § 4 Profiling) per host and runtime, and per
//! attempt. Records come from stored TaskQ telemetry blocks (#182) and, for hosts without telemetry or during the
//! cutover, from `taskq-stage` lines of a local `taskq.log`. Records hold fixed codes and numbers only; missing data
//! stays unknown.

use crate::page::esc;
use crate::source::clock;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

const PREFIX: &str = "taskq-stage ";
pub const STAGES: [&str; 7] = ["claim", "start", "build", "checks", "review", "ask", "close"];
const RUNTIMES: [&str; 4] = ["claude", "codex", "codex-app", "other"];
const OUTCOMES: [&str; 6] = ["ok", "ask", "retried", "failed", "cancelled", "unknown"];
const ERRORS: [&str; 11] = ["none", "http4xx", "http5xx", "timeout", "permission", "invalid", "other", "panic", "exit", "left-board", "unreadable"];
const UNKNOWN: [&str; 6] = ["no-model", "no-usage", "log-unreadable", "app-turn", "adopted", "open-stage"];
/// A log read, or the newest telemetry block, older than this is stale (D6); the service rereads logs every 60 s.
const LIMIT: u64 = 300;
/// Attempt rows listed per document, newest first (D21).
const ATTEMPTS: usize = crate::explain::CAP;

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

/// One telemetry line (coreq export row `{seq, id, received_ms, observation}`) as a `taskq-stage` record, the inverse
/// of taskq v4.6.32 `telemetry/semantic.rs` field names, plus `host` (else the installation's `label`), `installation`
/// and `seq` for coverage; a queue transition as `{transition, task, entry, from, prev_s}` (#184 queue waits); `None`
/// for other kinds. Codes are checked later like a log line's.
pub fn from_telemetry(l: &Value, label: &str, installation: &str) -> Option<Value> {
    let o = &l["observation"];
    (o["product"] == "taskq").then_some(())?;
    if o["kind"] == "transition" {
        return Some(json!({"transition": true, "task": o["task"], "entry": o["entry"], "from": o["from"], "prev_s": o["previous_seconds"]}));
    }
    (o["kind"] == "stage").then_some(())?;
    let mut r = json!({"stage": o["operation"], "task": o["task"], "attempt": o["attempt"], "rt": o["runtime"], "us": o["duration_us"],
        "outcome": o["outcome"], "error": o["error"], "at_ms": o["started_ms"], "project": o["project"],
        "host": o["host"].as_str().unwrap_or(label), "installation": installation, "seq": l["seq"]});
    for (k, f) in [("open", "open"), ("calls", "calls"), ("tokens", "tokens"), ("model", "model"), ("output_tokens", "output_tokens"), ("unknown", "usage_status")] {
        if !o[f].is_null() {
            r[k] = o[f].clone();
        }
    }
    Some(r)
}

fn valid(r: &Value) -> bool {
    let is = |k: &str, set: &[&str]| r[k].as_str().is_some_and(|s| set.contains(&s));
    // an open lifetime marker may lack its time (taskq: no clock); it then evidences nothing
    let at = r["at_ms"].is_u64() || (open(r) && r["at_ms"].is_null());
    is("stage", &STAGES) && is("rt", &RUNTIMES) && is("outcome", &OUTCOMES) && is("error", &ERRORS)
        && (r["unknown"].is_null() || is("unknown", &UNKNOWN)) && r["task"].is_u64() && r["attempt"].is_u64() && at
        && (r["us"].is_null() || r["us"].is_u64()) && ["calls", "tokens", "output_tokens"].iter().all(|k| r[*k].is_null() || r[*k].is_u64())
        && (r["model"].is_null() || r["model"].as_str().is_some_and(|m| crate::data::safe(m, 64, b"-._@")))
        && (r["project"].is_null() || r["project"].is_string())
}

fn open(r: &Value) -> bool {
    r["open"] == true
}

/// The same stage record read from a log and from telemetry counts once (#182 cutover).
fn key(r: &Value) -> String {
    format!("{}|{}|{}|{}|{}|{}", r["task"], r["attempt"], r["stage"], r["rt"], r["at_ms"], open(r))
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

/// Model ids of `v` with their record counts, `known` of `of` and the reasons of the rest (taskq's codes).
fn models(v: &[&Value]) -> Value {
    let reasons: BTreeSet<&str> = v.iter().filter(|r| r["model"].is_null()).map(|r| r["unknown"].as_str().unwrap_or("not-reported")).collect();
    let ids = counts(v.iter().filter_map(|r| r["model"].as_str()));
    json!({"ids": ids, "known": ids.values().sum::<u64>(), "of": v.len(), "reasons": reasons})
}

/// Estimates need input split by cache; taskq stage tokens are context tokens with cache included (#184).
const NO_SPLIT: &str = "context tokens not split by cache";

fn counts<'a>(it: impl Iterator<Item = &'a str>) -> BTreeMap<&'a str, u64> {
    let mut m = BTreeMap::new();
    for k in it {
        *m.entry(k).or_insert(0) += 1;
    }
    m
}

/// Microseconds covered by the union of the records' intervals [end − duration, end]: overlapping or nested stages
/// count once (A3). Callers pass timed, closed records only.
fn union_us(v: &[&Value]) -> u64 {
    let mut iv: Vec<(u64, u64)> = v.iter().filter_map(|r| {
        let end = r["at_ms"].as_u64()?.saturating_mul(1000);
        Some((end.saturating_sub(r["us"].as_u64()?), end))
    }).collect();
    iv.sort_unstable();
    let (mut total, mut cur): (u64, Option<(u64, u64)>) = (0, None);
    for (a, b) in iv {
        cur = match cur {
            Some((x, y)) if a <= y => Some((x, y.max(b))),
            Some((x, y)) => {
                total += y - x;
                Some((a, b))
            }
            None => Some((a, b)),
        };
    }
    total + cur.map_or(0, |(x, y)| y - x)
}

/// Coverage of one attempt's records (D21): `complete` with no reason, else the state and reason its elapsed time
/// is unknown. `holes` are each installation's telemetry seq ranges no stored block covers.
fn coverage(recs: &[&Value], holes: &BTreeMap<String, Vec<(u64, u64)>>) -> (&'static str, String) {
    let at = |r: &&Value| r["at_ms"].as_u64();
    let term = recs.iter().filter(|r| !open(r) && matches!(r["stage"].as_str(), Some("build" | "review"))).filter_map(at).max();
    let opened = recs.iter().filter(|r| open(r)).filter_map(at).max();
    match (term, opened) {
        (None, Some(_)) => return ("open", "open-stage".into()),
        (None, None) => return ("open", "no terminal record".into()),
        (Some(t), Some(o)) if o > t => return ("open", "open-stage".into()),
        _ => {}
    }
    if let Some(r) = recs.iter().find(|r| !open(r) && r["us"].is_null()) {
        return match r["unknown"].as_str() {
            Some("adopted") => ("adopted", "adopted".into()),
            why => ("untimed", why.unwrap_or("not-recorded").into()),
        };
    }
    let mut span: BTreeMap<&str, (u64, u64)> = BTreeMap::new();
    for r in recs {
        if let (Some(i), Some(s)) = (r["installation"].as_str(), r["seq"].as_u64()) {
            let e = span.entry(i).or_insert((s, s));
            *e = (e.0.min(s), e.1.max(s));
        }
    }
    // ponytail: only the listed holes (at most 32 per installation) are checked
    if span.iter().any(|(i, (lo, hi))| holes.get(*i).into_iter().flatten().any(|(a, b)| a > lo && b < hi)) {
        return ("partial", "rows not received".into());
    }
    if !recs.iter().any(|r| open(r) || r["stage"] == "claim") {
        return ("partial", "attempt start not retained".into());
    }
    ("complete", String::new())
}

#[derive(Default)]
struct Host {
    src: Value,
    provenance: BTreeSet<&'static str>,
    records: Vec<Value>,
    keys: HashSet<String>,
    corrupt: u64,
    /// No value read at all (a missing log, or telemetry that could not be read).
    unread: bool,
    /// Telemetry rows not received (seq holes) or pruned by DashQ: earlier or inner rows may be missing.
    holes: u64,
    lossy: bool,
}

impl Host {
    fn add(&mut self, host: &str, mut r: Value) {
        if r["host"].is_null() {
            r["host"] = json!(host);
        }
        if !valid(&r) {
            self.corrupt += 1;
        } else if self.keys.insert(key(&r)) {
            self.records.push(r);
        }
    }
}

/// `/data.agent_work` from `inp.agent_telemetry` (D11 telemetry stage records per TaskQ installation) and
/// `inp.agent_work` `{host: snap}` of local logs (both absent: not configured), the period and host filter.
pub fn build(inp: &Value) -> Value {
    let (now, period) = (inp["now"].as_u64().unwrap_or(0), inp["period"].as_u64().unwrap_or(1800));
    let from = now.saturating_sub(period);
    let tele = inp["agent_telemetry"].as_array().filter(|t| !t.is_empty());
    let logs = inp["agent_work"].as_object();
    if tele.is_none() && logs.is_none() {
        return json!({"source": crate::data::source(&Value::Null, now, LIMIT, None), "window_s": period, "from": from, "to": now, "hosts": [], "rows": [], "attempts": [], "attempts_total": 0});
    }
    let want = |h: &str| inp["filters"]["host"].as_str().is_none_or(|x| x == h);
    // hosts in source order (telemetry rows, then configured logs)
    let mut acc: Vec<(String, Host)> = vec![];
    fn entry<'a>(acc: &'a mut Vec<(String, Host)>, host: &str) -> &'a mut Host {
        let i = acc.iter().position(|(h, _)| h == host).unwrap_or_else(|| {
            acc.push((host.into(), Host::default()));
            acc.len() - 1
        });
        &mut acc[i].1
    }
    let mut holes: BTreeMap<String, Vec<(u64, u64)>> = BTreeMap::new();
    // time in `ready` before a worker spawn: taskq's spawn entry id is the attempt id (taskq.md § 2)
    let mut waits: BTreeMap<(u64, u64), u64> = BTreeMap::new();
    let rank = |s: &Value| ["ok", "incomplete", "stale", "unconfirmed", "unknown"].iter().position(|x| s["state"] == *x).unwrap_or(4);
    for t in tele.into_iter().flatten() {
        let tj = &t["telemetry"];
        let inst = t["installation"].as_str().unwrap_or("");
        holes.insert(inst.into(), tj["holes"].as_array().into_iter().flatten().filter_map(|h| Some((h["from_seq"].as_u64()?, h["to_seq"].as_u64()?))).collect());
        let src = match (t["error"].as_str(), tj["state"].as_str()) {
            (Some(e), _) => json!({"state": "unknown", "at": null, "age_s": null, "reason": format!("read failed ({e})"), "incomplete": false}),
            (None, Some("ok")) => crate::data::source(&json!({"value": true, "at": tj["last_synced"]}), now, LIMIT, None),
            _ => json!({"state": "unknown", "at": null, "age_s": null, "reason": tj["reason"].as_str().unwrap_or("no telemetry block yet"), "incomplete": false}),
        };
        let (moves, records): (Vec<Value>, Vec<Value>) = t["records"].as_array().cloned().unwrap_or_default().into_iter().partition(|r| r["transition"] == true);
        for m in moves.iter().filter(|m| m["from"] == "ready") {
            if let (Some(k), Some(e), Some(s)) = (m["task"].as_u64(), m["entry"].as_u64(), m["prev_s"].as_u64()) {
                waits.insert((k, e), s);
            }
        }
        let mut names: BTreeSet<String> = records.iter().filter_map(|r| r["host"].as_str().map(String::from)).collect();
        if names.is_empty() {
            names.insert(t["label"].as_str().unwrap_or(inst).into());
        }
        for name in names.into_iter().filter(|h| want(h)) {
            let h = entry(&mut acc, &name);
            // one host fed by several installations reads as the worst of them
            if !h.provenance.contains("telemetry") || rank(&src) > rank(&h.src) {
                h.src = src.clone();
            }
            h.provenance.insert("telemetry");
            h.unread |= !t["error"].is_null() || tj["state"] != "ok";
            h.holes += tj["hole_rows"].as_u64().unwrap_or(0);
            h.lossy |= tj["pruned"].as_array().is_some_and(|p| !p.is_empty());
            for r in records.iter().filter(|r| r["host"] == name.as_str()) {
                h.add(&name, r.clone());
            }
        }
    }
    for (host, snap) in logs.into_iter().flatten().filter(|(h, _)| want(h)) {
        let h = entry(&mut acc, host);
        // a host with telemetry keeps the telemetry's state; its log only fills the cutover, deduplicated
        if !h.provenance.contains("telemetry") {
            h.src = crate::data::source(snap, now, LIMIT, None);
            h.unread = snap["value"].is_null();
            h.lossy = true;
        }
        h.provenance.insert("log");
        h.corrupt += snap["value"]["corrupt"].as_u64().unwrap_or(0);
        for r in snap["value"]["records"].as_array().into_iter().flatten() {
            h.add(host, r.clone());
        }
    }
    let (mut hosts, mut rows) = (vec![], vec![]);
    let mut all: Vec<&Value> = vec![];
    for (host, h) in &acc {
        let mut src = h.src.clone();
        let retained = h.records.iter().filter_map(|r| r["at_ms"].as_u64()).min().map(|m| m / 1000);
        let label = if h.provenance.contains("telemetry") { "telemetry" } else { "log" };
        // a bounded log or pruned telemetry: records before the oldest kept one are unknown, never absent
        let coverage = match retained {
            _ if h.unread => Value::Null,
            None => json!(format!("no stage records in {label}")),
            Some(r) if r > from && h.lossy => json!(format!("{label} retains stages from {}; earlier unknown", clock(r, now))),
            Some(_) => Value::Null,
        };
        // nothing observed (no profiling taskq, or no attempt yet): unknown, never 0; skipped records, rows not
        // received or a partly covered window make the counts a lower bound: incomplete
        let gaps: Vec<String> = (h.corrupt > 0).then(|| format!("{} unreadable stage records skipped", h.corrupt)).into_iter()
            .chain((h.holes > 0).then(|| format!("{} telemetry rows not received", h.holes)))
            .chain(coverage.as_str().map(String::from)).collect();
        let observed = !h.unread && retained.is_some();
        if !h.unread && !gaps.is_empty() {
            src["incomplete"] = json!(h.corrupt > 0 || h.holes > 0 || observed);
            if src["state"] == "ok" {
                src["state"] = json!(if observed { "incomplete" } else { "unknown" });
                src["reason"] = json!(gaps.join("; "));
            }
        }
        let win: Vec<&Value> = h.records.iter().filter(|r| !open(r) && r["at_ms"].as_u64().is_some_and(|m| (from..=now).contains(&(m / 1000)))).collect();
        hosts.push(json!({"host": host, "source": src, "provenance": h.provenance, "records": observed.then_some(win.len()), "retained_from": retained, "coverage": coverage}));
        all.extend(h.records.iter());
    }
    // attempts across hosts: one TaskQ attempt is one row, whichever hosts observed it
    let mut by: BTreeMap<(u64, u64), Vec<&Value>> = BTreeMap::new();
    for r in all.iter().copied().filter(|r| r["attempt"].as_u64().is_some_and(|a| a > 0)) {
        by.entry((r["task"].as_u64().unwrap_or(0), r["attempt"].as_u64().unwrap_or(0))).or_default().push(r);
    }
    let cov: BTreeMap<(u64, u64), (&str, String)> = by.iter().map(|(k, v)| (*k, coverage(v, &holes))).collect();
    for (host, h) in &acc {
        for rt in RUNTIMES {
            let mine: Vec<&Value> = h.records.iter().filter(|r| r["rt"] == rt && !open(r) && r["at_ms"].as_u64().is_some_and(|m| (from..=now).contains(&(m / 1000)))).collect();
            if !mine.is_empty() {
                rows.push(row(host, rt, &mine, &by, &cov));
            }
        }
    }
    // worst first: the slowest stage p95 of each row; untimed rows last
    let worst = |r: &Value| r["stages"][0]["p95_ms"].as_f64().unwrap_or(-1.0);
    rows.sort_by(|a, b| worst(b).total_cmp(&worst(a)));
    let projects: Vec<&str> = inp["projects"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut attempts: Vec<(u64, Value)> = by
        .iter()
        .filter(|(_, v)| v.iter().any(|r| r["at_ms"].as_u64().is_some_and(|m| (from..=now).contains(&(m / 1000)))))
        .map(|(k, v)| (v.iter().filter_map(|r| r["at_ms"].as_u64()).max().unwrap_or(0), attempt(*k, v, &cov[k], &projects, waits.get(k).copied())))
        .collect();
    attempts.sort_by_key(|a| std::cmp::Reverse(a.0));
    let total = attempts.len();
    // D23: `cursor` pages the newest-first list
    let cursor = inp["filters"]["cursor"].as_str().and_then(|c| c.parse().ok()).unwrap_or(0);
    let attempts: Vec<Value> = attempts.into_iter().skip(cursor).take(ATTEMPTS).map(|(_, a)| a).collect();
    // the worst host names the block's state; one missing source never reads ok
    let bad = ["unknown", "unconfirmed", "stale", "incomplete"].iter().find_map(|st| hosts.iter().find(|h| h["source"]["state"] == *st));
    let mut src = match (hosts.first(), bad) {
        (None, _) => json!({"state": "unknown", "at": null, "age_s": null, "reason": "no stage source for this host filter", "incomplete": false}),
        (_, Some(h)) => {
            let mut s = h["source"].clone();
            s["reason"] = json!(format!("{}: {}", h["host"].as_str().unwrap_or(""), s["reason"].as_str().unwrap_or("")));
            s
        }
        (Some(h), None) => h["source"].clone(),
    };
    // one host with skipped or uncovered records makes the block's counts a lower bound too
    src["incomplete"] = json!(hosts.iter().any(|h| h["source"]["incomplete"] == true));
    json!({"source": src, "window_s": period, "from": from, "to": now, "hosts": hosts, "rows": rows, "attempts": attempts, "attempts_total": total})
}

fn attempt((task, id): (u64, u64), v: &[&Value], (state, why): &(&str, String), projects: &[&str], wait: Option<u64>) -> Value {
    let project = v.iter().find_map(|r| r["project"].as_str());
    let closed: Vec<&Value> = v.iter().copied().filter(|r| !open(r)).collect();
    let models: Vec<&Value> = closed.iter().copied().filter(|r| matches!(r["stage"].as_str(), Some("build" | "review"))).collect();
    let outcome = models.iter().max_by_key(|r| r["at_ms"].as_u64()).map(|r| r["outcome"].clone());
    let set = |k: &str| v.iter().filter_map(|r| r[k].as_str()).collect::<BTreeSet<_>>();
    let complete = *state == "complete";
    // a reviewer is spawned inside `review`, so its wait is no state transition
    let wait_why = if v.iter().any(|r| r["stage"] == "review") { "reviewer wait not recorded" } else { "ready transition not retained" };
    json!({
        "project": project, "task": task, "attempt": id,
        // only a configured board names an issue; an attempt without a project stays unlinked
        "link": project.filter(|p| projects.contains(p)).map(|p| format!("https://github.com/{p}/issues/{task}")),
        "hosts": set("host"), "runtimes": set("rt"),
        "stages": STAGES.iter().filter(|s| closed.iter().any(|r| r["stage"] == **s)).collect::<Vec<_>>(),
        "outcome": outcome, "state": state, "reason": (!why.is_empty()).then_some(why),
        // wall time (overlaps once), summed stage execution (overlaps added) and the queue wait before it (A2)
        "elapsed_ms": complete.then(|| ms(union_us(&closed))),
        "execution_ms": complete.then(|| ms(closed.iter().filter_map(|r| r["us"].as_u64()).sum())),
        "queue_wait_s": wait, "queue_wait_reason": wait.is_none().then_some(wait_why),
        "tokens": usage(&models, "tokens"), "calls": usage(&models, "calls"), "output_tokens": usage(&models, "output_tokens"), "models": self::models(&models),
        "estimate": {"amounts": null, "reason": NO_SPLIT},
    })
}

fn row(host: &str, rt: &str, v: &[&Value], by: &BTreeMap<(u64, u64), Vec<&Value>>, cov: &BTreeMap<(u64, u64), (&str, String)>) -> Value {
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
    // elapsed time of this row's part of each complete attempt; nested stages count once (A3)
    let keys: BTreeSet<(u64, u64)> = v.iter().filter_map(|r| Some((r["task"].as_u64()?, r["attempt"].as_u64().filter(|a| *a > 0)?))).collect();
    let (mut known, mut total, mut reasons) = (0, 0u64, BTreeSet::new());
    for k in &keys {
        match cov.get(k) {
            Some(("complete", _)) => {
                known += 1;
                let mine: Vec<&Value> = by[k].iter().copied().filter(|r| !open(r) && r["host"] == host && r["rt"] == rt).collect();
                total += union_us(&mine);
            }
            Some((state, why)) => {
                reasons.insert(if why.is_empty() { state.to_string() } else { why.clone() });
            }
            None => {}
        }
    }
    json!({
        "host": host, "runtime": rt, "attempts": attempts.len(), "tasks": tasks.len(),
        // the attempt's hand-in outcome lives on its build/review stage (taskq.md § 4 Profiling)
        "outcomes": counts(models.iter().filter_map(|r| r["outcome"].as_str())),
        "time_ms": (known > 0).then(|| ms(total)),
        "elapsed": {"known": known, "of": keys.len(), "reasons": reasons},
        "untimed": v.iter().filter(|r| r["us"].is_null()).count(),
        "tokens": usage(&models, "tokens"), "calls": usage(&models, "calls"), "output_tokens": usage(&models, "output_tokens"), "models": self::models(&models),
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

fn unknown(why: &str) -> String {
    format!("<span class=\"unknown\" title=\"unknown: {0}\">—</span> <small>unknown: {0}</small>", esc(why))
}

/// Model ids with counts; ids not reported read `k of n known` with taskq's reasons.
fn model_cell(m: &Value) -> String {
    let reasons: Vec<&str> = m["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    match (m["ids"].as_object().filter(|o| !o.is_empty()), m["known"] == m["of"]) {
        (None, _) if reasons == ["no-model"] => "none (no model)".into(),
        (None, _) => unknown(&reasons.join(", ")),
        (Some(_), true) => map(&m["ids"]),
        (Some(_), false) => format!("{} <small>({} of {} known; {})</small>", map(&m["ids"]), m["known"], m["of"], esc(&reasons.join(", "))),
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
    let _ = writeln!(w, "<section id=\"agentwork\">\n<h2>Agent work</h2>\n<p class=\"note\">taskq attempt stages ended in the period, per host and runtime; slowest stage (p95) first. Wall time is each complete attempt's elapsed time (nested stages count once); summed execution adds overlapping stages; queue wait is the time in ready before the worker started. Budgets unknown. Model use counts build and review only.</p>");
    for h in a["hosts"].as_array().into_iter().flatten() {
        let s = &h["source"];
        let state = s["state"].as_str().unwrap_or("unknown");
        let mut notes: Vec<String> = s["reason"].as_str().map(esc).into_iter().collect();
        notes.extend(h["coverage"].as_str().filter(|c| !s["reason"].as_str().is_some_and(|r| r.contains(c))).map(esc));
        let _ = writeln!(w, "<p class=\"note\">{} {} · {} stage records{}{}</p>", esc(h["host"].as_str().unwrap_or("")), crate::page::badge(state, state), h["records"].as_u64().map_or("—".into(), |r| r.to_string()),
            s["at"].as_u64().map(|t| format!(" · read {}", clock(t, now))).unwrap_or_default(), if notes.is_empty() { String::new() } else { format!(" · {}", notes.join(" · ")) });
    }
    for r in a["rows"].as_array().into_iter().flatten() {
        let untimed = r["untimed"].as_u64().filter(|u| *u > 0).map(|u| format!(" ({u} untimed)")).unwrap_or_default();
        let e = &r["elapsed"];
        let reasons: Vec<&str> = e["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        let why = esc(&reasons.join(", "));
        let time = match r["time_ms"].as_f64() {
            Some(_) => n(&r["time_ms"]),
            None => format!("<span class=\"unknown\" title=\"unknown: {why}\">—</span>"),
        };
        let untimed = if e["known"] == e["of"] { untimed } else { format!("{untimed} <small>({} of {} attempts complete; {why})</small>", e["known"], e["of"]) };
        let _ = writeln!(w, "<h3>{} · {}</h3>\n<p>{} · {} · outcomes {} · time {}{untimed} · context tokens {} · output tokens {} · model calls {} · models {}</p>",
            esc(r["host"].as_str().unwrap_or("")), esc(r["runtime"].as_str().unwrap_or("")), plural(&r["attempts"], "attempt"), plural(&r["tasks"], "task"), map(&r["outcomes"]), time, use_cell(&r["tokens"]), use_cell(&r["output_tokens"]), use_cell(&r["calls"]), model_cell(&r["models"]));
        let _ = writeln!(w, "<table>\n<tr><th>Stage</th><th>Count</th><th>p50 · p95 · max</th><th>Total time</th><th>Outcomes</th><th>Errors</th><th>Model calls</th><th>Context tokens</th></tr>");
        for s in r["stages"].as_array().into_iter().flatten() {
            let timed = if s["timed"] == s["count"] { String::new() } else { format!(" <small>({} of {} timed)</small>", s["timed"], s["count"]) };
            let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{} · {} · {}{timed}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(s["stage"].as_str().unwrap_or("")), s["count"], n(&s["p50_ms"]), n(&s["p95_ms"]), n(&s["max_ms"]), n(&s["total_ms"]), map(&s["outcomes"]), map(&s["errors"]), use_cell(&s["calls"]), use_cell(&s["tokens"]));
        }
        let _ = writeln!(w, "</table>");
    }
    let attempts = a["attempts"].as_array().filter(|x| !x.is_empty());
    if let Some(list) = attempts {
        let more = a["attempts_total"].as_u64().filter(|t| *t > list.len() as u64).map(|t| format!(", newest {} of {t}", list.len())).unwrap_or_default();
        let _ = writeln!(w, "<h3>Attempts{more}</h3>\n<table>\n<tr><th>Task</th><th>Attempt</th><th>Hosts</th><th>Runtime</th><th>Stages</th><th>Outcome</th><th>Wall time</th><th>Summed execution</th><th>Queue wait</th><th>Model calls</th><th>Context tokens</th><th>Output tokens</th><th>Models</th><th>API-equivalent estimate</th></tr>");
        let join = |v: &Value| v.as_array().into_iter().flatten().filter_map(Value::as_str).map(esc).collect::<Vec<_>>().join(", ");
        for t in list {
            let name = format!("{}#{}", t["project"].as_str().unwrap_or(""), t["task"]);
            let task = match t["link"].as_str() {
                Some(l) => format!("<a href=\"{}\">{}</a>", esc(l), esc(&name)),
                None => esc(&name),
            };
            // elapsed only for complete coverage; otherwise the state and taskq's reason, never a partial sum
            let elapsed = match t["elapsed_ms"].as_f64() {
                Some(_) => n(&t["elapsed_ms"]),
                None => {
                    let (state, reason) = (t["state"].as_str().unwrap_or("unknown"), t["reason"].as_str().unwrap_or(""));
                    let why = if reason.is_empty() || reason == state { state.to_string() } else { format!("{state}: {reason}") };
                    format!("<span class=\"unknown\" title=\"unknown: {0}\">—</span> <small>{0}</small>", esc(&why))
                }
            };
            let execution = if t["execution_ms"].is_null() { unknown(t["reason"].as_str().or(t["state"].as_str()).unwrap_or("unknown")) } else { n(&t["execution_ms"]) };
            let wait = match t["queue_wait_s"].as_f64() {
                Some(s) => n(&json!(s * 1000.0)),
                None => unknown(t["queue_wait_reason"].as_str().unwrap_or("unknown")),
            };
            let _ = writeln!(w, "<tr><td>{task}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{elapsed}</td><td>{execution}</td><td>{wait}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                t["attempt"], join(&t["hosts"]), join(&t["runtimes"]), join(&t["stages"]), esc(t["outcome"].as_str().unwrap_or("—")), use_cell(&t["calls"]), use_cell(&t["tokens"]),
                use_cell(&t["output_tokens"]), model_cell(&t["models"]), unknown(t["estimate"]["reason"].as_str().unwrap_or("unknown")));
        }
        let _ = writeln!(w, "</table>");
    }
    if a["rows"].as_array().is_none_or(|r| r.is_empty()) && attempts.is_none() {
        // a zero is shown only when every log covers the period; else what is missing reads unknown
        let line = match (a["source"]["state"].as_str(), a["source"]["reason"].as_str()) {
            (Some("ok"), _) => "No attempt stages in this period.".to_string(),
            (_, why) => format!("Attempt stages in this period unknown: {}.", esc(why.unwrap_or("no reason"))),
        };
        let _ = writeln!(w, "<p class=\"note\">{line}</p>");
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
        let empty = json!({"value": {"records": [], "corrupt": 0}, "at": 2990, "error": null, "incomplete": false});
        let doc = build(&json!({"now": 3000, "period": 1800, "filters": {}, "agent_work": {"mac": snap, "fra": {"value": null, "at": null, "error": "log missing", "incomplete": false}, "win": empty}}));
        let _ = std::fs::remove_dir_all(&dir);
        let hosts = doc["hosts"].as_array().unwrap();
        assert_eq!((hosts[1]["host"].as_str(), hosts[1]["source"]["state"].as_str(), hosts[1]["records"].clone()), (Some("fra"), Some("unknown"), Value::Null));
        assert_eq!((hosts[0]["source"]["state"].as_str(), hosts[0]["source"]["reason"].as_str(), hosts[0]["source"]["incomplete"].as_bool()), (Some("incomplete"), Some("1 unreadable stage records skipped"), Some(true)));
        let w = &hosts[2]["source"];
        assert_eq!((w["state"].as_str(), w["reason"].as_str(), hosts[2]["records"].clone()), (Some("unknown"), Some("no stage records in log"), Value::Null), "an empty log observed nothing: unknown, never ok/0");
        assert_eq!((doc["source"]["state"].as_str(), doc["source"]["incomplete"].as_bool()), (Some("unknown"), Some(true)), "a missing log is not ok; skipped records leave the block incomplete");
        let day = build(&json!({"now": 3000, "period": 86400, "filters": {"host": "mac"}, "agent_work": {"mac": snap}}));
        assert_eq!((day["source"]["state"].as_str(), day["source"]["incomplete"].as_bool()), (Some("incomplete"), Some(true)), "a window the log covers only in part is a lower bound");
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
        let mut nothing = String::new();
        render(&mut nothing, &json!({"agent_work": build(&json!({"now": 3000, "period": 1800, "filters": {}, "agent_work": {"win": empty}}))}));
        assert!(nothing.contains("Attempt stages in this period unknown: win: no stage records in log.") && !nothing.contains("No attempt stages"), "{nothing}");
        let mut none = String::new();
        render(&mut none, &json!({"agent_work": build(&json!({"now": 1, "period": 1800}))}));
        assert!(none.is_empty(), "unconfigured: no empty panel (D19)");
    }
}
