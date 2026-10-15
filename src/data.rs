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
const SERVICES: [&str; 7] = ["contextq", "serverq", "taskq", "gateway", "hermes-gateway", "langfuse", "updater"];
const USAGE: [&str; 8] = ["input", "output", "total", "input_cached_tokens", "cache_read_input_tokens", "input_cache_creation_1h", "input_cache_creation_5m", "output_reasoning_tokens"];
const NATIVE_FAILURE: [&str; 27] = [
    "schema", "at", "jobId", "version", "stage", "nodeCode", "exitCode", "signal", "killed", "stdoutCapturedBytes", "stderrCapturedBytes", "outputCapped", "elapsedMs", "sdkPhase", "httpStatus",
    "sdkErrorType", "sdkFailure", "sdkRequests", "sdkMaxBodyBytes", "sdkMaxSpanBytes", "sdkMaxSpans", "sdkRequestFinishMs", "sdkResponseMs", "sdkResponseStatus", "sdkRequestMs", "sdkTimedOut", "sdkSocketWrittenBytes",
];
const NORMAL: [&str; 9] = ["compressed", "retrieval", "content_type", "ineligible", "no_gain", "empty", "too_large", "request_budget", "passthrough"];

fn reason_text(code: &str) -> &'static str {
    match code {
        "worker_retiring" => "New connections use the new version; the old one serves existing ones.",
        "worker_update_failed" => "New worker failed its check or crashed; see incidents.",
        "native_stats_unavailable" => "Headroom counters unavailable; Codex processing unconfirmed.",
        "native_exporter_missing" => "Native exporter not connected or its hooks are off.",
        "telemetry_delivery_unverified" => "Delivery to Langfuse unconfirmed; the monitor retries.",
        "proxy_health_not_agent_liveness" => "Proxy reachable; agent activity is shown apart.",
        "proxy_unavailable" => "Proxy unreachable: check the local ContextQ service.",
        "compression_telemetry_disabled" => "Compression telemetry is off in the proxy.",
        "telemetry_delivery_failed" => "Telemetry send failed; data stays queued.",
        "delivery_backlog" => "Data waits for delivery over 10 min.",
        "queue_unreadable" => "Telemetry queue unreadable.",
        "endpoint_reachable" => "Service answered its check.",
        "endpoint_unavailable" => "Service did not answer its check.",
        "automated_checks_only" => "Automated serverq checks only; manual checks not verified.",
        "status_snapshot" => "Status report received.",
        "report_unavailable" => "No status report: connect the source.",
        "stale_report" => "Status report is stale.",
        "invalid_report" => "Status report is invalid.",
        "updates_disabled" => "Auto-updates are off.",
        "update_migration_required" => "One-time package migration needed.",
        "update_failed" => "Update failed its check; the working version stays.",
        "release_subscription_connected" => "Release subscription works; hourly fallback check.",
        "release_subscription_unavailable" => "Release subscription unconfirmed.",
        "invalid_report_policy" => "Invalid report age setting.",
        _ => "No explanation for this state.",
    }
}

fn compression_failure(outcome: &str) -> Option<&'static str> {
    Some(match outcome {
        "bridge_unavailable" => "compressor file unavailable",
        "bridge_failed" => "compressor failed to start",
        "missing_current_query" => "current query unknown",
        "timeout" => "compression timeout",
        "missing_key" | "key_conflict" | "invalid_key_config" => "compressor access misconfigured",
        "request_failed" | "invalid_response" => "compressor request failed",
        "neural_unavailable" => "SuperCompress Neural unavailable",
        "fallback" => "context processing failed",
        "lost_critical_evidence" => "compressor lost critical data",
        "invalid_device_config" => "invalid device registration",
        o if o.len() == 8 && o.starts_with("http_") && matches!(o.as_bytes()[5], b'4' | b'5') && o[6..].bytes().all(|b| b.is_ascii_digit()) => "compressor HTTP error",
        _ => return None,
    })
}

fn incident_title(code: &str) -> String {
    if code.len() == 8 && code.starts_with("http_") {
        return format!("Service answered {}", &code[5..]);
    }
    match code {
        "timeout" => "Service timeout",
        "startup_failed" => "Service failed to start",
        "configuration_drift" => "Configuration changed",
        "invalid_response" => "Invalid response",
        "request_failed" => "Request failed",
        "missing_key" => "Access not configured",
        "delivery_failed" => "Telemetry delivery failed",
        "runtime_integrity" => "Integrity check failed",
        _ => "Local ContextQ incident",
    }
    .into()
}

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

/// One Hosts row (hosts.md H1-H7): freshness from the row's agentq heartbeat; `snapshot` only while a legacy health
/// row exists for it (H5); `reason` names a row that joins no machine (H2).
fn host_row(id: &str) -> Value {
    json!({
        "id": id,
        "reason": null,
        "freshness": {"state": "unknown", "at": null, "age_s": null, "reason": "no agentq heartbeat"},
        "snapshot": null,
        "taskq": {"version": null, "latest": null, "behind": null, "state": "unknown", "age_s": null, "reason": "no heartbeat"},
        "agentq": {"installation": null, "running": null, "monitor": null, "proxy": null, "selected": null, "available": null, "latest": null, "updater": null, "subscription": null, "behind": null},
        "serverq": {"status": null, "state": "unknown", "age_s": null, "reason": "no serverq report for this host"},
        "services": [],
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
        let wsl = rt == "wsl" || (rt == "linux" && self.windows.contains(&name.to_ascii_lowercase()));
        Some(self.id(name, if wsl { "wsl" } else { "native" }))
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
    let mut counters = Vec::new();
    let (mut legacy, mut heartbeats): (Vec<Value>, BTreeMap<String, Value>) = (Vec::new(), BTreeMap::new());
    const AQ_KEYS: [&str; 4] = ["agentq.installation", "agentq.run", "agentq.retrieval", "agentq.incident"];
    let machines = Machines::new(AQ_KEYS.iter().flat_map(|k| rows(k)), now);

    // Health snapshots: a device's newest snapshot owns its identity (old labels never duplicate a host).
    let health = rows("health");
    let device = |r: &Value| tag(r, "device").map(String::from).unwrap_or_else(|| format!("local:{}", identity(r).0.unwrap_or_default()));
    let mut newest: HashMap<String, u64> = HashMap::new();
    for r in &health {
        if let Some(a) = row_at(r).filter(|a| *a <= now + 60) {
            let e = newest.entry(device(r)).or_default();
            *e = (*e).max(a);
        }
    }
    for r in &health {
        let (Some(id), agent) = identity(r) else { continue };
        let (Some(at), Some(service), Some(state)) = (row_at(r), tag(r, "service"), tag(r, "state")) else { continue };
        if newest.get(&device(r)) != Some(&at) || !SERVICES.contains(&service) || !["ok", "warning", "error", "unknown"].contains(&state) {
            continue;
        }
        let h = host_entry(&mut hosts, &machines.canon(&id));
        if h["snapshot"]["at"].as_u64().is_none_or(|x| at > x) {
            h["snapshot"] = source(&json!({"value": true, "at": at}), now, SNAPSHOT_LIMIT, None);
        }
        let fresh = now.saturating_sub(at) <= SNAPSHOT_LIMIT;
        if service == "updater" {
            let u = |k: &str| attr(r, &format!("update.{k}"));
            let a = &mut h["agentq"];
            a["selected"] = semver(u("installed"));
            a["available"] = semver(u("available"));
            a["updater"] = u("status").as_str().filter(|s| safe(s, 32, b"_")).map_or(json!("unknown"), |s| json!(s));
            a["subscription"] = u("subscription").as_str().filter(|s| ["connected", "reconnecting", "disabled"].contains(s)).map_or(json!("unknown"), |s| json!(s));
            if let Some(d) = tag(r, "device").filter(|d| safe_id(d)) {
                legacy.push(json!({"device": d, "host": id, "at": at, "monitor": semver(attr(r, "version")), "installed": a["selected"], "available": a["available"]}));
            }
        }
        if !semver(attr(r, "version")).is_null() {
            h["agentq"]["monitor"] = semver(attr(r, "version"));
        }
        if service == "contextq" && !semver(attr(r, "proxy.version")).is_null() {
            h["agentq"]["proxy"] = semver(attr(r, "proxy.version"));
        }
        let agent = agent.filter(|a| agent_name(a).is_some());
        let mut metrics = Map::new();
        if let Some(meta) = r["metadata"].as_object() {
            for (k, v) in meta {
                if let (Some(m), Some(n)) = (k.strip_prefix("attributes.agent_context.monitor."), num(v))
                    && safe(m, 64, b"_")
                {
                    metrics.insert(m.into(), json!(n));
                }
            }
        }
        let native_failure = (fresh && !attr(r, "native_failure.schema").is_null()).then(|| {
            let mut o = Map::new();
            for k in NATIVE_FAILURE {
                let v = attr(r, &format!("native_failure.{k}"));
                let ok = v.is_number() || v.is_boolean() || v.as_str().is_some_and(|s| safe(s, 64, b"_.:-"));
                o.insert(k.into(), if ok { v.clone() } else { Value::Null });
            }
            Value::Object(o)
        });
        let svc = json!({
            "service": service, "agent": agent, "at": at, "trace_id": trace(r),
            "state": if fresh { state } else { "unknown" },
            "reason": if fresh { reason_text(r["statusMessage"].as_str().unwrap_or("")).to_string() } else { "no check in 10 min".into() },
        });
        let what = format!("{service}{}", agent.map(|a| format!(" · {}", agent_name(a).unwrap())).unwrap_or_default());
        let item = json!({"source": "Langfuse", "host": id, "what": format!("{what}: {}", svc["state"].as_str().unwrap()), "since": at, "detail": svc["reason"], "trace_id": svc["trace_id"], "native_failure": native_failure});
        match svc["state"].as_str().unwrap() {
            "error" | "warning" => current.push(item),
            "unknown" => gaps.push(item),
            _ => {}
        }
        if !metrics.is_empty() {
            counters.push(json!({"host": id, "service": service, "agent": agent, "at": at, "metrics": metrics}));
        }
        h["services"].as_array_mut().unwrap().push(svc);
    }
    for (id, h) in &hosts {
        if !h["snapshot"].is_null() && h["snapshot"]["state"] != "ok" {
            gaps.push(json!({"source": "Langfuse", "host": id, "what": "no fresh host snapshot", "since": h["snapshot"]["at"], "detail": "Last snapshot older than 10 min: check the monitor and its delivery.", "trace_id": null}));
        }
    }

    // Agents: activity, compression, native RTK per host and agent (D8.4).
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
    let mut compression: BTreeMap<(String, String), Value> = BTreeMap::new();
    for r in rows("compression").into_iter().filter(|r| matches(r) && in_period(r).is_some()) {
        let Some(k) = keyed(r) else { continue };
        let c = compression.entry(k.clone()).or_insert_with(|| {
            json!({"host": k.0, "agent": k.1, "compressed": 0, "normal_skips": 0, "failed": 0, "unknown": 0, "outcomes": {}, "chars_before": 0, "chars_after": 0, "size_samples": 0, "trace_id": null, "error_trace_id": null, "unknown_trace_id": null})
        });
        let outcome = attr(r, "compression.outcome").as_str().unwrap_or("");
        let failure = compression_failure(outcome);
        if !NORMAL.contains(&outcome) && failure.is_none() && r["level"] != "ERROR" {
            add(c, "unknown", 1.0);
            if c["unknown_trace_id"].is_null() {
                c["unknown_trace_id"] = trace(r);
            }
            continue;
        }
        let kind = if r["level"] == "ERROR" || failure.is_some() { "failed" } else if outcome == "compressed" { "compressed" } else { "normal_skips" };
        add(c, kind, 1.0);
        add(&mut c["outcomes"], if kind == "failed" { failure.unwrap_or("compression error") } else { outcome }, 1.0);
        if let (Some(b), Some(a)) = (num(attr(r, "compression.input_chars")), num(attr(r, "compression.output_chars"))) {
            add(c, "chars_before", b);
            add(c, "chars_after", a);
            add(c, "size_samples", 1.0);
        }
        if c["trace_id"].is_null() {
            c["trace_id"] = trace(r);
        }
        if kind == "failed" && c["error_trace_id"].is_null() {
            c["error_trace_id"] = trace(r);
        }
    }
    let mut rtk: BTreeMap<(String, String), Value> = BTreeMap::new();
    let mut seen = HashSet::new();
    for r in rows("native_tools").into_iter().filter(|r| matches(r) && in_period(r).is_some()) {
        let Some(k) = keyed(r) else { continue };
        let event = attr(r, "compression.id").as_str().filter(|e| e.len() == 36 && e.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'));
        let dev = tag(r, "device").or(attr(r, "device").as_str()).filter(|d| safe_id(d));
        // ponytail: old events without device/event ids dedup by observation id; upgrade when producers send both
        let dedup = match (dev, event, r["id"].as_str()) {
            (Some(d), Some(e), _) => Some(format!("{d}/{}/{}/{e}", k.0, k.1)),
            (_, _, Some(id)) => Some(format!("span:{id}")),
            _ => None,
        };
        if dedup.as_ref().is_some_and(|d| !seen.insert(d.clone())) {
            continue;
        }
        let x = rtk.entry(k.clone()).or_insert_with(|| {
            json!({"host": k.0, "agent": k.1, "calls": 0, "samples": 0, "input_estimate": null, "output_estimate": null, "reduced": 0, "unchanged": 0, "expanded": 0, "unknown": 0,
                   "origins": {"production": 0, "qualification": 0, "unknown": 0}, "session_links": 0, "call_links": 0, "exit_samples": 0, "nonzero_exits": 0, "trace_id": null})
        });
        add(x, "calls", 1.0);
        if r["sessionId"].as_str().is_some_and(safe_id) {
            add(x, "session_links", 1.0);
        }
        if attr(r, "tool_call_id").as_str().is_some_and(safe_id) {
            add(x, "call_links", 1.0);
        }
        let origin = attr(r, "compression.origin").as_str().filter(|o| ["production", "qualification"].contains(o)).unwrap_or("unknown");
        add(&mut x["origins"], origin, 1.0);
        let (b, a) = (num(attr(r, "compression.input_tokens")), num(attr(r, "compression.output_tokens")));
        match (b, a) {
            (Some(b), Some(a)) if attr(r, "compression.strategy") == "rtk-native" && attr(r, "compression.token_source") == "rtk_estimate" => {
                add(x, "samples", 1.0);
                add(x, "input_estimate", b);
                add(x, "output_estimate", a);
                add(x, if a < b { "reduced" } else if a > b { "expanded" } else { "unchanged" }, 1.0);
            }
            _ => add(x, "unknown", 1.0),
        }
        if let Some(e) = attr(r, "compression.exit_code").as_i64() {
            add(x, "exit_samples", 1.0);
            if e != 0 {
                add(x, "nonzero_exits", 1.0);
            }
        }
        if x["trace_id"].is_null() {
            x["trace_id"] = trace(r);
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
    let compression: Vec<Value> = compression
        .into_values()
        .map(|mut c| {
            let name = agent_name(c["agent"].as_str().unwrap()).unwrap();
            if c["failed"] != 0 {
                current.push(json!({"source": "Langfuse", "host": c["host"], "what": format!("{name}: compression failures"), "since": null, "detail": format!("{} measured calls failed in the period.", c["failed"]), "trace_id": c["error_trace_id"]}));
            }
            if c["unknown"] != 0 {
                gaps.push(json!({"source": "Langfuse", "host": c["host"], "what": format!("{name}: unknown compression outcome"), "since": null, "detail": format!("{} records with a missing or unknown outcome; counted as neither success nor failure.", c["unknown"]), "trace_id": c["unknown_trace_id"]}));
            }
            // character reduction only when every counted call has before/after
            let counted = ["compressed", "normal_skips", "failed"].iter().map(|k| c[*k].as_f64().unwrap()).sum::<f64>();
            if c["size_samples"].as_f64() != Some(counted) || counted == 0.0 {
                c["chars_before"] = Value::Null;
                c["chars_after"] = Value::Null;
            }
            c
        })
        .collect();
    let rtk: Vec<Value> = rtk
        .into_values()
        .inspect(|x| {
            let calls = &x["calls"];
            if x["unknown"] != 0 || x["session_links"] != *calls || x["call_links"] != *calls || x["origins"]["unknown"] != 0 {
                let name = agent_name(x["agent"].as_str().unwrap()).unwrap();
                gaps.push(json!({"source": "Langfuse", "host": x["host"], "what": format!("{name}: incomplete RTK accounting"),
                    "since": null, "detail": format!("Events {calls}; with session id {}; with tool-call id {}; without estimate {}; unknown origin {}.", x["session_links"], x["call_links"], x["unknown"], x["origins"]["unknown"]), "trace_id": x["trace_id"]}));
            }
        })
        .collect();

    // Incident history: 24 h, deduplicated, never current.
    let mut by_fp: BTreeMap<String, Value> = BTreeMap::new();
    for r in rows("incidents") {
        let Some(at) = row_at(r).filter(|a| *a + 86400 >= now && *a <= now + 60) else { continue };
        let (h, a) = identity(r);
        let fp = attr(r, "incident.fingerprint").as_str().filter(|x| x.len() == 24 && x.bytes().all(|b| b.is_ascii_hexdigit())).map(String::from).or(trace(r).as_str().map(String::from));
        let Some(fp) = fp else { continue };
        let item = json!({"source": "Langfuse", "host": h, "what": format!("{}: {}", a.and_then(agent_name).unwrap_or("ContextQ"), incident_title(r["statusMessage"].as_str().unwrap_or(""))),
            "since": at, "occurrences": num(attr(r, "incident.occurrences")), "trace_id": trace(r)});
        if by_fp.get(&fp).is_none_or(|o| o["since"].as_u64() < Some(at)) {
            by_fp.insert(fp, item);
        }
    }
    history.extend(by_fp.into_values());

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
            let ttl = body.lines().find_map(|l| l.strip_prefix("ttl ")?.trim().parse().ok()).unwrap_or(HEARTBEAT_TTL);
            let first = attempts.len();
            for l in body.lines() {
                if let Some((n, id)) = l.strip_prefix('#').and_then(|l| l.split_once(' '))
                    && let Ok(n) = n.parse::<u64>()
                {
                    attempts.push(json!({"host": name, "task": n, "attempt": id.trim()}));
                }
            }
            // Age at collection uses only the board clock.
            let at = hb["updated_at"].as_str().and_then(iso_secs);
            let age = date.zip(at).map(|(d, u)| d.saturating_sub(u));
            let cached_age = age.zip(snap["at"].as_u64()).map(|(a, read_at)| a.saturating_add(now.saturating_sub(read_at)));
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
        for i in board["failed"].as_array().unwrap() {
            let detail = i["reason"].as_str().filter(|r| !r.is_empty()).map_or(i["title"].clone(), |r| json!(r));
            current.push(json!({"source": "taskq", "host": null, "what": format!("{repo}#{}: failed", i["number"]), "since": null, "detail": detail, "trace_id": null, "url": i["url"]}));
        }
        boards.push(board);
    }
    // AgentQ coverage (#116): a running installation whose host's agents leave no compression or activity records.
    // ponytail: Langfuse heartbeats carry no routing or hook state; those rows come from enrolled snapshots below.
    // Absence is asserted only on a fresh, uncapped read of every record kind it rests on; otherwise it is a gap (D6).
    let coverage_keys = ["activity", "compression", "native_tools", "agentq.installation", "agentq.compress", "agentq.run"];
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
        let legacy_rows = |host: &Value, agent: &Value| compression.iter().chain(&rtk).any(|c| c["host"] == *host && c["agent"] == *agent);
        for x in fleet.iter().filter(|x| x["state"] == "ok") {
            let (inst, host) = (x["installation"].as_str().unwrap().to_string(), &x["host"]);
            let n = |a: &str| covered.get(&(inst.clone(), a)).copied().unwrap_or(0);
            let total: u64 = covered.iter().filter(|((i, _), _)| *i == inst).map(|(_, n)| n).sum();
            let calls: Vec<&Value> = activity.iter().filter(|a| a["host"] == *host).collect();
            for a in &calls {
                let agent = a["agent"].as_str().unwrap();
                if n(agent) == 0 && n("unknown") == 0 && !legacy_rows(host, &a["agent"]) {
                    let client = agent_name(agent).unwrap();
                    current.push(json!({"source": "agentq", "host": host, "client": client, "what": format!("{client}: no compression records"), "since": null,
                        "detail": format!("{} model calls in {}, but installation {inst} sent no agentq.compress or agentq.run record for this client.", a["calls"], dur(period)), "trace_id": a["trace_id"]}));
                }
            }
            let name = host.as_str().unwrap().to_ascii_lowercase();
            if let Some(tasks) = working.get(&name).filter(|t| !t.is_empty() && calls.is_empty()) {
                let none = if total == 0 && !compression.iter().chain(&rtk).any(|c| c["host"] == *host) { "activity or compression" } else { "activity" };
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
            legacy: compression.iter().chain(&rtk).map(|c| (c["host"].as_str().unwrap_or("").to_string(), c["agent"].as_str().unwrap_or("").to_string())).collect(),
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

    for h in hosts.values_mut() {
        h["taskq"]["latest"] = latest("taskqorg/taskq");
        h["taskq"]["behind"] = behind(&h["taskq"]["latest"], &h["taskq"]["version"]);
        let a = &mut h["agentq"];
        a["latest"] = latest("taskqorg/agentq");
        // the running installation (H6) decides; legacy monitor and proxy versions only without one
        a["behind"] = match (behind(&a["latest"], &a["running"]), behind(&a["latest"], &a["monitor"]), behind(&a["latest"], &a["proxy"])) {
            (Value::Bool(r), _, _) => json!(r),
            (_, Value::Bool(m), Value::Bool(p)) => json!(m || p),
            (_, Value::Null, x) => x,
            (_, x, _) => x,
        };
    }

    // Resources (D4.4): enrolled history first, Langfuse samples otherwise; a stale or missing sample is a gap.
    let mut resources = crate::resource::build(&rows("agentq.resource"), capped("agentq.resource"), &inp["enrolled_resources"], now, period, fhost);
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
    let versions = crate::version::build(inp, &fleet, &legacy, &heartbeats.into_values().collect::<Vec<_>>());
    if inp["installations"].is_object() {
        sources["enrollment"] = inp["installations"]["source"].clone();
    }
    let mut doc = json!({
        "schema": 1,
        "generated_at": now,
        "period": period,
        "filters": f,
        "sources": sources,
        "hosts": hosts.into_values().filter(|h| fhost.is_none_or(|x| h["id"].as_str().is_some_and(|i| i.eq_ignore_ascii_case(x)))).collect::<Vec<_>>(),
        "attention": {"source": worst(att_states.iter().map(|(n, s)| (n.clone(), s)), now),
                      "current": hostf(&current), "gaps": hostf(&gaps), "incidents": hostf(&history)},
        "boards": boards,
        "resources": resources,
        "agents": {"source": lf_src, "incomplete": {"activity": capped("activity"), "compression": capped("compression"), "native_tools": capped("native_tools"),
                                                      "agentq.installation": capped("agentq.installation"), "agentq.run": capped("agentq.run"), "agentq.retrieval": capped("agentq.retrieval")},
                   "fleet": fleet, "clients": clients, "activity": activity, "compression": compression, "native_rtk": rtk, "counters": hostf(&counters), "unattributed": unattributed},
        "checks": {"source": sq_src, "at": sq_src["at"], "results": results, "incidents": open, "manual_rules": manual,
                   "installations": enrolled_checks.into_iter().filter(|c| fhost.is_none_or(|x| c["host"] == x)).collect::<Vec<_>>()},
        "versions": versions,
        "optimization": crate::optimization::build(inp),
        "agent_work": agent_work,
    });
    if inp["installations"].is_object() {
        doc["installations"] = inp["installations"].clone();
    }
    doc
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
            results.push(json!({"check": name, "status": status, "reason": reason, "observed_at": observed, "evidence": evidence}));
        }
        for (name, i) in p["incidents"].as_object().into_iter().flatten().filter(|(k, _)| rule_id(k)) {
            let t = |k: &str| i[k].as_f64().filter(|t| t.is_finite() && *t >= 0.0).map(|a| a as u64);
            let state = i["state"].as_str().filter(|s| ["observed", "verified"].contains(s)).unwrap_or("unknown");
            if state != "verified" {
                open.push(json!({"check": name, "state": state, "first_seen": t("first_seen"), "last_seen": t("last_seen"), "occurrences": i["occurrences"]}));
            }
            if t("last_seen").is_some_and(|l| l + 86400 >= now) {
                history.push(json!({"source": "serverq", "host": host, "installation": installation, "what": format!("{name}: {state}"), "since": t("first_seen"), "occurrences": i["occurrences"], "trace_id": null}));
            }
        }
        let worst_status = ["warn", "unknown", "ok"].into_iter().find(|s| results.iter().any(|r| r["status"] == *s));
        let h = host_entry(hosts, &host);
        if h["serverq"]["at"].as_u64().is_none_or(|a| src["at"].as_u64().is_some_and(|b| b >= a)) {
            h["serverq"] = json!({"status": worst_status, "state": src["state"], "at": src["at"], "age_s": src["age_s"], "reason": src["reason"], "installation": installation});
        }
        out.push(json!({"installation": installation, "workspace": r["workspace"], "host": host, "version": p["version"].as_str().filter(|v| safe(v, 64, b".+-")),
            "catalog": p["catalog"].as_str().filter(|c| safe(c, 64, b"")), "source": src, "at": src["at"], "results": results, "incidents": open, "manual_rules": manual}));
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
    use super::*;

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
            assert!(h["snapshot"].is_null() && h["reason"].is_null(), "{h}");
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
