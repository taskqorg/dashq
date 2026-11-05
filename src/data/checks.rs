use super::*;
use serde_json::Map;

/// Enrolled `serverq-report/1` snapshots (serverq.md § 3 projection, schema 1): one Server checks entry per
/// installation and workspace, its hosts cell and Attention items. Producer time dates everything (serverq limit
/// 7200 s, enroll::stale_secs); manual rules stay not verified, unknown stays unknown, a revoked grant shows nothing
/// current. A host keeps the newest observation when several installations share its label and runtime.
pub(super) fn serverq_enrolled(inp: &Value, now: u64, machines: &Machines, hosts: &mut BTreeMap<String, Value>, current: &mut Vec<Value>, gaps: &mut Vec<Value>, history: &mut Vec<Value>) -> Vec<Value> {
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
