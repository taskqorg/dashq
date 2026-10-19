//! D15 / finding-tasks H2: observation only; identities never follow host labels.
use serde_json::{Value, json};
use std::collections::BTreeMap;

const CATALOG: &[u8] = include_bytes!("../fixture/serverq/rules.json");
pub fn catalog_hash() -> String {
    ring::digest::digest(&ring::digest::SHA256, CATALOG).as_ref().iter().map(|b| format!("{b:02x}")).collect()
}
fn time(v: &Value) -> Option<u64> {
    v.as_f64().filter(|t| t.is_finite() && *t >= 0.0 && *t < u64::MAX as f64).map(|t| t as u64)
}
fn id(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Samples are already scoped by installation/workspace. A late sample can supply an older episode,
/// never replace the newest observation. Missing rules/incidents do not mean recovery.
pub fn build(row: &Value, retained: &Value, now: u64) -> Value {
    let current = &row["snapshot"]["payload"];
    let mut samples: Vec<Value> = retained["samples"].as_array().cloned().unwrap_or_default();
    samples.push(json!({"at": row["observation"]["at"], "payload": current}));
    samples.sort_by_key(|s| time(&s["at"]).unwrap_or(0));
    let mut episodes: BTreeMap<(String, String), Value> = BTreeMap::new();
    let incomplete = retained["incomplete"] == true || retained["error"].is_string() || current["complete"] == false || current["incomplete"] == true;
    let catalog: Value = serde_json::from_slice(CATALOG).unwrap();
    let expected = catalog_hash();
    for s in samples {
        let p = &s["payload"];
        if !crate::enroll::serverq_payload(p) { continue }
        let at = time(&s["at"]);
        for (rule, incident) in p["incidents"].as_object().into_iter().flatten().filter(|(k, _)| id(k)) {
            let Some(first) = time(&incident["first_seen"]) else { continue };
            let Some(last) = time(&incident["last_seen"]).filter(|t| *t >= first) else { continue };
            let x = &p["rules"][rule];
            let observed = time(&x["observed_at"]);
            let verified = time(&incident["verified_at"]);
            let fresh = |t: Option<u64>| t.is_some_and(|t| t <= now && now - t <= crate::enroll::SERVERQ_STALE_SECS);
            let complete = p["complete"] != false && p["incomplete"] != true && current["complete"] != false && current["incomplete"] != true && !incomplete;
            let recovered = incident["state"] == "verified" && x["status"] == "ok"
                && row["observation"]["state"] == "ok" && fresh(at) && fresh(observed) && complete
                && verified.is_some_and(|v| v > last && observed.is_some_and(|o| o >= v) && at.is_some_and(|a| a >= observed.unwrap()));
            let matching = p["catalog"] == expected && catalog[rule].is_object();
            let catalog_state = if p["catalog"].is_null() { "unknown" } else if matching { "matched" } else { "mismatch" };
            let evidence: serde_json::Map<String, Value> = x["evidence"].as_object().into_iter().flatten()
                .filter(|(k, v)| id(&k.replace('_', "-")) && v.is_number()).map(|(k, v)| (k.clone(), v.clone())).collect();
            let source_state = if !complete { "incomplete" } else if row["observation"]["state"] != "ok" {
                row["observation"]["state"].as_str().unwrap_or("unknown")
            } else if !fresh(observed) { "stale" } else if x["status"] == "unknown" || x["status"].is_null() { "unknown" } else { "ok" };
            let mut f = json!({"installation": row["installation"], "workspace": row["workspace"], "rule": rule,
                "first_seen": incident["first_seen"], "last_seen": incident["last_seen"], "occurrences": incident["occurrences"].as_u64(),
                "verified_at": if verified.is_some() { incident["verified_at"].clone() } else { Value::Null }, "observed_at": if observed.is_some() { x["observed_at"].clone() } else { Value::Null }, "report_at": s["at"],
                "state": if recovered { "verified" } else { "open" }, "producer_state": incident["state"].as_str().filter(|s| ["observed", "verified"].contains(s)),
                "status": x["status"].as_str().filter(|s| ["ok", "warn", "unknown", "manual"].contains(s)),
                "evidence": evidence, "source": {"state": source_state, "at": observed, "age_s": observed.and_then(|t| now.checked_sub(t)), "incomplete": !complete,
                    "reason": if source_state == "ok" { Value::Null } else { json!("episode verification unavailable: incomplete, unknown or stale observation") }},
                "catalog": {"state": catalog_state, "sha256": p["catalog"], "expected": expected},
                "recommendation": if matching { catalog[rule].clone() } else { Value::Null }});
            let key = (rule.clone(), incident["first_seen"].to_string());
            if let Some(previous) = episodes.get(&key).filter(|f| f["state"] == "verified") {
                // Unknown does not undo a proved historical recovery; recurrence has another first_seen.
                f["state"] = json!("verified");
                f["verified_at"] = previous["verified_at"].clone();
            }
            // A report older than the retained episode never overwrites it, including its verification.
            if episodes.get(&key).is_none_or(|old| time(&old["report_at"]) <= at) {
                episodes.insert(key, f);
            }
        }
    }
    json!({"episodes": episodes.into_values().collect::<Vec<_>>(), "incomplete": incomplete, "reason": retained["error"]})
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: u64 = 20000;
    fn row() -> Value {
        json!({"installation": "sq-1", "workspace": "ws-1", "observation": {"at": NOW, "state": "ok"}, "snapshot": {"payload": {
            "schema": 1, "product": "serverq", "catalog": catalog_hash(), "observed_at": NOW,
            "rules": {"disk-headroom": {"status": "ok", "observed_at": NOW, "evidence": {"available": 20, "total": 100}}},
            "incidents": {"disk-headroom": {"state": "verified", "first_seen": 19000, "last_seen": 19100, "verified_at": 19200, "occurrences": 1}}
        }}})
    }
    /// H2: each independent missing/failing prerequisite must prevent false recovery.
    #[test]
    fn serverq_verification_and_catalog_require_exact_evidence() {
        let good = row();
        let f = build(&good, &Value::Null, NOW)["episodes"][0].clone();
        assert_eq!(f["state"], "verified");
        assert_eq!(f["catalog"]["state"], "matched");
        assert_eq!(f["recommendation"]["minimum_percent"], 15);
        assert_eq!(f["evidence"]["available"], 20);
        for (pointer, value) in [
            ("/observation/state", json!("stale")),
            ("/observation/state", json!("unknown")),
            ("/snapshot/payload/rules/disk-headroom/status", json!("warn")),
            ("/snapshot/payload/rules/disk-headroom/status", json!("unknown")),
            ("/snapshot/payload/rules/disk-headroom/observed_at", json!(10000)),
            ("/snapshot/payload/rules/disk-headroom/observed_at", json!(NOW + 1)),
            ("/snapshot/payload/rules/disk-headroom/observed_at", json!(19150)),
            ("/snapshot/payload/incidents/disk-headroom/verified_at", json!(null)),
            ("/snapshot/payload/incidents/disk-headroom/verified_at", json!(19050)),
        ] {
            let mut r = good.clone();
            *r.pointer_mut(pointer).unwrap() = value;
            assert_eq!(build(&r, &Value::Null, NOW)["episodes"][0]["state"], "open", "{pointer}: {r}");
        }
        for key in ["complete", "incomplete"] {
            let mut r = good.clone();
            r["snapshot"]["payload"][key] = json!(key == "incomplete");
            assert_eq!(build(&r, &Value::Null, NOW)["episodes"][0]["state"], "open");
        }
        for hash in [Value::Null, json!("different")] {
            let mut r = good.clone(); r["snapshot"]["payload"]["catalog"] = hash;
            let f = build(&r, &Value::Null, NOW)["episodes"][0].clone();
            assert!(f["recommendation"].is_null());
            assert_ne!(f["catalog"]["state"], "matched");
        }
        assert_eq!(build(&good, &json!({"incomplete": true}), NOW)["episodes"][0]["state"], "open");
    }
    /// H1/H2: recurrence is a distinct episode; missing/late frames cannot erase retained episodes.
    #[test]
    fn serverq_retained_episodes_survive_missing_rules_and_late_samples() {
        let mut old = row();
        old["snapshot"]["payload"]["rules"]["disk-headroom"]["observed_at"] = json!(19800);
        let mut newer = row();
        newer["snapshot"]["payload"]["incidents"]["disk-headroom"] = json!({"state": "observed", "first_seen": 19900, "last_seen": NOW, "occurrences": 1});
        newer["snapshot"]["payload"]["rules"]["disk-headroom"]["status"] = json!("warn");
        let history = json!({"samples": [{"at": 19800, "payload": old["snapshot"]["payload"]}, {"at": NOW, "payload": newer["snapshot"]["payload"]}]});
        // Delayed old warning has the same episode key but must not undo a later verified record.
        let mut late = old["snapshot"]["payload"].clone();
        late["incidents"]["disk-headroom"]["state"] = json!("observed");
        let mut history = history;
        history["samples"].as_array_mut().unwrap().push(json!({"at": 19500, "payload": late}));
        let f = build(&newer, &history, NOW);
        assert_eq!(f["episodes"][0]["state"], "verified");
        assert_eq!(f["episodes"].as_array().unwrap().len(), 2);
        assert_eq!(f["episodes"][1]["state"], "open");
        assert_eq!(f["episodes"][1]["first_seen"], 19900);
        newer["snapshot"]["payload"]["rules"] = json!({});
        newer["snapshot"]["payload"]["incidents"] = json!({});
        let retained = build(&newer, &history, NOW);
        assert_eq!(retained["episodes"].as_array().unwrap().len(), 2);
        assert_eq!(retained["episodes"][1]["state"], "open");
        newer["installation"] = json!("sq-2"); newer["workspace"] = json!("ws-2");
        assert!(build(&newer, &Value::Null, NOW)["episodes"].as_array().unwrap().is_empty());
    }
}
