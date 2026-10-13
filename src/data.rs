//! The `/data` document (dashq.md D6, D7) from source snapshots: pure, so a fixture pins it.
//!
//! Input: `{now, period, filters, projects, serverq_host, langfuse, boards: {repo: snap}, releases: {repo: snap}, serverq}`;
//! a snap is `{value, at, error, incomplete}` (source.rs).

use crate::source::{clock, iso_secs};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub const PERIODS: [u64; 3] = [1800, 7200, 86400];
pub const AGENTS: [(&str, &str); 3] = [("codex", "Codex"), ("claude-code", "Claude Code"), ("hermes", "Hermes")];
const SNAPSHOT_LIMIT: u64 = 600;
const HEARTBEAT_TTL: u64 = 300;
const GITHUB_LIMIT: u64 = 180;
const SERVERQ_LIMIT: u64 = 7200;
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

fn safe(s: &str, max: usize, extra: &[u8]) -> bool {
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

fn semver(v: &Value) -> Value {
    v.as_str().filter(|s| s.split('.').count() == 3 && s.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))).map_or(Value::Null, |s| json!(s))
}

fn trace(row: &Value) -> Value {
    row["traceId"].as_str().filter(|t| safe(t, 128, b"_-")).map_or(Value::Null, |t| json!(t))
}

fn row_at(row: &Value) -> Option<u64> {
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

fn host_row(id: &str) -> Value {
    json!({
        "id": id,
        "snapshot": {"state": "unknown", "at": null, "age_s": null, "reason": "no snapshot in 24 h"},
        "taskq": {"version": null, "latest": null, "behind": null, "state": "unknown", "age_s": null, "reason": "no heartbeat"},
        "agentq": {"monitor": null, "proxy": null, "selected": null, "available": null, "latest": null, "updater": null, "subscription": null, "behind": null},
        "serverq": {"status": null, "state": "unknown", "age_s": null, "reason": "no serverq report for this host"},
        "services": [],
    })
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
        let h = hosts.entry(id.clone()).or_insert_with(|| host_row(&id));
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
        if h["snapshot"]["state"] != "ok" {
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
    for k in activity.keys().chain(compression.keys()).chain(rtk.keys()) {
        hosts.entry(k.0.clone()).or_insert_with(|| host_row(&k.0));
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

    // Heartbeats and boards (D4.2).
    let releases = &inp["releases"];
    let latest = |repo: &str| releases[repo]["value"].clone();
    let mut boards = Vec::new();
    let mut gh_states = Vec::new();
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
        for hb in v["hosts"].as_array().into_iter().flatten() {
            let Some(name) = hb["title"].as_str().and_then(|t| t.strip_prefix("taskq host ")).filter(|n| safe(n, 64, b"_.-")) else { continue };
            let body = hb["body"].as_str().unwrap_or("");
            let ttl = body.lines().find_map(|l| l.strip_prefix("ttl ")?.trim().parse().ok()).unwrap_or(HEARTBEAT_TTL);
            for l in body.lines() {
                if let Some((n, id)) = l.strip_prefix('#').and_then(|l| l.split_once(' '))
                    && let Ok(n) = n.parse::<u64>()
                {
                    attempts.push(json!({"host": name, "task": n, "attempt": id.trim()}));
                }
            }
            // age = response Date minus updated_at: board clock only, never the local one
            let age = date.zip(hb["updated_at"].as_str().and_then(iso_secs)).map(|(d, u)| d.saturating_sub(u));
            let version = semver(&json!(body.lines().find_map(|l| l.strip_prefix("version ")).map(str::trim)));
            let h = hosts.entry(name.into()).or_insert_with(|| host_row(name));
            if h["taskq"]["age_s"].as_u64().is_some_and(|a| age.is_none_or(|x| x >= a)) {
                continue;
            }
            let fresh = age.is_some_and(|a| a <= ttl);
            let t = &mut h["taskq"];
            t["version"] = version;
            t["age_s"] = json!(age);
            t["state"] = json!(if age.is_none() { "unknown" } else if fresh { "ok" } else { "stale" });
            t["reason"] = if fresh { Value::Null } else { json!(age.map_or("heartbeat age unknown".into(), |a| format!("heartbeat {} old (ttl {})", dur(a), dur(ttl)))) };
        }
        let failed = list(&["q-failed"]);
        for i in failed.as_array().unwrap() {
            current.push(json!({"source": "taskq", "host": null, "what": format!("{repo}#{}: failed", i["number"]), "since": null, "detail": i["title"], "trace_id": null, "url": i["url"]}));
        }
        boards.push(json!({
            "project": repo, "url": format!("https://github.com/{repo}/issues"), "source": src,
            "in_work": issues.map(|_| json!(with(&["q-doing", "q-review"]).unwrap().len())), "waiting": count(&["q-ask"]), "ready": count(&["q-ready"]),
            "questions": list(&["q-ask"]), "failed": failed, "attempts": attempts, "latest": latest(repo),
        }));
    }
    let gh_src = if gh_states.is_empty() { source(&Value::Null, now, GITHUB_LIMIT, None) } else { worst(gh_states.iter().map(|(n, s)| (n.clone(), s)), now) };

    // serverq (D4.3)
    let sq = &inp["serverq"];
    let sq_src = source(sq, now, SERVERQ_LIMIT, None);
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
        let h = hosts.entry(sq_host.into()).or_insert_with(|| host_row(sq_host));
        h["serverq"] = json!({"status": worst_status, "state": sq_src["state"], "age_s": sq_src["age_s"], "reason": sq_src["reason"]});
    }
    let manual: Vec<Value> = report["manual_rules"].as_array().into_iter().flatten().map(|r| json!({"rule": r, "status": "not verified"})).collect();

    for h in hosts.values_mut() {
        h["taskq"]["latest"] = latest("taskqorg/taskq");
        h["taskq"]["behind"] = behind(&h["taskq"]["latest"], &h["taskq"]["version"]);
        let a = &mut h["agentq"];
        a["latest"] = latest("taskqorg/agentq");
        a["behind"] = match (behind(&a["latest"], &a["monitor"]), behind(&a["latest"], &a["proxy"])) {
            (Value::Bool(m), Value::Bool(p)) => json!(m || p),
            (Value::Null, x) => x,
            (x, _) => x,
        };
    }

    let hostf = |v: &Vec<Value>| -> Vec<Value> { v.iter().filter(|i| fhost.is_none_or(|x| i["host"] == x || i["host"].is_null())).cloned().collect() };
    history.sort_by_key(|i| std::cmp::Reverse(i["since"].as_u64()));
    json!({
        "schema": 1,
        "generated_at": now,
        "period": period,
        "filters": f,
        "sources": {"langfuse": lf_src, "github": gh_src, "serverq": sq_src},
        "hosts": hosts.into_values().filter(|h| fhost.is_none_or(|x| h["id"] == x)).collect::<Vec<_>>(),
        "attention": {"source": worst([("Langfuse".to_string(), &lf_src), ("GitHub".to_string(), &gh_src), ("serverq".to_string(), &sq_src)].iter().map(|(n, s)| (n.clone(), *s)), now),
                      "current": hostf(&current), "gaps": hostf(&gaps), "incidents": hostf(&history)},
        "boards": boards,
        "agents": {"source": lf_src, "incomplete": {"activity": capped("activity"), "compression": capped("compression"), "native_tools": capped("native_tools")},
                   "activity": activity, "compression": compression, "native_rtk": rtk, "counters": hostf(&counters), "unattributed": unattributed},
        "checks": {"source": sq_src, "at": sq_src["at"], "results": results, "incidents": open, "manual_rules": manual},
    })
}

/// 503 when every source failed with no value to show (D7).
pub fn all_failed(doc: &Value) -> bool {
    doc["sources"].as_object().is_some_and(|s| s.values().all(|x| x["state"] == "unknown" && x["reason"].as_str().is_some_and(|r| r.contains("read failed"))))
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
