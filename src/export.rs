//! Bounded Langfuse forwarding of `agentq-event/1` (D11, C5). The outbox drains in arrival order on its own
//! thread: a slow or failed destination never blocks ingestion, snapshots or projection.

use crate::source;
use crate::store::{Pending, Store};
use serde_json::{Value, json};
use std::sync::Mutex;

/// The export schema DashQ forwards: `{"name": <agentq.md § 6 span name>, "metadata": {..}}`, metadata allowlisted at ingest.
pub const SCHEMA: &str = "agentq-event/1";
pub const NAMES: [&str; 6] = ["agentq.installation", "agentq.run", "agentq.compress", "agentq.retrieval", "agentq.incident", "agentq.resource"];
/// Rows per ingestion request.
const BATCH: usize = 100;

/// An `agentq-event/1` payload: exactly a known span name and a metadata object.
pub fn valid(p: &Value) -> bool {
    p.as_object().is_some_and(|o| o.len() == 2 && o.get("metadata").is_some_and(Value::is_object) && o.get("name").and_then(Value::as_str).is_some_and(|n| NAMES.contains(&n)))
}

/// The DashQ observation ID: installation, generation and event ID, so a retry carries the same identity.
/// `:` never occurs in installation or event IDs (`A-Z a-z 0-9 . _ -`, enforced at ingest), so the ID is unambiguous.
pub fn observation(r: &Pending) -> String {
    format!("{}:{}:{}", r.installation, r.generation, r.event_id)
}

/// OTEL trace and span IDs (hex, 32 and 16 digits) derived from the observation ID: a retry re-sends the same span.
pub fn span_ids(r: &Pending) -> (String, String) {
    let h: String = ring::digest::digest(&ring::digest::SHA256, observation(r).as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
    (h[..32].into(), h[32..48].into())
}

/// OTLP time of a producer observation: unsigned 64-bit nanoseconds. `None` before 1970 or after 2554, which
/// OTLP cannot express; ingest rejects such exports and a pass turns any left in the outbox into gaps.
pub fn nanos(observed: i64) -> Option<String> {
    u64::try_from(observed).ok()?.checked_mul(1_000_000_000).map(|n| n.to_string())
}

/// OTLP/JSON for Langfuse v4 ingestion: one complete `event` span per row, timed at producer observation, never
/// at retry time (C5). Metadata keys become `langfuse.observation.metadata.*` (non-strings as JSON text). Rows
/// without an OTLP time are left out.
pub fn batch(rows: &[Pending]) -> Value {
    let attr = |k: &str, v: &Value| json!({"key": k, "value": {"stringValue": v.as_str().map_or_else(|| v.to_string(), String::from)}});
    let spans: Vec<Value> = rows
        .iter()
        .filter_map(|r| Some((r, nanos(r.observed)?)))
        .map(|(r, at)| {
            let p: Value = serde_json::from_str(&r.payload).unwrap_or(Value::Null);
            let (trace, span) = span_ids(r);
            let mut attrs = vec![attr("langfuse.observation.type", &json!("event")), attr("langfuse.trace.name", &p["name"])];
            let ids = [("dashq.id", json!(observation(r))), ("dashq.installation", json!(r.installation)), ("dashq.generation", json!(r.generation)), ("dashq.event_id", json!(r.event_id))];
            for (k, v) in p["metadata"].as_object().into_iter().flatten().map(|(k, v)| (k.as_str(), v.clone())).chain(ids) {
                attrs.push(attr(&format!("langfuse.observation.metadata.{k}"), &v));
            }
            json!({"traceId": trace, "spanId": span, "name": p["name"], "kind": 1, "startTimeUnixNano": at, "endTimeUnixNano": at, "attributes": attrs})
        })
        .collect();
    json!({"resourceSpans": [{"resource": {"attributes": [attr("service.name", &json!("dashq"))]}, "scopeSpans": [{"scope": {"name": "dashq"}, "spans": spans}]}]})
}

/// Last forwarding attempt: `(time, failure reason)`; `None` before the first attempt.
pub type Last = Mutex<Option<(i64, Option<String>)>>;

/// Sends `rows` as one OTLP request: `Ok(Some(n))` accepted with `n` spans rejected (`partialSuccess.rejectedSpans`,
/// a number or OTLP/JSON int64 string; an `errorMessage` with zero rejected is a warning), `Ok(None)` refused for good
/// (4xx other than 429). An unreadable `rejectedSpans` counts every span as rejected.
fn post(base: &str, auth: &str, rows: &[Pending]) -> Result<Option<usize>, String> {
    let headers = [format!("Authorization: {auth}"), "Content-Type: application/json".into(), "x-langfuse-ingestion-version: 4".into()];
    let r = source::send(&format!("{base}/api/public/otel/v1/traces"), &headers, Some(&batch(rows).to_string())).map_err(|(e, _)| e)?;
    match r.status {
        200 => Ok(Some(match &r.body["partialSuccess"]["rejectedSpans"] {
            Value::Null => 0,
            v => v.as_u64().or_else(|| v.as_str()?.parse().ok()).map_or(rows.len(), |n| (n as usize).min(rows.len())),
        })),
        s if (400..500).contains(&s) && s != 429 => Ok(None),
        s => Err(format!("HTTP {s}")),
    }
}

/// One pass: sends the oldest rows of the oldest row's grant, so any loss stays scoped to one installation and
/// workspace. An accepted request removes its rows from the outbox; if Langfuse rejected `n` of them without naming
/// which, they become one gap of `n` events over the rows' time window and are not resent. A refused request is
/// retried row by row in the same pass and only the refused rows become export gaps, as do rows without an OTLP
/// time. Anything else keeps the rows for the next pass under the same span IDs.
pub fn pass(store: &Store, base: &str, auth: &str, now: i64) -> Result<(), String> {
    let (mut rows, untimed): (Vec<Pending>, Vec<Pending>) = store.outbox(BATCH).map_err(|e| format!("store {e:?}"))?.into_iter().partition(|r| nanos(r.observed).is_some());
    if let Some(head) = rows.first().map(|r| (r.installation.clone(), r.generation)) {
        rows.retain(|r| (&r.installation, r.generation) == (&head.0, head.1));
    }
    let (mut sent, mut refused) = (vec![], untimed.iter().map(|r| r.id).collect::<Vec<_>>());
    if rows.is_empty() {
        return store.exported(&sent, &refused, now).map_err(|e| format!("store {e:?}"));
    }
    let mut err = None;
    match post(base, auth, &rows)? {
        Some(0) => sent = rows.iter().map(|r| r.id).collect(),
        Some(n) => {
            store.partially_exported(&rows, n as i64, now).map_err(|e| format!("store {e:?}"))?;
            sent = rows.iter().map(|r| r.id).collect();
        }
        None => {
            for row in &rows {
                match post(base, auth, std::slice::from_ref(row)) {
                    Ok(Some(0)) => sent.push(row.id),
                    Ok(_) => refused.push(row.id),
                    Err(e) => err = Some(e),
                }
            }
        }
    }
    store.exported(&sent, &refused, now).map_err(|e| format!("store {e:?}"))?;
    match (rows.len() + untimed.len() - sent.len() - refused.len(), err) {
        (0, _) => Ok(()),
        (n, e) => Err(format!("{n} events not confirmed{}", e.map(|e| format!(" ({e})")).unwrap_or_default())),
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
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::Arc;

    const NOW: i64 = 1_800_000_000;
    const SECRET: &str = "sk-lf-SECRET";

    /// Recorded ingestion stub (PM: no Docker stacks): stores exact request bodies and upserts observations by
    /// span ID like the live readback showed. `plan` answers each request in turn: `ok`, `lost` (stored, then 500),
    /// `500`, `refuse <id>` (400 for a request containing that DashQ ID, else ok), `partial <n>` (200 with
    /// `partialSuccess.rejectedSpans` as int64 string, the last n spans not stored), `warn` (200, zero rejected, a message).
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
                let spans = doc["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().unwrap().clone();
                let refused = step.strip_prefix("refuse ").is_some_and(|x| spans.iter().any(|sp| meta(sp, "dashq.id") == Some(x.to_string())));
                let dropped = step.strip_prefix("partial ").map_or(0, |n| n.parse().unwrap());
                if !refused && step != "500" {
                    o.lock().unwrap().extend(spans[..spans.len() - dropped].iter().cloned().map(|sp| (sp["spanId"].as_str().unwrap().to_string(), sp)));
                }
                let status = match step {
                    "500" | "lost" => "500 Internal Server Error",
                    _ if refused => "400 Bad Request",
                    _ => "200 OK",
                };
                let reply = match step {
                    _ if dropped > 0 => format!(r#"{{"partialSuccess":{{"rejectedSpans":"{dropped}","errorMessage":"invalid span"}}}}"#),
                    "warn" => r#"{"partialSuccess":{"rejectedSpans":0,"errorMessage":"attribute truncated"}}"#.into(),
                    _ => "{}".into(),
                };
                let mut s = s;
                let _ = write!(s, "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}", reply.len());
            }
        });
        Stub { base, bodies, observations }
    }

    fn meta(span: &Value, key: &str) -> Option<String> {
        let k = format!("langfuse.observation.metadata.{key}");
        span["attributes"].as_array()?.iter().find(|a| a["key"] == k.as_str())?["value"]["stringValue"].as_str().map(String::from)
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

        let lf = stub(vec!["500", "lost", "refuse inst-1:1:r2", "refuse inst-1:1:r2", "refuse inst-1:1:r2"]);
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Err("HTTP 500".into()));
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Err("HTTP 500".into()));
        assert_eq!(s.export_counts().unwrap(), (2, 0));
        // the reply was lost after Langfuse stored both; the retry's batch is refused, so each row goes alone:
        // r1 re-sends the same span, r2 is refused for good
        assert_eq!(pass(&s, &lf.base, &auth, NOW), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 1));
        assert_eq!(s.project(crate::store::Scope::All, |_, _| true).unwrap()[0].export_gap, 1);

        let bodies = lf.bodies.lock().unwrap();
        let ids = |b: &str| serde_json::from_str::<Value>(b).unwrap()["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().unwrap().iter().map(|sp| meta(sp, "dashq.id").unwrap()).collect::<Vec<_>>();
        assert_eq!(bodies.iter().map(|b| ids(b)).collect::<Vec<_>>(), [&["inst-1:1:r1", "inst-1:1:r2"][..], &["inst-1:1:r1", "inst-1:1:r2"], &["inst-1:1:r1", "inst-1:1:r2"], &["inst-1:1:r1"], &["inst-1:1:r2"]]);
        // credentials travel only in the Authorization header, never in a forwarded body
        assert!(bodies.iter().all(|b| !b.contains(SECRET) && !b.contains("Basic")));
        // readback: one span per event despite three deliveries of r1, producer time and event ID kept
        let obs = lf.observations.lock().unwrap();
        assert_eq!(obs.len(), 2);
        let r1 = obs.values().find(|sp| meta(sp, "dashq.event_id").as_deref() == Some("r1")).unwrap();
        assert_eq!((r1["name"].as_str(), r1["startTimeUnixNano"].as_str()), (Some("agentq.run"), Some(((NOW as u64 - 30) * 1_000_000_000).to_string().as_str())));
        assert_eq!((meta(r1, "runner").as_deref(), meta(r1, "exit").as_deref()), (Some("cargo test"), Some("0")));
        // a replayed ingest of a forwarded event is the same receipt, never a second export
        assert_eq!(s.accept(b"v1", &[event("r1", run)], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.export_counts().unwrap(), (0, 1));
    }

    /// Chain A3 live: the exact forwarder against a real Langfuse v4 project. Each event is delivered twice (a lost
    /// reply, then the confirming pass) and must read back as one observation once Langfuse merges. Opt-in, tagged `test: dashq#37`:
    /// `DASHQ_LIVE_LANGFUSE=https://<host> LANGFUSE_PUBLIC_KEY=.. LANGFUSE_SECRET_KEY=.. cargo test -- --ignored live`
    #[test]
    #[ignore]
    fn live_langfuse_readback_is_idempotent() {
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} unset"));
        let base = env("DASHQ_LIVE_LANGFUSE");
        let auth = format!("Basic {}", crate::base64(format!("{}:{}", env("LANGFUSE_PUBLIC_KEY"), env("LANGFUSE_SECRET_KEY")).as_bytes()));
        let s = store("live");
        let now = source::now() as i64;
        let id = |n: &str| &*Box::leak(format!("live-{now}-{n}").into_boxed_str());
        let payload = r#"{"name":"agentq.run","metadata":{"test":"dashq#37","runner":"live"}}"#;
        let evs: Vec<Event> = ["a", "b"].iter().map(|n| Event { id: id(n), schema: SCHEMA, observed: now - 30, kind: Kind::Export, payload }).collect();
        assert!(s.accept(b"v1", &evs, now).unwrap().iter().all(|o| *o == Outcome::Accepted));
        let rows = s.outbox(BATCH).unwrap();
        assert_eq!(post(&base, &auth, &rows), Ok(Some(0)), "first delivery, reply treated as lost");
        assert_eq!(pass(&s, &base, &auth, now), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 0));
        let headers = [format!("Authorization: {auth}")];
        let read = |r: &Pending| {
            let url = format!("{base}/api/public/v2/observations?traceId={}&fields=core,basic,metadata", span_ids(r).0);
            source::get(&url, &headers).map_err(|e| e.0).unwrap().body["data"].as_array().cloned().unwrap_or_default()
        };
        // ingestion is queued and the read path merges duplicate rows of one span only later (seen: 2 reads of one
        // span for up to 3 min): wait until each span reads exactly once on two reads 30 s apart, at most 10 min
        let (mut found, mut once, mut doubled) = (vec![], false, false);
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_secs(if once { 30 } else { 10 }));
            found = rows.iter().map(read).collect::<Vec<_>>();
            doubled |= found.iter().any(|f| f.len() > 1);
            let single = found.iter().all(|f| f.len() == 1);
            if single && once {
                break;
            }
            once = single;
        }
        println!("transient duplicate reads before merge: {doubled}");
        for (r, f) in rows.iter().zip(&found) {
            assert_eq!(f.len(), 1, "{} read back {} observations", observation(r), f.len());
            let (o, at) = (&f[0], source::iso(r.observed as u64));
            assert_eq!((o["id"].as_str(), o["name"].as_str(), o["metadata"]["dashq.id"].as_str()), (Some(span_ids(r).1.as_str()), Some("agentq.run"), Some(observation(r).as_str())));
            assert_eq!((o["metadata"]["test"].as_str(), o["startTime"].as_str().map(|t| t.starts_with(&at[..19]))), (Some("dashq#37"), Some(true)));
            println!("readback {} -> span {} ok, 1 observation after 2 deliveries", observation(r), span_ids(r).1);
        }
    }

    /// Chain A2: an export time OTLP cannot express never reaches the outbox, and one already there becomes a gap
    /// while the pass goes on; neither overflows. The last representable second still forwards exactly.
    #[test]
    fn untimed_exports_are_refused_or_gapped() {
        let last = (u64::MAX / 1_000_000_000) as i64;
        assert_eq!((nanos(last).as_deref(), nanos(last + 1), nanos(-1)), (Some("18446744073000000000"), None, None));
        let s = store("untimed");
        let run = r#"{"name":"agentq.run","metadata":{}}"#;
        let ev = |id, observed| Event { id, schema: SCHEMA, observed, kind: Kind::Export, payload: run };
        assert_eq!(s.accept(b"v1", &[ev("big", i64::MAX), ev("edge", last)], NOW).unwrap(), [Outcome::Rejected("payload"), Outcome::Accepted]);
        // rows queued before ingest refused such times
        let db = std::env::temp_dir().join(format!("dashq-export-untimed-{}", std::process::id())).join("dashq.db");
        let queue = |id: &str, observed: i64| {
            let sql = "INSERT INTO outbox (installation, generation, event_id, observed, queued, payload) VALUES ('inst-1', 1, ?1, ?2, ?3, ?4)";
            rusqlite::Connection::open(&db).unwrap().execute(sql, rusqlite::params![id, observed, NOW, run]).unwrap();
        };
        queue("old", i64::MAX);
        let lf = stub(vec!["ok"]);
        assert_eq!(pass(&s, &lf.base, "Basic x", NOW), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 1));
        let obs = lf.observations.lock().unwrap();
        let edge = obs.values().map(|sp| (meta(sp, "dashq.event_id").unwrap(), sp["startTimeUnixNano"].as_str().unwrap().to_string())).collect::<Vec<_>>();
        assert_eq!(edge, [("edge".to_string(), "18446744073000000000".to_string())]);
        // only untimed rows left: they become gaps without a request
        queue("neg", -1);
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap();
        assert_eq!(pass(&s, &format!("http://{dead}"), "Basic x", NOW), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 2));
    }

    /// Chain A2/C5: a 200 whose `partialSuccess` rejects spans loses them; they become one gap of exactly that many
    /// events in the sending grant's workspace, nothing is resent and a warning without rejections confirms the batch.
    /// A batch never mixes grants, so the gap cannot be charged to the wrong installation.
    #[test]
    fn partial_success_records_scoped_gaps() {
        let s = store("partial");
        let other = Installation { id: "inst-2", product: "agentq", runtime: "native", label: "hel" };
        s.grant(&other, "w2", "a", r#"["agentq-event/1"]"#, b"v2", NOW).unwrap();
        let run = r#"{"name":"agentq.run","metadata":{}}"#;
        assert!(s.accept(b"v1", &[event("a", run), event("b", run), event("c", run)], NOW).unwrap().iter().all(|o| *o == Outcome::Accepted));
        assert!(s.accept(b"v2", &[event("d", run)], NOW).unwrap().iter().all(|o| *o == Outcome::Accepted));
        let lf = stub(vec!["partial 1", "warn"]);
        assert_eq!(pass(&s, &lf.base, "Basic x", NOW), Ok(()));
        // inst-1's three rows left with one unnamed loss; inst-2's row waits for its own request
        assert_eq!(s.export_counts().unwrap(), (1, 1));
        assert_eq!(s.export_gaps().unwrap(), [("inst-1".to_string(), 1)]);
        assert_eq!(pass(&s, &lf.base, "Basic x", NOW), Ok(()));
        assert_eq!(s.export_counts().unwrap(), (0, 1));
        let ws: Vec<_> = s.project(crate::store::Scope::All, |_, _| true).unwrap().iter().map(|r| (r.installation.clone(), r.export_gap)).collect();
        assert_eq!(ws, [("inst-1".to_string(), 1), ("inst-2".to_string(), 0)]);
        let bodies = lf.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2, "partial success is never resent");
        assert_eq!(lf.observations.lock().unwrap().len(), 3);
    }

    #[test]
    fn only_known_span_names_with_metadata_are_exports() {
        assert!(valid(&json!({"name": "agentq.resource", "metadata": {}})));
        assert!(!valid(&json!({"name": "agentq.other", "metadata": {}})));
        assert!(!valid(&json!({"name": "agentq.run", "metadata": {}, "extra": 1})));
        assert!(!valid(&json!({"name": "agentq.run", "metadata": 1})));
        // two keys without metadata: rejected, never a panic
        assert!(!valid(&json!({"name": "agentq.run", "extra": {}})));
    }

    /// Distinct (installation, generation, event) triples never share a DashQ ID or span.
    #[test]
    fn observation_ids_are_unambiguous() {
        let row = |i: &str, g: i64, e: &str| Pending { id: 0, installation: i.into(), generation: g, event_id: e.into(), observed: 0, payload: String::new() };
        assert_ne!(observation(&row("a-1", 1, "x")), observation(&row("a", 1, "1-x")));
        assert_eq!(observation(&row("a-1", 1, "x")), "a-1:1:x");
        assert_ne!(span_ids(&row("a-1", 1, "x")), span_ids(&row("a", 1, "1-x")));
        assert_eq!(span_ids(&row("a", 1, "x")).0.len(), 32);
    }
}
