use super::*;

pub(super) fn machine_route(e: &Enroll, r: &Req, now: i64) -> Resp {
    let Ok(body) = serde_json::from_slice::<Value>(r.body) else { return error("400 Bad Request", "invalid_request") };
    if body["protocol"] != 1 {
        return error("400 Bad Request", "unsupported_protocol");
    }
    let unavailable = |_| error("503 Service Unavailable", "unavailable");
    match r.path {
        "/machine/connect/device" => device(e, r, &body, now),
        "/machine/connect/token" => {
            let Some(code) = body["device_code"].as_str() else { return error("400 Bad Request", "invalid_request") };
            let device = sha256(code);
            match e.store.poll(&device, now) {
                Ok(Poll::Pending) => error("400 Bad Request", "authorization_pending"),
                Ok(Poll::SlowDown) => error("400 Bad Request", "slow_down"),
                Ok(Poll::Denied) => error("400 Bad Request", "access_denied"),
                Ok(Poll::Expired) => error("400 Bad Request", "expired_token"),
                Ok(Poll::Invalid) => error("400 Bad Request", "invalid_grant"),
                Ok(Poll::Token { sealed, grant, product, schemas }) => {
                    if !same_board(&body, &grant) {
                        return error("403 Forbidden", "scope");
                    }
                    let Some(credential) = open_sealed(e, &device, sealed) else { return error("400 Bad Request", "expired_token") };
                    let mut v = json!({"protocol": 1, "credential": credential, "installation": grant.installation, "generation": grant.generation,
                        "product": product, "workspace": grant.workspace, "schemas": serde_json::from_str::<Value>(&schemas).unwrap_or(json!([]))});
                    with_board(&mut v, &grant);
                    // the approved subset, present only when the request named capabilities
                    if let Some(c) = grant.capabilities.as_deref().and_then(|c| serde_json::from_str::<Value>(c).ok()) {
                        v["capabilities"] = c;
                    }
                    json("200 OK", v)
                }
                Err(x) => unavailable(x),
            }
        }
        _ => {
            let Some(cred) = r.header("authorization").and_then(|a| a.strip_prefix("Bearer ")).filter(|c| !c.is_empty()) else {
                return error("401 Unauthorized", "invalid_credential");
            };
            let v = sha256(cred);
            match e.store.credential(&v) {
                // revoke always ends a connection, whatever scope it names
                Ok(Some(c)) if r.path != "/machine/connection/revoke" && !same_board(&body, &c.grant) => return error("403 Forbidden", "scope"),
                Err(x) => return unavailable(x),
                _ => {}
            }
            if ["/machine/ingest", "/machine/connection/receipt", "/machine/operations", "/machine/operations/result"].contains(&r.path) {
                return device_data(e, r.path, &v, &body, now);
            }
            let res = if r.path == "/machine/connect/ack" { e.store.ack(&v, now) } else { e.store.revoke_credential(&v, now) };
            match res {
                Ok(None) => error("401 Unauthorized", "invalid_credential"),
                // a revoked credential cannot activate; revoke repeats its reply so offline retries end
                Ok(Some(c)) if c.revoked && r.path == "/machine/connect/ack" => error("401 Unauthorized", "revoked"),
                Ok(Some(c)) => {
                    let mut x = json!({"protocol": 1, "state": if c.revoked { "revoked" } else { "active" }, "generation": c.grant.generation});
                    with_board(&mut x, &c.grant);
                    json("200 OK", x)
                }
                Err(x) => unavailable(x),
            }
        }
    }
}

/// `/machine/ingest` and `/machine/connection/receipt` for an authenticated device; every call rechecks revocation.
pub(super) fn device_data(e: &Enroll, path: &str, v: &[u8], body: &Value, now: i64) -> Resp {
    let c = match e.store.credential(v) {
        Ok(Some(c)) => c,
        Ok(None) => return error("401 Unauthorized", "invalid_credential"),
        Err(_) => return error("503 Service Unavailable", "unavailable"),
    };
    if c.revoked {
        return error("401 Unauthorized", "revoked");
    }
    if !c.active {
        return error("403 Forbidden", "not_active");
    }
    if let Err(wait) = e.spend(&c.grant, now) {
        let mut x = error("429 Too Many Requests", "rate_limited");
        x.headers = format!("Retry-After: {wait}\r\n");
        return x;
    }
    let g = &c.grant;
    let refused = |x| match x {
        store::Error::Denied => error("401 Unauthorized", "revoked"),
        _ => error("503 Service Unavailable", "unavailable"),
    };
    if path == "/machine/operations" {
        let Some(cap) = body["capability"].as_str() else { return error("400 Bad Request", "invalid_request") };
        return match e.store.pull(v, cap, now) {
            Ok(Some(ops)) => json("200 OK", json!({"protocol": 1, "operations": ops})),
            Ok(None) => error("403 Forbidden", "scope"),
            Err(x) => refused(x),
        };
    }
    if path == "/machine/operations/result" {
        let (Some(op), Some(digest)) = (body["operation"].as_str(), body["digest"].as_str()) else { return error("400 Bad Request", "invalid_request") };
        // a result is metadata (C4), at most 16 KiB
        let result = body["result"].to_string();
        if result.len() > coreq::enroll::MAX_PAYLOAD || !result_metadata(&body["result"]) {
            return error("400 Bad Request", "invalid_request");
        }
        return match e.store.post_result(v, op, digest, &result, now) {
            Ok(None) => json("200 OK", json!({"protocol": 1, "operation": op, "state": "accepted"})),
            Ok(Some(reason)) => json("200 OK", json!({"protocol": 1, "operation": op, "state": "rejected", "reason": reason})),
            Err(x) => refused(x),
        };
    }
    if path == "/machine/connection/receipt" {
        let Some(id) = body["event_id"].as_str() else { return error("400 Bad Request", "invalid_request") };
        return match e.store.receipt(v, id) {
            Ok(Some((schema, o))) => {
                let mut x = json!({"protocol": 1, "installation": g.installation, "generation": g.generation, "schema": schema, "event_id": id,
                    "state": if o == Outcome::Projected { "projected" } else { "accepted" }});
                with_board(&mut x, g);
                json("200 OK", x)
            }
            Ok(None) => error("404 Not Found", "unknown_event"),
            Err(store::Error::Denied) => error("401 Unauthorized", "revoked"),
            Err(_) => error("503 Service Unavailable", "unavailable"),
        };
    }
    let Some(records) = body["records"].as_array().filter(|a| a.len() <= coreq::enroll::MAX_RECORDS) else { return error("400 Bad Request", "invalid_request") };
    let Some(ids) = records.iter().map(|r| r["event_id"].as_str().filter(|x| id(x, 128))).collect::<Option<Vec<&str>>>() else {
        return error("400 Bad Request", "invalid_request");
    };
    // the grant supplies ownership; authority fields in a payload are data, and only allowlisted metadata is stored
    let stripped: Vec<Value> = records.iter().map(|r| {
        let mut p = r["payload"].clone();
        strip_line(r["schema"].as_str().unwrap_or(""), &mut p);
        p
    }).collect();
    let mut payloads: Vec<String> = stripped.iter().map(Value::to_string).collect();
    // a telemetry block keeps its allowlisted lines; any other line is dropped and counted (#181)
    let mut tele = vec![false; records.len()];
    for (i, r) in records.iter().enumerate().filter(|(_, r)| r["schema"] == TELEMETRY) {
        match telemetry(&r["payload"]) {
            Some(Some(kept)) => (tele[i], payloads[i]) = (true, kept.to_string()),
            Some(None) => tele[i] = true,
            None => {}
        }
    }
    let mut out: Vec<Option<&'static str>> = vec![None; records.len()];
    let mut events = Vec::new();
    let mut index = Vec::new();
    for (i, r) in records.iter().enumerate() {
        match (r["schema"].as_str(), r["observed_at"].as_i64()) {
            // `agentq-event/1` goes to the export outbox (C5); every other schema is a snapshot (C2)
            (Some(crate::export::SCHEMA), Some(_)) if !crate::export::valid(&r["payload"]) => out[i] = Some("payload"),
            (Some(schema), Some(_)) if !in_board(schema, &r["payload"], g) => out[i] = Some("scope"),
            (Some(TELEMETRY), Some(observed)) if tele[i] => {
                events.push(Event { id: ids[i], schema: TELEMETRY, observed, kind: Kind::Telemetry, payload: &payloads[i] });
                index.push(i);
            }
            (Some(TELEMETRY), Some(_)) => out[i] = Some("payload"),
            (Some(crate::observations::SCHEMA), Some(observed)) if observation(&stripped[i], ids[i]) => {
                events.push(Event { id: ids[i], schema: crate::observations::SCHEMA, observed, kind: Kind::Observation, payload: &payloads[i] });
                index.push(i);
            }
            (Some(crate::observations::SCHEMA), Some(_)) => out[i] = Some("payload"),
            (Some(RESOURCES), Some(observed)) => match resources(&r["payload"], g) {
                Ok(()) => {
                    events.push(Event { id: ids[i], schema: RESOURCES, observed, kind: Kind::Resources, payload: &payloads[i] });
                    index.push(i);
                }
                Err(why) => out[i] = Some(why),
            },
            (Some(schema), Some(observed)) if snapshot_metadata(schema, &stripped[i]) => {
                let kind = if schema == crate::export::SCHEMA { Kind::Export } else { Kind::Snapshot };
                events.push(Event { id: ids[i], schema, observed, kind, payload: &payloads[i] });
                index.push(i);
            }
            (Some(_), Some(_)) => out[i] = Some("payload"),
            (None, _) => out[i] = Some("scope"),
            _ => out[i] = Some("payload"),
        }
    }
    let stored = match e.store.accept(v, &events, now) {
        Ok(s) => s,
        Err(store::Error::Denied) => return error("401 Unauthorized", "revoked"),
        Err(store::Error::Full) => return error("503 Service Unavailable", "store_full"),
        Err(_) => return error("503 Service Unavailable", "unavailable"),
    };
    let mut receipts = Vec::with_capacity(records.len());
    let mut stored = index.into_iter().zip(stored).collect::<HashMap<_, _>>();
    for (i, id) in ids.iter().enumerate() {
        let reason = match (out[i], stored.remove(&i)) {
            (Some(reason), _) | (None, Some(Outcome::Rejected(reason))) => reason,
            _ => {
                receipts.push(json!({"event_id": id, "state": "accepted"}));
                continue;
            }
        };
        // nothing is acknowledged into a full store: the client retries the same records later, replays stay exact
        if ["store_full", "outbox_full"].contains(&reason) {
            return error("503 Service Unavailable", reason);
        }
        receipts.push(json!({"event_id": id, "state": "rejected", "reason": reason}));
    }
    json("200 OK", json!({"protocol": 1, "receipts": receipts}))
}

/// TaskQ export blocks (taskq#1353 S3, D11 #161).
pub const TELEMETRY: &str = "taskq-telemetry/1";
/// S3: at most 512 KiB compressed per record; decompressed rows are bounded by the same 1 MiB a request may carry.
const TELEMETRY_GZIP: usize = 512 << 10;
const TELEMETRY_BYTES: u64 = MAX_BODY as u64;

/// A block cursor: TaskQ sends `{seq, id}` (taskq v4.6.30 `telemetry_sync`); a bare seq number stays accepted.
pub fn cursor(v: &Value) -> Option<(u64, Option<&str>)> {
    match v {
        Value::Object(o) if o.len() == 2 => Some((o.get("seq")?.as_u64()?, Some(o.get("id")?.as_str().filter(|x| id(x, 128))?))),
        _ => Some((v.as_u64()?, None)),
    }
}

/// A `taskq-telemetry/1` payload `{schema: 1, first, last, rows, bytes, gzip, status?}`: `gzip` is the base64 gzip of
/// `bytes` bytes holding `rows` JSON lines of `taskq profile export`, each one metadata (C4) like any snapshot, so
/// nothing outside the allowlist hides inside the block. `first`/`last` are [`cursor`]s; a `{seq, id}` cursor must
/// name the first and last line, whose seqs rise. `status` carries the sender's `connection.telemetry` counters
/// `backlog_rows`, `backlog_bytes`, `gap_rows` (numbers or null).
/// `None` refuses the block; `Some(None)` stores it as sent. A line outside the allowlist (owner-approved, #181) is
/// dropped alone, so one bad record never stalls the sender: `Some(Some(kept))` is the block re-encoded without it,
/// `rows`/`bytes` of the kept lines and `dropped` counting the rest. Cursors still name the block as sent.
pub(super) fn telemetry(p: &Value) -> Option<Option<Value>> {
    use std::io::Write;
    let (sent, lines) = telemetry_lines(p)?;
    let (kept, bad): (Vec<_>, Vec<_>) = sent.lines().zip(&lines).partition(|(_, v)| metadata_keys(v, 0, counter_field));
    if bad.is_empty() {
        return Some(None);
    }
    let plain: String = kept.iter().map(|(l, _)| format!("{l}\n")).collect();
    let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    z.write_all(plain.as_bytes()).ok()?;
    let mut out = p.clone();
    out["rows"] = json!(kept.len());
    out["bytes"] = json!(plain.len());
    out["gzip"] = json!(crate::base64(&z.finish().ok()?));
    out["dropped"] = json!(bad.len());
    Some(Some(out))
}

/// The block's plain text and its lines, each a JSON object, with the envelope and cursor checks of [`telemetry`].
pub(super) fn telemetry_lines(p: &Value) -> Option<(String, Vec<Value>)> {
    use std::io::Read;
    telemetry_envelope(p).then_some(())?;
    let n = |k: &str| p.get(k).and_then(Value::as_u64);
    let (first, last, rows, bytes) = (cursor(&p["first"])?, cursor(&p["last"])?, n("rows")?, n("bytes")?);
    let z = unbase64(p["gzip"].as_str()?).filter(|z| z.len() <= TELEMETRY_GZIP)?;
    let mut plain = Vec::new();
    if flate2::read::GzDecoder::new(z.as_slice()).take(bytes + 1).read_to_end(&mut plain).is_err() || plain.len() as u64 != bytes {
        return None;
    }
    let plain = String::from_utf8(plain).ok()?;
    let lines = plain.lines().map(|l| serde_json::from_str::<Value>(l).ok().filter(Value::is_object)).collect::<Option<Vec<Value>>>()?;
    if lines.len() as u64 != rows {
        return None;
    }
    // coverage is read from the cursors (D11): an identified cursor must be the block's own first and last row
    let named = |c: (u64, Option<&str>), l: &Value| c.1.is_none_or(|i| l["seq"] == c.0 && l["id"] == i);
    let ok = named(first, &lines[0]) && named(last, &lines[lines.len() - 1])
        && (first.1.is_none() || lines.windows(2).all(|w| w[0]["seq"].as_u64().zip(w[1]["seq"].as_u64()).is_some_and(|(a, b)| a < b)));
    ok.then_some((plain, lines))
}

pub(super) fn telemetry_envelope(p: &Value) -> bool {
    let Some(o) = p.as_object() else { return false };
    let n = |k: &str| o.get(k).and_then(Value::as_u64);
    let status = o.get("status").is_none_or(|s| {
        s.as_object().is_some_and(|s| s.iter().all(|(k, v)| ["backlog_rows", "backlog_bytes", "gap_rows"].contains(&k.as_str()) && (v.is_null() || v.is_u64())))
    });
    let (Some(first), Some(last)) = (cursor(&p["first"]), cursor(&p["last"])) else { return false };
    let (Some(rows), Some(bytes), Some(_)) = (n("rows"), n("bytes"), o.get("gzip").and_then(Value::as_str)) else { return false };
    !(p["schema"] != 1 || !status || o.keys().any(|k| !["schema", "first", "last", "rows", "bytes", "gzip", "status"].contains(&k.as_str())) || first.0 > last.0 || rows == 0 || bytes > TELEMETRY_BYTES)
}

/// A `coreq-observation/1` payload (D28) `{seq, id, received_ms, observation}`: the shipper's shared record, metadata
/// by the same allowlist as a telemetry line, and quantities only, never a price.
pub(super) fn observation(p: &Value, event: &str) -> bool {
    let Some(o) = p.as_object() else { return false };
    let r = &p["observation"];
    let priced = |k: &String| { let k = k.to_ascii_lowercase(); k.contains("price") || k.contains("cost") };
    let code = |k: &str| r[k].as_str().is_some_and(|s| !s.is_empty());
    o.len() == 4 && p["seq"].is_u64() && p["received_ms"].is_u64() && p["id"] == event && r["schema"] == 1
        && ["kind", "operation", "outcome", "error"].iter().all(|k| code(k))
        && r.as_object().is_some_and(|r| !r.keys().any(priced))
        && metadata_keys(p, 0, counter_field)
}

/// TaskQ's allowlisted resource projection (taskq shared-resource-controls.md, Project overlay: Enrollment record).
pub const RESOURCES: &str = "taskq-resources/1";
/// The record is resent only with the board (every 300 s) while unchanged, so its status time ages that long between
/// sends. ponytail: two send intervals; take a producer-stated limit once taskq publishes one.
const RESOURCES_STALE_SECS: u64 = 600;

/// A `taskq-resources/1` payload: a `taskq.resources/1` projection whose every target belongs to the sending
/// installation and generation (else `scope`), metadata only (else `payload`).
pub(super) fn resources(p: &Value, g: &store::Grant) -> Result<(), &'static str> {
    let targets = p["targets"].as_array().filter(|_| p["schema"] == "taskq.resources/1" && (p["observed_at"].is_null() || p["observed_at"].is_u64()) && metadata(p, 0)).ok_or("payload")?;
    if !targets.iter().all(|t| t["target"]["project_id"].is_string() && t["target"]["runtime_id"].is_string()) {
        return Err("payload");
    }
    if !targets.iter().all(|t| t["target"]["installation_id"] == g.installation.as_str() && t["target"]["generation"] == g.generation) {
        return Err("scope");
    }
    Ok(())
}

/// `/data` installation `project_limits` (#160) from the newest stored record of the current generation: per target
/// the saved overlay (`limit`, null: no override) and what the host ran (`effective`, null: unknown, never zero),
/// aged by the projection's status time.
pub(super) fn project_limits(r: &store::Row, now: u64) -> Value {
    if !serde_json::from_str::<Vec<String>>(&r.scopes).unwrap_or_default().iter().any(|s| s == RESOURCES) {
        return json!({"state": "off", "reason": "resources not in grant"});
    }
    let Some(p) = r.resources.as_ref().filter(|(g, _)| *g == r.generation).and_then(|(_, p)| serde_json::from_str::<Value>(p).ok()) else {
        return json!({"state": "unknown", "observed_at": null, "age_s": null, "targets": [], "reason": "no resources record yet"});
    };
    let src = match p["observed_at"].as_u64() {
        None => json!({"state": "unknown", "age_s": null, "reason": "no status from this host"}),
        Some(t) if t > now => json!({"state": "unknown", "age_s": null, "reason": "clock uncertain: observed in the future"}),
        Some(t) => crate::data::source(&json!({"value": true, "at": t}), now, RESOURCES_STALE_SECS, None),
    };
    let targets: Vec<Value> = p["targets"].as_array().into_iter().flatten().map(|t| {
        let schemas: Vec<&Value> = t["supported"].as_array().into_iter().flatten().map(|s| &s["policy_schema"]).collect();
        let blockers: Vec<Value> = t["blockers"].as_array().into_iter().flatten().map(|b| json!({"code": b["code"], "knowledge": b["knowledge"]})).collect();
        json!({"project_id": t["target"]["project_id"], "runtime_id": t["target"]["runtime_id"], "policy_schemas": schemas,
            "current_revision": t["current_revision"], "limit": t["current_policy"]["runtime_limit"],
            "effective_revision": t["effective_revision"], "effective": t["effective_policy"]["runtime_limit"],
            "blockers": blockers, "completeness": t["completeness"]})
    }).collect();
    let reason = src["reason"].as_str().filter(|s| !s.is_empty());
    json!({"state": src["state"], "observed_at": p["observed_at"], "age_s": src["age_s"], "targets": targets, "reason": reason, "groups": limit_groups(r, &p)})
}

/// The shared component's input for the project-limits editor (#159): one group per registered project of an active
/// connection, its projection targets unchanged, labelled by the opaque project ID (no name is in the projection,
/// and a hostname is never a join). The browser decides editability from the actor's own grants.
pub(super) fn limit_groups(r: &store::Row, p: &Value) -> Value {
    if r.connection != "active" {
        return json!([]);
    }
    let mut groups: Vec<Value> = Vec::new();
    for t in p["targets"].as_array().into_iter().flatten() {
        let Some(project) = t["target"]["project_id"].as_str() else { continue };
        let key = format!("{}:{}:{project}", r.installation, r.workspace);
        match groups.iter_mut().find(|g| g["key"] == key.as_str()) {
            Some(g) => g["targets"].as_array_mut().into_iter().for_each(|a| a.push(t.clone())),
            None => groups.push(json!({"key": key, "observed_at": p["observed_at"], "labels": {"host": r.label, "project": project}, "targets": [t]})),
        }
    }
    json!(groups)
}

/// Standard padded base64; `None` on any other byte.
pub(super) fn unbase64(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let b = s.as_bytes();
    if !b.len().is_multiple_of(4) {
        return None;
    }
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for (i, q) in b.chunks(4).enumerate() {
        let pad = if i + 1 == b.len() / 4 { q.iter().rev().take_while(|&&c| c == b'=').count() } else { 0 };
        if pad > 2 {
            return None;
        }
        let mut n = 0u32;
        for &c in &q[..4 - pad] {
            n = n << 6 | val(c)? as u32;
        }
        n <<= 6 * pad as u32;
        out.extend_from_slice(&n.to_be_bytes()[1..4 - pad]);
    }
    Some(out)
}

/// Metadata only (C4): objects of short identifier keys, numbers, booleans and short single-line strings;
/// no field named like content, commands, output, paths or secrets, and no path-like value. Objects nest five deep;
/// an array of scalars may sit in the fifth (AgentQ `configuration.profiles[].writes.reasons`).
// ponytail: one shape allowlist for every schema; per-schema field catalogs when producers publish them
pub(super) fn metadata(v: &Value, depth: usize) -> bool {
    metadata_keys(v, depth, field)
}

/// A telemetry line's key (#181, owner-approved): TaskQ request counters name `method:route` (`GET:issues.N`), so
/// `:` may join segments that each pass [`field`]; the deny list applies to every segment.
pub(super) fn counter_field(k: &str) -> bool {
    k.len() <= 128 && k.split(':').all(field)
}

pub(super) fn metadata_keys(v: &Value, depth: usize, key: fn(&str) -> bool) -> bool {
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => depth > 0,
        Value::String(s) => {
            depth > 0 && s.chars().count() <= 256 && !s.chars().any(char::is_control) && !s.starts_with(['/', '~', '\\']) && !s.get(1..3).is_some_and(|x| x == ":\\" || x == ":/")
        }
        Value::Array(a) => depth > 0 && depth < 6 && a.len() <= 256 && a.iter().all(|x| metadata_keys(x, depth + 1, key)),
        Value::Object(o) => depth < 5 && o.len() <= 256 && o.iter().all(|(k, x)| key(k) && metadata_keys(x, depth + 1, key)),
    }
}

/// Record payload allowlist (D11). The one text exception (#125): a `taskq-board/1` choice's `text` at
/// `tasks[].choices[].text`, the owner-approved ask-card option, still a short single-line string. Any other `text`
/// field, and choice text in another schema, stays refused.
pub(super) fn snapshot_metadata(schema: &str, p: &Value) -> bool {
    if schema != "taskq-board/1" || !p["tasks"].is_array() {
        return metadata(p, 0);
    }
    let mut p = p.clone();
    for t in p["tasks"].as_array_mut().into_iter().flatten() {
        for c in t.get_mut("choices").and_then(Value::as_array_mut).into_iter().flatten() {
            if let Some(o) = c.as_object_mut().filter(|o| o.get("text").is_some_and(|x| x.is_string() && metadata(x, 5))) {
                o.remove("text");
            }
        }
    }
    metadata(&p, 0)
}

/// D13 (#180): TaskQ session progress is host, runtime, stage and time only. A worker `line` in a `taskq-board/1`
/// task `session` is dropped before storing, and from snapshots stored earlier until retention ages them out.
pub(super) fn strip_line(schema: &str, p: &mut Value) {
    if schema != "taskq-board/1" {
        return;
    }
    for t in p.get_mut("tasks").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(s) = t.get_mut("session").and_then(Value::as_object_mut) {
            s.remove("line");
        }
    }
}

pub(super) fn field(k: &str) -> bool {
    const DENY: [&str; 18] = [
        "prompt", "command", "cmd", "args", "stdout", "stderr", "output", "input", "content", "text", "message", "path", "cwd", "env", "token", "secret",
        "password", "credential",
    ];
    let k = k.to_ascii_lowercase();
    id(&k, 64) && !DENY.iter().any(|d| k == *d || k.ends_with(&format!("_{d}")))
}

/// An operation result (D16) is metadata one level deeper than a snapshot: AgentQ posts `{result, readback}`, and a
/// plan nests per-profile `changes` objects under `result`.
pub(super) fn result_metadata(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() <= 256 && o.iter().all(|(k, x)| field(k) && metadata(x, if x.is_object() { 0 } else { 1 })))
}
