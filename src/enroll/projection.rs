use super::*;

/// `/data` `installations` (D11): expected installations of the actor's scope with producer-time freshness;
/// `None` while enrollment is not set up. `scope` `None`: a remote request without a verified subject sees none.
pub fn projection(e: &Result<Enroll, String>, scope: Option<Scope>, host: Option<&str>, now: u64) -> Option<Value> {
    let e = e.as_ref().ok()?;
    let src = |state: &str, reason: Option<&str>| json!({"state": state, "at": now, "age_s": 0, "reason": reason, "incomplete": false});
    let Some(scope) = scope else { return Some(json!({"source": src("unknown", Some("no signed-in subject")), "rows": []})) };
    let rows = match e.project(scope) {
        Ok(r) => r,
        Err(x) => {
            let why = match x {
                store::Error::Busy => "busy",
                store::Error::Full => "full",
                store::Error::Corrupt => "corrupt",
                _ => "unavailable",
            };
            let mut s = src("unknown", Some(&format!("read failed (store {why})")));
            s["at"] = Value::Null;
            s["age_s"] = Value::Null;
            return Some(json!({"source": s, "rows": []}));
        }
    };
    let rows: Vec<Value> = rows
        .into_iter()
        .filter(|r| host.is_none_or(|h| h == format!("{}/{}", r.label, r.runtime)))
        .map(|r| {
            let (obs, snapshot) = match &r.snapshot {
                None => (json!({"state": "unknown", "at": null, "age_s": null, "reason": "no snapshot yet", "incomplete": false}), Value::Null),
                Some(s) => {
                    let obs = if s.clock_uncertain {
                        json!({"state": "unknown", "at": s.observed, "age_s": null, "reason": "clock uncertain: observed in the future", "incomplete": false})
                    } else {
                        crate::data::source(&json!({"value": true, "at": s.observed.max(0)}), now, stale_secs(&s.schema), None)
                    };
                    let mut payload = serde_json::from_str::<Value>(&s.payload).unwrap_or(Value::Null);
                    strip_line(&s.schema, &mut payload);
                    (obs, json!({"event_id": s.event_id, "schema": s.schema, "payload": payload}))
                }
            };
            // history warmup and export gaps are separate from connection and data age (C5)
            let history = if r.samples < TREND_SAMPLES {
                json!({"samples": r.samples, "state": "warmup", "reason": format!("collecting history: {} of {TREND_SAMPLES} samples", r.samples)})
            } else {
                json!({"samples": r.samples, "state": "ok", "reason": null})
            };
            let export = if r.export_gap > 0 {
                json!({"gap_events": r.export_gap, "state": "gap", "reason": format!("events expired before export: {}", r.export_gap)})
            } else {
                json!({"gap_events": 0, "state": if r.product == "agentq" { "unknown" } else { "not_applicable" }, "reason": if r.product == "agentq" { "export forwarding not qualified" } else { "no Langfuse export required" }})
            };
            let telemetry = if !serde_json::from_str::<Vec<String>>(&r.scopes).unwrap_or_default().iter().any(|s| s == TELEMETRY) {
                json!({"state": "off", "reason": "telemetry not in grant"})
            } else {
                telemetry_json(&r.telemetry)
            };
            let activity = activity(&r, now);
            json!({"installation": r.installation, "product": r.product, "runtime": r.runtime, "host_label": r.label, "workspace": r.workspace, "telemetry": telemetry, "activity": activity,
                "project_limits": if r.product == "taskq" { project_limits(&r, now) } else { Value::Null },
                "generation": r.generation, "connection": r.connection,
                "projected": r.snapshot.as_ref().is_some_and(|s| s.generation == r.generation && renders(&r.product, s)),
                "received_at": r.received_at, "scopes": serde_json::from_str::<Value>(&r.scopes).unwrap_or(Value::Null),
                "capabilities": r.capabilities.as_deref().map(|s| serde_json::from_str::<Value>(s).unwrap_or(Value::Null)).unwrap_or(json!([])),
                "scope": r.board.as_deref().and_then(|b| serde_json::from_str::<Value>(b).ok()), "observation": obs, "history": history, "export": export, "snapshot": snapshot})
        })
        .collect();
    Some(json!({"source": src("ok", None), "rows": rows}))
}

/// #242 A1: no report for longer than this makes an installation silent and an Attention request.
pub const SILENT_SECS: u64 = 3600;
/// #242 A3: no report for longer than this retires an installation from every active view; its records stay.
pub const RETIRE_SECS: u64 = 30 * 86400;

/// An installation row's `activity` (#242): the last report is the newest durable receipt or trusted producer time;
/// nothing ever received counts from the grant. Revoked connections are history, never silent.
pub(super) fn activity(r: &store::Row, now: u64) -> Value {
    let last = r.received_at.into_iter().chain(r.snapshot.as_ref().filter(|s| !s.clock_uncertain).map(|s| s.observed)).max().map(|t| t.max(0) as u64);
    let age = now.saturating_sub(last.unwrap_or(r.created.max(0) as u64));
    let silence = match last {
        Some(_) => format!("no report for {}", crate::data::dur(age)),
        None => format!("no report since approval {} ago", crate::data::dur(age)),
    };
    let (state, reason) = match () {
        _ if r.connection == "revoked" => ("revoked", Some("connection revoked".to_string())),
        _ if age > RETIRE_SECS => ("retired", Some(format!("{silence}; retired from active views, history kept"))),
        _ if age > SILENT_SECS => ("silent", Some(silence)),
        _ => ("reporting", None),
    };
    json!({"state": state, "last_report": last, "age_s": age, "reason": reason})
}

/// An installation row's `telemetry` (D11 #161, #181 A3): DashQ's retained range and its known loss, and the
/// sender's own counters from its newest block. DashQ never infers the sender's counters; missing ones are unknown.
pub(super) fn telemetry_json(t: &store::Telemetry) -> Value {
    if t.blocks == 0 && t.pruned.is_empty() {
        return json!({"state": "unknown", "blocks": 0, "last_synced": null, "backlog_rows": null, "backlog_bytes": null, "gap_rows": null, "reason": "no telemetry block yet",
            "sender": {"state": "unknown", "reason": "no telemetry block yet"}, "retained": null, "pruned": [], "holes": [], "holes_total": 0, "hole_rows": 0, "restarts": 0, "evidence": []});
    }
    let status = t.status.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()).unwrap_or(Value::Null);
    let count = |k: &str| status.get(k).cloned().unwrap_or(Value::Null);
    let missing = ["backlog_rows", "backlog_bytes", "gap_rows"].iter().any(|k| count(k).is_null());
    let why = missing.then_some("status not reported by taskq");
    let sender = if status.is_null() {
        json!({"state": "unknown", "reason": "status not reported by taskq"})
    } else {
        json!({"state": if missing { "unknown" } else { "ok" }, "backlog_rows": count("backlog_rows"), "backlog_bytes": count("backlog_bytes"), "gap_rows": count("gap_rows"), "reason": why})
    };
    let retained = (t.blocks > 0).then(|| json!({"blocks": t.blocks, "rows": t.rows, "bytes": t.bytes, "first_seq": t.first_seq, "last_seq": t.last_seq, "oldest_received": t.oldest, "newest_received": t.newest}));
    json!({"state": if t.blocks > 0 { "ok" } else { "unknown" }, "blocks": t.blocks, "last_synced": t.newest,
        "backlog_rows": count("backlog_rows"), "backlog_bytes": count("backlog_bytes"), "gap_rows": count("gap_rows"),
        "reason": if t.blocks == 0 { json!("every stored block was pruned") } else { json!(why) }, "sender": sender, "retained": retained,
        "pruned": t.pruned.iter().map(|(cause, blocks, rows, through)| json!({"cause": cause, "blocks": blocks, "rows": rows, "through_seq": through})).collect::<Vec<_>>(),
        "holes": t.holes.iter().map(|(a, b)| json!({"from_seq": a, "to_seq": b, "rows": b - a + 1})).collect::<Vec<_>>(),
        "holes_total": t.holes_total, "hole_rows": t.hole_rows, "restarts": t.restarts, "evidence": t.evidence})
}

/// `/data.telemetry_block` for `block=ID` (#181 A6): the stored block named `id` among the projected installation
/// rows, byte for byte as accepted, with its receipt and its installation's coverage; an accepted block no longer
/// stored, or none at all, is `unknown` with the reason.
pub fn telemetry_block(e: &Result<Enroll, String>, rows: &Value, id: &str) -> Value {
    let unknown = |why: &str| json!({"event_id": id, "state": "unknown", "reason": why, "blocks": []});
    let Ok(e) = e else { return unknown("enrollment not set up") };
    let pairs: Vec<(String, String)> = rows.as_array().into_iter().flatten().filter_map(|r| Some((r["installation"].as_str()?.into(), r["workspace"].as_str()?.into()))).collect();
    let (blocks, gone) = match e.store.telemetry_block(id, &pairs) {
        Ok(x) => x,
        Err(_) => return unknown("read failed (store unavailable)"),
    };
    if blocks.is_empty() {
        return match gone.first() {
            Some((_, _, at)) => json!({"event_id": id, "state": "unknown", "reason": "accepted, no longer retained", "received_at": at, "blocks": []}),
            None => unknown("no stored block with this event id"),
        };
    }
    let blocks: Vec<Value> = blocks
        .iter()
        .map(|b| {
            let payload = serde_json::from_str::<Value>(&b.payload).unwrap_or(Value::Null);
            json!({"installation": b.installation, "workspace": b.workspace, "generation": b.generation, "event_id": b.event_id, "schema": TELEMETRY,
                "observed_at": b.observed, "received_at": b.received, "receipt": b.receipt.as_deref().unwrap_or("unknown"),
                "first": payload["first"], "last": payload["last"], "rows": payload["rows"], "bytes": payload["bytes"],
                "payload": payload, "coverage": telemetry_json(&b.coverage)})
        })
        .collect();
    json!({"event_id": id, "state": "ok", "reason": null, "blocks": blocks})
}

/// D21 (#182): attempt stage records of each TaskQ installation in `rows` with telemetry in its grant, from its stored
/// `taskq-telemetry/1` blocks received at or after `since`: `[{installation, workspace, label, telemetry, records, error}]`,
/// `telemetry` the row's coverage, `records` as [`crate::agent_work::from_telemetry`] maps them, oldest block first.
pub fn stage_records(e: &Result<Enroll, String>, rows: &Value, since: i64, background: bool) -> Value {
    let Ok(e) = e else { return json!([]) };
    let reporting = || rows.as_array().into_iter().flatten().filter(|r| r["product"] == "taskq" && r["scopes"].as_array().is_some_and(|s| s.iter().any(|x| x == TELEMETRY)));
    if background && e.stages_warming.load(std::sync::atomic::Ordering::SeqCst) {
        return json!(reporting().map(|r| json!({"installation": r["installation"], "workspace": r["workspace"], "label": r["host_label"], "telemetry": r["telemetry"], "records": [], "error": "refresh in progress"})).collect::<Vec<_>>());
    }
    let mut memo = e.stages.lock().unwrap_or_else(|p| p.into_inner());
    let mut oldest = i64::MAX;
    let out: Vec<Value> = reporting()
        .filter_map(|r| {
            let (inst, ws) = (r["installation"].as_str()?, r["workspace"].as_str()?);
            let label = r["host_label"].as_str().unwrap_or(inst);
            let (mut records, mut error) = (vec![], Value::Null);
            match e.store.telemetry_since(inst, ws, since, |id| !memo.contains_key(&id)) {
                Ok(blocks) => {
                    for (id, payload) in blocks {
                        oldest = oldest.min(id);
                        if let Some(p) = payload {
                            // blocks were checked at ingest; one that no longer decodes yields no records
                            let lines = serde_json::from_str::<Value>(&p).ok().and_then(|p| telemetry_lines(&p)).map_or(vec![], |(_, l)| l);
                            memo.insert(id, std::sync::Arc::new(lines.iter().filter_map(|l| crate::agent_work::from_telemetry(l, label, inst)).collect()));
                        }
                        records.extend(memo[&id].iter().cloned());
                    }
                }
                Err(_) => error = json!("store unavailable"),
            }
            Some(json!({"installation": inst, "workspace": ws, "label": label, "telemetry": r["telemetry"], "records": records, "error": error}))
        })
        .collect();
    // ponytail: ids grow with receipt time, so older entries fell out of every caller's window; a narrower scope may
    // evict a block another scope still reads, which only costs one more decode
    memo.retain(|id, _| *id >= oldest);
    json!(out)
}

/// D26: retain only Fleet observation kinds from actor-scoped, unrevoked telemetry.
/// A bounded read reports truncation; raw rows remain internal and never enter `/data`.
pub fn fleet_records(e: &Result<Enroll, String>, rows: &Value, since: i64, background: bool) -> Value {
    let Ok(e) = e else { return json!([]) };
    json!(rows.as_array().into_iter().flatten().filter(|r| r["product"] == "taskq" && r["connection"] != "revoked" && r["activity"]["state"] != "retired").map(|r| {
        // Authorization precedes cache access; each workspace keeps a separate single-flight last-good read.
        let key=format!("fleet:{}",json!([r["installation"],r["workspace"]]));
        let (db, row) = (e.fleet_db.clone(), r.clone());
        let read = move || {
            let blocks=Store::fleet_blocks(&db, row["installation"].as_str().unwrap_or(""), row["workspace"].as_str().unwrap_or(""), since).map_err(|_|("store unavailable".to_string(),None))?;
            let mut known_loss = vec![];
            let lines = blocks.into_iter().flat_map(|(id, p)| {
                let block = serde_json::from_str::<Value>(&p).ok().and_then(|p| telemetry_lines(&p));
                if block.is_none() { known_loss.push(json!({"block_id": id, "reason": format!("stored block {id} invalid")})); }
                block.map(|(_, l)| l).unwrap_or_default()
            }).filter(|l| matches!(l["observation"]["kind"].as_str(), Some("agent.turn" | "transition" | "worker")));
            // A3: blocks arrive oldest first; the cap keeps the newest rows so current limits stay readable.
            let mut kept = crate::fleet::keep_newest(lines, 10_000);
            kept["known_loss"] = json!(known_loss);
            let capped = kept["capped"] == true;
            kept["telemetry"] = row["telemetry"].clone();
            Ok((kept,capped))
        };
        let snap = if background { e.fleet_cache.refresh(&key,30,read) } else { e.fleet_cache.get(&key,30,read) };
        json!({"installation": r["installation"], "workspace": r["workspace"], "host": r["host_label"], "telemetry": snap["value"]["telemetry"], "records": snap["value"]["records"], "known_loss": snap["value"]["known_loss"], "dropped_newest_ms": snap["value"]["dropped_newest_ms"], "error": snap["error"], "capped": snap["incomplete"]})
    }).collect::<Vec<_>>())
}

/// D15 retained episode evidence: bounded, workspace-scoped, independent of the activity period.
pub fn finding_samples(e: &Result<Enroll, String>, rows: &Value) -> Value {
    let Ok(e) = e else { return json!([]) };
    json!(rows.as_array().into_iter().flatten().filter(|r| r["product"] == "serverq" && r["connection"] != "revoked").map(|r| {
        let (samples, incomplete, error) = match e.store.samples(r["installation"].as_str().unwrap_or(""), r["workspace"].as_str().unwrap_or(""), 0, 10_000) {
            Ok((s, capped)) => (s, capped, Value::Null),
            Err(_) => (vec![], true, json!("retained history unavailable")),
        };
        let samples: Vec<Value> = samples.into_iter().filter_map(|(at, _, p)| {
            let p: Value = serde_json::from_str(&p).ok()?;
            crate::enroll::serverq_payload(&p).then(|| json!({"at": at, "payload": p}))
        }).collect();
        json!({"installation": r["installation"], "workspace": r["workspace"], "samples": samples, "incomplete": incomplete, "error": error})
    }).collect::<Vec<_>>())
}

/// Resource history of projected `rows` (already scoped) since `since`: input of `resource::build`. Only samples
/// carrying `agentq.resource.*` fields count; other snapshot schemas share the history table.
pub fn resource_samples(e: &Result<Enroll, String>, rows: &Value, since: i64) -> Value {
    let Ok(e) = e else { return json!([]) };
    let out: Vec<Value> = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let (inst, ws) = (r["installation"].as_str()?, r["workspace"].as_str()?);
            let (samples, incomplete, error) = match e.store.samples(inst, ws, since, crate::resource::CAP) {
                Ok((s, capped)) => (s, capped, Value::Null),
                Err(_) => (Vec::new(), false, json!("store unavailable")),
            };
            let samples: Vec<Value> = samples
                .into_iter()
                .filter_map(|(at, id, p)| {
                    let payload: Value = serde_json::from_str(&p).ok()?;
                    payload.as_object()?.keys().any(|k| k.starts_with("agentq.resource.")).then(|| json!({"at": at.max(0), "event_id": id, "payload": payload}))
                })
                .collect();
            let host = format!("{}/{}", r["host_label"].as_str()?, r["runtime"].as_str()?);
            Some(json!({"installation": inst, "workspace": ws, "host": host, "runtime": r["runtime"], "samples": samples, "incomplete": incomplete, "error": error}))
        })
        .collect();
    json!(out)
}
