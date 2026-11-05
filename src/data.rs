//! The `/data` document (dashq.md D6, D7) from source snapshots: pure, so a fixture pins it.
//!
//! Input: `{now, period, filters, projects, serverq_host, langfuse, boards: {repo: snap}, releases: {repo: snap}, serverq}`;
//! a snap is `{value, at, error, incomplete}` (source.rs).

use crate::source::{clock, iso_secs};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PERIODS: [u64; 3] = [1800, 7200, 86400];
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

/// Validated installation identity; host rows are named separately by `Machines`.
pub(crate) fn aq_identity(row: &Value) -> Option<String> {
    let inst = aq(row, "agentq.installation").as_str().filter(|i| safe_id(i))?;
    aq(row, "host.name").as_str().filter(|h| safe(h, 64, b"_.-"))?;
    Some(inst.into())
}

fn agent_name(a: &str) -> Option<&'static str> {
    crate::runtimes::client_key(a).map(|c| c.label)
}

/// `period=&host=&agent=&session=&block=&cursor=` (D7): the period and the filters, or `Err` for 400.
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
            // #181 A6: a telemetry block event id; it adds `telemetry_block` and filters nothing else
            "block" => crate::enroll::id(&v, 128),
            // D23: offset into the capped lists
            "cursor" => v.len() <= 9 && v.parse::<usize>().is_ok(),
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
        0..60 => "<1m".into(),
        60..3600 => format!("{}m", s / 60),
        3600..86400 => format!("{}h", s / 3600),
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

    /// A configured `<name>/<kind>` id in the same spelling.
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

const AQ_KEYS: [&str; 4] = ["agentq.installation", "agentq.run", "agentq.retrieval", "agentq.incident"];

fn observations<'a>(lf: &'a Value, k: &str) -> Vec<&'a Value> {
    // Langfuse v4 synthetic roots mirror real spans: count real observations only.
    lf["value"][k].as_array().map(|a| a.iter().filter(|r| !r["id"].as_str().unwrap_or("").starts_with("t-")).collect()).unwrap_or_default()
}

pub fn build(full: &Value) -> Value {
    // #242 A3: a retired installation leaves every active view (hosts, checks, versions, Attention); `installations`
    // keeps it with its history
    let retired = |r: &Value| r["activity"]["state"] == "retired";
    let trimmed;
    let inp = if full["installations"]["rows"].as_array().is_some_and(|rows| rows.iter().any(retired)) {
        let mut c = full.clone();
        c["installations"]["rows"].as_array_mut().unwrap().retain(|r| !retired(r));
        trimmed = c;
        &trimmed
    } else {
        full
    };
    let now = inp["now"].as_u64().unwrap_or(0);
    let period = inp["period"].as_u64().unwrap_or(1800);
    let f = &inp["filters"];
    let fhost = f["host"].as_str();

    let lf = &inp["langfuse"];
    let lf_src = source(lf, now, SNAPSHOT_LIMIT, None);
    let rows = |k: &str| observations(lf, k);
    let capped = |k: &str| lf["value"]["truncated"].as_array().is_some_and(|t| t.iter().any(|x| x == k));
    let mut hosts: BTreeMap<String, Value> = BTreeMap::new();
    let (mut current, mut gaps, mut history) = (Vec::new(), Vec::new(), Vec::new());
    let machines = Machines::new(AQ_KEYS.iter().flat_map(|k| rows(k)), now);

    let (activity, unattributed) = agents::activity(inp, &mut current);
    let fleet = agents::fleet(inp, &machines, &mut hosts, &mut gaps, &mut history);
    let boards::Read { boards, heartbeats, gh_states, taskq_states, working, unconfirmed, taskq_hosts } =
        boards::read(inp, &machines, &mut hosts, &mut current);
    let latest = |repo: &str| inp["releases"][repo]["value"].clone();
    let clients = agents::clients(inp, &fleet, &activity, &working, &unconfirmed, &mut current, &mut gaps);
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
    let enrolled_checks = checks::serverq_enrolled(inp, now, &machines, &mut hosts, &mut current, &mut gaps, &mut history);

    boards::taskq_observations(inp, now, &machines, &taskq_hosts, &mut hosts);
    for h in hosts.values_mut() {
        let id = h["id"].as_str().unwrap_or("").to_ascii_lowercase();
        h["taskq"]["observed_running"] = if unconfirmed.contains_key(&id) { Value::Null } else {
            working.get(&id).map_or(Value::Null, |tasks| json!(tasks.iter().collect::<HashSet<_>>().len()))
        };
        h["taskq"]["cli_heartbeats"] = json!(heartbeats.values().filter(|r| r["host"] == h["id"]).collect::<Vec<_>>());
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
    let cursor = f["cursor"].as_str().and_then(|c| c.parse().ok()).unwrap_or(0);
    let agent_work = crate::agent_work::build(inp);
    // listed only when configured, like the taskq adapter
    if agent_work["source"]["reason"] != "not configured" {
        sources["agent_work"] = agent_work["source"].clone();
        att_states.push(("Agent work".to_string(), agent_work["source"].clone()));
    }
    let model_use = crate::model_use::build(inp, &agent_work);
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
        "model_use": model_use,
        "runtimes": crate::runtimes::descriptors(),
    });
    // D28: only once a row in scope names the schema or stored observations
    if !inp["observations"].is_null() {
        doc["sources"]["observations"] = inp["observations"]["source"].clone();
        doc["observations"] = inp["observations"].clone();
    }
    doc["fleet"] = crate::fleet::build(inp, &doc["hosts"]);
    if doc["fleet"]["hosts"].as_array().is_some_and(|a| !a.is_empty()) { doc["sources"]["fleet"] = doc["fleet"]["source"].clone(); }
    // D23: changes while away, person requests and the query shape, from the parts above
    let mut sources: Vec<(&str, Option<String>)> = Vec::new();
    if !sq.is_null() || doc["checks"]["installations"].as_array().is_some_and(|a| !a.is_empty()) {
        let capped = doc["checks"]["installations"].as_array().into_iter().flatten().any(|c| c["findings_incomplete"] == true);
        sources.push(("serverq", match (doc["checks"]["installations"].as_array().is_some_and(|a| !a.is_empty()), capped) {
            (false, _) => Some("no baseline: the local report keeps no verified end time".into()),
            (true, true) => Some("episode history capped".into()),
            _ => None,
        }));
    }
    if !lf.is_null() {
        sources.push(("agentq", (!matches!(lf_src["state"].as_str(), Some("ok" | "incomplete"))).then(|| format!("Langfuse: {}", lf_src["reason"].as_str().unwrap_or("read not confirmed")))));
        sources.push(("Langfuse", Some("no baseline: model errors carry no start time".into())));
    }
    if !doc["boards"].as_array().is_none_or(Vec::is_empty) {
        sources.push(("taskq", Some("no baseline: TaskQ reports no state change times".into())));
    }
    let items: Vec<Value> = doc["attention"]["current"].as_array().into_iter().flatten().chain(doc["attention"]["gaps"].as_array().into_iter().flatten()).cloned().collect();
    // incident events, and every retained ServerQ episode (not only the 24 h history list)
    let mut events: Vec<Value> = doc["attention"]["incidents"].as_array().into_iter().flatten().filter(|i| i["source"] != "serverq").cloned().collect();
    for c in doc["checks"]["installations"].as_array().into_iter().flatten() {
        events.extend(c["findings"].as_array().into_iter().flatten().map(|f| json!({"source": "serverq", "host": c["host"], "installation": c["installation"],
            "what": format!("{}: {}", f["rule"].as_str().unwrap_or("unknown"), f["state"].as_str().unwrap_or("unknown")), "since": f["first_seen"], "verified_at": f["verified_at"], "episode": f})));
    }
    doc["changes"] = crate::explain::changes(&items, &events, &sources, now, period, cursor);
    let longest = doc["changes"]["rows_total"].as_u64().max(doc["agent_work"]["attempts_total"].as_u64()).unwrap_or(0) as usize;
    doc["query"] = crate::explain::query(period, now, cursor, longest);
    // retired rows stay listed here, with their history, behind the Connections filter (#242 A3)
    if full["installations"].is_object() {
        doc["installations"] = full["installations"].clone();
        for r in doc["installations"]["rows"].as_array_mut().into_iter().flatten() {
            r["route"] = crate::explain::route(r);
        }
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
    let mut requests = crate::explain::requests(&doc, now);
    // #242 A1 (owner decision): an installation silent for over an hour needs a person to reconnect it
    for r in doc["installations"]["rows"].as_array().into_iter().flatten().filter(|r| r["activity"]["state"] == "silent") {
        let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
        let host = machines.id(label, if r["runtime"] == "wsl" { "wsl" } else { "native" });
        let a = &r["activity"];
        let since = if a["last_report"].is_null() { Value::Null } else { a["last_report"].clone() };
        let target = format!("/integrations?period={period}#inst-{}-{}", crate::source::enc(r["installation"].as_str().unwrap_or("")), crate::source::enc(r["workspace"].as_str().unwrap_or("")));
        requests.push(json!({"kind": "silent", "source": r["product"], "host": host, "project": null, "task": null, "rule": null, "installation": r["installation"],
            "reason": format!("{} silent: {}", r["product"].as_str().unwrap_or("installation"), a["reason"].as_str().unwrap_or("no report")),
            "since": since, "age_s": a["age_s"], "action": "Reconnect", "target": target}));
    }
    doc["attention"]["requests"] = requests.into();
    doc
}

/// One open question in the S3 shape: source choice ids, order and text kept; a card that does not parse
/// keeps its verbatim text and no choices. A choice the answer route refuses names why (D13).
fn question(t: &Value) -> Value {
    let q = &t["question"];
    let parsed = q["parsed"] == true;
    let choices: Vec<Value> = q["choices"].as_array().into_iter().flatten().filter(|_| parsed).map(|c| json!({"k": c["id"], "text": c["text"], "unavailable": crate::answer::unavailable(c)})).collect();
    let recommended = q["choices"].as_array().into_iter().flatten().filter(|_| parsed).find(|c| c["recommended"] == true).map_or(Value::Null, |c| c["id"].clone());
    let text = if parsed { q["problem"].clone() } else { q["text"].clone() };
    json!({"id": t["url"], "number": t["n"], "title": t["title"], "url": t["url"], "state": t["state"], "revision": t["revision"],
           "parsed": parsed, "done": q["done"], "text": text, "choices": choices, "recommended": recommended, "free_text": t["state"] == "ask"})
}

/// 503 when every source failed with no value to show (D7).
pub fn all_failed(doc: &Value) -> bool {
    let sources: Vec<&Value> = doc["sources"].as_object().into_iter().flatten()
        .filter(|(name, x)| name.as_str() != "export" && x["reason"] != "not configured").map(|(_, x)| x).collect();
    !sources.is_empty() && sources.iter().all(|x| x["state"] == "unknown" && x["reason"].as_str().is_some_and(|r| r.contains("read failed") && !r.contains("refresh in progress")))
}

mod agents;
mod boards;
mod checks;

#[cfg(test)]
mod tests;
