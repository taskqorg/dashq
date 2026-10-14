//! Bounded Langfuse forwarding of `agentq-event/1` (D11, C5). The outbox drains in arrival order on its own
//! thread: a slow or failed destination never blocks ingestion, snapshots or projection.

use crate::source;
use crate::store::{Pending, Store};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

/// The export schema DashQ forwards: `{"name": <agentq.md § 6 span name>, "metadata": {..}}`, metadata allowlisted at ingest.
pub const SCHEMA: &str = "agentq-event/1";
pub const NAMES: [&str; 6] = ["agentq.installation", "agentq.run", "agentq.compress", "agentq.retrieval", "agentq.incident", "agentq.resource"];
/// Rows per ingestion request.
const BATCH: usize = 100;

/// An `agentq-event/1` payload: exactly a known span name and a metadata object.
pub fn valid(p: &Value) -> bool {
    p.as_object().is_some_and(|o| o.len() == 2 && o["metadata"].is_object() && o.get("name").and_then(Value::as_str).is_some_and(|n| NAMES.contains(&n)))
}

/// The Langfuse observation ID: installation, generation and event ID, so a retry upserts the same observation.
pub fn observation(r: &Pending) -> String {
    format!("{}-{}-{}", r.installation, r.generation, r.event_id)
}

/// One `event-create` per row, timed at producer observation, never at retry time (C5).
pub fn batch(rows: &[Pending]) -> Value {
    let events: Vec<Value> = rows
        .iter()
        .map(|r| {
            let p: Value = serde_json::from_str(&r.payload).unwrap_or(Value::Null);
            let (id, at) = (observation(r), source::iso(r.observed.max(0) as u64));
            let mut meta = p["metadata"].clone();
            meta["dashq.installation"] = json!(r.installation);
            meta["dashq.generation"] = json!(r.generation);
            meta["dashq.event_id"] = json!(r.event_id);
            json!({"id": id, "timestamp": at, "type": "event-create",
                "body": {"id": id, "traceId": id, "name": p["name"], "startTime": at, "metadata": meta}})
        })
        .collect();
    json!({"batch": events})
}

/// Last forwarding attempt: `(time, failure reason)`; `None` before the first attempt.
pub type Last = Mutex<Option<(i64, Option<String>)>>;

/// One pass: sends the oldest rows. Accepted rows leave the outbox; rows the destination refuses for good (4xx other
/// than 429) become export gaps; everything else stays for the next pass under the same observation ID.
pub fn pass(store: &Store, base: &str, auth: &str, now: i64) -> Result<(), String> {
    let rows = store.outbox(BATCH).map_err(|e| format!("store {e:?}"))?;
    if rows.is_empty() {
        return Ok(());
    }
    let headers = [format!("Authorization: {auth}"), "Content-Type: application/json".into()];
    let r = source::send(&format!("{base}/api/public/ingestion"), &headers, Some(&batch(&rows).to_string())).map_err(|(e, _)| e)?;
    if !matches!(r.status, 200 | 207) {
        return Err(format!("HTTP {}", r.status));
    }
    let ids = |k: &str| -> HashMap<String, u64> {
        r.body[k].as_array().into_iter().flatten().filter_map(|x| Some((x["id"].as_str()?.to_string(), x["status"].as_u64().unwrap_or(0)))).collect()
    };
    let (ok, failed) = (ids("successes"), ids("errors"));
    let (mut sent, mut refused) = (vec![], vec![]);
    for row in &rows {
        let id = observation(row);
        match failed.get(&id) {
            Some(s) if (400..500).contains(s) && *s != 429 => refused.push(row.id),
            Some(_) => {}
            None if ok.contains_key(&id) => sent.push(row.id),
            // neither confirmed nor refused: unknown, kept
            None => {}
        }
    }
    store.exported(&sent, &refused, now).map_err(|e| format!("store {e:?}"))?;
    match rows.len() - sent.len() - refused.len() {
        0 => Ok(()),
        n => Err(format!("{n} events not confirmed")),
    }
}

/// `sources.export` for `/data`: `unknown` without a destination or before the first attempt; `ok` only after a
/// confirmed pass; `stale` (delayed) while a pass fails. Counts are global outbox rows and gap events.
pub fn status(store: Option<&Store>, last: &Last, configured: bool, now: u64) -> Value {
    let src = |state: &str, reason: Option<String>, at: Option<i64>| json!({"state": state, "reason": reason, "at": at, "age_s": at.map(|a| now as i64 - a), "incomplete": false});
    let Some(store) = store else { return src("unknown", Some("enrollment not set up".into()), None) };
    let (pending, gaps) = match store.export_counts() {
        Ok(c) => c,
        Err(e) => return src("unknown", Some(format!("read failed (store {e:?})")), None),
    };
    let mut s = match (configured, last.lock().map(|l| l.clone()).unwrap_or(None)) {
        (false, _) => src("unknown", Some("export not configured".into()), None),
        (true, None) => src("unknown", Some("no forwarding attempt yet".into()), None),
        (true, Some((at, None))) => src("ok", None, Some(at)),
        (true, Some((at, Some(why)))) => src("stale", Some(format!("export delayed: {why}")), Some(at)),
    };
    s["pending"] = json!(pending);
    s["gap_events"] = json!(gaps);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Event, Installation, Kind, LIMITS, Outcome};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    const NOW: i64 = 1_800_000_000;
    const SECRET: &str = "sk-lf-SECRET";

    /// Recorded ingestion stub (PM: no Docker stacks): stores exact request bodies and upserts observations by
    /// ID like Langfuse. `plan` answers each request in turn: `ok`, `lost` (stored, then 500), `500`, `refuse <id>`.
    struct Stub {
        base: String,
        bodies: Arc<Mutex<Vec<String>>>,
        observations: Arc<Mutex<HashMap<String, Value>>>,
    }

    fn stub(plan: Vec<&'static str>) -> Stub {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let (bodies, observations) = (Arc::new(Mutex::new(vec![])), Arc::new(Mutex::new(HashMap::new())));
        let (b, o) = (bodies.clone(), observations.clone());
        std::thread::spawn(move || {
            for (s, step) in l.incoming().flatten().zip(plan) {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut len = 0;
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                let body = String::from_utf8(body).unwrap();
                b.lock().unwrap().push(body.clone());
                let doc: Value = serde_json::from_str(&body).unwrap();
                let (mut ok, mut err) = (vec![], vec![]);
                for e in doc["batch"].as_array().unwrap() {
                    let id = e["body"]["id"].as_str().unwrap().to_string();
                    if step.strip_prefix("refuse ") == Some(id.as_str()) {
                        err.push(json!({"id": id, "status": 400, "message": "invalid"}));
                    } else if step != "500" {
                        o.lock().unwrap().insert(id.clone(), e["body"].clone());
                        ok.push(json!({"id": id, "status": 201}));
                    }
                }
                let (status, reply) = match step {
                    "500" | "lost" => ("500 Internal Server Error", "{}".to_string()),
                    _ => ("207 Multi-Status", json!({"successes": ok, "errors": err}).to_string()),
                };
                let mut s = s;
                let _ = write!(s, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len());
            }
        });
        Stub { base, bodies, observations }
    }

    fn store(name: &str) -> Store {
        let d = std::env::temp_dir().join(format!("dashq-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let s = Store::open(&d.join("dashq.db"), LIMITS).unwrap();
        let inst = Installation { id: "inst-1", product: "agentq", runtime: "native", label: "fra" };
        s.grant(&inst, "w", "a", r#"["agentq-snapshot/1", "agentq-event/1"]"#, b"v1", NOW).unwrap();
        s
    }

    fn event(id: &'static str, payload: &'static str) -> Event<'static> {
        Event { id, schema: SCHEMA, observed: NOW - 30, kind: Kind::Export, payload }
    }

    /// Chain: ingest -> outbox -> Langfuse ingestion -> readback (A1-A3). Outage and a lost reply keep rows and
    /// event IDs; the retry upserts the same observation; a refusal becomes a gap; snapshots stay available.
    #[test]
    fn forwarding_retries_by_event_id_and_reports_gaps() {
        let s = store("chain");
        let run = r#"{"name":"agentq.run","metadata":{"runner":"cargo test","exit":0}}"#;
        let bad = r#"{"name":"agentq.compress","metadata":{"bytes":1}}"#;
        assert_eq!(s.accept(b"v1", &[event("r1", run), event("r2", bad)], NOW).unwrap(), [Outcome::Accepted, Outcome::Accepted]);
        let auth = format!("Basic {SECRET}");
        let last: Last = Mutex::new(None);
        assert_eq!(status(Some(&s), &last, true, NOW as u64)["reason"], "no forwarding attempt yet");

        // outage: nothing listens; rows stay, delayed is explicit, snapshot ingestion still works
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        let e = pass(&s, &format!("http://{dead}"), &auth, NOW).unwrap_err();
        *last.lock().unwrap() = Some((NOW, Some(e)));
        let st = status(Some(&s), &last, true, NOW as u64);
        assert_eq!((st["state"].as_str(), st["pending"].as_i64()), (Some("stale"), Some(2)));
        let snap = Event { id: "s1", schema: "agentq-snapshot/1", observed: NOW, kind: Kind::Snapshot, payload: "{}" };
        assert_eq!(s.accept(b"v1", &[snap], NOW).unwrap(), [Outcome::Accepted]);

        let lf = stub(vec!["500", "lost", "refuse inst-1-1-r2", "ok"]);
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Err("HTTP 500".into()));
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Err("HTTP 500".into()));
        assert_eq!(s.export_counts().unwrap(), (2, 0));
        // the reply was lost after Langfuse stored both; the retry sends the same IDs and r2 is refused for good
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 1));
        assert_eq!(s.project(crate::store::Scope::All).unwrap()[0].export_gap, 1);

        let bodies = lf.bodies.lock().unwrap();
        let ids = |b: &str| serde_json::from_str::<Value>(b).unwrap()["batch"].as_array().unwrap().iter().map(|e| e["body"]["id"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(bodies.len(), 3);
        assert!(bodies.iter().all(|b| ids(b) == ["inst-1-1-r1", "inst-1-1-r2"]));
        // credentials travel only in the Authorization header, never in a forwarded body
        assert!(bodies.iter().all(|b| !b.contains(SECRET) && !b.contains("Basic")));
        // readback: one observation per event despite three deliveries, producer time and event ID kept
        let obs = lf.observations.lock().unwrap();
        let r1 = &obs["inst-1-1-r1"];
        assert_eq!(obs.len(), 2);
        assert_eq!((r1["name"].as_str(), r1["startTime"].as_str()), (Some("agentq.run"), Some(source::iso(NOW as u64 - 30).as_str())));
        assert_eq!((r1["metadata"]["dashq.event_id"].as_str(), r1["metadata"]["runner"].as_str()), (Some("r1"), Some("cargo test")));
        // a replayed ingest of a forwarded event is the same receipt, never a second export
        assert_eq!(s.accept(b"v1", &[event("r1", run)], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.export_counts().unwrap(), (0, 1));
    }

    #[test]
    fn only_known_span_names_with_metadata_are_exports() {
        assert!(valid(&json!({"name": "agentq.resource", "metadata": {}})));
        assert!(!valid(&json!({"name": "agentq.other", "metadata": {}})));
        assert!(!valid(&json!({"name": "agentq.run", "metadata": {}, "extra": 1})));
        assert!(!valid(&json!({"name": "agentq.run", "metadata": 1})));
    }
}
