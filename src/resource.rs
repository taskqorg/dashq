//! Resource samples (dashq.md D4.4): agentq `agentq.resource` events from enrolled history or Langfuse, normalized
//! into a current value and a bounded raw window per installation. Pure, so the fixture pins it.

use crate::data::{dur, safe_id};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// Producer bucket (agentq.md § 6 Resource samples).
pub const BUCKET: u64 = 300;
/// Newest sample older than three buckets is stale.
pub const STALE: u64 = 900;
/// Samples per installation and window: 7 days of buckets, the producer's queue cap.
pub const CAP: usize = 2016;
/// A trend needs this many samples.
const TREND: usize = 3;

/// Short producer reason, or `fallback` when absent or not a plain short line.
fn why(v: &Value, fallback: &str) -> Value {
    json!(v.as_str().filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_graphic() || b == b' ')).unwrap_or(fallback))
}

fn bytes(v: &Value) -> Option<u64> {
    v.as_u64()
}

/// One normalized sample; `get(k)` reads producer attribute `agentq.resource.<k>`. Unknown stays `null` with a reason,
/// never zero; nothing is derived from other fields (D6).
pub fn sample(get: &dyn Fn(&str) -> Value, at: u64) -> Value {
    let pct = get("cpu_percent").as_f64();
    let cpu = match pct {
        Some(p) if (0.0..=100.0).contains(&p) => json!({"percent": (p * 10.0).round() / 10.0, "interval_ms": get("cpu_interval_ms").as_u64(), "reason": null}),
        Some(_) => json!({"percent": null, "interval_ms": null, "reason": "out of range"}),
        None => json!({"percent": null, "interval_ms": null, "reason": why(&get("cpu_unknown"), "not reported")}),
    };
    let (used, avail, total) = (bytes(&get("memory_used_bytes")), bytes(&get("memory_available_bytes")), bytes(&get("memory_total_bytes")));
    let memory = match (used, total) {
        (Some(u), Some(t)) if t > 0 && u <= t => json!({"used_bytes": u, "available_bytes": avail.filter(|a| *a <= t), "total_bytes": t, "reason": null}),
        (Some(_), Some(_)) => json!({"used_bytes": null, "available_bytes": null, "total_bytes": null, "reason": "inconsistent numbers"}),
        _ => json!({"used_bytes": null, "available_bytes": null, "total_bytes": null, "reason": why(&get("memory_unknown"), "not reported")}),
    };
    // a string from Langfuse, an array from an enrolled payload
    let raw = get("disks");
    let list = match &raw {
        Value::String(s) => serde_json::from_str::<Value>(s).ok().filter(Value::is_array),
        Value::Array(_) => Some(raw.clone()),
        _ => None,
    };
    let disk_reason = match (&list, &raw) {
        (Some(_), _) => why(&get("disk_unknown"), "").as_str().filter(|s| !s.is_empty()).map_or(Value::Null, |s| json!(s)),
        (None, Value::Null) => why(&get("disk_unknown"), "not reported"),
        (None, _) => json!("disk list unreadable"),
    };
    let mut disks: Vec<Value> = list.iter().flat_map(|l| l.as_array().into_iter().flatten()).filter_map(disk).collect();
    disks.sort_by(|a, b| (a["role"] != "root", a["id"].as_str()).cmp(&(b["role"] != "root", b["id"].as_str())));
    disks.dedup_by(|a, b| a["id"] == b["id"]);
    json!({"at": at, "cpu": cpu, "memory": memory, "disks": disks, "disk_reason": disk_reason, "clock_step_ms": get("clock_step_ms").as_i64()})
}

/// One filesystem; an entry without a usable id is dropped (no identity to join samples on).
fn disk(d: &Value) -> Option<Value> {
    let id = d["id"].as_str().filter(|i| i.len() <= 64 && safe_id(i))?;
    let role = d["role"].as_str().filter(|r| matches!(*r, "root" | "other"));
    let fstype = d["fstype"].as_str().filter(|f| f.len() <= 16 && safe_id(f));
    let (u, f, t) = (bytes(&d["used_bytes"]), bytes(&d["free_bytes"]), bytes(&d["total_bytes"]));
    let (nums, reason) = match (u, f, t) {
        // reserved blocks make used + free < total, never more
        (Some(u), Some(f), Some(t)) if t > 0 && u <= t && f <= t => ((json!(u), json!(f), json!(t)), Value::Null),
        (Some(_), Some(_), Some(_)) => ((Value::Null, Value::Null, Value::Null), json!("inconsistent numbers")),
        _ => ((Value::Null, Value::Null, Value::Null), why(&d["unknown"], "not reported")),
    };
    Some(json!({"id": id, "role": role, "fstype": fstype, "used_bytes": nums.0, "free_bytes": nums.1, "total_bytes": nums.2, "reason": reason}))
}

/// Reset markers: the producer's CPU baseline restarted, so the series breaks there instead of dropping.
fn reset(s: &Value) -> bool {
    matches!(s["cpu"]["reason"].as_str(), Some("warmup" | "counter reset"))
}

/// One installation's samples (time order, deduplicated by time) into `{source, current, history}`.
fn series(mut pts: Vec<Value>, now: u64, from: u64, incomplete: bool) -> Value {
    pts.sort_by_key(|p| p["at"].as_u64());
    pts.dedup_by(|a, b| a["at"] == b["at"]);
    let dropped = pts.len().saturating_sub(CAP);
    pts.drain(..dropped);
    let incomplete = incomplete || dropped > 0;
    let current = pts.last().cloned().unwrap_or(Value::Null);
    let at = current["at"].as_u64();
    let age = at.map(|a| now.saturating_sub(a));
    let (state, reason) = match age {
        None => ("unknown", json!("no sample in the window")),
        Some(a) if a > STALE => ("stale", json!(format!("sample is {} old (limit {})", dur(a), dur(STALE)))),
        _ if incomplete => ("incomplete", json!("capped query")),
        _ => ("ok", Value::Null),
    };
    let mut gaps = Vec::new();
    let mut edges: Vec<u64> = vec![from];
    edges.extend(pts.iter().filter_map(|p| p["at"].as_u64()));
    edges.push(now);
    for w in edges.windows(2) {
        // more than two buckets without a sample, or a stale tail
        let limit = if w[1] == now { STALE } else { 2 * BUCKET };
        if w[1].saturating_sub(w[0]) > limit {
            gaps.push(json!({"from": w[0], "to": w[1]}));
        }
    }
    let resets: Vec<Value> = pts.iter().filter(|p| reset(p)).map(|p| p["at"].clone()).collect();
    let (hstate, hreason) = if pts.len() < TREND { ("warmup", json!(format!("too few samples: {} of {TREND}", pts.len()))) } else { ("ok", Value::Null) };
    json!({
        "source": {"state": state, "at": at, "age_s": age, "reason": reason, "incomplete": incomplete},
        "current": current,
        "history": {"samples": pts.len(), "state": hstate, "reason": hreason, "incomplete": incomplete, "gaps": gaps, "resets": resets, "points": pts},
    })
}

/// `/data` `resources`: one row per installation over `[now - window, now]`. Enrolled history (durable,
/// authenticated) wins over Langfuse for the same installation; each row names its provenance.
/// `lf`: Langfuse `agentq.resource` observations; `enrolled`: `[{installation, workspace, host, runtime, samples:
/// [{at, event_id, payload}], incomplete, error}]` already scoped to the actor.
pub fn build(lf: &[&Value], lf_capped: bool, enrolled: &Value, now: u64, window: u64, fhost: Option<&str>) -> Value {
    let from = now.saturating_sub(window);
    let inside = |a: u64| a >= from && a <= now + 60;
    let mut rows: BTreeMap<String, Value> = BTreeMap::new();
    for e in enrolled.as_array().into_iter().flatten() {
        let (Some(inst), Some(host)) = (e["installation"].as_str(), e["host"].as_str()) else { continue };
        let pts: Vec<Value> = e["samples"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["at"].as_u64().is_some_and(inside))
            .map(|s| {
                let p = &s["payload"];
                let mut x = sample(&|k| p[format!("agentq.resource.{k}")].clone(), s["at"].as_u64().unwrap());
                x["event_id"] = s["event_id"].clone();
                x
            })
            .collect();
        if pts.is_empty() && e["error"].is_null() {
            continue;
        }
        let mut r = series(pts, now, from, e["incomplete"] == true);
        if let Some(err) = e["error"].as_str() {
            r["source"]["state"] = json!("unknown");
            r["source"]["reason"] = json!(format!("read failed ({err})"));
        }
        r["installation"] = json!(inst);
        r["host"] = json!(host);
        r["runtime"] = e["runtime"].clone();
        r["workspace"] = e["workspace"].clone();
        r["provenance"] = json!("enrollment");
        // one row per installation and workspace, as the projection (D11)
        rows.insert(format!("{inst}\n{}", e["workspace"].as_str().unwrap_or("")), r);
    }
    let enrolled: std::collections::HashSet<String> = rows.values().filter_map(|r| r["installation"].as_str().map(String::from)).collect();
    let attr = |r: &Value, k: &str| r["metadata"][format!("attributes.{k}")].clone();
    let mut lf_rows: BTreeMap<String, (String, Value, Vec<Value>)> = BTreeMap::new();
    for r in lf {
        let (Some(inst), Some(name)) = (attr(r, "agentq.installation").as_str().filter(|i| safe_id(i)).map(String::from), attr(r, "host.name").as_str().filter(|h| h.len() <= 64 && safe_id(h)).map(String::from)) else { continue };
        let Some(at) = r["startTime"].as_str().and_then(crate::source::iso_secs).filter(|a| inside(*a)) else { continue };
        if enrolled.contains(&inst) {
            continue;
        }
        let runtime = attr(r, "agentq.runtime");
        let host = format!("{name}/{}", if runtime == "wsl" { "wsl" } else { "native" });
        let mut x = sample(&|k| attr(r, &format!("agentq.resource.{k}")), at);
        x["event_id"] = r["traceId"].as_str().filter(|t| safe_id(t)).map_or(Value::Null, |t| json!(t));
        let e = lf_rows.entry(inst).or_insert_with(|| (host.clone(), Value::Null, Vec::new()));
        // the newest sample names host and runtime
        if e.2.iter().all(|p| p["at"].as_u64() <= Some(at)) {
            e.0 = host;
            e.1 = runtime.as_str().filter(|s| s.len() <= 16 && safe_id(s)).map_or(Value::Null, |s| json!(s));
        }
        e.2.push(x);
    }
    for (inst, (host, runtime, pts)) in lf_rows {
        let mut r = series(pts, now, from, lf_capped);
        r["installation"] = json!(inst);
        r["host"] = json!(host);
        r["runtime"] = runtime;
        r["workspace"] = Value::Null;
        r["provenance"] = json!("langfuse");
        rows.insert(inst, r);
    }
    let rows: Vec<Value> = rows.into_values().filter(|r| fhost.is_none_or(|h| r["host"] == h)).collect();
    json!({"window_s": window, "from": from, "to": now, "bucket_s": BUCKET, "aggregation": "none", "cap": CAP, "rows": rows})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lf(id: &str, at: &str, attrs: Value) -> Value {
        let mut m = json!({"attributes.agentq.installation": "inst-a", "attributes.host.name": "fra", "attributes.agentq.runtime": "linux"});
        for (k, v) in attrs.as_object().unwrap() {
            m[format!("attributes.agentq.resource.{k}")] = v.clone();
        }
        json!({"id": id, "traceId": id, "startTime": at, "metadata": m})
    }

    #[test]
    fn unknown_stays_unknown_and_nothing_is_inferred() {
        let s = sample(&|k| json!({"cpu_unknown": "warmup", "memory_used_bytes": 5, "memory_total_bytes": 4, "disks": "[{\"id\":\"ab\",\"role\":\"root\",\"used_bytes\":null,\"unknown\":\"stat failed\"},{\"id\":\"cd\",\"used_bytes\":1,\"free_bytes\":2,\"total_bytes\":4}]"})[k].clone(), 10);
        assert_eq!(s["cpu"], json!({"percent": null, "interval_ms": null, "reason": "warmup"}));
        assert_eq!(s["memory"]["reason"], "inconsistent numbers");
        assert!(s["memory"]["used_bytes"].is_null(), "never a clamped or derived value");
        assert_eq!(s["disks"][0], json!({"id": "ab", "role": "root", "fstype": null, "used_bytes": null, "free_bytes": null, "total_bytes": null, "reason": "stat failed"}));
        assert_eq!((s["disks"][1]["used_bytes"].as_u64(), s["disks"][1]["total_bytes"].as_u64()), (Some(1), Some(4)));
        let none = sample(&|_| Value::Null, 10);
        assert_eq!((none["cpu"]["reason"].as_str(), none["memory"]["reason"].as_str(), none["disk_reason"].as_str()), (Some("not reported"), Some("not reported"), Some("not reported")));
        assert_eq!(sample(&|k| if k == "disks" { json!("[{\"id\":") } else { Value::Null }, 1)["disk_reason"], "disk list unreadable");
    }

    #[test]
    fn window_keeps_gaps_resets_stale_and_provenance() {
        let now = 1_791_875_139; // 07:05:39Z
        let ok = json!({"cpu_percent": 12.34, "memory_used_bytes": 2, "memory_total_bytes": 8, "disks": "[]"});
        let rows = [
            lf("t1", "2026-10-13T06:40:00Z", ok.clone()),
            lf("t2", "2026-10-13T06:45:00Z", json!({"cpu_unknown": "counter reset"})),
            // 06:50 and 06:55 missing: a gap; a resend of the same bucket counts once
            lf("t3", "2026-10-13T07:00:00Z", ok.clone()),
            lf("t3", "2026-10-13T07:00:00Z", ok.clone()),
            lf("old", "2026-10-12T07:00:00Z", ok.clone()),
        ];
        let refs: Vec<&Value> = rows.iter().collect();
        let doc = build(&refs, false, &json!([]), now, 1800, None);
        let r = &doc["rows"][0];
        assert_eq!((r["provenance"].as_str(), r["host"].as_str(), r["source"]["state"].as_str()), (Some("langfuse"), Some("fra/native"), Some("ok")));
        assert_eq!(r["current"]["cpu"]["percent"], 12.3);
        let h = &r["history"];
        assert_eq!((h["samples"].as_u64(), h["state"].as_str()), (Some(3), Some("ok")));
        assert_eq!(h["resets"], json!([1_791_873_900u64]));
        assert_eq!(h["gaps"], json!([{"from": 1_791_873_900u64, "to": 1_791_874_800u64}]));
        // stale and capped stay visible; enrolled history replaces Langfuse for the same installation
        let late = build(&refs, true, &json!([]), now + 3600, 7200, None);
        assert_eq!(late["rows"][0]["source"]["state"], "stale");
        assert_eq!(late["rows"][0]["history"]["incomplete"], true);
        let enrolled = json!([{"installation": "inst-a", "workspace": "w", "host": "fra/linux", "runtime": "linux", "incomplete": false, "error": null,
                               "samples": [{"at": now - 60, "event_id": "e1", "payload": {"agentq.resource.cpu_percent": 50, "agentq.resource.disks": []}}]}]);
        let both = build(&refs, false, &enrolled, now, 1800, None);
        assert_eq!(both["rows"].as_array().unwrap().len(), 1);
        let e = &both["rows"][0];
        assert_eq!((e["provenance"].as_str(), e["current"]["cpu"]["percent"].as_f64(), e["history"]["state"].as_str()), (Some("enrollment"), Some(50.0), Some("warmup")));
        assert!(build(&refs, false, &json!([]), now, 1800, Some("other/native"))["rows"].as_array().unwrap().is_empty());
    }
}
