//! Waiting work, changes while away, delivery route, person requests and the query shape (dashq.md D23, #183).
//! Pure functions over the `/data` parts `data::build` already holds: nothing here reads a source.

use serde_json::{Value, json};
use std::sync::OnceLock;

/// D23 Query: rows per page of `changes.rows` and `agent_work.attempts`.
pub const CAP: usize = 200;

/// TaskQ v4.6.32 admission vocabulary (taskq#1616), pinned by hash in a test.
pub const CODES: &str = include_str!("../fixture/taskq/admission-codes.json");

/// The fixed meaning of a TaskQ admission code; `None` outside the pinned vocabulary.
pub fn meaning(code: &str) -> Option<&'static str> {
    static M: OnceLock<Vec<(String, String)>> = OnceLock::new();
    let m = M.get_or_init(|| {
        let v: Value = serde_json::from_str(CODES).unwrap_or(Value::Null);
        v["codes"].as_array().into_iter().flatten().filter_map(|c| Some((c["code"].as_str()?.to_string(), c["meaning"].as_str()?.to_string()))).collect()
    });
    m.iter().find(|(c, _)| c == code).map(|(_, x)| x.as_str())
}

/// A producer string shown as is: one short line, else dropped.
fn line(v: &Value, max: usize) -> Value {
    v.as_str().filter(|s| !s.is_empty() && s.len() <= max && !s.contains(['\n', '\r'])).map_or(Value::Null, |s| json!(s))
}

/// D23 Admission: TaskQ's own decision for a ready or waiting row, or `unknown` with the reason; never inferred.
/// `raw` is the producer record and where it came from.
pub fn admission(raw: Option<(&Value, &str)>, now: u64) -> Value {
    let unknown = |why: &str, src: Value| json!({"code": "unknown", "meaning": meaning("unknown"), "observed": null, "age_s": null, "host": null, "reason": why, "source": src});
    let Some((a, src)) = raw.filter(|(a, _)| a.is_object()) else { return unknown("admission not reported by taskq", Value::Null) };
    let Some(code) = a["code"].as_str().filter(|c| meaning(c).is_some()) else { return unknown("unsupported admission code", json!(src)) };
    let observed = a["observed"].as_u64();
    // the document's `now` is taken before the reads it builds on: a record up to 60 s ahead is that read
    if observed.is_some_and(|t| t > now + 60) {
        return unknown("clock uncertain: observed in the future", json!(src));
    }
    json!({"code": code, "meaning": meaning(code), "observed": observed, "age_s": observed.map(|t| now.saturating_sub(t)),
        "host": a["host"].as_str().filter(|h| crate::data::safe(h, 64, b"_.-")), "reason": line(&a["reason"], 160), "source": src})
}

/// `#finding-<id>` of one ServerQ episode: SHA-256 of `[workspace, installation, rule, first_seen]` (page.rs anchors).
pub fn finding_id(f: &Value) -> String {
    let identity = serde_json::to_vec(&json!([f["workspace"], f["installation"], f["rule"], f["first_seen"]])).unwrap();
    ring::digest::digest(&ring::digest::SHA256, &identity).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

/// D23 Requests: only producer requests needing a person, each with the exact place to act.
pub fn requests(doc: &Value, now: u64) -> Vec<Value> {
    let period = doc["period"].as_u64().unwrap_or(1800);
    let enc = crate::source::enc;
    let item = |kind: &str, source: &str, host: &Value, project: &Value, task: &Value, rule: &Value, inst: &Value, reason: Value, since: &Value, action: &str, target: String| {
        json!({"kind": kind, "source": source, "host": host, "project": project, "task": task, "rule": rule, "installation": inst, "reason": reason,
            "since": since, "age_s": since.as_u64().filter(|t| *t <= now).map(|t| now - t), "action": action, "target": target})
    };
    let mut out = Vec::new();
    for b in doc["boards"].as_array().into_iter().flatten() {
        let project = &b["project"];
        for q in b["questions"].as_array().into_iter().flatten() {
            let target = format!("/projects?period={period}#board-{}-q-{}", enc(project.as_str().unwrap_or("")), q["number"]);
            out.push(item("question", "taskq", &Value::Null, project, &q["number"], &Value::Null, &Value::Null, q["title"].clone(), &Value::Null, "Answer", target));
        }
        for f in b["failed"].as_array().into_iter().flatten() {
            let reason = f["reason"].as_str().filter(|r| !r.is_empty()).map_or(f["title"].clone(), |r| json!(r));
            out.push(item("failed", "taskq", &Value::Null, project, &f["number"], &Value::Null, &Value::Null, reason, &Value::Null, "Review failure", f["url"].as_str().unwrap_or("").into()));
        }
    }
    let checks = &doc["checks"];
    let warn = |c: &Value| -> Vec<(String, Value)> {
        c["results"].as_array().into_iter().flatten().filter(|r| r["status"] == "warn").map(|r| (r["check"].as_str().unwrap_or("").to_string(), r.get("reason").or(r.get("detail")).cloned().unwrap_or(Value::Null))).collect()
    };
    for (rule, reason) in warn(checks) {
        let since = checks["incidents"].as_array().into_iter().flatten().find(|i| i["check"] == rule.as_str()).map_or(checks["at"].clone(), |i| i["first_seen"].clone());
        let host = doc["attention"]["current"].as_array().into_iter().flatten().find(|i| i["source"] == "serverq" && i["installation"].is_null()).map_or(Value::Null, |i| i["host"].clone());
        out.push(item("finding", "serverq", &host, &Value::Null, &Value::Null, &json!(rule), &Value::Null, reason, &since, "Diagnostics", format!("/checks?period={period}#check-report-{}", enc(&rule))));
    }
    for c in checks["installations"].as_array().into_iter().flatten() {
        for (rule, reason) in warn(c) {
            let episode = c["findings"].as_array().into_iter().flatten().find(|f| f["rule"] == rule.as_str() && f["state"] == "open");
            let target = episode.map_or_else(|| format!("#check-{}-{}", enc(c["installation"].as_str().unwrap_or("")), enc(&rule)), |f| format!("#finding-{}", finding_id(f)));
            let since = episode.map_or(&c["at"], |f| &f["first_seen"]);
            out.push(item("finding", "serverq", &c["host"], &Value::Null, &Value::Null, &json!(rule), &c["installation"], reason, since, "Diagnostics", format!("/checks?period={period}{target}")));
        }
    }
    out
}

/// D23 Changes: new, ongoing and resolved conditions of the window with per-source coverage; `items` are Attention
/// current and gap items, `history` incident events and episodes. `sources` are the configured condition sources
/// with their baseline: `(name, Some(reason))` keeps no history of the window.
pub fn changes(items: &[Value], history: &[Value], sources: &[(&str, Option<String>)], now: u64, period: u64, cursor: usize) -> Value {
    let from = now.saturating_sub(period);
    let (mut rows, mut unclassified) = (Vec::new(), serde_json::Map::new());
    let row = |state: &str, i: &Value, until: &Value| json!({"state": state, "source": i["source"], "host": i["host"], "what": i["what"], "since": i["since"], "until": until, "installation": i.get("installation").cloned().unwrap_or(Value::Null)});
    for i in items {
        match i["since"].as_u64() {
            Some(t) => rows.push(row(if t >= from { "new" } else { "ongoing" }, i, &Value::Null)),
            None => {
                let n = unclassified.entry(i["source"].as_str().unwrap_or("unknown").to_string()).or_insert(json!(0));
                *n = json!(n.as_u64().unwrap_or(0) + 1);
            }
        }
    }
    for h in history {
        let verified = h["verified_at"].as_u64().or_else(|| h["episode"]["verified_at"].as_u64());
        match (verified, h["since"].as_u64()) {
            // an episode that ended inside the window
            (Some(v), _) if (from..=now).contains(&v) => rows.push(row("resolved", h, &json!(v))),
            // a point event (agentq incident) inside the window; open episodes are already current items
            (None, Some(t)) if h["episode"].is_null() && h["source"] != "serverq" && t >= from => rows.push(row("new", h, &Value::Null)),
            _ => {}
        }
    }
    rows.sort_by_key(|r| std::cmp::Reverse(r["until"].as_u64().or(r["since"].as_u64())));
    let coverage: Vec<Value> = sources.iter().map(|(name, why)| json!({"source": name, "baseline": why.is_none(), "reason": why})).collect();
    let with = sources.iter().filter(|(_, w)| w.is_none()).count();
    let comparison = match with {
        0 => json!({"state": "unavailable", "reason": if sources.is_empty() { "no condition source configured" } else { "no baseline" }}),
        n if n == sources.len() => json!({"state": "available", "reason": null}),
        _ => json!({"state": "partial", "reason": "some sources keep no baseline"}),
    };
    let total = rows.len();
    let page: Vec<Value> = rows.into_iter().skip(cursor).take(CAP).collect();
    json!({"window_s": period, "from": from, "to": now, "timezone": "UTC", "comparison": comparison, "coverage": coverage,
        "rows": page, "rows_total": total, "unclassified": unclassified})
}

/// D23 Route of one installation row: each stage known, unknown or not applicable, from its own evidence only.
pub fn route(r: &Value) -> Value {
    let stage = |state: &str, value: Value, at: &Value, reason: Option<&str>| json!({"state": state, "value": value, "at": at, "reason": reason});
    let t = &r["telemetry"];
    let sender = &t["sender"];
    let counted = |k: &str| if sender["state"] == "ok" && !sender[k].is_null() { stage("known", sender[k].clone(), &t["last_synced"], None) } else {
        stage("unknown", Value::Null, &Value::Null, Some(sender["reason"].as_str().or(t["reason"].as_str()).unwrap_or("sender reports no counters")))
    };
    let telemetry = r["product"] == "taskq" && t["state"] != "off" && !t.is_null();
    // the sender's backlog is its queue; what it emitted in total no producer reports yet
    let emitted = stage("unknown", Value::Null, &Value::Null, Some("producer reports no emitted count"));
    let queued = if telemetry { counted("backlog_rows") } else { stage("unknown", Value::Null, &Value::Null, Some("producer reports no queue")) };
    let accepted = match r["received_at"].as_u64() {
        Some(at) => stage("known", json!(true), &json!(at), None),
        None => stage("unknown", Value::Null, &Value::Null, Some("no receipt")),
    };
    let snap = !r["snapshot"].is_null();
    let projected = match (r["projected"] == true, snap) {
        (true, _) => stage("known", json!(true), &r["observation"]["at"], None),
        (false, true) => stage("known", json!(false), &r["observation"]["at"], Some("snapshot not projected for this generation")),
        _ => stage("unknown", Value::Null, &Value::Null, Some("no snapshot yet")),
    };
    let x = &r["export"];
    let exported = match x["state"].as_str() {
        Some("not_applicable") => stage("not_applicable", Value::Null, &Value::Null, x["reason"].as_str()),
        Some("gap") => stage("known", json!(false), &Value::Null, x["reason"].as_str()),
        _ => stage("unknown", Value::Null, &Value::Null, Some(x["reason"].as_str().unwrap_or("no export evidence"))),
    };
    let retained = telemetry && t["retained"].is_object();
    let queryable = match (r["projected"] == true && r["connection"] != "revoked", retained) {
        (true, _) => stage("known", json!(true), &r["observation"]["at"], None),
        (false, true) => stage("known", json!(true), &t["last_synced"], Some("telemetry blocks by block=ID only")),
        _ if r["connection"] == "revoked" => stage("known", json!(false), &Value::Null, Some("connection revoked")),
        _ => stage("unknown", Value::Null, &Value::Null, Some("nothing projected or retained")),
    };
    json!({"emitted": emitted, "queued": queued, "accepted": accepted, "projected": projected, "exported": exported, "queryable": queryable})
}

/// D23 Query: what this document covers and the next page; `longest` is the larger capped list.
pub fn query(period: u64, now: u64, cursor: usize, longest: usize) -> Value {
    let next = (cursor + CAP < longest).then_some(cursor + CAP);
    json!({"period": period, "from": now.saturating_sub(period), "to": now, "timezone": "UTC", "cursor": cursor, "next_cursor": next, "cap": CAP})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// taskq#1616 A4: every released code and unknown reason reads as TaskQ sent it; the bytes are the release's.
    #[test]
    fn admission_codes_pass_through_from_the_pinned_release() {
        let hash: String = ring::digest::digest(&ring::digest::SHA256, CODES.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hash, "cdc3b201a5d0049f0119647abe62497f239247b6e096f9fd10786d08825b5d63", "TaskQ v4.6.32 exact bytes");
        let v: Value = serde_json::from_str(CODES).unwrap();
        for t in v["tasks"].as_array().unwrap().iter().filter(|t| t["admission"].is_object()) {
            let a = admission(Some((&t["admission"], "taskq-snapshot")), 1060);
            assert_eq!((&a["code"], &a["observed"], &a["age_s"], &a["host"]), (&t["admission"]["code"], &json!(1000), &json!(60), &t["admission"]["host"]), "{t}");
            assert_eq!(a["reason"], t["admission"]["reason"].clone(), "TaskQ's own reason, never DashQ's");
            assert!(a["meaning"].is_string());
        }
    }

    /// D23 A2: missing, unknown-vocabulary and future records stay unknown with a reason; nothing becomes a stall.
    #[test]
    fn admission_without_a_decision_stays_unknown() {
        let cases = [
            (None, "admission not reported by taskq"),
            (Some(json!({"code": "stalled", "observed": 1})), "unsupported admission code"),
            (Some(json!({"code": "capacity", "observed": 1061})), "clock uncertain: observed in the future"),
        ];
        for (raw, why) in cases {
            let a = admission(raw.as_ref().map(|r| (r, "x")), 1000);
            assert_eq!((&a["code"], &a["reason"], &a["age_s"]), (&json!("unknown"), &json!(why), &Value::Null));
        }
    }

    /// D23 A3: start inside the window is new, before it ongoing, a verified end inside it resolved; no start time
    /// is unclassified; no baseline reads comparison unavailable, never empty-and-fine.
    #[test]
    fn changes_classify_by_producer_time_and_name_coverage() {
        let items = [json!({"source": "serverq", "what": "disk: warn", "since": 900}), json!({"source": "serverq", "what": "load: warn", "since": 100}), json!({"source": "taskq", "what": "#1: failed", "since": null})];
        let history = [json!({"source": "serverq", "what": "mem: verified", "since": 50, "verified_at": 950, "episode": {"verified_at": 950}}),
            json!({"source": "serverq", "what": "old: verified", "since": 10, "verified_at": 20, "episode": {}}), json!({"source": "agentq", "what": "agentq incident (run)", "since": 990})];
        let c = changes(&items, &history, &[("serverq", None), ("taskq", Some("no baseline".into()))], 1000, 200, 0);
        let got: Vec<(&str, &str)> = c["rows"].as_array().unwrap().iter().map(|r| (r["state"].as_str().unwrap(), r["what"].as_str().unwrap())).collect();
        assert_eq!(got, [("new", "agentq incident (run)"), ("resolved", "mem: verified"), ("new", "disk: warn"), ("ongoing", "load: warn")]);
        assert_eq!((&c["unclassified"]["taskq"], &c["comparison"]["state"], &c["timezone"]), (&json!(1), &json!("partial"), &json!("UTC")));
        let none = changes(&[], &[], &[("taskq", Some("no baseline".into()))], 1000, 200, 0);
        assert_eq!((&none["comparison"]["state"], &none["comparison"]["reason"]), (&json!("unavailable"), &json!("no baseline")));
        assert_eq!(changes(&items, &[], &[], 1000, 200, 0)["comparison"]["reason"], "no condition source configured");
        // the cursor pages by CAP over the same ordering
        assert_eq!(changes(&items, &history, &[], 1000, 200, 3)["rows"].as_array().unwrap().len(), 1);
    }

    /// D23 A4: each stage is known only from its own evidence; a sender without counters and a non-AgentQ export
    /// never read as delivered or zero.
    #[test]
    fn route_names_each_stage_known_or_unknown() {
        let quiet = json!({"product": "taskq", "connection": "active", "received_at": 900, "projected": true, "observation": {"at": 890},
            "snapshot": {"schema": "taskq-board/1"}, "export": {"state": "not_applicable", "reason": "no Langfuse export required"},
            "telemetry": {"state": "ok", "last_synced": 900, "retained": {"rows": 5}, "sender": {"state": "unknown", "reason": "status not reported by taskq"}}});
        let r = route(&quiet);
        let states: Vec<&str> = ["emitted", "queued", "accepted", "projected", "exported", "queryable"].iter().map(|k| r[*k]["state"].as_str().unwrap()).collect();
        assert_eq!(states, ["unknown", "unknown", "known", "known", "not_applicable", "known"]);
        assert_eq!((&r["queued"]["reason"], &r["accepted"]["at"]), (&json!("status not reported by taskq"), &json!(900)));
        let mut counted = quiet.clone();
        counted["telemetry"]["sender"] = json!({"state": "ok", "backlog_rows": 7});
        assert_eq!((&route(&counted)["queued"]["state"], &route(&counted)["queued"]["value"]), (&json!("known"), &json!(7)));
        let new = route(&json!({"product": "agentq", "connection": "pending", "snapshot": null, "export": {"state": "unknown", "reason": "export forwarding not qualified"}}));
        assert_eq!((&new["accepted"]["state"], &new["projected"]["reason"], &new["exported"]["state"]), (&json!("unknown"), &json!("no snapshot yet"), &json!("unknown")));
    }
}
