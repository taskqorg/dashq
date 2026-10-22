//! The `/data` document (dashq.md D6, D7) from source snapshots: pure, so a fixture pins it.
//!
//! Input: `{now, period, filters, projects, serverq_host, langfuse, boards: {repo: snap}, releases: {repo: snap}, serverq}`;
//! a snap is `{value, at, error, incomplete}` (source.rs).

use crate::source::{clock, iso_secs};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PERIODS: [u64; 3] = [1800, 7200, 86400];
pub const AGENTS: [(&str, &str); 3] = [("codex", "Codex"), ("claude-code", "Claude Code"), ("hermes", "Hermes")];
/// Open TaskQ states in counter order (TaskQ task-counters.md); `waiting` is shown as Blocked, Closed follows.
pub const COUNTS: [&str; 7] = ["doing", "review", "ask", "failed", "ready", "waiting", "later"];
const SNAPSHOT_LIMIT: u64 = 600;
const HEARTBEAT_TTL: u64 = 300;
const GITHUB_LIMIT: u64 = 180;
const SERVERQ_LIMIT: u64 = 7200;
pub const AQ_HEARTBEAT_LIMIT: u64 = 7200;
const USAGE: [&str; 8] = ["input", "output", "total", "input_cached_tokens", "cache_read_input_tokens", "input_cache_creation_1h", "input_cache_creation_5m", "output_reasoning_tokens"];
pub(crate) fn safe(s: &str, max: usize, extra: &[u8]) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || extra.contains(&b))
}

pub fn safe_id(s: &str) -> bool {
    safe(s, 128, b"_.:-")
}

fn tag<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row["tags"].as_array()?.iter().filter_map(Value::as_str).find_map(|t| t.strip_prefix(key)?.strip_prefix(':'))
}

fn attr<'a>(row: &'a Value, key: &str) -> &'a Value {
    &row["metadata"][format!("attributes.agent_context.{key}")]
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|n| n.is_finite() && *n >= 0.0)
}

/// `MAJOR.MINOR.PATCH[-pre]` (dev builds such as `4.2.0-dev.1791961334`); build metadata and other forms read unknown.
pub fn semver(v: &Value) -> Value {
    let ok = |s: &str| {
        let (core, pre) = s.split_once('-').map_or((s, None), |(c, p)| (c, Some(p)));
        core.split('.').count() == 3
            && core.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
            && pre.is_none_or(|p| p.split('.').all(|i| !i.is_empty() && i.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')))
    };
    v.as_str().filter(|s| s.len() <= 64 && ok(s)).map_or(Value::Null, |s| json!(s))
}

fn trace(row: &Value) -> Value {
    row["traceId"].as_str().filter(|t| safe(t, 128, b"_-")).map_or(Value::Null, |t| json!(t))
}

pub(crate) fn row_at(row: &Value) -> Option<u64> {
    row["startTime"].as_str().and_then(iso_secs)
}

/// `(host id, agent)`; host id is `<host>/<runtime>`.
fn identity(row: &Value) -> (Option<String>, Option<&str>) {
    let host = tag(row, "host").or(attr(row, "host").as_str());
    let runtime = tag(row, "runtime").or(attr(row, "runtime").as_str()).unwrap_or("native");
    let agent = tag(row, "agent").or(attr(row, "agent").as_str());
    let id = host.filter(|h| safe(h, 64, b"_-") && matches!(runtime, "native" | "wsl")).map(|h| format!("{h}/{runtime}"));
    (id, agent)
}

pub(crate) fn aq<'a>(row: &'a Value, key: &str) -> &'a Value {
    &row["metadata"][format!("attributes.{key}")]
}

/// `(installation, host id)` of an agentq event; the host id joins the legacy `<host>/<runtime>`, so one host is one row.
pub(crate) fn aq_identity(row: &Value) -> Option<(String, String)> {
    let inst = aq(row, "agentq.installation").as_str().filter(|i| safe_id(i))?;
    let host = aq(row, "host.name").as_str().filter(|h| safe(h, 64, b"_.-"))?;
    let runtime = if aq(row, "agentq.runtime") == "wsl" { "wsl" } else { "native" };
    Some((inst.into(), format!("{host}/{runtime}")))
}

fn agent_name(a: &str) -> Option<&'static str> {
    AGENTS.iter().find(|(k, _)| *k == a).map(|(_, n)| *n)
}

/// `period=&host=&agent=&session=` (D7): the period and the filters, or `Err` for 400.
pub fn query(q: &str) -> Result<(u64, Value), String> {
    let (mut period, mut f) = (1800, json!({}));
    for pair in q.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = pair.split_once('=').ok_or("bad parameter")?;
        let v = crate::source::dec(v).ok_or("bad parameter")?;
        let ok = match k {
            "period" => {
                period = v.parse().map_err(|_| "bad period")?;
                PERIODS.contains(&period)
            }
            "host" => safe(v.split_once('/').map_or(v.as_str(), |(h, r)| if matches!(r, "native" | "wsl") { h } else { "" }), 64, b"_.-"),
            "agent" => agent_name(&v).is_some(),
            "session" => safe_id(&v),
            _ => false,
        };
        if !ok || (k != "period" && f.get(k).is_some()) {
            return Err(format!("bad {k}"));
        }
        if k != "period" {
            f[k] = json!(v);
        }
    }
    Ok((period, f))
}

const RANK: [&str; 5] = ["ok", "incomplete", "stale", "unconfirmed", "unknown"];

/// `{state, at, age_s, reason, incomplete}` of one snap (D6); `data_at` overrides the read time for freshness.
pub fn source(snap: &Value, now: u64, limit: u64, data_at: Option<u64>) -> Value {
    let at = data_at.or(snap["at"].as_u64());
    let age = at.map(|a| now.saturating_sub(a));
    let incomplete = snap["incomplete"].as_bool().unwrap_or(false);
    let (state, reason) = match (snap["value"].is_null(), snap["error"].as_str()) {
        (true, Some(e)) if snap["at"].is_null() => ("unknown", format!("read failed ({e})")),
        (true, None) if snap.is_null() => ("unknown", "not configured".into()),
        (true, None) if snap["at"].is_null() => ("unknown", "no value yet".into()),
        (_, Some(e)) => ("unconfirmed", format!("read failed ({e}), showing {}", at.map_or("—".into(), |a| clock(a, now)))),
        _ if age.is_some_and(|a| a > limit) => ("stale", format!("data is {} old (limit {})", dur(age.unwrap()), dur(limit))),
        _ if incomplete => ("incomplete", "capped query".into()),
        _ => ("ok", String::new()),
    };
    json!({"state": state, "at": at, "age_s": age, "reason": if reason.is_empty() { Value::Null } else { json!(reason) }, "incomplete": incomplete})
}

pub fn dur(s: u64) -> String {
    match s {
        0..120 => format!("{s}s"),
        120..7200 => format!("{}m", s / 60),
        7200..172_800 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

fn worst<'a>(states: impl Iterator<Item = (String, &'a Value)>, now: u64) -> Value {
    let mut out = json!({"state": "ok", "at": null, "age_s": null, "reason": null, "incomplete": false});
    let mut reasons = Vec::new();
    for (name, s) in states {
        let r = |x: &Value| RANK.iter().position(|k| x["state"] == *k).unwrap_or(4);
        if r(s) > r(&out) {
            out["state"] = s["state"].clone();
        }
        if let Some(a) = s["at"].as_u64()
            && out["at"].as_u64().is_none_or(|o| a < o)
        {
            out["at"] = json!(a);
            out["age_s"] = json!(now.saturating_sub(a));
        }
        out["incomplete"] = json!(out["incomplete"] == true || s["incomplete"] == true);
        if let Some(t) = s["reason"].as_str() {
            reasons.push(format!("{name}: {t}"));
        }
    }
    if !reasons.is_empty() {
        out["reason"] = json!(reasons.join("; "));
    }
    out
}

/// One Hosts row (hosts.md H1-H7): freshness from the row's agentq heartbeat; `reason` names a row that joins no
/// machine (H2).
fn host_row(id: &str) -> Value {
    json!({
        "id": id,
        "reason": null,
        "freshness": {"state": "unknown", "at": null, "age_s": null, "reason": "no agentq heartbeat"},
        "taskq": {"observations": [], "version": null, "latest": null, "behind": null, "state": "unknown", "age_s": null, "reason": "no heartbeat"},
        "agentq": {"installation": null, "running": null, "latest": null, "behind": null},
        "serverq": {"status": null, "state": "unknown", "age_s": null, "reason": "no serverq report for this host"},
    })
}

/// The Hosts row keyed case-insensitively: one machine is one row whatever the spelling (H1).
fn host_entry<'a>(hosts: &'a mut BTreeMap<String, Value>, id: &str) -> &'a mut Value {
    hosts.entry(id.to_ascii_lowercase()).or_insert_with(|| host_row(id))
}

/// Machines of the Hosts view (hosts.md H1): agentq `host.name`, compared case-insensitively and spelled as its newest
/// event; a `linux` installation on a machine that also runs a `windows` one is that machine's `wsl` kind.
struct Machines {
    names: HashMap<String, (String, u64)>,
    windows: HashSet<String>,
}

impl Machines {
    fn new<'a>(rows: impl Iterator<Item = &'a Value>, now: u64) -> Self {
        let mut m = Machines { names: HashMap::new(), windows: HashSet::new() };
        for r in rows {
            let (Some(name), Some(at)) = (aq(r, "host.name").as_str().filter(|h| safe(h, 64, b"_.-")), row_at(r).filter(|a| *a <= now + 60)) else { continue };
            let e = m.names.entry(name.to_ascii_lowercase()).or_insert_with(|| (name.into(), at));
            if at >= e.1 {
                *e = (name.into(), at);
            }
            if aq(r, "agentq.runtime") == "windows" {
                m.windows.insert(name.to_ascii_lowercase());
            }
        }
        m
    }

    fn known(&self, name: &str) -> bool {
        self.names.contains_key(&name.to_ascii_lowercase())
    }

    /// `<machine>/<kind>`, the machine spelled as agentq reports it.
    fn id(&self, name: &str, kind: &str) -> String {
        format!("{}/{kind}", self.names.get(&name.to_ascii_lowercase()).map_or(name, |(d, _)| d.as_str()))
    }

    /// A configured or legacy `<name>/<kind>` id in the same spelling.
    fn canon(&self, id: &str) -> String {
        let (name, kind) = id.split_once('/').unwrap_or((id, "native"));
        self.id(name, kind)
    }

    /// The row of an agentq event (H1).
    fn of(&self, r: &Value) -> Option<String> {
        let name = aq(r, "host.name").as_str().filter(|h| safe(h, 64, b"_.-"))?;
        let rt = aq(r, "agentq.runtime");
        Some(self.runtime(name, rt.as_str().unwrap_or("")))
    }

    fn runtime(&self, name: &str, rt: &str) -> String {
        let wsl = rt == "wsl" || (rt == "linux" && self.windows.contains(&name.to_ascii_lowercase()));
        self.id(name, if wsl { "wsl" } else { "native" })
    }
}

fn behind(latest: &Value, running: &Value) -> Value {
    match (latest.as_str(), running.as_str()) {
        (Some(l), Some(r)) => json!(coreq::update::newer(l, r)),
        _ => Value::Null,
    }
}

/// Counts stay integers in the JSON; sums with fractions (cost) stay floats.
fn add(m: &mut Value, k: &str, n: f64) {
    let v = m[k].as_f64().unwrap_or(0.0) + n;
    m[k] = if v.fract() == 0.0 && v < 9e15 { json!(v as u64) } else { json!(v) };
}

pub fn build(inp: &Value) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let period = inp["period"].as_u64().unwrap_or(1800);
    let f = &inp["filters"];
    let fhost = f["host"].as_str();
    let since = now.saturating_sub(period);
    let in_period = |row: &Value| row_at(row).filter(|a| *a >= since && *a <= now + 60);
    let matches = |row: &Value| {
        let (h, a) = identity(row);
        fhost.is_none_or(|x| h.as_deref() == Some(x)) && f["agent"].as_str().is_none_or(|x| a == Some(x)) && f["session"].as_str().is_none_or(|x| row["sessionId"] == x)
    };

    let lf = &inp["langfuse"];
    let lf_src = source(lf, now, SNAPSHOT_LIMIT, None);
    let rows = |k: &str| -> Vec<&Value> {
        // Langfuse v4 synthetic roots (`t-` ids) mirror real spans: count the real observations only
        lf["value"][k].as_array().map(|a| a.iter().filter(|r| !r["id"].as_str().unwrap_or("").starts_with("t-")).collect()).unwrap_or_default()
    };
    let capped = |k: &str| lf["value"]["truncated"].as_array().is_some_and(|t| t.iter().any(|x| x == k));
    let mut hosts: BTreeMap<String, Value> = BTreeMap::new();
    let (mut current, mut gaps, mut history) = (Vec::new(), Vec::new(), Vec::new());
    let mut heartbeats: BTreeMap<String, Value> = BTreeMap::new();
    const AQ_KEYS: [&str; 4] = ["agentq.installation", "agentq.run", "agentq.retrieval", "agentq.incident"];
    let machines = Machines::new(AQ_KEYS.iter().flat_map(|k| rows(k)), now);

    // Agents: model activity per host and agent (D8.4).
    let mut unattributed = 0;
    let mut keyed = |r: &Value| -> Option<(String, String)> {
        let (h, a) = identity(r);
        match (h, a.filter(|a| agent_name(a).is_some())) {
            (Some(h), Some(a)) => Some((h, a.to_string())),
            _ => {
                unattributed += 1;
                None
            }
        }
    };
    let mut activity: BTreeMap<(String, String), Value> = BTreeMap::new();
    for r in rows("activity").into_iter().filter(|r| matches(r) && in_period(r).is_some()) {
        let Some(k) = keyed(r) else { continue };
        let a = activity.entry(k.clone()).or_insert_with(|| json!({"host": k.0, "agent": k.1, "calls": 0, "errors": 0, "sessions": [], "last_at": 0, "trace_id": null, "models": {}}));
        add(a, "calls", 1.0);
        if r["level"] == "ERROR" {
            add(a, "errors", 1.0);
        }
        let at = in_period(r).unwrap();
        if a["last_at"].as_u64().is_some_and(|l| at >= l) {
            a["last_at"] = json!(at);
            a["trace_id"] = trace(r);
        }
        if let Some(s) = r["sessionId"].as_str()
            && !a["sessions"].as_array().unwrap().iter().any(|x| x == s)
        {
            a["sessions"].as_array_mut().unwrap().push(json!(s));
        }
        let model = r["model"].as_str().filter(|m| safe(m, 128, b"_.:/-")).unwrap_or("unknown");
        let m = a["models"].as_object_mut().unwrap().entry(model).or_insert_with(|| json!({"model": model, "calls": 0, "usage": {}, "cost": null, "cost_samples": 0, "trace_id": trace(r)}));
        add(m, "calls", 1.0);
        for k in USAGE {
            if let Some(n) = num(&r["usageDetails"][k]) {
                let u = &mut m["usage"][k];
                if u.is_null() {
                    *u = json!({"sum": 0, "samples": 0});
                }
                add(u, "sum", n);
                add(u, "samples", 1.0);
            }
        }
        if let Some(c) = num(&r["costDetails"]["total"]) {
            add(m, "cost", c);
            add(m, "cost_samples", 1.0);
        }
    }
    let activity: Vec<Value> = activity
        .into_values()
        .map(|mut a| {
            a["sessions"] = json!(a["sessions"].as_array().unwrap().len());
            a["models"] = Value::Array(a["models"].as_object().unwrap().values().cloned().collect());
            if a["errors"] != 0 {
                current.push(json!({"source": "Langfuse", "host": a["host"], "what": format!("{}: model call errors", agent_name(a["agent"].as_str().unwrap()).unwrap()), "since": null, "detail": format!("{} ERROR calls in the period.", a["errors"]), "trace_id": a["trace_id"]}));
            }
            a
        })
        .collect();
    // agentq fleet (D4.1): one row per installation; usage from runs and retrievals, incidents into the history.
    let aq_latest = inp["releases"]["taskqorg/agentq"]["value"].clone();
    let scoped = f["agent"].is_null() && f["session"].is_null();
    let mut fleet: BTreeMap<String, Value> = BTreeMap::new();
    let mut aq_seen = HashSet::new();
    for key in AQ_KEYS {
        for r in rows(key) {
            let (Some((inst, _)), Some(host), Some(at)) = (aq_identity(r), machines.of(r), row_at(r).filter(|a| *a <= now + 60)) else { continue };
            if fhost.is_some_and(|x| !x.eq_ignore_ascii_case(&host)) {
                continue;
            }
            let x = fleet.entry(inst.clone()).or_insert_with(|| {
                json!({"installation": inst, "host": host, "runtime": null, "version": null, "version_at": null, "last_seen": 0, "heartbeat_at": null,
                       "runs": if scoped { json!(0) } else { Value::Null }, "nonzero_exits": if scoped { json!(0) } else { Value::Null }, "exit_samples": 0,
                       "retrievals": if scoped { json!(0) } else { Value::Null }, "failed_retrievals": if scoped { json!(0) } else { Value::Null }, "trace_id": null})
            });
            // the newest event names the host, runtime and version
            if x["last_seen"].as_u64().is_some_and(|l| at >= l) {
                x["last_seen"] = json!(at);
                x["host"] = json!(host);
                x["runtime"] = aq(r, "agentq.runtime").as_str().filter(|s| safe(s, 16, b"")).map_or(Value::Null, |s| json!(s));
                x["version"] = semver(aq(r, "agentq.version"));
                x["version_at"] = if x["version"].is_null() { Value::Null } else { json!(at) };
                x["trace_id"] = trace(r);
            }
            match key {
                "agentq.installation" if x["heartbeat_at"].as_u64().is_none_or(|h| at > h) => x["heartbeat_at"] = json!(at),
                "agentq.run" if scoped && at >= since => {
                    add(x, "runs", 1.0);
                    if let Some(e) = aq(r, "agentq.exit_code").as_i64() {
                        add(x, "exit_samples", 1.0);
                        add(x, "nonzero_exits", if e != 0 { 1.0 } else { 0.0 });
                    }
                }
                "agentq.retrieval" if scoped && at >= since => {
                    add(x, "retrievals", 1.0);
                    add(x, "failed_retrievals", if aq(r, "agentq.ok") == false { 1.0 } else { 0.0 });
                }
                "agentq.incident" if at + 86400 >= now && trace(r).as_str().is_some_and(|t| aq_seen.insert(t.to_string())) => {
                    let stage = aq(r, "agentq.stage").as_str().filter(|s| safe(s, 32, b"_-")).unwrap_or("unknown stage");
                    history.push(json!({"source": "agentq", "host": host, "what": format!("agentq incident ({stage})"), "since": at, "occurrences": null, "trace_id": trace(r)}));
                }
                _ => {}
            }
        }
    }
    let mut fleet: Vec<Value> = fleet
        .into_values()
        .map(|mut x| {
            let age = x["heartbeat_at"].as_u64().map(|h| now.saturating_sub(h));
            let fresh = age.is_some_and(|a| a <= AQ_HEARTBEAT_LIMIT);
            x["state"] = json!(if fresh { "ok" } else { "stale" });
            x["age_s"] = json!(age);
            x["reason"] = if fresh { Value::Null } else { json!(age.map_or("no heartbeat in 24 h".into(), |a| format!("heartbeat {} old (limit {})", dur(a), dur(AQ_HEARTBEAT_LIMIT)))) };
            x["latest"] = aq_latest.clone();
            x["behind"] = behind(&aq_latest, &x["version"]);
            x
        })
        .collect();
    // One installation owns a row's agentq columns (H6): fresh first, then the higher version, then the newest event.
    // Another one on the row is superseded (H7) when a fresh owner saw newer events or runs a higher version.
    let better = |a: &Value, b: &Value| {
        if (a["state"] == "ok") != (b["state"] == "ok") {
            return a["state"] == "ok";
        }
        match (behind(&a["version"], &b["version"]), behind(&b["version"], &a["version"])) {
            (Value::Bool(true), _) => return true,
            (_, Value::Bool(true)) => return false,
            _ if a["version"].is_null() != b["version"].is_null() => return b["version"].is_null(),
            _ => {}
        }
        a["last_seen"].as_u64() > b["last_seen"].as_u64()
    };
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    for (i, x) in fleet.iter().enumerate() {
        let k = x["host"].as_str().unwrap_or("").to_ascii_lowercase();
        if owners.get(&k).is_none_or(|&o| better(x, &fleet[o])) {
            owners.insert(k, i);
        }
    }
    for i in 0..fleet.len() {
        let (w, x) = (&fleet[owners[&fleet[i]["host"].as_str().unwrap_or("").to_ascii_lowercase()]], &fleet[i]);
        let superseded = x["installation"] != w["installation"]
            && w["state"] == "ok"
            && (x["last_seen"].as_u64() < w["last_seen"].as_u64() || (x["state"] == "ok" && behind(&w["version"], &x["version"]) == true));
        let by = superseded.then(|| json!(format!("superseded by {} {}", w["installation"].as_str().unwrap_or(""), w["version"].as_str().unwrap_or("(version unknown)"))));
        fleet[i]["superseded"] = by.unwrap_or(Value::Null);
    }
    for x in fleet.iter().filter(|x| x["state"] != "ok" && x["superseded"].is_null()) {
        gaps.push(json!({"source": "Langfuse", "host": x["host"], "what": format!("agentq installation {}: stale", x["installation"].as_str().unwrap()), "since": x["heartbeat_at"], "detail": x["reason"], "trace_id": x["trace_id"]}));
    }
    for &o in owners.values() {
        let x = &fleet[o];
        let h = host_entry(&mut hosts, x["host"].as_str().unwrap_or(""));
        h["freshness"] = json!({"state": x["state"], "at": x["heartbeat_at"], "age_s": x["age_s"], "reason": x["reason"]});
        // a failed, stale or capped Langfuse read leaves the cached heartbeat unconfirmed, never green (D6)
        let rank = |v: &Value| RANK.iter().position(|k| v["state"] == *k).unwrap_or(4);
        if rank(&lf_src) > rank(&h["freshness"]) {
            h["freshness"]["state"] = lf_src["state"].clone();
            h["freshness"]["reason"] = json!(format!("Langfuse: {}", lf_src["reason"].as_str().unwrap_or("read not confirmed")));
        }
        h["agentq"]["installation"] = x["installation"].clone();
        h["agentq"]["running"] = x["version"].clone();
    }

    // Heartbeats and boards (D4.2).
    let releases = &inp["releases"];
    let latest = |repo: &str| releases[repo]["value"].clone();
    let mut boards = Vec::new();
    let mut gh_states = Vec::new();
    let mut taskq_states = Vec::new();
    // live taskq attempts per lowercase Hosts row id from fresh heartbeats: agents run there (#116);
    // `unconfirmed`: hosts whose heartbeat came from a failed, stale or cached board read, with that reason
    let mut working: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unconfirmed: BTreeMap<String, String> = BTreeMap::new();
    let mut taskq_hosts = BTreeMap::new();
    for repo in inp["projects"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let snap = &inp["boards"][repo];
        let src = source(snap, now, GITHUB_LIMIT, None);
        gh_states.push((repo.to_string(), src.clone()));
        if let Some(r) = releases.get(repo) {
            let s = source(r, now, 900, None);
            if s["state"] != "ok" {
                gh_states.push((format!("{repo} release"), s));
            }
        }
        let v = &snap["value"];
        let issues = v["issues"].as_array();
        let with = |labels: &[&str]| issues.map(|is| is.iter().filter(|i| i["labels"].as_array().is_some_and(|l| l.iter().any(|x| labels.iter().any(|w| x == *w)))).collect::<Vec<_>>());
        let count = |labels: &[&str]| with(labels).map_or(Value::Null, |v| json!(v.len()));
        let list = |labels: &[&str]| with(labels).map_or(json!([]), |v| v.iter().map(|i| json!({"number": i["number"], "title": i["title"], "url": i["url"]})).collect());
        let mut attempts = Vec::new();
        let date = v["date"].as_u64();
        for (index, hb) in v["hosts"].as_array().into_iter().flatten().enumerate() {
            let Some(name) = hb["title"].as_str().and_then(|t| t.strip_prefix("taskq host ")).filter(|n| safe(n, 64, b"_.-")) else { continue };
            let body = hb["body"].as_str().unwrap_or("");
            // H2: the `machine <host.name>[ wsl]` line names the row; without it a label that is a machine joins its native row
            let machine = body.lines().find_map(|l| l.strip_prefix("machine ")).and_then(|m| {
                let mut p = m.split_whitespace();
                p.next().filter(|n| safe(n, 64, b"_.-")).map(|n| (n, p.next() == Some("wsl")))
            });
            let (row, joined) = match machine {
                Some((n, wsl)) => (machines.id(n, if wsl { "wsl" } else { "native" }), true),
                None if machines.known(name) => (machines.id(name, "native"), true),
                None => (name.to_string(), false),
            };
            taskq_hosts.insert((repo.to_ascii_lowercase(), name.to_ascii_lowercase()), row.clone());
            let ttl = body.lines().find_map(|l| l.strip_prefix("ttl ")?.trim().parse().ok()).unwrap_or(HEARTBEAT_TTL);
            let first = attempts.len();
            for l in body.lines() {
                if let Some((n, id)) = l.strip_prefix('#').and_then(|l| l.split_once(' '))
                    && let Ok(n) = n.parse::<u64>()
                {
                    attempts.push(json!({"host": name, "host_id": row, "task": n, "attempt": id.trim()}));
                }
            }
            // Age at collection uses only the board clock.
            let at = hb["updated_at"].as_str().and_then(iso_secs);
            let age = date.zip(at).map(|(d, u)| d.saturating_sub(u));
            let cached_age = age.zip(snap["at"].as_u64()).map(|(a, read_at)| a.saturating_add(now.saturating_sub(read_at)));
            for a in &mut attempts[first..] {
                a["current"] = json!(src["state"] == "ok" && cached_age.is_some_and(|a| a <= ttl));
            }
            if src["state"] != "ok" {
                unconfirmed.entry(row.to_ascii_lowercase()).or_insert_with(|| format!("{repo}: {}", src["reason"].as_str().unwrap_or("board read not confirmed")));
            } else if cached_age.is_some_and(|a| a <= ttl) {
                let tasks = working.entry(row.to_ascii_lowercase()).or_default();
                tasks.extend(attempts[first..].iter().map(|a| format!("{repo}#{}", a["task"])));
            }
            let version = semver(&json!(body.lines().find_map(|l| l.strip_prefix("version ")).map(str::trim)));
            // taskq's own installation id (taskq.md § 4 `service`), never the host name; older heartbeats have none
            let service = body.lines().find_map(|l| l.strip_prefix("service ")).map(str::trim).filter(|s| s.len() == 12 && s.bytes().all(|b| b.is_ascii_hexdigit()));
            let key = format!("{repo}:{}", service.map_or_else(|| format!("unidentified:{index}"), String::from));
            if heartbeats.get(&key).is_none_or(|o: &Value| o["age_s"].as_u64().is_some_and(|a| cached_age.is_some_and(|x| x < a))) {
                heartbeats.insert(key, json!({"service": service, "workspace": repo, "host": row, "at": at, "age_s": cached_age, "ttl": ttl, "version": version, "source": src}));
            }
            let h = host_entry(&mut hosts, &row);
            if !joined {
                h["reason"] = json!("no machine line: update taskq");
            }
            // H3: several heartbeats on one row show the newest version and the freshest state
            if h["taskq"]["version"].is_null() || behind(&version, &h["taskq"]["version"]) == true {
                h["taskq"]["version"] = version.clone();
            }
            if h["taskq"]["age_s"].as_u64().is_some_and(|a| age.is_none_or(|x| x >= a)) {
                continue;
            }
            let fresh = age.is_some_and(|a| a <= ttl);
            let t = &mut h["taskq"];
            t["age_s"] = json!(age);
            t["state"] = json!(if age.is_none() { "unknown" } else if fresh { "ok" } else { "stale" });
            t["reason"] = if fresh { Value::Null } else { json!(age.map_or("heartbeat age unknown".into(), |a| format!("heartbeat {} old (ttl {})", dur(a), dur(ttl)))) };
        }
        let mut board = json!({
            "project": repo, "url": format!("https://github.com/{repo}/issues"), "source": src, "adapter": "taskq-labels",
            "in_work": issues.map(|_| json!(with(&["q-doing", "q-review"]).unwrap().len())), "waiting": count(&["q-ask"]), "ready": count(&["q-ready"]),
            "questions": list(&["q-ask"]), "failed": list(&["q-failed"]), "attempts": attempts, "latest": latest(repo),
        });
        // TaskQ counter order (task-counters.md); `waiting` is Blocked; Closed has no authoritative total yet: unknown
        let mut counts = json!({"closed": null});
        for k in COUNTS {
            counts[k] = count(&[&format!("q-{k}")]);
        }
        board["counts"] = counts;
        board["tasks"] = issues.into_iter().flatten().map(|i| {
            let state = COUNTS.iter().find(|k| i["labels"].as_array().is_some_and(|ls| ls.iter().any(|l| l == &format!("q-{k}"))));
            json!({"n": i["number"], "title": i["title"], "url": i["url"], "state": state, "revision": null})
        }).collect::<Vec<_>>().into();
        // taskq-snapshot (adapters.md S3 R1) replaces counts, questions and failures when configured; heartbeats stay
        if let Some(ts) = inp["taskq"].get(repo) {
            let tsrc = source(ts, now, GITHUB_LIMIT, None);
            taskq_states.push((repo.to_string(), tsrc.clone()));
            let tasks = ts["value"]["tasks"].as_array();
            let total = |k: &[&str]| -> Value { k.iter().map(|k| ts["value"]["totals"][k].as_u64()).sum::<Option<u64>>().map_or(Value::Null, |n| json!(n)) };
            let pick = |f: &dyn Fn(&Value) -> bool, m: &dyn Fn(&Value) -> Value| -> Value { tasks.into_iter().flatten().filter(|t| f(t)).map(m).collect::<Vec<_>>().into() };
            board["source"] = tsrc;
            board["adapter"] = json!("taskq-snapshot");
            // D13: preserve canonical question identities/limits/evidence; no second parser or session authority.
            board["tasks"] = tasks.into_iter().flatten().map(|t| json!({
                "n": t["n"], "title": t["title"], "url": t["url"], "state": t["state"],
                "revision": t["revision"], "question": t["question"], "card": t["card"]
            })).collect::<Vec<_>>().into();
            board["in_work"] = total(&["doing", "review"]);
            board["waiting"] = total(&["ask"]);
            board["ready"] = total(&["ready"]);
            for k in COUNTS {
                board["counts"][k] = total(&[k]);
            }
            board["questions"] = pick(&|t| t["state"] == "ask" || (t["state"] == "review" && t["question"].is_object()), &question);
            board["failed"] = pick(&|t| t["state"] == "failed", &|t| json!({"number": t["n"], "title": t["title"], "url": t["url"], "reason": t["failed"]}));
        }
        // TaskQ R6 3 (#1565): a live session's progress comes from its host's enrolled `taskq-board/1` task `session`
        // (runtime, stage, update time; never a worker line, #180); the newest per task wins, shown on doing and review rows only.
        // the host shown is the enrolled row's label, not the payload's; a future `at` cannot pin newest-wins
        let mut progress: BTreeMap<u64, (&Value, &str)> = BTreeMap::new();
        for r in inp["installations"]["rows"].as_array().into_iter().flatten()
            .filter(|r| r["product"] == "taskq" && r["connection"] != "revoked" && r["snapshot"]["schema"] == "taskq-board/1")
        {
            let p = &r["snapshot"]["payload"];
            let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
            if !p["board"]["repo"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(repo)) {
                continue;
            }
            for t in p["tasks"].as_array().into_iter().flatten() {
                let (Some(n), Some(at)) = (t["n"].as_u64(), t["session"]["at"].as_u64().filter(|&at| at <= now)) else { continue };
                if progress.get(&n).is_none_or(|(v, _)| v["at"].as_u64() < Some(at)) {
                    progress.insert(n, (&t["session"], label));
                }
            }
        }
        for t in board["tasks"].as_array_mut().unwrap() {
            if let Some(&(s, host)) = t["n"].as_u64().and_then(|n| progress.get(&n)).filter(|_| matches!(t["state"].as_str(), Some("doing" | "review"))) {
                t["session"] = json!({"host": host, "runtime": s["runtime"], "stage": s["stage"], "age_s": now - s["at"].as_u64().unwrap()});
            }
        }
        for i in board["failed"].as_array().unwrap() {
            let detail = i["reason"].as_str().filter(|r| !r.is_empty()).map_or(i["title"].clone(), |r| json!(r));
            current.push(json!({"source": "taskq", "host": null, "what": format!("{repo}#{}: failed", i["number"]), "since": null, "detail": detail, "trace_id": null, "url": i["url"]}));
        }
        boards.push(board);
    }
    // AgentQ coverage (#116): a running installation whose host's agents leave no compression or activity records.
    // ponytail: Langfuse heartbeats carry no routing or hook state; those rows come from enrolled snapshots below.
    // Absence is asserted only on a fresh, uncapped read of every record kind it rests on; otherwise it is a gap (D6).
    let coverage_keys = ["activity", "agentq.installation", "agentq.compress", "agentq.run"];
    let lf_gap = match coverage_keys.iter().filter(|k| capped(k)).copied().collect::<Vec<_>>() {
        _ if !matches!(lf_src["state"].as_str(), Some("ok" | "incomplete")) => lf_src["reason"].as_str().map(String::from),
        c if c.is_empty() => None,
        c => Some(format!("capped query ({})", c.join(", "))),
    };
    if scoped && let Some(reason) = &lf_gap {
        for x in fleet.iter().filter(|x| x["state"] == "ok") {
            gaps.push(json!({"source": "agentq", "host": x["host"], "client": null, "what": "AgentQ coverage unknown", "since": null,
                "detail": format!("Langfuse {reason}: missing compression or activity records of installation {} are not confirmed.", x["installation"].as_str().unwrap()), "trace_id": null}));
        }
    }
    if scoped && lf_gap.is_none() {
        let mut covered: HashMap<(String, &str), u64> = HashMap::new();
        for r in rows("agentq.compress").into_iter().chain(rows("agentq.run")).filter(|r| in_period(r).is_some()) {
            let Some((inst, _)) = aq_identity(r) else { continue };
            let agent = match aq(r, "agentq.agent").as_str() {
                Some("claude") => "claude-code",
                Some(a) if agent_name(a).is_some() => a,
                _ => "unknown",
            };
            *covered.entry((inst, agent)).or_default() += 1;
        }
        for x in fleet.iter().filter(|x| x["state"] == "ok") {
            let (inst, host) = (x["installation"].as_str().unwrap().to_string(), &x["host"]);
            let n = |a: &str| covered.get(&(inst.clone(), a)).copied().unwrap_or(0);
            let total: u64 = covered.iter().filter(|((i, _), _)| *i == inst).map(|(_, n)| n).sum();
            let calls: Vec<&Value> = activity.iter().filter(|a| a["host"] == *host).collect();
            for a in &calls {
                let agent = a["agent"].as_str().unwrap();
                if n(agent) == 0 && n("unknown") == 0 {
                    let client = agent_name(agent).unwrap();
                    current.push(json!({"source": "agentq", "host": host, "client": client, "what": format!("{client}: no compression records"), "since": null,
                        "detail": format!("{} model calls in {}, but installation {inst} sent no agentq.compress or agentq.run record for this client.", a["calls"], dur(period)), "trace_id": a["trace_id"]}));
                }
            }
            let name = host.as_str().unwrap().to_ascii_lowercase();
            if let Some(tasks) = working.get(&name).filter(|t| !t.is_empty() && calls.is_empty()) {
                let none = if total == 0 { "activity or compression" } else { "activity" };
                current.push(json!({"source": "agentq", "host": host, "client": "TaskQ workers", "what": format!("TaskQ workers: no {none} records"), "since": null,
                    "detail": format!("TaskQ attempts {} run on this host, but no {none} records arrived in {} (installation {inst}).", tasks.join(", "), dur(period)), "trace_id": null}));
            }
            if let Some(reason) = unconfirmed.get(&name).filter(|_| calls.is_empty() && working.get(&name).is_none_or(|t| t.is_empty())) {
                gaps.push(json!({"source": "agentq", "host": host, "client": "TaskQ workers", "what": "TaskQ workers: liveness unknown", "since": null,
                    "detail": format!("TaskQ board read {reason}: whether agents run here without records is not confirmed (installation {inst})."), "trace_id": null}));
            }
        }
    }
    // Enrolled AgentQ profiles: an onboarded (AgentQ-owned) client that routes direct or lost its hooks.
    for r in inp["installations"]["rows"].as_array().into_iter().flatten().filter(|r| r["product"] == "agentq" && r["connection"] != "revoked") {
        let c = &r["snapshot"]["payload"]["configuration"];
        if !matches!(c["state"].as_str(), Some("fresh" | "stale")) {
            continue;
        }
        let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
        let host = format!("{label}/{}", if r["runtime"] == "wsl" { "wsl" } else { "native" });
        let last = if c["state"] == "stale" || r["observation"]["state"] != "ok" { " Last known: the snapshot is not fresh." } else { "" };
        for p in c["profiles"].as_array().into_iter().flatten().filter(|p| p["owned"] == true) {
            let client = p["label"].as_str().filter(|l| safe(l, 48, b" ._-")).unwrap_or("unknown client");
            let item = |what: String, detail: String| json!({"source": "agentq", "host": host, "client": client, "what": format!("{client}: {what}"), "since": c["at"], "detail": detail + last, "trace_id": null});
            match p["routing"].as_str() {
                Some("direct") => current.push(item("routing direct".into(), "Onboarded client calls its provider directly: no AgentQ routing or provider compression.".into())),
                Some("bypassed") => current.push(item("routing bypassed".into(), "Another provider owns the route of this onboarded client: no AgentQ routing.".into())),
                _ => {}
            }
            if p["operations"].as_array().is_some_and(|o| o.iter().any(|x| x == "hooks")) {
                match p["hooks"].as_str() {
                    Some(h @ ("missing" | "absent" | "partial")) => current.push(item(format!("hooks {h}"), format!("AgentQ hooks of this onboarded client are {h}: hook compression and evidence stop."))),
                    Some("registered" | "disabled" | "unsupported") => {}
                    _ => gaps.push(item("hooks unknown".into(), "AgentQ could not read the hooks of this onboarded client.".into())),
                }
            }
        }
    }
    // Per-client use (#131): heartbeat routing and hooks, enrolled MCP, 24 h compress and retrieval records.
    let use_gap = |k: &str| match lf_src["state"].as_str() {
        Some("ok" | "incomplete") if capped(k) => Some(format!("capped query ({k})")),
        Some("ok" | "incomplete") => None,
        _ => Some(lf_src["reason"].as_str().unwrap_or("read not confirmed").to_string()),
    };
    let mut clients = crate::clients::build(
        &crate::clients::Ctx {
            now, heartbeats: rows("agentq.installation"), compress: rows("agentq.compress.24h"), retrieval: rows("agentq.retrieval.24h"),
            compress_gap: use_gap("agentq.compress.24h"), retrieval_gap: use_gap("agentq.retrieval.24h"),
            incomplete: json!({"agentq.compress.24h": capped("agentq.compress.24h"), "agentq.retrieval.24h": capped("agentq.retrieval.24h")}),
            scoped, agent: f["agent"].as_str(),
        },
        &fleet, &activity, &inp["installations"], &mut current, &mut gaps,
    );
    if !matches!(lf_src["state"].as_str(), Some("ok" | "incomplete")) {
        // a stale or failed Langfuse read shows the cached heartbeat as last known, never as current (D6)
        for r in clients["rows"].as_array_mut().into_iter().flatten() {
            r["heartbeat"]["state"] = lf_src["state"].clone();
        }
    }
    let taskq_src = (!taskq_states.is_empty()).then(|| worst(taskq_states.iter().map(|(n, s)| (n.clone(), s)), now));
    let gh_src = if gh_states.is_empty() { source(&Value::Null, now, GITHUB_LIMIT, None) } else { worst(gh_states.iter().map(|(n, s)| (n.clone(), s)), now) };

    // serverq (D4.3)
    let sq = &inp["serverq"];
    let sq_src = source(sq, now, SERVERQ_LIMIT, sq["value"]["at"].as_u64());
    let sq_host = inp["serverq_host"].as_str().unwrap_or("");
    let report = &sq["value"];
    let mut results = Vec::new();
    for (name, r) in report["results"].as_object().into_iter().flatten() {
        let status = r["status"].as_str().filter(|s| ["ok", "warn", "unknown"].contains(s)).unwrap_or("unknown");
        let detail = r["detail"].as_str().unwrap_or("");
        let first_seen = report["incidents"][name]["first_seen"].as_f64().map(|a| a as u64);
        let item = json!({"source": "serverq", "host": sq_host, "what": format!("{name}: {status}"), "since": first_seen.or(sq_src["at"].as_u64()), "detail": detail, "trace_id": null});
        match status {
            "warn" => current.push(item),
            "unknown" => gaps.push(item),
            _ => {}
        }
        results.push(json!({"check": name, "status": status, "detail": detail}));
    }
    let mut open = Vec::new();
    for (name, i) in report["incidents"].as_object().into_iter().flatten() {
        let (first, last) = (i["first_seen"].as_f64().map(|a| a as u64), i["last_seen"].as_f64().map(|a| a as u64));
        let o = json!({"check": name, "state": i["state"], "first_seen": first, "last_seen": last, "occurrences": i["occurrences"]});
        if i["state"] != "verified" {
            open.push(o);
        }
        if last.is_some_and(|l| l + 86400 >= now) {
            history.push(json!({"source": "serverq", "host": sq_host, "what": format!("{name}: {}", i["state"].as_str().unwrap_or("unknown")), "since": first, "occurrences": i["occurrences"], "trace_id": null}));
        }
    }
    if !sq_host.is_empty() && !sq.is_null() {
        let worst_status = ["warn", "unknown", "ok"].into_iter().find(|s| results.iter().any(|r| r["status"] == *s));
        let h = host_entry(&mut hosts, &machines.canon(sq_host));
        h["serverq"] = json!({"status": worst_status, "state": sq_src["state"], "at": sq_src["at"], "age_s": sq_src["age_s"], "reason": sq_src["reason"], "installation": null});
    }
    let manual: Vec<Value> = report["manual_rules"].as_array().into_iter().flatten().map(|r| json!({"rule": r, "status": "not verified"})).collect();
    let enrolled_checks = serverq_enrolled(inp, now, &machines, &mut hosts, &mut current, &mut gaps, &mut history);

    taskq_observations(inp, now, &machines, &taskq_hosts, &mut hosts);
    for h in hosts.values_mut() {
        let id = h["id"].as_str().unwrap_or("").to_ascii_lowercase();
        h["taskq"]["observed_running"] = if unconfirmed.contains_key(&id) { Value::Null } else {
            working.get(&id).map_or(Value::Null, |tasks| json!(tasks.iter().collect::<HashSet<_>>().len()))
        };
        h["taskq"]["latest"] = latest("taskqorg/taskq");
        h["taskq"]["behind"] = behind(&h["taskq"]["latest"], &h["taskq"]["version"]);
        let a = &mut h["agentq"];
        a["latest"] = latest("taskqorg/agentq");
        // the running installation (H6) decides
        a["behind"] = behind(&a["latest"], &a["running"]);
    }

    // Resources (D4.4): enrolled history first, Langfuse samples otherwise; a stale or missing sample is a gap.
    let mut resources = crate::resource::build(&rows("agentq.resource"), capped("agentq.resource"), &inp["enrolled_resources"], now, period, None);
    for r in resources["rows"].as_array_mut().into_iter().flatten() {
        r["host"] = fleet.iter().find(|f| f["installation"] == r["installation"]).map_or_else(|| {
            let name = r["host"].as_str().unwrap_or("").split('/').next().unwrap_or("");
            json!(machines.runtime(name, r["runtime"].as_str().unwrap_or("")))
        }, |f| f["host"].clone());
    }
    resources["rows"].as_array_mut().unwrap().retain(|r| fhost.is_none_or(|h| r["host"].as_str().is_some_and(|id| id.eq_ignore_ascii_case(h))));
    if lf_src["state"] == "unconfirmed" {
        for r in resources["rows"].as_array_mut().into_iter().flatten().filter(|r| r["provenance"] == "langfuse") {
            r["source"]["state"] = json!("unconfirmed");
            r["source"]["reason"] = lf_src["reason"].clone();
        }
    }
    for r in resources["rows"].as_array().into_iter().flatten().filter(|r| r["source"]["state"] != "ok") {
        gaps.push(json!({"source": r["provenance"], "host": r["host"], "what": format!("resources of {}: {}", r["installation"].as_str().unwrap_or(""), r["source"]["state"].as_str().unwrap_or("")),
            "since": r["source"]["at"], "detail": r["source"]["reason"], "trace_id": null}));
    }

    let hostf = |v: &Vec<Value>| -> Vec<Value> { v.iter().filter(|i| fhost.is_none_or(|x| i["host"] == x || i["host"].is_null())).cloned().collect() };
    history.sort_by_key(|i| std::cmp::Reverse(i["since"].as_u64()));
    let mut sources = json!({"langfuse": lf_src, "github": gh_src, "serverq": sq_src});
    let mut att_states = vec![("Langfuse".to_string(), lf_src.clone()), ("GitHub".to_string(), gh_src.clone()), ("serverq".to_string(), sq_src.clone())];
    // listed only when configured: an absent adapter is not a failed source (adapters.md S2)
    if let Some(t) = taskq_src {
        sources["taskq"] = t.clone();
        att_states.push(("taskq".to_string(), t));
    }
    let agent_work = crate::agent_work::build(inp);
    // listed only when configured, like the taskq adapter
    if !inp["agent_work"].is_null() {
        sources["agent_work"] = agent_work["source"].clone();
        att_states.push(("Agent work".to_string(), agent_work["source"].clone()));
    }
    let versions = crate::version::build(inp, &fleet, &heartbeats.into_values().collect::<Vec<_>>());
    if inp["installations"].is_object() {
        sources["enrollment"] = inp["installations"]["source"].clone();
    }
    let mut doc = json!({
        "schema": 2,
        "generated_at": now,
        "period": period,
        "filters": f,
        "sources": sources,
        "hosts": hosts.into_values().filter(|h| fhost.is_none_or(|x| h["id"].as_str().is_some_and(|i| i.eq_ignore_ascii_case(x)))).collect::<Vec<_>>(),
        "attention": {"source": worst(att_states.iter().map(|(n, s)| (n.clone(), s)), now),
                      "current": hostf(&current), "gaps": hostf(&gaps), "incidents": hostf(&history)},
        "boards": boards,
        "resources": resources,
        "agents": {"source": lf_src, "incomplete": {"activity": capped("activity"),
                                                      "agentq.installation": capped("agentq.installation"), "agentq.run": capped("agentq.run"), "agentq.retrieval": capped("agentq.retrieval")},
                   "fleet": fleet, "clients": clients, "activity": activity, "unattributed": unattributed},
        "checks": {"source": sq_src, "at": sq_src["at"], "results": results, "incidents": open, "manual_rules": manual,
                   "installations": enrolled_checks.into_iter().filter(|c| fhost.is_none_or(|x| c["host"] == x)).collect::<Vec<_>>()},
        "versions": versions,
        "optimization": crate::optimization::build(inp),
        "agent_work": agent_work,
    });
    if inp["installations"].is_object() {
        doc["installations"] = inp["installations"].clone();
        // Filter after canonical host joins: one TaskQ sender can observe other hosts of its approved board.
        if let Some(host) = fhost {
            let owners: HashSet<String> = doc["hosts"].as_array().into_iter().flatten().flat_map(|h| h["taskq"]["observations"].as_array().into_iter().flatten())
                .filter_map(|o|o["installation"].as_str().map(String::from)).collect();
            if let Some(rows) = doc["installations"]["rows"].as_array_mut() { rows.retain(|r| {
                format!("{}/{}",r["host_label"].as_str().unwrap_or(""),r["runtime"].as_str().unwrap_or("")).eq_ignore_ascii_case(host)
                    || r["installation"].as_str().is_some_and(|i|owners.contains(i))
            }); }
        }
    }
    doc
}

/// D8 / #178: newest board-scoped evidence, never inferred from activity or shared queue totals.
fn taskq_observations(inp: &Value, now: u64, machines: &Machines, aliases: &BTreeMap<(String, String), String>, hosts: &mut BTreeMap<String, Value>) {
    let mut latest: BTreeMap<(String, String, String, String), Value> = BTreeMap::new();
    for r in inp["installations"]["rows"].as_array().into_iter().flatten().filter(|r|
        r["product"] == "taskq" && r["connection"] == "active" && r["projected"] != false && r["snapshot"]["schema"] == "taskq-board/1") {
        let p = &r["snapshot"]["payload"];
        let b = &p["board"];
        let Some(repo) = b["repo"].as_str() else { continue };
        if p["schema"] != 1 || b["kind"] != "github" || b["host"] != "github.com" { continue }
        for o in p["observations"].as_array().into_iter().flatten() {
            let (Some(host), Some(kind)) = (o["host"].as_str().filter(|h| safe(h,64,b"_.-")), o["kind"].as_str().filter(|k| matches!(*k,"online"|"login"|"queue"))) else { continue };
            let row = aliases.get(&(repo.to_ascii_lowercase(), host.to_ascii_lowercase())).cloned()
                .unwrap_or_else(|| if machines.known(host) { machines.id(host,"native") } else { host.to_string() });
            let key = (repo.to_ascii_lowercase(), r["workspace"].as_str().unwrap_or("").into(), row.to_ascii_lowercase(), kind.into());
            let mut v = o.clone();
            v["board"] = b.clone(); v["workspace"] = r["workspace"].clone(); v["installation"] = r["installation"].clone();
            let rank = |v: &Value| (v["observed"].as_u64().is_some_and(|t|t<=now), v["observed"].as_u64());
            if let Some(old) = latest.get_mut(&key) {
                if rank(old) > rank(&v) { continue }
                if rank(old) == rank(&v) {
                    let mut a = old.clone(); let mut b = v.clone();
                    for x in [&mut a, &mut b] { x.as_object_mut().unwrap().remove("installation"); }
                    if a != b { old["state"] = json!("unknown"); old["complete"] = json!(false); old["reason"] = json!("conflicting observations"); }
                    continue;
                }
            }
            latest.insert(key, v);
            host_entry(hosts, &row);
        }
    }
    for ((repo, workspace, row, _), v) in latest.clone() {
        for kind in ["online", "login", "queue"] {
            latest.entry((repo.clone(), workspace.clone(), row.clone(), kind.into())).or_insert_with(|| json!({
                "host":v["host"],"kind":kind,"board":v["board"],"workspace":workspace,"installation":null,
                "producer":null,"observed":null,"valid_until":null,"state":"unknown","complete":false,"reason":"no observation"}));
        }
    }
    for ((_, _, row, kind), mut o) in latest {
        let valid = match kind.as_str() {
            "online" => matches!(o["state"].as_str(),Some("online"|"offline")),
            "login" => matches!(o["state"].as_str(),Some("logged in"|"not logged in")),
            _ => matches!(o["state"].as_str(),Some("busy"|"idle")) && o["running"].as_u64().is_some() && o["queued"].as_u64().is_some(),
        };
        let reason = if o["observed"].as_u64().is_some_and(|t|t>now) { Some("clock uncertain") }
            else if o["valid_until"].as_u64().is_some_and(|t|now>t) { Some("expired") }
            else if o["complete"] != true || o["state"] == "unknown" { Some(o["reason"].as_str().unwrap_or("incomplete observation")) }
            else if o["observed"].as_u64().is_none_or(|t|t==0) || o["valid_until"].as_u64().is_none_or(|t|Some(t)<o["observed"].as_u64()) { Some("invalid observation time") }
            else if kind == "login" && ["claude","codex"].iter().any(|k| !matches!(o[*k].as_str(),Some("logged in"|"not logged in"|"not installed"))) { Some("runtime login unknown") }
            else if !valid || !o["producer"].as_str().is_some_and(|p|p.len()==12 && p.bytes().all(|b|b.is_ascii_hexdigit())) { Some("invalid observation") }
            else { None };
        if let Some(reason) = reason.map(str::to_string) {
            o["state"] = json!("unknown"); o["complete"] = json!(false); o["reason"] = json!(reason);
            for k in ["running","queued"] { if o.get(k).is_some() { o[k] = Value::Null; } }
            for k in ["claude","codex","codex-app"] { if o.get(k).is_some() { o[k] = json!("unknown"); } }
        }
        let h = hosts.get_mut(&row).unwrap();
        if !h["taskq"]["observations"].is_array() { h["taskq"]["observations"] = json!([]); }
        h["taskq"]["observations"].as_array_mut().unwrap().push(o);
    }
}

/// Enrolled `serverq-report/1` snapshots (serverq.md § 3 projection, schema 1): one Server checks entry per
/// installation and workspace, its hosts cell and Attention items. Producer time dates everything (serverq limit
/// 7200 s, enroll::stale_secs); manual rules stay not verified, unknown stays unknown, a revoked grant shows nothing
/// current. A host keeps the newest observation when several installations share its label and runtime.
fn serverq_enrolled(inp: &Value, now: u64, machines: &Machines, hosts: &mut BTreeMap<String, Value>, current: &mut Vec<Value>, gaps: &mut Vec<Value>, history: &mut Vec<Value>) -> Vec<Value> {
    let rule_id = |k: &str| (1..=64).contains(&k.len()) && k.bytes().all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'));
    let mut out = Vec::new();
    for r in inp["installations"]["rows"].as_array().into_iter().flatten() {
        if r["product"] != "serverq" || r["connection"] == "revoked" || r["snapshot"]["schema"] != "serverq-report/1" {
            continue;
        }
        let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
        let host = machines.id(label, if r["runtime"] == "wsl" { "wsl" } else { "native" });
        let installation = r["installation"].clone();
        let p = &r["snapshot"]["payload"];
        let mut src = r["observation"].clone();
        let valid = crate::enroll::serverq_payload(p);
        if !valid {
            src = json!({"state": "unknown", "at": src["at"], "age_s": src["age_s"], "reason": "unsupported serverq projection (schema 1 expected)", "incomplete": false});
        }
        if valid && (p["complete"] == false || p["incomplete"] == true) {
            src["state"] = json!("incomplete");
            src["incomplete"] = json!(true);
            src["reason"] = json!("producer projection incomplete");
        }
        let last = if src["state"] == "ok" { "" } else { " Last known: the report is not fresh." };
        let (mut results, mut manual, mut open) = (Vec::new(), Vec::new(), Vec::new());
        for (name, x) in p["rules"].as_object().into_iter().flatten().filter(|(k, _)| rule_id(k)) {
            let status = x["status"].as_str().filter(|s| ["ok", "warn", "unknown", "manual"].contains(s)).unwrap_or("unknown");
            if status == "manual" {
                manual.push(json!({"rule": name, "status": "not verified"}));
                continue;
            }
            let reason = x["reason"].as_str().filter(|s| safe(s, 64, b"-")).map_or(Value::Null, |s| json!(s));
            let observed = x["observed_at"].as_f64().filter(|t| t.is_finite() && *t >= 0.0).map_or(Value::Null, |t| json!(t as u64));
            let evidence: Map<String, Value> = x["evidence"].as_object().into_iter().flatten().filter(|(k, v)| safe(k, 32, b"_") && v.is_number()).map(|(k, v)| (k.clone(), v.clone())).collect();
            let first_seen = p["incidents"][name]["first_seen"].as_f64().map(|a| a as u64);
            let detail = format!("{}{last}", reason.as_str().unwrap_or("no reason reported"));
            let item = json!({"source": "serverq", "host": host, "installation": installation, "what": format!("{name}: {status}"), "since": first_seen.or(observed.as_u64()).or(src["at"].as_u64()), "detail": detail, "trace_id": null});
            match status {
                "warn" => current.push(item),
                "unknown" => gaps.push(item),
                _ => {}
            }
            let mut rule_source = src.clone();
            if src["state"] == "ok" {
                let (state, reason) = match observed.as_u64() {
                    None => ("unknown", Some("rule observation time unavailable")),
                    Some(t) if t > now => ("unknown", Some("clock uncertain")),
                    Some(t) if now - t > crate::enroll::SERVERQ_STALE_SECS => ("stale", Some("rule observation stale")),
                    _ => ("ok", None),
                };
                rule_source = json!({"state": state, "at": observed, "age_s": observed.as_u64().and_then(|t| now.checked_sub(t)), "reason": reason, "incomplete": false});
            }
            results.push(json!({"check": name, "status": status, "reason": reason, "observed_at": observed, "evidence": evidence, "source": rule_source}));
        }
        let retained = inp["serverq_samples"].as_array().into_iter().flatten().find(|s| s["installation"] == installation && s["workspace"] == r["workspace"]).cloned().unwrap_or(Value::Null);
        let findings = crate::findings::build(r, &retained, now);
        if findings["incomplete"] == true {
            gaps.push(json!({"source": "serverq", "host": host, "installation": installation, "workspace": r["workspace"], "what": "retained episode history incomplete", "since": src["at"], "detail": "History capped or unavailable; no recovery inferred.", "trace_id": null}));
        }
        for f in findings["episodes"].as_array().into_iter().flatten() {
            if f["state"] == "open" {
                open.push(json!({"check": f["rule"], "state": "observed", "first_seen": f["first_seen"], "last_seen": f["last_seen"], "occurrences": f["occurrences"]}));
            }
            if f["last_seen"].as_u64().is_some_and(|t| t.saturating_add(86400) >= now) {
                history.push(json!({"source": "serverq", "host": host, "installation": installation, "workspace": r["workspace"], "what": format!("{}: {}", f["rule"].as_str().unwrap_or("unknown"), f["state"].as_str().unwrap_or("unknown")), "since": f["first_seen"], "verified_at": f["verified_at"], "occurrences": f["occurrences"], "episode": f, "trace_id": null}));
            }
        }
        for f in findings["episodes"].as_array().into_iter().flatten().filter(|f| f["state"] == "open") {
            if p["rules"][f["rule"].as_str().unwrap_or("")]["status"] != "warn" {
                gaps.push(json!({"source": "serverq", "host": host, "installation": installation, "workspace": r["workspace"], "what": format!("{}: open episode, verification unavailable", f["rule"].as_str().unwrap_or("unknown")), "since": f["first_seen"], "detail": "A stale, unknown or incomplete observation does not resolve an episode.", "episode": f, "trace_id": null}));
            }
        }
        let worst_status = if results.iter().any(|r| r["status"] == "warn") { Some("warn") }
            else if results.iter().any(|r| r["status"] == "unknown" || r["source"]["state"] != "ok") { Some("unknown") }
            else if results.is_empty() { None } else { Some("ok") };
        let h = host_entry(hosts, &host);
        if h["serverq"]["at"].as_u64().is_none_or(|a| src["at"].as_u64().is_some_and(|b| b >= a)) {
            h["serverq"] = json!({"status": worst_status, "state": src["state"], "at": src["at"], "age_s": src["age_s"], "reason": src["reason"], "installation": installation});
        }
        out.push(json!({"installation": installation, "workspace": r["workspace"], "host": host, "version": p["version"].as_str().filter(|v| safe(v, 64, b".+-")),
            "findings": findings["episodes"], "findings_incomplete": findings["incomplete"], "catalog": p["catalog"].as_str().filter(|c| safe(c, 64, b"")), "source": src, "at": src["at"], "results": results, "incidents": open, "manual_rules": manual}));
    }
    out
}

/// One open question in the S3 shape: source choice ids, order and text kept; a card that does not parse
/// keeps its verbatim text and no choices. A choice the answer route refuses names why (D13).
fn question(t: &Value) -> Value {
    let q = &t["question"];
    let parsed = q["parsed"] == true;
    let choices: Vec<Value> = q["choices"].as_array().into_iter().flatten().filter(|_| parsed).map(|c| json!({"k": c["id"], "text": c["text"], "unavailable": crate::answer::unavailable(&c["text"])})).collect();
    let recommended = q["choices"].as_array().into_iter().flatten().filter(|_| parsed).find(|c| c["recommended"] == true).map_or(Value::Null, |c| c["id"].clone());
    let text = if parsed { q["problem"].clone() } else { q["text"].clone() };
    json!({"id": t["url"], "number": t["n"], "title": t["title"], "url": t["url"], "state": t["state"], "revision": t["revision"],
           "parsed": parsed, "done": q["done"], "text": text, "choices": choices, "recommended": recommended, "free_text": t["state"] == "ask"})
}

/// 503 when every source failed with no value to show (D7).
pub fn all_failed(doc: &Value) -> bool {
    let sources: Vec<&Value> = doc["sources"].as_object().into_iter().flatten()
        .filter(|(name, x)| name.as_str() != "export" && x["reason"] != "not configured").map(|(_, x)| x).collect();
    !sources.is_empty() && sources.iter().all(|x| x["state"] == "unknown" && x["reason"].as_str().is_some_and(|r| r.contains("read failed")))
}

#[cfg(test)]
mod tests {
    /// D8/H1: WSL resources use the same installation identity as the canonical host, including filtered entry.
    #[test]
    fn review_resource_runtime_identity() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../fixture/hosts.json")).unwrap();
        let mut inp = fixture["input"].clone();
        let now = inp["now"].as_u64().unwrap();
        inp["langfuse"]["value"]["agentq.resource"] = serde_json::json!([{"id":"r-wsl","startTime":crate::source::iso(now-10),"metadata":{
            "attributes.agentq.installation":"inst-wsl","attributes.host.name":"desktop","attributes.agentq.runtime":"linux","attributes.agentq.resource.cpu_percent":33}}]);
        for filter in [serde_json::json!({}),serde_json::json!({"host":"DESKTOP/wsl"})] {
            inp["filters"] = filter;
            let d = super::build(&inp);
            assert_eq!(d["resources"]["rows"][0]["host"],"DESKTOP/wsl");
            assert_eq!(d["resources"]["rows"].as_array().unwrap().len(),1);
        }
    }
    use super::*;

    /// H2: producer verification time must survive the enrolled read-to-page path.
    #[test]
    fn serverq_verified_episode_preserves_producer_time() {
        let mut input: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let p: Value = serde_json::from_str(include_str!("../fixture/serverq/linux.json")).unwrap();
        input["now"] = json!(1792000000u64);
        input["installations"] = json!({"rows": [{"product": "serverq", "installation": "sq-1", "workspace": "ws-1", "host_label": "same", "runtime": "native", "connection": "active", "observation": {"state": "ok", "at": 1792000000u64}, "snapshot": {"schema": "serverq-report/1", "payload": p}}]});
        let doc = build(&input);
        assert_eq!(doc["checks"]["installations"][0]["findings"][0]["verified_at"], 1791907200u64);
    }

    /// D19 U2: cached resource collection failure is unconfirmed, with producer ages and other boards retained.
    #[test]
    fn startup_partial_collection_failure() {
        let mut input: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let good = build(&input);
        input["langfuse"]["error"] = json!("timeout");
        let failed = build(&input);
        assert_eq!(failed["boards"][0], good["boards"][0], "independently fresh board is unaffected");
        for (last, current) in good["resources"]["rows"].as_array().unwrap().iter().zip(failed["resources"]["rows"].as_array().unwrap()) {
            assert_eq!(current["current"], last["current"]);
            assert_eq!(current["source"]["at"], last["source"]["at"]);
            assert_eq!(current["source"]["age_s"], last["source"]["age_s"]);
            assert_eq!(current["source"]["state"], "unconfirmed");
        }
    }

    #[test]
    fn query_validates_every_parameter() {
        assert_eq!(query("").unwrap().0, 1800);
        let (p, f) = query("period=7200&host=fra%2Fnative&agent=codex&session=s-1").unwrap();
        assert_eq!((p, f["host"].as_str()), (7200, Some("fra/native")));
        for bad in ["period=60", "host=a%2Fb", "agent=x", "session=a%20b", "x=1", "host=a&host=b", "period"] {
            assert!(query(bad).is_err(), "{bad}");
        }
    }

    /// #178 A2–A5 / D6: released producer cases must reach the shared JSON/report projection without false health.
    #[test]
    fn taskq_host_observations_released_cases() {
        let bytes = include_bytes!("../fixture/taskq/host-observations.json");
        let hash: String = ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b|format!("{b:02x}")).collect();
        assert_eq!(hash,"c0fba64e81d7845b12ef3c329b05cbe72e7e5cddbf36be8a66e4d1ade9808c56","TaskQ v4.6.30 exact bytes");
        let cases: Value = serde_json::from_str(include_str!("../fixture/taskq/host-observations.json")).unwrap();
        for (name, case) in cases.as_object().unwrap() {
            let obs = case.as_array().cloned().unwrap_or_else(|| vec![case.clone()]);
            let now = if name == "read expired" { 2000 } else { 1000 };
            let inp = json!({"now":now,"projects":["taskqorg/dashq"],"installations":{"source":{"state":"ok"},"rows":[{
                "product":"taskq","connection":"active","workspace":"ws","installation":"tq-one","observation":{"state":"ok"},
                "snapshot":{"schema":"taskq-board/1","payload":{"schema":1,"board":{"kind":"github","host":"github.com","repo":"taskqorg/dashq"},"observations":obs}}
            }]}});
            let doc = build(&inp);
            assert_eq!(doc["schema"], 2);
            for o in obs {
                let h = doc["hosts"].as_array().unwrap().iter().find(|h| h["id"] == o["host"]).expect("observation host must exist");
                let got = h["taskq"]["observations"].as_array().unwrap().iter().find(|v| v["kind"] == o["kind"]).unwrap();
                for k in ["producer","observed","valid_until"] { assert_eq!(got[k],o[k],"{name}: {k}"); }
                assert_eq!(got["board"]["repo"], "taskqorg/dashq");
                if o["complete"] == true { assert_eq!(got["state"],o["state"],"{name}"); }
                else {
                    assert_eq!(got["state"],"unknown","{name}");
                    assert_eq!(got["reason"],o["reason"],"{name}");
                    assert!(got["running"].is_null() && got["queued"].is_null(),"partial counts are not current");
                    assert_ne!(got["claude"],"logged in");
                }
            }
        }
    }

    /// #178 / H2 / D11: identity, expiry, conflict and revocation cannot become positive host evidence.
    #[test]
    fn taskq_host_observations_identity_and_freshness() {
        let cases: Value = serde_json::from_str(include_str!("../fixture/taskq/host-observations.json")).unwrap();
        let online = cases["read fresh"].as_array().unwrap().iter().find(|o|o["host"]=="mac" && o["kind"]=="online").unwrap().clone();
        let record = |o: Value| json!({"product":"taskq","connection":"active","workspace":"ws","installation":"one","projected":true,
            "snapshot":{"schema":"taskq-board/1","payload":{"schema":1,"board":{"kind":"github","host":"github.com","repo":"taskqorg/dashq"},"observations":[o]}}});
        let mut inp = json!({"now":1000,"projects":["taskqorg/dashq"],"boards":{"taskqorg/dashq":{"at":1000,"value":{"date":1000,"hosts":[{
            "title":"taskq host mac","body":"machine DESKTOP wsl","updated_at":"1970-01-01T00:16:40Z"}]}}},"installations":{"rows":[record(online.clone())]}});
        let get = |d: Value| d["hosts"].as_array().unwrap().iter().find(|h|h["id"]=="DESKTOP/wsl").unwrap()["taskq"]["observations"].as_array().unwrap().iter().find(|o|o["kind"]=="online").unwrap().clone();
        assert_eq!(get(build(&inp))["state"],"online");
        let missing = build(&inp);
        assert!(missing["hosts"][0]["taskq"]["observations"].as_array().unwrap().iter().any(|o|o["kind"]=="login" && o["reason"]=="no observation"));
        inp["now"] = json!(1300); assert_eq!(get(build(&inp))["state"],"online","expiry boundary inclusive");
        inp["now"] = json!(1301); assert_eq!(get(build(&inp))["reason"],"expired");
        inp["now"] = json!(999); assert_eq!(get(build(&inp))["reason"],"clock uncertain");
        inp["now"] = json!(1000);
        let mut other = record(online); other["installation"] = json!("two");
        inp["installations"]["rows"].as_array_mut().unwrap().push(other);
        assert_eq!(get(build(&inp))["state"],"online","duplicates do not sum or conflict");
        inp["installations"]["rows"][1]["snapshot"]["payload"]["observations"][0]["state"] = json!("offline");
        assert_eq!(get(build(&inp))["reason"],"conflicting observations");
        for r in inp["installations"]["rows"].as_array_mut().unwrap() { r["connection"] = json!("revoked"); }
        assert!(build(&inp)["hosts"][0]["taskq"]["observations"].as_array().unwrap().is_empty());
        inp["installations"]["rows"][0]["connection"] = json!("active");
        inp["installations"]["rows"][0]["projected"] = json!(false);
        assert!(build(&inp)["hosts"][0]["taskq"]["observations"].as_array().unwrap().is_empty(),"previous grant snapshot is not current");
    }

    #[test]
    fn taskq_snapshot_feeds_boards_questions_and_failures() {
        let doc = build(&serde_json::from_str(include_str!("../fixture/input.json")).unwrap());
        let b = &doc["boards"][0];
        assert_eq!((b["adapter"].as_str(), b["in_work"].as_u64(), b["waiting"].as_u64(), b["ready"].as_u64()), (Some("taskq-snapshot"), Some(2), Some(1), Some(1)));
        let source: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let original = &source["taskq"]["taskqorg/taskq"]["value"]["tasks"];
        for t in b["tasks"].as_array().unwrap() {
            let row = original.as_array().unwrap().iter().find(|r| r["n"] == t["n"]).unwrap();
            assert_eq!(t["revision"], row["revision"]);
            assert_eq!(t["question"], row["question"], "D13 canonical choices/evidence/limits unchanged");
        }
        let q = &b["questions"];
        // parsed card: source k, order and text kept, ★ as recommended; unparsed review card: verbatim, no choices
        assert_eq!(q[1], json!({"id": "https://github.com/taskqorg/taskq/issues/12", "number": 12, "title": "Which option? <b>", "url": "https://github.com/taskqorg/taskq/issues/12", "state": "ask",
            "revision": 6139994000u64, "parsed": true, "done": "checked", "text": "pick a store <i>", "choices": [{"k": 1, "text": "SQLite", "unavailable": null}, {"k": 3, "text": "Files", "unavailable": null}], "recommended": 3, "free_text": true}));
        assert_eq!((q[0]["state"].as_str(), q[0]["text"].as_str(), q[0]["choices"].as_array().map(Vec::len)), (Some("review"), Some("Merge now or wait?"), Some(0)));
        assert_eq!(b["failed"][0]["reason"], "Reason: CI <red> on every runner");
        assert!(doc["attention"]["current"].as_array().unwrap().iter().any(|i| i["what"] == "taskqorg/taskq#14: failed" && i["detail"] == "Reason: CI <red> on every runner"));
        // a failed snapshot reads unknown with its reason, never zero
        let d = &doc["boards"][2];
        assert_eq!((d["adapter"].as_str(), d["source"]["state"].as_str(), d["source"]["reason"].as_str()), (Some("taskq-snapshot"), Some("unknown"), Some("read failed (taskq timeout)")));
        assert!(d["in_work"].is_null() && d["questions"] == json!([]));
        assert_eq!(doc["boards"][1]["adapter"], "taskq-labels");
        assert_eq!(doc["sources"]["taskq"]["state"], "unknown");
    }

    // Chain: enrolled taskq-board/1 task session -> board task row (#1565, #180). Risk: an off-host Codex session shows
    // no progress, an older or another board's record wins, a closed task keeps a stale row, a payload names another
    // host, a future time pins the newest choice, or a worker line reaches the row.
    #[test]
    fn taskq_session_progress_joins_the_board_task_row() {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        let now = inp["now"].as_u64().unwrap();
        let tasks = build(&inp)["boards"][0]["tasks"].clone();
        let n = |state: &str| tasks.as_array().unwrap().iter().find(|t| t["state"] == state).unwrap()["n"].as_u64().unwrap();
        let (doing, ready) = (n("doing"), n("ready"));
        let tasks_of = |at: u64, line: Option<&str>| [doing, ready].map(|n| {
            let mut s = json!({"host": "spoof", "runtime": "codex", "stage": "doing", "at": at});
            if let Some(l) = line {
                s["line"] = json!(l);
            }
            json!({"n": n, "state": "doing", "session": s})
        });
        let rec = |repo: &str, at: u64, line: Option<&str>| json!({"product": "taskq", "connection": "active", "host_label": "win", "snapshot": {"schema": "taskq-board/1", "payload": {"schema": 1,
            "board": {"kind": "github", "host": "github.com", "repo": repo}, "tasks": tasks_of(at, line)}}});
        // A4: a snapshot with and without a worker line renders the same progress row
        let rows = [Some("Running tests"), None].map(|line| {
            inp["installations"] = json!({"rows": [rec("taskqorg/taskq", now - 90, Some("old")), rec("taskqorg/taskq", now - 30, line), rec("taskqorg/other", now, Some("other board")), rec("taskqorg/taskq", now + 60, Some("future"))]});
            let doc = build(&inp);
            let row = |n: u64| doc["boards"][0]["tasks"].as_array().unwrap().iter().find(|t| t["n"] == n).unwrap().clone();
            assert!(row(ready).get("session").is_none(), "only doing and review rows show progress");
            row(doing)["session"].clone()
        });
        assert_eq!(rows[0], json!({"host": "win", "runtime": "codex", "stage": "doing", "age_s": 30}));
        assert_eq!(rows[0], rows[1]);
    }

    #[test]
    fn source_states_never_turn_missing_into_ok() {
        let now = 10_000;
        let s = |v: Value| source(&v, now, 600, None)["state"].clone();
        assert_eq!(s(Value::Null), "unknown");
        assert_eq!(s(json!({"value": null, "at": null, "error": "timeout"})), "unknown");
        assert_eq!(s(json!({"value": 1, "at": 9000, "error": "timeout"})), "unconfirmed");
        assert_eq!(s(json!({"value": 1, "at": 9000, "error": null})), "stale");
        assert_eq!(s(json!({"value": 1, "at": 9900, "error": null, "incomplete": true})), "incomplete");
        assert_eq!(s(json!({"value": 1, "at": 9900, "error": null})), "ok");
        let r = source(&json!({"value": 1, "at": 9000, "error": "timeout"}), now, 600, None);
        assert_eq!(r["reason"], "read failed (timeout), showing 02:30 UTC");
    }

    #[test]
    fn agentq_rows_build_the_fleet_and_join_legacy_hosts() {
        let doc = build(&serde_json::from_str(include_str!("../fixture/input.json")).unwrap());
        let ids: Vec<&str> = doc["hosts"].as_array().unwrap().iter().filter_map(|h| h["id"].as_str()).collect();
        assert_eq!(ids.iter().filter(|i| **i == "fra/native").count(), 1, "legacy and agentq name one host once");
        assert!(ids.contains(&"win-1/native"));
        let fleet = doc["agents"]["fleet"].as_array().unwrap();
        let (fra, win) = (&fleet[0], &fleet[1]);
        assert_eq!((fra["installation"].as_str(), fra["state"].as_str(), fra["version"].as_str()), (Some("inst-fra"), Some("ok"), Some("0.5.0")));
        assert_eq!((fra["runs"].as_u64(), fra["nonzero_exits"].as_u64(), fra["retrievals"].as_u64()), (Some(2), Some(1), Some(1)));
        assert_eq!((win["state"].as_str(), win["failed_retrievals"].as_u64()), (Some("stale"), Some(1)));
        let incidents = doc["attention"]["incidents"].as_array().unwrap();
        assert_eq!(incidents.iter().filter(|i| i["source"] == "agentq").count(), 1, "one event id is one incident");
        assert!(doc["attention"]["gaps"].as_array().unwrap().iter().any(|g| g["what"] == "agentq installation inst-win: stale"));
    }

    /// Chain hosts.md H1-H7 (#140): live-shaped sources give one Hosts row per machine and kind; taskq heartbeats join
    /// by their machine line, the freshest agentq installation fills the agentq columns and an older one is superseded
    /// with its reason. Risk: one machine split over several rows, or a fresh host with empty agentq columns.
    #[test]
    fn hosts_join_one_row_per_machine() {
        let mut fx: Value = serde_json::from_str(include_str!("../fixture/hosts.json")).unwrap();
        let doc = build(&fx["input"]);
        if std::env::var_os("DASHQ_UPDATE_FIXTURE").is_some() {
            fx["hosts"] = doc["hosts"].clone();
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/fixture/hosts.json"), serde_json::to_string_pretty(&fx).unwrap() + "\n").unwrap();
        }
        assert_eq!(doc["hosts"], fx["hosts"], "rerun with DASHQ_UPDATE_FIXTURE=1 after an intended change");
        let hosts = doc["hosts"].as_array().unwrap();
        let ids: Vec<&str> = hosts.iter().filter_map(|h| h["id"].as_str()).collect();
        assert_eq!(ids, ["DESKTOP/native", "DESKTOP/wsl", "hermes-fra1/native", "macbook-m2.local/native", "wsl"]);
        for h in hosts.iter().filter(|h| h["freshness"]["state"] == "ok") {
            assert!(h["agentq"]["running"].is_string() && h["agentq"]["latest"].is_string() && h["agentq"]["behind"].is_boolean(), "{h}");
            assert!(h["reason"].is_null(), "{h}");
        }
        assert_eq!(hosts[3]["taskq"]["version"], "4.2.0", "H3: the newest version of two heartbeats");
        assert_eq!(hosts[4]["reason"], "no machine line: update taskq");
        let old = doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-win-old").unwrap();
        assert_eq!(old["reason"], "superseded by inst-win-new 0.6.0");
        assert!(!doc["attention"]["gaps"].to_string().contains("inst-win-old"), "a superseded installation is no gap");
        // D6: a failed, stale or capped Langfuse read never leaves a Hosts row green.
        let now = fx["input"]["now"].as_u64().unwrap();
        for (k, v, state) in [("error", json!("timeout"), "unconfirmed"), ("at", json!(now - 3600), "stale"), ("incomplete", json!(true), "incomplete")] {
            let mut inp = fx["input"].clone();
            inp["langfuse"][k] = v;
            let mac = build(&inp)["hosts"][3]["freshness"].clone();
            assert_eq!(mac["state"], state, "{k}: {mac}");
            assert!(mac["reason"].as_str().is_some_and(|r| r.starts_with("Langfuse: ")), "{k}: {mac}");
        }
    }

    /// Chain #116 A1: routing, hook and silent-compression failures of AgentQ installations reach Attention with host,
    /// client and reason, and clear once the installation routes and compresses again. Risk: a fleet that reads ok while
    /// every client bypasses AgentQ.
    #[test]
    fn agentq_coverage_failures_reach_attention_and_clear() {
        let now = 1_791_875_139;
        let at = |ago: u64| crate::source::iso(now - ago);
        let aq_row = |name: &str, inst: &str, host: &str, agent: Option<&str>| {
            let mut m = json!({"attributes.agentq.installation": inst, "attributes.host.name": host, "attributes.agentq.runtime": "linux"});
            if let Some(a) = agent {
                m["attributes.agentq.agent"] = json!(a);
            }
            json!({"id": format!("{name}-{inst}-{}", agent.unwrap_or("")), "name": name, "startTime": at(120), "traceId": "t1", "metadata": m})
        };
        let call = |host: &str, agent: &str| json!({"id": format!("g-{host}-{agent}"), "type": "GENERATION", "startTime": at(60), "traceId": "g1", "tags": [format!("host:{host}"), "runtime:native", format!("agent:{agent}")]});
        let cases: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        let mut payload = cases["cases"]["profile_state_and_revision"].clone();
        payload["configuration"]["profiles"][1]["owned"] = json!(true);
        payload["configuration"]["profiles"][1]["hooks"] = json!("missing");
        let mut inp = json!({
            "now": now, "period": 1800, "filters": {}, "projects": ["taskqorg/dashq"],
            "boards": {"taskqorg/dashq": {"at": now, "value": {"issues": [], "date": now, "hosts": [{"title": "taskq host WIN-1", "body": "version 4.2.0
#7 a1", "updated_at": at(60)}]}}},
            "langfuse": {"at": now, "value": {
                "agentq.installation": [aq_row("agentq.installation", "inst-fra", "fra", None), aq_row("agentq.installation", "inst-win", "win-1", None)],
                "activity": [call("fra", "codex"), call("fra", "claude-code")],
                "agentq.compress": [aq_row("agentq.compress", "inst-fra", "fra", Some("claude"))]}},
            "installations": {"source": {"state": "ok"}, "rows": [{"installation": "inst-mac", "product": "agentq", "connection": "active", "host_label": "mac", "runtime": "macos",
                "observation": {"state": "ok"}, "snapshot": {"payload": payload}}]},
        });
        let rows = |inp: &Value| -> Vec<(String, String, String)> {
            let doc = build(inp);
            doc["attention"]["current"].as_array().unwrap().iter().filter(|i| i["source"] == "agentq").map(|i| (i["host"].as_str().unwrap().into(), i["client"].as_str().unwrap().into(), i["what"].as_str().unwrap().into())).collect()
        };
        let mut got = rows(&inp);
        got.sort();
        let want: Vec<(String, String, String)> = [
            ("fra/native", "Codex", "Codex: no compression records"),
            ("mac/native", "Claude Code", "Claude Code: hooks missing"),
            ("mac/native", "Claude Code", "Claude Code: routing direct"),
            ("mac/native", "Codex", "Codex: routing direct"),
            ("win-1/native", "TaskQ workers", "TaskQ workers: no activity or compression records"),
        ]
        .iter()
        .map(|(h, c, w)| (h.to_string(), c.to_string(), w.to_string()))
        .collect();
        assert_eq!(got, want);
        // D6: a cached Langfuse read after a timeout, or a capped query, never asserts absence; it is a gap per installation
        let gaps = |inp: &Value| -> Vec<(String, String)> {
            let doc = build(inp);
            doc["attention"]["gaps"].as_array().unwrap().iter().filter(|i| i["source"] == "agentq").map(|i| (i["host"].as_str().unwrap().into(), i["what"].as_str().unwrap().into())).collect()
        };
        let coverage_rows = |inp: &Value| rows(inp).into_iter().filter(|r| r.0 != "mac/native").collect::<Vec<_>>();
        let unknown = vec![("fra/native".to_string(), "AgentQ coverage unknown".to_string()), ("win-1/native".into(), "AgentQ coverage unknown".into())];
        let mut timed_out = inp.clone();
        timed_out["langfuse"]["error"] = json!("timeout");
        assert_eq!(coverage_rows(&timed_out), vec![]);
        assert_eq!(gaps(&timed_out), unknown);
        let mut capped = inp.clone();
        capped["langfuse"]["value"]["truncated"] = json!(["activity"]);
        capped["langfuse"]["incomplete"] = json!(true);
        assert_eq!(coverage_rows(&capped), vec![]);
        assert_eq!(gaps(&capped), unknown);
        // an unconfirmed board read never asserts that TaskQ workers run without records
        let mut board_down = inp.clone();
        board_down["boards"]["taskqorg/dashq"]["error"] = json!("timeout");
        assert!(!rows(&board_down).iter().any(|r| r.1 == "TaskQ workers"));
        assert_eq!(gaps(&board_down), vec![("win-1/native".to_string(), "TaskQ workers: liveness unknown".to_string())]);
        // recovery: the next complete read asserts again
        assert_eq!(rows(&inp).len(), 5);
        assert_eq!(gaps(&inp), vec![]);

        let doc = build(&inp);
        let win = doc["attention"]["current"].as_array().unwrap().iter().find(|i| i["host"] == "win-1/native").unwrap();
        assert_eq!(win["detail"], "TaskQ attempts taskqorg/dashq#7 run on this host, but no activity or compression records arrived in 30m (installation inst-win).");

        // agentq fixes routing: clients route, hooks registered, compression and activity records arrive
        for p in inp["installations"]["rows"][0]["snapshot"]["payload"]["configuration"]["profiles"].as_array_mut().unwrap() {
            p["routing"] = json!("routed");
            p["hooks"] = json!("registered");
        }
        let v = &mut inp["langfuse"]["value"];
        v["agentq.compress"].as_array_mut().unwrap().extend([aq_row("agentq.compress", "inst-fra", "fra", Some("codex")), aq_row("agentq.compress", "inst-win", "win-1", Some("codex"))]);
        v["activity"].as_array_mut().unwrap().push(call("win-1", "codex"));
        assert_eq!(rows(&inp), vec![]);
    }
}
