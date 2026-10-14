//! Pairing, device grants, ingestion and receipts (docs/design/enrollment.md C3/C4, coreq wire `coreq-enrollment/1`).
//! Machine routes take JSON and authorize by pairing or device credential only; the browser route `/connect`
//! takes the provider subject from the trusted proxy header and checks workspace membership, origin and CSRF.

use crate::store::{self, Decision, Event, Installation, Kind, Outcome, Poll, Scope, Store};
use ring::{aead, hmac, rand::SecureRandom, rand::SystemRandom};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// Set by the reverse proxy from the guard's verified subject after stripping any client copy (README, fixture/Caddyfile).
pub const SUBJECT: &str = "x-dashq-subject";
/// Client address set by the reverse proxy; loopback callers have none.
pub const CLIENT: &str = "x-dashq-client";
/// C5: one ingest request is at most 1 MiB.
pub const MAX_BODY: usize = coreq::enroll::MAX_BODY;
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const PRODUCTS: [&str; 3] = ["agentq", "serverq", "taskq"];
/// C3: new pairing sessions per source address per minute, and live sessions overall.
const PER_MINUTE: usize = 10;
const MAX_PENDING: i64 = 1000;
/// C5 per-device bucket: 60 requests a minute, burst 10.
const BURST: f64 = 10.0;
/// C5 heartbeat: a snapshot older than this is stale.
pub const STALE_SECS: u64 = 180;
/// C5 resources: a trend needs at least this many samples; fewer is history warmup.
pub const TREND_SAMPLES: i64 = 3;
/// C5: first-connection progress polls every 2 s for at most 60 s.
const WAIT_SECS: i64 = 60;
const MACHINE: [&str; 6] = ["/connect/device", "/connect/token", "/connect/ack", "/ingest", "/connection/receipt", "/connection/revoke"];

pub struct Req<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// Lowercase names.
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

impl Req<'_> {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

pub struct Resp {
    pub status: &'static str,
    pub ctype: &'static str,
    pub body: Vec<u8>,
    /// Extra header lines, each ending in CRLF.
    pub headers: String,
    /// Same-origin form posts allowed (CSP `form-action 'self'`); only the approval page.
    pub forms: bool,
}

pub struct Enroll {
    store: Store,
    seal: aead::LessSafeKey,
    csrf: hmac::Key,
    public_host: String,
    rng: SystemRandom,
    recent: Mutex<HashMap<String, Vec<i64>>>,
    /// Per-device token buckets: (tokens, last refill).
    buckets: Mutex<HashMap<(String, i64), (f64, i64)>>,
}

/// The deployment unit: `dashq.db` and its key `dashq.key` (0600) in the config folder (C4).
fn paths(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    (dir.join("dashq.db"), dir.join("dashq.key"))
}

impl Enroll {
    /// Opens a set-up deployment; `Err` when `dashq admin` never ran there or the key is unusable.
    pub fn open(dir: &Path, public_host: &str) -> Result<Enroll, String> {
        let (db, key) = paths(dir);
        if !db.exists() || !key.exists() {
            return Err("enrollment not set up (dashq admin)".into());
        }
        #[cfg(unix)]
        if std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&key).map_err(|e| e.to_string())?.permissions()) & 0o077 != 0 {
            return Err("dashq.key: mode must be 0600".into());
        }
        let k = std::fs::read(&key).map_err(|_| "dashq.key: unreadable")?;
        if k.len() != 32 {
            return Err("dashq.key: must be 32 bytes".into());
        }
        let store = Store::open(&db, store::LIMITS).map_err(|e| format!("dashq.db: {e:?}"))?;
        Ok(Enroll {
            store,
            seal: aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &k).map_err(|_| "dashq.key: bad key")?),
            // separate keys for sealing and CSRF, both from the one deployment key
            csrf: hmac::Key::new(hmac::HMAC_SHA256, ring::digest::digest(&ring::digest::SHA256, &[b"dashq csrf\n".as_slice(), &k].concat()).as_ref()),
            public_host: public_host.into(),
            rng: SystemRandom::new(),
            recent: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
        })
    }

    /// Setup by the host administrator (C3): creates the key and store if missing and binds `subject` as a
    /// workspace administrator. Only a shell on the host can run it, so no first visitor becomes admin.
    pub fn admin(dir: &Path, workspace: &str, subject: &str) -> Result<(), String> {
        if !id(workspace, 64) || !valid_subject(subject) {
            return Err("admin: bad workspace or subject".into());
        }
        let (db, key) = paths(dir);
        if !key.exists() {
            let mut k = [0u8; 32];
            SystemRandom::new().fill(&mut k).map_err(|_| "admin: no randomness")?;
            let mut o = std::fs::OpenOptions::new();
            o.write(true).create_new(true);
            #[cfg(unix)]
            std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
            std::io::Write::write_all(&mut o.open(&key).map_err(|e| format!("dashq.key: {e}"))?, &k).map_err(|e| format!("dashq.key: {e}"))?;
        }
        let s = Store::open(&db, store::LIMITS).map_err(|e| format!("dashq.db: {e:?}"))?;
        s.add_member(workspace, subject, "admin").map_err(|e| format!("dashq.db: {e:?}"))
    }

    fn random(&self, n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n];
        self.rng.fill(&mut b).expect("system randomness");
        b
    }

    fn token(&self, subject: &str, code: &str) -> String {
        self.token_for(&format!("{subject}\n{code}"))
    }

    /// CSRF token bound to account and purpose.
    pub fn token_for(&self, purpose: &str) -> String {
        hex(hmac::sign(&self.csrf, purpose.as_bytes()).as_ref())
    }

    pub fn token_ok(&self, purpose: &str, token: &str) -> bool {
        unhex(token).is_some_and(|t| hmac::verify(&self.csrf, purpose.as_bytes(), &t).is_ok())
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The host administrator grants or removes the explicit answer capability of `subject` in `workspace`
    /// (answers.md); answering also needs the workspace administrator role.
    pub fn answerer(dir: &Path, workspace: &str, subject: &str, allow: bool) -> Result<(), String> {
        if !id(workspace, 64) || !valid_subject(subject) {
            return Err("answerer: bad workspace or subject".into());
        }
        let e = Enroll::open(dir, "")?;
        e.store.set_answerer(workspace, subject, allow).map_err(|e| format!("dashq.db: {e:?}"))
    }

    /// True when another session is allowed for `ip` now; else the seconds to wait.
    fn admit(&self, ip: &str, now: i64) -> Result<(), i64> {
        let mut m = self.recent.lock().unwrap_or_else(|p| p.into_inner());
        // ponytail: in-memory window, reset by a restart; the store's global cap still holds
        if m.len() > 10_000 {
            m.retain(|_, v| v.iter().any(|t| now - t < 60));
        }
        let v = m.entry(ip.into()).or_default();
        v.retain(|t| now - t < 60);
        if v.len() >= PER_MINUTE {
            return Err(60 - (now - v[0]));
        }
        v.push(now);
        Ok(())
    }
}

impl Enroll {
    /// One request from a device; else the seconds until the next token.
    // ponytail: in-memory buckets, reset by a restart; qualify under the C5 load hold
    fn spend(&self, g: &store::Grant, now: i64) -> Result<(), i64> {
        let mut m = self.buckets.lock().unwrap_or_else(|p| p.into_inner());
        if m.len() > 10_000 {
            m.retain(|_, (_, t)| now - *t < 60);
        }
        let (tokens, last) = m.entry((g.installation.clone(), g.generation)).or_insert((BURST, now));
        *tokens = (*tokens + (now - *last).max(0) as f64).min(BURST);
        *last = now;
        if *tokens < 1.0 {
            return Err(1);
        }
        *tokens -= 1.0;
        Ok(())
    }

    /// The scoped dashboard projection; also run by the service pass so receipts become `projected` (C1.5).
    pub fn project(&self, scope: Scope) -> Result<Vec<store::Row>, store::Error> {
        self.store.project(scope)
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len().is_multiple_of(2).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()).flatten()
}

fn b64url(b: &[u8]) -> String {
    crate::base64(b).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

pub fn sha256(s: &str) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, s.as_bytes()).as_ref().to_vec()
}

pub fn id(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

pub fn valid_subject(s: &str) -> bool {
    !s.is_empty() && s.len() <= 256 && s.chars().all(|c| !c.is_control())
}

/// `ABCD-EFGH` from any case, dashes or spaces; `None` unless 8 alphabet symbols.
pub fn user_code(s: &str) -> Option<String> {
    let c: String = s.chars().filter(|c| *c != '-' && !c.is_whitespace()).map(|c| c.to_ascii_uppercase()).collect();
    (c.len() == 8 && c.bytes().all(|b| ALPHABET.contains(&b))).then(|| format!("{}-{}", &c[..4], &c[4..]))
}

fn json(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

fn error(status: &'static str, code: &str) -> Resp {
    json(status, json!({"protocol": 1, "error": code}))
}

fn html(status: &'static str, body: &str) -> Resp {
    html_head(status, "", body)
}

/// `head`: extra head lines, e.g. the 2 s progress refresh.
fn html_head(status: &'static str, head: &str, body: &str) -> Resp {
    let page = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{head}<title>DashQ · Connect</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n</head>\n<body>\n<main>\n<h1>Connect a product</h1>\n{body}</main>\n</body>\n</html>\n"
    );
    Resp { status, ctype: "text/html; charset=utf-8", body: page.into_bytes(), headers: String::new(), forms: true }
}

/// Enrollment routes; `None` for every other path. `e` is `Err` when enrollment is not set up or unavailable.
pub fn route(e: &Result<Enroll, String>, r: &Req, now: i64) -> Option<Resp> {
    let machine = MACHINE.contains(&r.path);
    if !machine && r.path != "/connect" {
        return None;
    }
    if (machine && r.method != "POST") || !["GET", "POST"].contains(&r.method) {
        return Some(Resp { status: "405 Method Not Allowed", ctype: "text/plain", body: b"method not allowed".to_vec(), headers: String::new(), forms: false });
    }
    let e = match e {
        Ok(e) => e,
        Err(_) if machine => return Some(error("503 Service Unavailable", "unavailable")),
        Err(_) => return Some(html("503 Service Unavailable", "<p>Connections are not set up on this dashboard yet.</p>\n")),
    };
    Some(if machine { machine_route(e, r, now) } else { browser(e, r, now) })
}

fn machine_route(e: &Enroll, r: &Req, now: i64) -> Resp {
    let Ok(body) = serde_json::from_slice::<Value>(r.body) else { return error("400 Bad Request", "invalid_request") };
    if body["protocol"] != 1 {
        return error("400 Bad Request", "unsupported_protocol");
    }
    let unavailable = |_| error("503 Service Unavailable", "unavailable");
    match r.path {
        "/connect/device" => device(e, r, &body, now),
        "/connect/token" => {
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
                Ok(Some(c)) if r.path != "/connection/revoke" && !same_board(&body, &c.grant) => return error("403 Forbidden", "scope"),
                Err(x) => return unavailable(x),
                _ => {}
            }
            if ["/ingest", "/connection/receipt"].contains(&r.path) {
                return device_data(e, r.path, &v, &body, now);
            }
            let res = if r.path == "/connect/ack" { e.store.ack(&v, now) } else { e.store.revoke_credential(&v, now) };
            match res {
                Ok(None) => error("401 Unauthorized", "invalid_credential"),
                // a revoked credential cannot activate; revoke repeats its reply so offline retries end
                Ok(Some(c)) if c.revoked && r.path == "/connect/ack" => error("401 Unauthorized", "revoked"),
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

/// `/ingest` and `/connection/receipt` for an authenticated device; every call rechecks revocation.
fn device_data(e: &Enroll, path: &str, v: &[u8], body: &Value, now: i64) -> Resp {
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
    if path == "/connection/receipt" {
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
    let payloads: Vec<String> = records.iter().map(|r| r["payload"].to_string()).collect();
    let mut out: Vec<Option<&'static str>> = vec![None; records.len()];
    let mut events = Vec::new();
    let mut index = Vec::new();
    for (i, r) in records.iter().enumerate() {
        match (r["schema"].as_str(), r["observed_at"].as_i64()) {
            (Some(schema), Some(observed)) if metadata(&r["payload"], 0) => {
                // ponytail: every schema is a snapshot until export schemas and forwarding exist (C2)
                events.push(Event { id: ids[i], schema, observed, kind: Kind::Snapshot, payload: &payloads[i] });
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

/// Metadata only (C4): objects of short identifier keys, numbers, booleans and short single-line strings;
/// no field named like content, commands, output, paths or secrets, and no path-like value.
// ponytail: one shape allowlist for every schema; per-schema field catalogs when producers publish them
fn metadata(v: &Value, depth: usize) -> bool {
    const DENY: [&str; 18] = [
        "prompt", "command", "cmd", "args", "stdout", "stderr", "output", "input", "content", "text", "message", "path", "cwd", "env", "token", "secret",
        "password", "credential",
    ];
    match v {
        Value::Null | Value::Bool(_) | Value::Number(_) => depth > 0,
        Value::String(s) => {
            depth > 0 && s.chars().count() <= 256 && !s.chars().any(char::is_control) && !s.starts_with(['/', '~', '\\']) && !s.get(1..3).is_some_and(|x| x == ":\\" || x == ":/")
        }
        Value::Array(a) => depth > 0 && depth < 5 && a.len() <= 256 && a.iter().all(|x| metadata(x, depth + 1)),
        Value::Object(o) => {
            depth < 5
                && o.len() <= 256
                && o.iter().all(|(k, x)| {
                    let k = k.to_ascii_lowercase();
                    id(&k, 64) && !DENY.iter().any(|d| k == *d || k.ends_with(&format!("_{d}"))) && metadata(x, depth + 1)
                })
        }
    }
}

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
                        crate::data::source(&json!({"value": true, "at": s.observed.max(0)}), now, STALE_SECS, None)
                    };
                    (obs, json!({"event_id": s.event_id, "schema": s.schema, "payload": serde_json::from_str::<Value>(&s.payload).unwrap_or(Value::Null)}))
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
                json!({"gap_events": 0, "state": "ok", "reason": null})
            };
            json!({"installation": r.installation, "product": r.product, "runtime": r.runtime, "host_label": r.label, "workspace": r.workspace,
                "generation": r.generation, "connection": r.connection, "observation": obs, "history": history, "export": export, "snapshot": snapshot})
        })
        .collect();
    Some(json!({"source": src("ok", None), "rows": rows}))
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

/// The optional board scope of a device request (coreq#36): `{"name": "<id>/<version>", "value": "<short line>"}`;
/// `Ok(None)` when absent, `Err` when malformed.
fn board(b: &Value) -> Result<Option<String>, ()> {
    let s = &b["scope"];
    if s.is_null() {
        return Ok(None);
    }
    let (Some(name), Some(value)) = (s["name"].as_str(), s["value"].as_str()) else { return Err(()) };
    let named = name.split_once('/').is_some_and(|(n, v)| id(n, 32) && !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    if !named || s.as_object().is_some_and(|o| o.len() != 2) || value.is_empty() || value.chars().count() > 128 || value.chars().any(char::is_control) {
        return Err(());
    }
    Ok(Some(json!({"name": name, "value": value}).to_string()))
}

/// A request after approval may repeat its scope but never change it (A2); one without a scope is unchanged.
fn same_board(b: &Value, g: &store::Grant) -> bool {
    b["scope"].is_null() || board(b).ok().flatten().is_some_and(|x| g.board.as_deref() == Some(x.as_str()))
}

/// Names the approved scope in token, status and receipt replies; replies without one keep today's shape.
fn with_board(v: &mut Value, g: &store::Grant) {
    if let Some(b) = g.board.as_deref().and_then(|b| serde_json::from_str::<Value>(b).ok()) {
        v["scope"] = b;
    }
}

fn open_sealed(e: &Enroll, device: &[u8], mut sealed: Vec<u8>) -> Option<String> {
    if sealed.len() < 12 {
        return None;
    }
    let ct = sealed.split_off(12);
    let mut ct = ct;
    let nonce = aead::Nonce::try_assume_unique_for_key(&sealed).ok()?;
    let plain = e.seal.open_in_place(nonce, aead::Aad::from(device), &mut ct).ok()?;
    String::from_utf8(plain.to_vec()).ok()
}

fn seal(e: &Enroll, device: &[u8], credential: &str) -> Vec<u8> {
    let nonce = e.random(12);
    let mut buf = credential.as_bytes().to_vec();
    e.seal.seal_in_place_append_tag(aead::Nonce::try_assume_unique_for_key(&nonce).expect("12 bytes"), aead::Aad::from(device), &mut buf).expect("seal");
    [nonce, buf].concat()
}

fn device(e: &Enroll, r: &Req, b: &Value, now: i64) -> Resp {
    let s = |k: &str| b[k].as_str().unwrap_or("");
    let product = s("product");
    let schemas: Option<Vec<&str>> = b["schemas"].as_array().filter(|a| (1..=16).contains(&a.len())).and_then(|a| a.iter().map(Value::as_str).collect());
    // a grant only ever covers its own product's schemas (C3 scope isolation)
    let ok_schema = |x: &&str| x.strip_prefix(product).and_then(|x| x.strip_prefix('-')).and_then(|x| x.split_once('/')).is_some_and(|(n, v)| id(n, 32) && !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()));
    let Some(schemas) = schemas.filter(|v| PRODUCTS.contains(&product) && v.iter().all(ok_schema)) else { return error("400 Bad Request", "invalid_request") };
    let Ok(board) = board(b) else { return error("400 Bad Request", "invalid_request") };
    if !id(s("installation"), 64) || !matches!(s("runtime"), "native" | "wsl") || !id(s("host_label"), 64) {
        return error("400 Bad Request", "invalid_request");
    }
    if let Err(wait) = e.admit(r.header(CLIENT).unwrap_or("local"), now) {
        let mut x = error("429 Too Many Requests", "rate_limited");
        x.headers = format!("Retry-After: {wait}\r\n");
        return x;
    }
    let inst = Installation { id: s("installation"), product, runtime: s("runtime"), label: s("host_label") };
    // a collision on 40 random bits among live codes is rare; retry it a few times
    for _ in 0..4 {
        let device = b64url(&e.random(32));
        let code: String = e.random(8).iter().map(|b| ALPHABET[(b & 31) as usize] as char).collect();
        let code = user_code(&code).expect("alphabet");
        match e.store.pair(&sha256(&device), &code, &inst, &json!(schemas).to_string(), board.as_deref(), now, MAX_PENDING) {
            Ok(()) => {
                return json(
                    "200 OK",
                    json!({"protocol": 1, "device_code": device, "user_code": code, "verification_uri": format!("https://{}/connect", e.public_host),
                        "expires_in": store::PAIRING_SECS, "interval": 5}),
                );
            }
            Err(store::Error::Exists) => continue,
            Err(store::Error::TooMany) => {
                let mut x = error("429 Too Many Requests", "rate_limited");
                x.headers = "Retry-After: 60\r\n".into();
                return x;
            }
            Err(_) => break,
        }
    }
    error("503 Service Unavailable", "unavailable")
}

pub fn form(body: &[u8]) -> HashMap<String, String> {
    std::str::from_utf8(body)
        .unwrap_or("")
        .split('&')
        .filter_map(|p| p.split_once('='))
        .filter_map(|(k, v)| Some((crate::source::dec(k)?, crate::source::dec(v)?)))
        .collect()
}

fn browser(e: &Enroll, r: &Req, now: i64) -> Resp {
    let esc = crate::page::esc;
    let Some(subject) = r.header(SUBJECT).filter(|s| valid_subject(s)) else {
        return html("401 Unauthorized", "<p>Sign-in required: open this page through the dashboard address.</p>\n");
    };
    let fail = |status, msg: &str| html(status, &format!("<p>{}</p>\n<p><a href=\"/connect\">Enter another code</a></p>\n", esc(msg)));
    let Ok(spaces) = e.store.workspaces(subject) else { return fail("503 Service Unavailable", "The connection store is unavailable; nothing was changed.") };
    let who = format!("<p>Signed in as <strong>{}</strong>.</p>\n", esc(subject));
    if spaces.is_empty() {
        return html("403 Forbidden", &format!("{who}<p>This account is not a member of any workspace. Ask a workspace administrator to add it.</p>\n"));
    }
    if r.method == "POST" {
        // normal browser mutations: same origin, form encoding and a token bound to this account and code
        let origin = format!("https://{}", e.public_host);
        if r.header("origin") != Some(origin.as_str()) || !r.header("content-type").is_some_and(|c| c.starts_with("application/x-www-form-urlencoded")) {
            return fail("403 Forbidden", "Request refused: it did not come from this dashboard's own page.");
        }
        let f = form(r.body);
        let get = |k: &str| f.get(k).map(String::as_str).unwrap_or("");
        if get("decision") == "revoke" {
            let (inst, ws) = (get("installation"), get("workspace"));
            if unhex(get("csrf")).is_none_or(|t| hmac::verify(&e.csrf, revoke_purpose(subject, inst, ws).as_bytes(), &t).is_err()) {
                return fail("403 Forbidden", "Request refused: the form expired or belongs to another account. Open Revoke again.");
            }
            return match e.store.revoke_as(inst, ws, subject, now) {
                Ok(true) => html("200 OK", &format!("{who}<p>Revoked: installation <code>{}</code> in workspace <strong>{}</strong> can no longer send data. Its history stays; reconnecting needs a new approval.</p>\n<p><a href=\"/#{}\">Back to the overview</a></p>\n", esc(inst), esc(ws), anchor(inst, ws))),
                Ok(false) => fail("404 Not Found", "Nothing to revoke: the connection is already revoked or not visible to this account."),
                Err(_) => fail("503 Service Unavailable", "The connection store is unavailable; nothing was revoked."),
            };
        }
        let Some(code) = user_code(get("code")) else { return fail("400 Bad Request", "That code is not valid.") };
        if unhex(get("csrf")).is_none_or(|t| hmac::verify(&e.csrf, format!("{subject}\n{code}").as_bytes(), &t).is_err()) {
            return fail("403 Forbidden", "Request refused: the form expired or belongs to another account. Open the code again.");
        }
        let approve = match get("decision") {
            "approve" => true,
            "deny" => false,
            _ => return fail("400 Bad Request", "Choose Approve or Deny."),
        };
        let credential = b64url(&e.random(32));
        let sealer = |device: &[u8]| seal(e, device, &credential);
        return match e.store.decide(&code, approve, get("workspace"), subject, &sha256(&credential), &sealer, now) {
            Ok(Decision::Approved(g)) => {
                // C1.5: this page follows the first projected snapshot of this generation to the overview
                let wait = format!("/connect?wait={}&workspace={}&generation={}&since={now}", g.installation, g.workspace, g.generation);
                html_head(
                    "200 OK",
                    &format!("<meta http-equiv=\"refresh\" content=\"2; url={}\">\n", esc(&wait)),
                    &format!("{who}<p>Approved: installation <code>{}</code> is connected to workspace <strong>{}</strong>. Return to the command; it finishes on its own.</p>\n<p><a href=\"{}\">Waiting for its first snapshot…</a></p>\n", esc(&g.installation), esc(&g.workspace), esc(&wait)),
                )
            }
            Ok(Decision::Already) => fail("200 OK", "This code is already approved; nothing new was granted."),
            Ok(Decision::Denied) => fail("200 OK", "Denied: no access was granted."),
            Ok(Decision::Invalid) => fail("400 Bad Request", "This code is unknown, expired or already used. Start the connection again on the host."),
            Ok(Decision::NotMember) => fail("403 Forbidden", "This account is not a member of that workspace."),
            Err(store::Error::Conflict) => fail("409 Conflict", "This installation belongs to another workspace or runtime; moving it is not supported yet."),
            Err(_) => fail("503 Service Unavailable", "The connection store is unavailable; nothing was granted."),
        };
    }
    let q = form(r.query.as_bytes());
    let qget = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
    if !qget("wait").is_empty() || !qget("revoke").is_empty() {
        return lifecycle(e, subject, &who, &q, now);
    }
    let code = qget("code").to_string();
    let entry = format!("{who}<form method=\"get\" action=\"/connect\">\n<label>Code shown by the command <input name=\"code\" autocomplete=\"off\" autofocus required></label>\n<button>Continue</button>\n</form>\n");
    if code.is_empty() {
        return html("200 OK", &entry);
    }
    let Some(code) = user_code(&code) else { return fail("400 Bad Request", "That code is not valid.") };
    let p = match e.store.pending(&code, now) {
        Ok(Some(p)) => p,
        Ok(None) => return fail("404 Not Found", "This code is unknown, expired or already used. Start the connection again on the host."),
        Err(_) => return fail("503 Service Unavailable", "The connection store is unavailable."),
    };
    // installation IDs are not secrets: say plainly when approval ends a live connection
    let replaces = if p.replaces { "<p><strong>This installation is already connected. Approving replaces that connection; its current credential stops working.</strong></p>\n" } else { "" };
    let scopes: Vec<String> = serde_json::from_str::<Vec<String>>(&p.schemas).unwrap_or_default().iter().map(|s| format!("<code>{}</code>", esc(s))).collect();
    // the person signs exactly the board scope the client gets; none renders as before
    let scope = p.board.as_deref().and_then(|b| serde_json::from_str::<Value>(b).ok()).map_or(String::new(), |b| {
        format!("<dt>Board scope</dt><dd><code>{}</code>: <code>{}</code></dd>\n", esc(b["name"].as_str().unwrap_or("")), esc(b["value"].as_str().unwrap_or("")))
    });
    // pick a workspace only when there is a choice (C1)
    let ws = if let [(w, _)] = spaces.as_slice() {
        format!("<dt>Workspace</dt><dd>{0}<input type=\"hidden\" name=\"workspace\" value=\"{0}\"></dd>\n", esc(w))
    } else {
        let opts: String = spaces.iter().map(|(w, _)| format!("<option>{}</option>", esc(w))).collect();
        format!("<dt>Workspace</dt><dd><select name=\"workspace\" required>{opts}</select></dd>\n")
    };
    html(
        "200 OK",
        &format!(
            "{who}{replaces}<form method=\"post\" action=\"/connect\">\n<dl>\n<dt>Code</dt><dd><strong>{code}</strong>: check that the command shows the same code</dd>\n<dt>Product</dt><dd>{}</dd>\n<dt>Host</dt><dd>{} ({})</dd>\n<dt>Installation</dt><dd><code>{}</code></dd>\n<dt>Data</dt><dd>{}: observation only, no commands or task answers</dd>\n{scope}{ws}</dl>\n<input type=\"hidden\" name=\"code\" value=\"{code}\">\n<input type=\"hidden\" name=\"csrf\" value=\"{}\">\n<button name=\"decision\" value=\"approve\">Approve</button>\n<button name=\"decision\" value=\"deny\">Deny</button>\n</form>\n",
            esc(&p.product),
            esc(&p.label),
            esc(&p.runtime),
            esc(&p.installation),
            scopes.join(", "),
            e.token(subject, &code)
        ),
    )
}

/// Overview row id of an installation in a workspace; both are identifiers (`id`).
pub fn anchor(installation: &str, workspace: &str) -> String {
    format!("inst-{installation}-{workspace}")
}

fn revoke_purpose(subject: &str, installation: &str, workspace: &str) -> String {
    format!("{subject}\nrevoke\n{installation}\n{workspace}")
}

/// `GET /connect?wait=` (first-connection progress) and `?revoke=` (confirmation) for the signed-in subject; both
/// read through the same scoped projection as the overview, so an account sees only its own installations.
fn lifecycle(e: &Enroll, subject: &str, who: &str, q: &HashMap<String, String>, now: i64) -> Resp {
    let esc = crate::page::esc;
    let get = |k: &str| q.get(k).map(String::as_str).unwrap_or("");
    let fail = |status, msg: &str| html(status, &format!("{who}<p>{}</p>\n<p><a href=\"/\">Open the overview</a></p>\n", esc(msg)));
    let (inst, ws) = (if get("wait").is_empty() { get("revoke") } else { get("wait") }, get("workspace"));
    let Ok(rows) = e.project(Scope::Subject(subject)) else { return fail("503 Service Unavailable", "The connection store is unavailable.") };
    let Some(row) = rows.into_iter().find(|r| r.installation == inst && r.workspace == ws) else {
        return fail("404 Not Found", "This installation is not visible to this account.");
    };
    let to = anchor(&row.installation, &row.workspace);
    if !get("revoke").is_empty() {
        if row.connection == "revoked" {
            return fail("200 OK", "This connection is already revoked; its history stays on the overview.");
        }
        return html(
            "200 OK",
            &format!(
                "{who}<form method=\"post\" action=\"/connect\">\n<p>Revoke <strong>{} ({})</strong>, {} installation <code>{}</code>, in workspace <strong>{}</strong>?</p>\n<p>Its credential stops working at once; the host stops sending on its next request. Accepted history stays. Reconnecting needs a new approval on the host.</p>\n<input type=\"hidden\" name=\"installation\" value=\"{}\">\n<input type=\"hidden\" name=\"workspace\" value=\"{}\">\n<input type=\"hidden\" name=\"csrf\" value=\"{}\">\n<button name=\"decision\" value=\"revoke\">Revoke</button>\n<a href=\"/#{to}\">Cancel</a>\n</form>\n",
                esc(&row.label), esc(&row.runtime), esc(&row.product), esc(inst), esc(ws), esc(inst), esc(ws), e.token_for(&revoke_purpose(subject, inst, ws))
            ),
        );
    }
    let generation: i64 = get("generation").parse().unwrap_or(-1);
    if row.generation != generation || row.connection == "revoked" {
        return fail("200 OK", "This connection was replaced or revoked; the overview shows its current state.");
    }
    // the projection above read this generation's snapshot as current: its receipt is `projected` now
    if row.snapshot.as_ref().is_some_and(|s| s.generation == generation) {
        let mut x = html("303 See Other", &format!("<p><a href=\"/#{to}\">Open the overview</a></p>\n"));
        x.headers = format!("Location: /#{to}\r\n");
        return x;
    }
    let since: i64 = get("since").parse().unwrap_or(0);
    if (0..WAIT_SECS).contains(&(now - since)) {
        let again = format!("/connect?wait={inst}&workspace={ws}&generation={generation}&since={since}");
        return html_head(
            "200 OK",
            &format!("<meta http-equiv=\"refresh\" content=\"2; url={}\">\n", esc(&again)),
            &format!("{who}<p role=\"status\">Approved. Waiting for the first snapshot from <strong>{} ({})</strong>; checked every 2 s for up to 60 s.</p>\n<p><a href=\"/#{to}\">Open the overview now</a></p>\n", esc(&row.label), esc(&row.runtime)),
        );
    }
    fail("200 OK", "No snapshot arrived within 60 s. The connection is approved; the overview shows the installation as \"no snapshot yet\" until its host sends one.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const HOST: &str = "dash.example.com";
    const T0: i64 = 1_800_000_000;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dashq-enroll-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn setup(name: &str) -> (PathBuf, Result<Enroll, String>) {
        let d = dir(name);
        Enroll::admin(&d, "ws-1", "github|1").unwrap();
        let e = Enroll::open(&d, HOST);
        (d, e)
    }

    fn call(e: &Result<Enroll, String>, method: &str, path: &str, headers: &[(&str, &str)], body: &str, now: i64) -> (String, String, Value) {
        let h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let r = route(e, &Req { method, path, query, headers: &h, body: body.as_bytes() }, now).expect("enrollment route");
        let text = String::from_utf8(r.body).unwrap();
        (r.status.to_string(), r.headers, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    fn machine(e: &Result<Enroll, String>, path: &str, auth: Option<&str>, body: Value, now: i64) -> (u16, Value) {
        let bearer = auth.map(|a| format!("Bearer {a}"));
        let h: Vec<(&str, &str)> = bearer.iter().map(|b| ("authorization", b.as_str())).collect();
        let (s, _, v) = call(e, "POST", path, &h, &body.to_string(), now);
        (s[..3].parse().unwrap(), v)
    }

    fn page(e: &Result<Enroll, String>, subject: &str, code: &str, now: i64) -> (String, String) {
        let (s, _, v) = call(e, "GET", &format!("/connect?code={code}"), &[(SUBJECT, subject)], "", now);
        (s, v.as_str().unwrap().to_string())
    }

    /// Browser decision as the approval page posts it.
    fn decide(e: &Result<Enroll, String>, subject: &str, code: &str, decision: &str, workspace: &str, now: i64) -> (String, String) {
        let csrf = e.as_ref().unwrap().token(subject, &user_code(code).unwrap_or_default());
        let body = format!("code={code}&workspace={workspace}&decision={decision}&csrf={csrf}");
        let h = [(SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        let (s, _, v) = call(e, "POST", "/connect", &h, &body, now);
        (s, v.as_str().unwrap().to_string())
    }

    fn start(e: &Result<Enroll, String>, inst: &str, now: i64) -> (String, String) {
        let (s, v) = machine(e, "/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"]}), now);
        assert_eq!(s, 200, "{v}");
        assert_eq!((&v["verification_uri"], &v["expires_in"], &v["interval"]), (&json!("https://dash.example.com/connect"), &json!(600), &json!(5)));
        (v["device_code"].as_str().unwrap().into(), v["user_code"].as_str().unwrap().into())
    }

    /// The shared coreq client pairs through these handlers; the browser approves on the first wait.
    struct Wire<'a> {
        e: &'a Result<Enroll, String>,
        now: i64,
        code: Option<String>,
        calls: Vec<String>,
    }

    impl coreq::enroll::Wire for Wire<'_> {
        fn post(&mut self, destination: &str, route: &str, auth: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            assert_eq!(destination, "https://dash.example.com");
            self.calls.push(route.into());
            Ok(machine(self.e, route, auth, body.clone(), self.now))
        }
        fn now(&self) -> u64 {
            self.now as u64
        }
        fn sleep(&mut self, secs: u64) {
            self.now += secs as i64;
            if let Some(code) = self.code.take() {
                assert!(decide(self.e, "github|1", &code, "approve", "ws-1", self.now).1.contains("Approved"));
            }
        }
        fn show(&mut self, uri: &str, code: &str) {
            assert_eq!(uri, "https://dash.example.com/connect");
            self.code = Some(code.into());
        }
    }

    #[test]
    fn coreq_client_pairs_once_and_the_grant_survives_restart() {
        let (d, e) = setup("client");
        let host = dir("client-host");
        let req = coreq::enroll::Request { product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![] };
        let a = coreq::enroll::pair(&host, "https://dash.example.com", &req, &mut w).unwrap();
        assert_eq!((&a["generation"], &a["workspace"], &a["product"]), (&json!(1), &json!("ws-1"), &json!("agentq")));
        assert_eq!(w.calls, ["/connect/device", "/connect/token", "/connect/token", "/connect/ack"]);
        // repeating enrollment returns the active connection without new requests or grants
        coreq::enroll::pair(&host, "https://dash.example.com", &req, &mut w).unwrap();
        assert_eq!(w.calls.len(), 4);
        let cred = a["credential"].as_str().unwrap();
        assert!(cred.len() >= 43, "256-bit credential");
        drop(e);
        let e = Enroll::open(&d, HOST);
        let c = e.as_ref().unwrap().store.credential(&sha256(cred)).unwrap().unwrap();
        assert!(c.active && !c.revoked && c.grant.account == "github|1");
        // revoke survives restart; the reply repeats so offline retries end
        assert_eq!(machine(&e, "/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 60), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        drop(e);
        let e = Enroll::open(&d, HOST);
        assert_eq!(machine(&e, "/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 61), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        assert_eq!(machine(&e, "/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(machine(&e, "/connect/ack", Some("forged"), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "invalid_credential"})));
        assert_eq!(machine(&e, "/connect/ack", None, json!({"protocol": 1}), T0 + 62).0, 401);
        assert!(e.as_ref().unwrap().store.authorize(&sha256(cred)).unwrap().is_none());
    }

    #[test]
    fn polling_lost_replies_and_expiry() {
        let (d, e) = setup("poll");
        assert_eq!(machine(&e, "/connect/device", None, json!({"protocol": 2}), T0), (400, json!({"protocol": 1, "error": "unsupported_protocol"})));
        let (device, code) = start(&e, "inst-1", T0);
        let tok = |e: &Result<Enroll, String>, now| machine(e, "/connect/token", None, json!({"protocol": 1, "device_code": device}), now);
        assert_eq!(tok(&e, T0 + 5).1["error"], "authorization_pending");
        assert_eq!(tok(&e, T0 + 7).1["error"], "slow_down");
        assert_eq!(tok(&e, T0 + 12).1["error"], "slow_down", "interval is now 10 s");
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 20).1.contains("Approved"));
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 21).1.contains("already approved"));
        // the token reply repeats after a lost response and a restart; the sealed copy is useless without the key
        let (s, first) = tok(&e, T0 + 30);
        assert_eq!(s, 200);
        drop(e);
        let raw = rusqlite::Connection::open(d.join("dashq.db")).unwrap().query_row("SELECT sealed FROM pairings", [], |r| r.get::<_, Vec<u8>>(0)).unwrap();
        assert!(!raw.windows(8).any(|w| first["credential"].as_str().unwrap().as_bytes().windows(8).any(|c| c == w)));
        let e = Enroll::open(&d, HOST);
        assert_eq!(tok(&e, T0 + 40), (200, first.clone()));
        let cred = first["credential"].as_str().unwrap();
        assert_eq!(machine(&e, "/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 41), (200, json!({"protocol": 1, "state": "active", "generation": 1})));
        assert_eq!(machine(&e, "/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 42).0, 200);
        assert_eq!(tok(&e, T0 + 43).1["error"], "invalid_grant");
        let sealed: Option<Vec<u8>> = rusqlite::Connection::open(d.join("dashq.db")).unwrap().query_row("SELECT sealed FROM pairings", [], |r| r.get(0)).unwrap();
        assert_eq!(sealed, None, "erased at acknowledgement");
        // approved but never acknowledged: expiry erases the token response
        let (device2, code2) = start(&e, "inst-2", T0 + 100);
        decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 110);
        assert_eq!(machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 700).1["error"], "expired_token");
        let (_, code3) = start(&e, "inst-3", T0 + 800);
        assert!(decide(&e, "github|1", &code3, "approve", "ws-1", T0 + 1400).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": "nope"}), T0).1["error"], "invalid_grant");
    }

    #[test]
    fn board_scope_is_shown_granted_named_and_never_changed() {
        let (_d, e) = setup("board");
        let scope = json!({"name": "taskq-board/1", "value": "taskqorg/taskq"});
        let req = |inst: &str, scope: Value| json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"], "scope": scope});
        for bad in [json!({"name": "taskq-board"}), json!({"name": "board", "value": "x"}), json!({"name": "taskq-board/1", "value": "a\nb"}), json!("taskq-board/1")] {
            assert_eq!(machine(&e, "/connect/device", None, req("inst-x", bad.clone()), T0).1["error"], "invalid_request", "{bad}");
        }
        // A1: the page names the scope the device requested; a request without one renders as before
        let (_, v) = machine(&e, "/connect/device", None, req("inst-1", scope.clone()), T0);
        let (device, code) = (v["device_code"].as_str().unwrap(), v["user_code"].as_str().unwrap());
        assert!(page(&e, "github|1", code, T0).1.contains("<dt>Board scope</dt><dd><code>taskq-board/1</code>: <code>taskqorg/taskq</code></dd>"));
        assert!(!page(&e, "github|1", &start(&e, "inst-2", T0).1, T0).1.contains("Board scope"));
        // A2: the grant keeps the approved scope and token, status and receipt replies name it
        assert!(decide(&e, "github|1", code, "approve", "ws-1", T0 + 5).1.contains("Approved"));
        let tok = |scope: Value| machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device, "scope": scope}), T0 + 10);
        assert_eq!(tok(json!({"name": "taskq-board/1", "value": "taskqorg/other"})), (403, json!({"protocol": 1, "error": "scope"})));
        let (s, t) = tok(scope.clone());
        assert_eq!((s, &t["scope"]), (200, &scope));
        let cred = t["credential"].as_str().unwrap();
        let (s, a) = machine(&e, "/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 11);
        assert_eq!((s, &a["scope"]), (200, &scope));
        let other = json!({"protocol": 1, "scope": {"name": "taskq-board/1", "value": "taskqorg/other"}, "records": [rec("e1", T0, json!({"n": 1}))]});
        assert_eq!(machine(&e, "/ingest", Some(cred), other, T0 + 12), (403, json!({"protocol": 1, "error": "scope"})));
        assert_eq!(ingest(&e, cred, vec![rec("e1", T0, json!({"n": 1}))], T0 + 12).0, 200);
        let (s, r) = machine(&e, "/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": "e1", "scope": scope}), T0 + 13);
        assert_eq!((s, &r["scope"]), (200, &scope));
        // revoke always ends the connection
        assert_eq!(machine(&e, "/connection/revoke", Some(cred), json!({"protocol": 1, "scope": {"name": "x/1", "value": "y"}}), T0 + 14).1["state"], "revoked");
    }

    #[test]
    fn browser_approval_denies_forgery_and_wrong_accounts() {
        let (_d, e) = setup("browser");
        let (device, code) = start(&e, "inst-1", T0);
        // no verified subject (proxy bypassed or header stripped): nothing shown or changed
        assert_eq!(call(&e, "GET", &format!("/connect?code={code}"), &[], "", T0).0, "401 Unauthorized");
        // an account without membership is never an administrator, however it arrived first
        let (s, body) = page(&e, "github|2", &code, T0);
        assert!(s == "403 Forbidden" && body.contains("not a member"), "{body}");
        let (s, body) = page(&e, "github|1", &code.to_lowercase().replace('-', ""), T0);
        assert!(s == "200 OK" && body.contains("agentq-snapshot/1") && body.contains(&code) && body.contains("type=\"hidden\" name=\"workspace\" value=\"ws-1\""), "{body}");
        // CSRF: foreign origin, missing token, a token minted for another account
        let csrf = e.as_ref().unwrap().token("github|1", &code);
        let form = format!("code={code}&workspace=ws-1&decision=approve&csrf={csrf}");
        let ok_h = [(SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        assert_eq!(call(&e, "POST", "/connect", &[ok_h[0], ("origin", "https://evil.example"), ok_h[2]], &form, T0).0, "403 Forbidden");
        assert_eq!(call(&e, "POST", "/connect", &[ok_h[0], ok_h[2]], &form, T0).0, "403 Forbidden");
        assert_eq!(call(&e, "POST", "/connect", &ok_h, &form.replace(&csrf, ""), T0).0, "403 Forbidden");
        let other = e.as_ref().unwrap().token("github|9", &code);
        assert_eq!(call(&e, "POST", "/connect", &ok_h, &form.replace(&csrf, &other), T0).0, "403 Forbidden");
        // re-pairing a connected installation says it replaces that connection
        assert!(!page(&e, "github|1", &code, T0).1.contains("already connected"));
        let (_, c0) = start(&e, "inst-0", T0);
        decide(&e, "github|1", &c0, "approve", "ws-1", T0);
        let (_, c1) = start(&e, "inst-0", T0);
        assert!(page(&e, "github|1", &c1, T0).1.contains("already connected"));
        // a forged subject header on a machine route grants nothing
        let forged = machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 5);
        assert_eq!(forged.1["error"], "authorization_pending");
        // wrong workspace counts as a failed approval; five invalidate the code
        Enroll::admin(&_d, "ws-2", "github|2").unwrap();
        let s = &e.as_ref().unwrap().store;
        s.add_member("ws-2", "github|1", "member").unwrap();
        assert!(page(&e, "github|1", &code, T0).1.contains("<select name=\"workspace\""));
        for _ in 0..4 {
            assert_eq!(decide(&e, "github|2", &code, "approve", "ws-1", T0 + 10).0, "403 Forbidden");
        }
        assert_eq!(decide(&e, "github|1", &code, "deny", "ws-1", T0 + 11).0, "200 OK");
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 12).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 20).1["error"], "access_denied");
        let (device2, code2) = start(&e, "inst-2", T0);
        for _ in 0..5 {
            decide(&e, "github|2", &code2, "approve", "ws-1", T0 + 10);
        }
        assert!(decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 11).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 20).1["error"], "invalid_grant");
        assert!(s.credential(b"none").unwrap().is_none());
    }

    #[test]
    fn setup_limits_and_methods() {
        let d = dir("setup");
        let none = Enroll::open(&d, HOST);
        assert!(none.is_err());
        assert_eq!(call(&none, "POST", "/connect/device", &[], "{}", T0).0, "503 Service Unavailable");
        assert!(Enroll::admin(&d, "ws 1", "x").is_err() && Enroll::admin(&d, "ws", "a\nb").is_err());
        let (d, e) = setup("limits");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(d.join("dashq.key")).unwrap().permissions()) & 0o777, 0o600);
        assert_eq!(call(&e, "GET", "/connect/token", &[], "", T0).0, "405 Method Not Allowed");
        assert!(route(&e, &Req { method: "GET", path: "/data", query: "", headers: &[], body: b"" }, T0).is_none());
        let dev = |ip: &str, inst: &str| {
            let h = [(CLIENT, ip)];
            call(&e, "POST", "/connect/device", &h, &json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "wsl", "host_label": "pc", "schemas": ["agentq-snapshot/1"]}).to_string(), T0)
        };
        for i in 0..10 {
            assert_eq!(dev("203.0.113.1", &format!("i{i}")).0, "200 OK");
        }
        let (s, h, v) = dev("203.0.113.1", "i10");
        assert_eq!((s.as_str(), h.as_str(), &v["error"]), ("429 Too Many Requests", "Retry-After: 60\r\n", &json!("rate_limited")));
        assert_eq!(dev("203.0.113.2", "i11").0, "200 OK");
        // scope: a product requests only its own schemas
        let bad = json!({"protocol": 1, "product": "serverq", "installation": "s1", "runtime": "native", "host_label": "pc", "schemas": ["agentq-snapshot/1"]});
        assert_eq!(machine(&e, "/connect/device", None, bad, T0).1["error"], "invalid_request");
        assert_eq!(user_code("abcd efgh"), Some("ABCD-EFGH".into()));
        assert_eq!(user_code("ABCD-EFG1"), None);
    }

    #[test]
    fn proxy_fixture_exempts_only_machine_routes() {
        let c = include_str!("../fixture/Caddyfile");
        let (head, guarded) = c.split_once("forward_auth").unwrap();
        // client copies of the trusted headers are stripped before anything else
        assert!(head.contains("request_header -X-Dashq-Subject") && head.contains("request_header -X-Dashq-Client"));
        let paths: Vec<&str> = head.lines().filter_map(|l| l.trim().strip_prefix("path ")).flat_map(str::split_whitespace).collect();
        assert_eq!(paths, MACHINE);
        assert!(head.contains("method POST") && !head.contains("/data") && !head.contains('*'));
        assert!(guarded.contains("copy_headers") && guarded.contains(">X-Dashq-Subject") && guarded.contains("reverse_proxy 127.0.0.1:18882"));
    }

    /// Approved and acknowledged connection of `inst` in `ws` as `subject`; returns the credential.
    fn connect(e: &Result<Enroll, String>, inst: &str, subject: &str, ws: &str, now: i64) -> String {
        let (device, code) = start(e, inst, now);
        assert!(decide(e, subject, &code, "approve", ws, now).1.contains("Approved"));
        let cred = machine(e, "/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(e, "/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        cred
    }

    fn rec(id: &str, observed: i64, payload: Value) -> Value {
        json!({"schema": "agentq-snapshot/1", "event_id": id, "observed_at": observed, "payload": payload})
    }

    fn ingest(e: &Result<Enroll, String>, cred: &str, records: Vec<Value>, now: i64) -> (u16, Value) {
        machine(e, "/ingest", Some(cred), json!({"protocol": 1, "records": records}), now)
    }

    fn states(v: &Value) -> Vec<String> {
        v["receipts"].as_array().unwrap().iter().map(|r| format!("{}{}", r["state"].as_str().unwrap(), r["reason"].as_str().map(|x| format!(":{x}")).unwrap_or_default())).collect()
    }

    /// Exact match where `"$name"` binds on first use and a new binding never repeats another's value (coreq traces).
    fn matches(want: &Value, got: &Value, vars: &mut HashMap<String, Value>, at: &str) {
        match (want, got) {
            (Value::String(s), _) if s.starts_with('$') => match vars.get(s) {
                Some(v) => assert_eq!(v, got, "{at}: {s}"),
                None => {
                    assert!(!vars.values().any(|v| v == got), "{at}: {s} repeats a bound value");
                    vars.insert(s.clone(), got.clone());
                }
            },
            (Value::Object(w), Value::Object(g)) => {
                assert_eq!(w.keys().collect::<std::collections::BTreeSet<_>>(), g.keys().collect(), "{at}: keys");
                w.iter().for_each(|(k, v)| matches(v, &g[k], vars, &format!("{at}.{k}")));
            }
            (Value::Array(w), Value::Array(g)) => {
                assert_eq!(w.len(), g.len(), "{at}: length");
                w.iter().zip(g).enumerate().for_each(|(i, (w, g))| matches(w, g, vars, &format!("{at}[{i}]")));
            }
            _ => assert_eq!(want, got, "{at}"),
        }
    }

    fn subst(v: &Value, vars: &HashMap<String, Value>) -> Value {
        match v {
            Value::String(s) if s.starts_with('$') => vars.get(s).cloned().unwrap_or_else(|| panic!("unbound {s}")),
            Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), subst(v, vars))).collect()),
            Value::Array(a) => Value::Array(a.iter().map(|v| subst(v, vars)).collect()),
            _ => v.clone(),
        }
    }

    /// coreq wire v1 conformance traces (fixture/enrollment: coreq v0.7.0 tests/enrollment, unchanged) run against
    /// these handlers; `{"server": "project"}` is the dashboard projection pass.
    #[test]
    fn coreq_conformance_traces() {
        let traces = [
            include_str!("../fixture/enrollment/agentq-connect.json"),
            include_str!("../fixture/enrollment/agentq-pairing.json"),
            include_str!("../fixture/enrollment/agentq-reject.json"),
            include_str!("../fixture/enrollment/serverq-connect.json"),
        ];
        for (n, text) in traces.iter().enumerate() {
            let trace: Value = serde_json::from_str(text).unwrap();
            assert_eq!(trace["contract"], coreq::enroll::CONTRACT);
            let d = dir(&format!("trace-{n}"));
            for ws in ["ws-1", "ws-2"] {
                Enroll::admin(&d, ws, "acct-1").unwrap();
            }
            let e = Enroll::open(&d, HOST);
            let mut vars = HashMap::new();
            for (i, step) in trace["steps"].as_array().unwrap().iter().enumerate() {
                let at_s = format!("{} step {i}", trace["about"]);
                let now = step["at"].as_i64().unwrap_or(0);
                if step["server"] == "project" {
                    e.as_ref().unwrap().project(Scope::All).unwrap();
                } else if let Some(action) = step["browser"].as_str() {
                    let s = subst(step, &vars);
                    let (_, body) = decide(&e, s["account"].as_str().unwrap(), s["user_code"].as_str().unwrap(), action, s["workspace"].as_str().unwrap(), now);
                    let got = if body.contains("Approved:") {
                        "approved"
                    } else if body.contains("already approved") {
                        "already approved"
                    } else if body.contains("Denied:") {
                        "denied"
                    } else {
                        "invalid"
                    };
                    assert_eq!(got, step["result"], "{at_s}: {body}");
                } else {
                    let auth = step.get("auth").map(|a| subst(a, &vars));
                    let (status, resp) = machine(&e, step["route"].as_str().unwrap(), auth.as_ref().and_then(Value::as_str), subst(&step["request"], &vars), now);
                    assert_eq!(status, step["status"], "{at_s}: {resp}");
                    matches(&step["response"], &resp, &mut vars, &at_s);
                    assert_eq!(coreq::enroll::check(&resp), Ok(()), "{at_s}");
                    if let Some(v) = step.get("verify") {
                        let got = coreq::enroll::verify(&resp, v["installation"].as_str().unwrap(), v["generation"].as_u64().unwrap(), v["schema"].as_str().unwrap(), v["event_id"].as_str().unwrap());
                        assert_eq!(got.err().unwrap_or("connected".into()), step["client"], "{at_s}");
                    }
                }
            }
        }
    }

    #[test]
    fn ingest_takes_ownership_from_the_grant_and_allowlists_payloads() {
        let (_d, e) = setup("ingest");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", T0);
        // authority fields in a payload are data: the grant decides installation and workspace
        let forged = json!({"installation": "inst-9", "workspace": "ws-9", "version": "1.2.0", "cpu": {"percent": 12.5}, "input_tokens": 10});
        let bad = [
            json!({"prompt": "hi"}),
            json!({"nested": {"stdout": "x"}}),
            json!({"log_path": "x"}),
            json!({"where": "/Users/me"}),
            json!({"where": "C:\\Users"}),
            json!({"note": "a\nb"}),
            json!({"long": "x".repeat(257)}),
            json!("not an object"),
            json!({"a": {"b": {"c": {"d": {"e": {"f": 1}}}}}}),
        ];
        let mut records = vec![rec("ev-1", T0, forged.clone())];
        records.extend(bad.iter().enumerate().map(|(i, p)| rec(&format!("bad-{i}"), T0, p.clone())));
        records.push(json!({"schema": "serverq-report/1", "event_id": "other", "observed_at": T0, "payload": {}}));
        records.push(json!({"schema": "agentq-snapshot/1", "event_id": "no-time", "payload": {}}));
        let (s, v) = ingest(&e, &cred, records, T0);
        assert_eq!(s, 200, "{v}");
        let mut want = vec!["accepted".to_string()];
        want.extend(std::iter::repeat_n("rejected:payload".to_string(), bad.len()));
        want.extend(["rejected:scope".into(), "rejected:payload".into()]);
        assert_eq!(states(&v), want);
        let rows = e.as_ref().unwrap().project(Scope::All).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].installation.as_str(), rows[0].workspace.as_str()), ("inst-1", "ws-1"));
        assert_eq!(serde_json::from_str::<Value>(&rows[0].snapshot.as_ref().unwrap().payload).unwrap(), forged);
        // stable IDs: a replay repeats the receipt, changed content under the same ID fails
        assert_eq!(states(&ingest(&e, &cred, vec![rec("ev-1", T0, forged.clone())], T0 + 1).1), ["accepted"]);
        assert_eq!(states(&ingest(&e, &cred, vec![rec("ev-1", T0, json!({"version": "9"}))], T0 + 2).1), ["rejected:replay_mismatch"]);
        assert_eq!(states(&ingest(&e, &cred, vec![rec("old", T0 - 9 * 86400, json!({}))], T0 + 3).1), ["rejected:expired"]);
        assert_eq!(e.as_ref().unwrap().store.history("inst-1").unwrap().len(), 1);
        // malformed batches are refused whole
        assert_eq!(ingest(&e, &cred, vec![json!({"schema": "agentq-snapshot/1"})], T0 + 4), (400, json!({"protocol": 1, "error": "invalid_request"})));
        assert_eq!(ingest(&e, &cred, (0..101).map(|i| rec(&format!("m{i}"), T0, json!({}))).collect(), T0 + 5).0, 400);
        assert_eq!(machine(&e, "/connection/receipt", Some(&cred), json!({"protocol": 1}), T0 + 6).0, 400);
        // per-device rate: burst 10, then 429 with Retry-After
        let codes: Vec<u16> = (0..12).map(|_| ingest(&e, &cred, vec![], T0 + 100).0).collect();
        assert_eq!(codes, [[200; 10].as_slice(), &[429, 429]].concat());
        let (s, h, _) = call(&e, "POST", "/ingest", &[("authorization", &format!("Bearer {cred}"))], r#"{"protocol": 1, "records": []}"#, T0 + 100);
        assert_eq!((s.as_str(), h.as_str()), ("429 Too Many Requests", "Retry-After: 1\r\n"));
        assert_eq!(ingest(&e, &cred, vec![], T0 + 101).0, 200);
    }

    #[test]
    fn projection_isolates_workspaces_and_members_and_keeps_expected_installations() {
        let (d, e) = setup("scope");
        Enroll::admin(&d, "ws-2", "github|2").unwrap();
        let s = &e.as_ref().unwrap().store;
        s.add_member("ws-1", "github|3", "member").unwrap();
        let admin = connect(&e, "inst-a", "github|1", "ws-1", T0);
        let member = connect(&e, "inst-m", "github|3", "ws-1", T0);
        let other = connect(&e, "inst-o", "github|2", "ws-2", T0);
        // approved, never acknowledged: expected, pending, no data
        let (device, code) = start(&e, "inst-p", T0);
        decide(&e, "github|1", &code, "approve", "ws-1", T0);
        for (c, id) in [(&admin, "a-1"), (&member, "m-1"), (&other, "o-1")] {
            assert_eq!(states(&ingest(&e, c, vec![rec(id, T0, json!({"v": 1}))], T0).1), ["accepted"]);
        }
        let seen = |scope| -> Vec<(String, &'static str, Option<String>)> {
            e.as_ref().unwrap().project(scope).unwrap().into_iter().map(|r| (r.installation, r.connection, r.snapshot.map(|s| s.event_id))).collect()
        };
        let a = |i: &str, c, ev: Option<&str>| (i.to_string(), c, ev.map(String::from));
        assert_eq!(seen(Scope::Subject("github|1")), [a("inst-a", "active", Some("a-1")), a("inst-m", "active", Some("m-1")), a("inst-p", "pending", None)]);
        assert_eq!(seen(Scope::Subject("github|3")), [a("inst-m", "active", Some("m-1"))]);
        assert_eq!(seen(Scope::Subject("github|2")), [a("inst-o", "active", Some("o-1"))]);
        assert!(seen(Scope::Subject("github|9")).is_empty());
        assert_eq!(seen(Scope::All).len(), 4);
        // a device reads only its own receipts
        assert_eq!(machine(&e, "/connection/receipt", Some(&other), json!({"protocol": 1, "event_id": "a-1"}), T0).0, 404);
        assert_eq!(machine(&e, "/connection/receipt", Some(&admin), json!({"protocol": 1, "event_id": "a-1"}), T0).1["state"], "projected");
        // revoked: kept as revoked, not dropped; a new approval into another workspace starts empty there
        assert_eq!(machine(&e, "/connection/revoke", Some(&admin), json!({"protocol": 1}), T0).0, 200);
        assert_eq!(ingest(&e, &admin, vec![rec("a-2", T0, json!({}))], T0 + 1), (401, json!({"protocol": 1, "error": "revoked"})));
        s.add_member("ws-2", "github|1", "member").unwrap();
        let moved = connect(&e, "inst-a", "github|1", "ws-2", T0 + 2);
        assert_eq!(seen(Scope::Subject("github|2")), [a("inst-a", "active", None), a("inst-o", "active", Some("o-1"))]);
        assert_eq!(states(&ingest(&e, &moved, vec![rec("a-1", T0 + 3, json!({"v": 2}))], T0 + 3).1), ["accepted"]);
        let ws1: Vec<_> = seen(Scope::Subject("github|1")).into_iter().filter(|r| r.0 == "inst-a").collect();
        assert_eq!(ws1, [a("inst-a", "revoked", Some("a-1")), a("inst-a", "active", Some("a-1"))]);
        let rows = e.as_ref().unwrap().project(Scope::Subject("github|1")).unwrap();
        let payloads: Vec<(&str, &str)> = rows.iter().filter(|r| r.installation == "inst-a").map(|r| (r.workspace.as_str(), r.snapshot.as_ref().unwrap().payload.as_str())).collect();
        assert_eq!(payloads, [("ws-1", r#"{"v":1}"#), ("ws-2", r#"{"v":2}"#)]);
        // a live grant is never moved to another workspace silently
        let (_, code) = start(&e, "inst-o", T0 + 4);
        assert_eq!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 4).0, "409 Conflict");
        drop(device);
    }

    /// Test config without Langfuse or GitHub and with an unreadable serverq report: every optional source fails.
    fn app(e: Result<Enroll, String>, langfuse: Option<(String, String)>) -> crate::App {
        let cfg = crate::Config { langfuse, links: json!({}), github_token: None, projects: vec![], taskq_bin: None, serverq_report: "/nonexistent/report.json".into(), serverq_host: "fra/native".into(), public_host: HOST.into(), answer: None };
        crate::App { cfg, cache: crate::source::Cache::default(), enroll: e, answers: Default::default() }
    }

    fn data(app: &crate::App, query: &str, scope: Option<Scope>) -> (&'static str, Value) {
        let (s, _, body) = crate::route(app, "GET", "/data", query, scope);
        (s, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn client_receipt_and_scoped_data_project_the_same_snapshot_through_outages() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("e2e");
        let host = dir("e2e-host");
        let req = coreq::enroll::Request { product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"] };
        let mut w = Wire { e: &e, now, code: None, calls: vec![] };
        let conn = coreq::enroll::pair(&host, "https://dash.example.com", &req, &mut w).unwrap();
        let payload = json!({"version": "1.2.0", "cpu": {"percent": 7}});
        let r = coreq::enroll::stamp(&host, "agentq-snapshot/1", "snap-1", now as u64, payload.clone()).unwrap();
        let got = coreq::enroll::deliver(&host, std::slice::from_ref(&r), &mut Default::default(), &mut w).unwrap();
        assert_eq!(got, [json!({"event_id": "snap-1", "state": "accepted"})]);
        assert_eq!(coreq::enroll::receipt(&host, &r, &mut w), Ok("accepted"));
        // Langfuse unreachable, GitHub and serverq missing: the snapshot still projects and /data answers 200
        let app = app(e, Some(("https://127.0.0.1:9".into(), "Basic x".into())));
        let (s, doc) = data(&app, "", Some(Scope::Subject("github|1")));
        assert_eq!(s, "200 OK", "{doc}");
        assert_eq!((doc["sources"]["langfuse"]["state"].as_str(), doc["sources"]["github"]["reason"].as_str()), (Some("unknown"), Some("not configured")));
        assert_eq!(doc["sources"]["enrollment"]["state"], "ok");
        let row = &doc["installations"]["rows"][0];
        assert_eq!((&row["installation"], &row["generation"], &row["connection"]), (&conn["installation"], &json!(1), &json!("active")));
        assert_eq!(row["snapshot"], json!({"event_id": "snap-1", "schema": "agentq-snapshot/1", "payload": payload}));
        assert_eq!(row["observation"]["state"], "ok");
        // the client's verified receipt now names the snapshot that /data showed
        let mut w = Wire { e: &app.enroll, now, code: None, calls: vec![] };
        assert_eq!(coreq::enroll::receipt(&host, &r, &mut w), Ok("projected"));
        // scope: another account, a remote request without a subject, a host filter
        assert_eq!(data(&app, "", Some(Scope::Subject("github|2"))).1["installations"]["rows"], json!([]));
        let (_, anon) = data(&app, "", None);
        assert_eq!((anon["installations"]["rows"].clone(), anon["sources"]["enrollment"]["reason"].as_str()), (json!([]), Some("no signed-in subject")));
        assert_eq!(data(&app, "host=fra%2Fnative", Some(Scope::All)).1["installations"]["rows"], json!([]));
        assert_eq!(data(&app, "host=mac-1%2Fnative", Some(Scope::All)).1["installations"]["rows"].as_array().unwrap().len(), 1);
        assert!(crate::actor(Some(HOST), HOST, None).is_none());
        assert!(matches!(crate::actor(Some(HOST), HOST, Some("github|1")), Some(Scope::Subject("github|1"))));
        assert!(matches!(crate::actor(Some("127.0.0.1:18882"), HOST, None), Some(Scope::All)));
        // enrollment not set up: the capability stays inactive, no installations section or source
        let (_, doc) = data(&self::app(Err("not set up".into()), None), "", Some(Scope::All));
        assert!(doc.get("installations").is_none() && doc["sources"].get("enrollment").is_none());
    }

    #[test]
    fn late_stale_and_future_observations_keep_truthful_freshness() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("fresh");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", now);
        let app = app(e, None);
        let e = &app.enroll;
        let obs = |q: &str| data(&app, q, Some(Scope::All)).1["installations"]["rows"][0].clone();
        // expected installation without any event
        assert_eq!(obs("")["observation"], json!({"state": "unknown", "at": null, "age_s": null, "reason": "no snapshot yet", "incomplete": false}));
        // observed 400 s ago: stale by producer time, whatever the arrival time
        assert_eq!(states(&ingest(e, &cred, vec![rec("s-1", now - 400, json!({"v": 1}))], now).1), ["accepted"]);
        let o = obs("");
        assert_eq!((o["observation"]["state"].as_str(), o["observation"]["at"].as_i64()), (Some("stale"), Some(now - 400)));
        // a late sample joins history only and is never projected as current
        assert_eq!(states(&ingest(e, &cred, vec![rec("s-0", now - 900, json!({"v": 0}))], now + 1).1), ["accepted"]);
        assert_eq!(obs("")["snapshot"]["event_id"], "s-1");
        assert_eq!(machine(e, "/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-0"}), now + 2).1["state"], "accepted");
        assert_eq!(machine(e, "/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 2).1["state"], "projected");
        // future time: shown as clock-uncertain, never fresh, and the next real sample replaces it
        ingest(e, &cred, vec![rec("s-f", now + 3600, json!({"v": 9}))], now + 3);
        let o = obs("");
        assert_eq!((o["snapshot"]["event_id"].as_str(), o["observation"]["state"].as_str(), o["observation"]["age_s"].clone()), (Some("s-f"), Some("unknown"), Value::Null));
        ingest(e, &cred, vec![rec("s-2", now, json!({"v": 2}))], now + 4);
        let o = obs("");
        assert_eq!((o["snapshot"]["event_id"].as_str(), o["observation"]["state"].as_str()), (Some("s-2"), Some("ok")));
    }

    fn get(e: &Result<Enroll, String>, subject: &str, path: &str, now: i64) -> (String, String, String) {
        let (s, h, v) = call(e, "GET", path, &[(SUBJECT, subject)], "", now);
        (s, h, v.as_str().unwrap_or_default().to_string())
    }

    fn revoke(e: &Result<Enroll, String>, subject: &str, token_subject: &str, inst: &str, ws: &str, now: i64) -> (String, String) {
        let csrf = e.as_ref().unwrap().token_for(&revoke_purpose(token_subject, inst, ws));
        let h = [(SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        let (s, _, v) = call(e, "POST", "/connect", &h, &format!("decision=revoke&installation={inst}&workspace={ws}&csrf={csrf}"), now);
        (s, v.as_str().unwrap().to_string())
    }

    #[test]
    fn approval_follows_projection_to_overview_and_lifecycle_reads_stay_scoped() {
        let now = crate::source::now() as i64;
        let (d, e) = setup("life");
        Enroll::admin(&d, "ws-2", "github|2").unwrap();
        e.as_ref().unwrap().store.add_member("ws-1", "github|3", "member").unwrap();
        // approval page follows this generation to the overview at 2 s
        let (device, code) = start(&e, "inst-1", now);
        let (_, body) = decide(&e, "github|1", &code, "approve", "ws-1", now);
        let wait = format!("/connect?wait=inst-1&workspace=ws-1&generation=1&since={now}");
        assert!(body.contains(&format!("content=\"2; url={}\"", wait.replace('&', "&amp;"))), "{body}");
        let cred = machine(&e, "/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let (s, _, b) = get(&e, "github|1", &wait, now + 2);
        assert!(s == "200 OK" && b.contains("Waiting for the first snapshot") && b.contains("http-equiv=\"refresh\""), "{b}");
        // another account cannot follow or even see it
        assert_eq!(get(&e, "github|2", &wait, now + 2).0, "404 Not Found");
        assert_eq!(states(&ingest(&e, &cred, vec![rec("s-1", now, json!({"v": 1}))], now + 3).1), ["accepted"]);
        let (s, h, _) = get(&e, "github|1", &wait, now + 4);
        assert_eq!((s.as_str(), h.as_str()), ("303 See Other", "Location: /#inst-inst-1-ws-1\r\n"));
        assert_eq!(machine(&e, "/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 4).1["state"], "projected");
        assert!(get(&e, "github|1", &wait.replace("generation=1", "generation=7"), now + 4).2.contains("replaced or revoked"));
        // a host that never sends: progress ends after 60 s with an honest state
        let other = connect(&e, "inst-2", "github|2", "ws-2", now);
        let w2 = format!("/connect?wait=inst-2&workspace=ws-2&generation=1&since={now}");
        assert!(get(&e, "github|2", &w2, now + 30).2.contains("Waiting for the first snapshot"));
        assert!(get(&e, "github|2", &w2, now + 61).2.contains("No snapshot arrived within 60 s"));
        // history warmup until 3 samples; export gaps per workspace; data age apart from both
        let app = app(e, None);
        let e = &app.enroll;
        let row = |scope, inst: &str| data(&app, "", scope).1["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).cloned();
        let r1 = row(Some(Scope::All), "inst-1").unwrap();
        assert_eq!((r1["history"].clone(), r1["observation"]["state"].clone()), (json!({"samples": 1, "state": "warmup", "reason": "collecting history: 1 of 3 samples"}), json!("ok")));
        ingest(e, &cred, vec![rec("s-2", now + 1, json!({"v": 2})), rec("s-3", now + 2, json!({"v": 3}))], now + 5);
        assert_eq!(row(Some(Scope::All), "inst-1").unwrap()["history"], json!({"samples": 3, "state": "ok", "reason": null}));
        let s = &e.as_ref().unwrap().store;
        s.accept(&sha256(&cred), &[Event { id: "x-1", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        s.accept(&sha256(&other), &[Event { id: "x-2", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }, Event { id: "x-3", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        s.prune(now + 7 * 86400 + 1).unwrap();
        assert_eq!(row(Some(Scope::All), "inst-1").unwrap()["export"], json!({"gap_events": 1, "state": "gap", "reason": "events expired before export: 1"}));
        assert_eq!(row(Some(Scope::All), "inst-2").unwrap()["export"]["gap_events"], 2);
        // scoped JSON and HTML: no other workspace's rows, evidence IDs or gaps; no subject: nothing
        let (_, doc) = data(&app, "", Some(Scope::Subject("github|2")));
        let text = doc["installations"].to_string();
        assert!(!text.contains("inst-1") && !text.contains("s-3") && text.contains("inst-2"), "{text}");
        let html = |scope| String::from_utf8(crate::route(&app, "GET", "/", "", scope).2).unwrap();
        let h2 = html(Some(Scope::Subject("github|2")));
        assert!(h2.contains("id=\"inst-inst-2-ws-2\"") && !h2.contains("inst-1") && !h2.contains("s-3"));
        let h1 = html(Some(Scope::Subject("github|1")));
        assert!(h1.contains("<code>s-3</code>") && h1.contains("gap · events expired before export: 1") && !h1.contains("inst-2"));
        assert!(h1.contains("href=\"/connect?revoke=inst-1&amp;workspace=ws-1\""));
        assert!(!html(None).contains("inst-"));
        let all = html(Some(Scope::All));
        assert!(all.contains("inst-inst-1-ws-1") && all.contains("inst-inst-2-ws-2"));
        // revoke: confirmation names the installation and effect; only the workspace admin or its approver
        assert_eq!(get(e, "github|2", "/connect?revoke=inst-1&workspace=ws-1", now).0, "404 Not Found");
        let (s1, _, b) = get(e, "github|1", "/connect?revoke=inst-1&workspace=ws-1", now);
        assert!(s1 == "200 OK" && b.contains("Revoke <strong>mac-1 (native)</strong>") && b.contains("Accepted history stays"), "{b}");
        assert_eq!(revoke(e, "github|2", "github|1", "inst-1", "ws-1", now).0, "403 Forbidden");
        assert!(revoke(e, "github|2", "github|2", "inst-1", "ws-1", now).1.contains("Nothing to revoke"));
        assert!(revoke(e, "github|3", "github|3", "inst-1", "ws-1", now).1.contains("Nothing to revoke"), "a member revokes only its own approvals");
        let (_, _, wrong_origin) = call(e, "POST", "/connect", &[(SUBJECT, "github|1"), ("origin", "https://evil.example"), ("content-type", "application/x-www-form-urlencoded")], "decision=revoke&installation=inst-1&workspace=ws-1", now);
        assert!(wrong_origin.as_str().unwrap().contains("did not come from"));
        assert!(revoke(e, "github|1", "github|1", "inst-1", "ws-1", now).1.contains("Revoked: installation <code>inst-1</code>"));
        assert_eq!(ingest(e, &cred, vec![rec("s-4", now, json!({}))], now + 6), (401, json!({"protocol": 1, "error": "revoked"})));
        let r1 = row(Some(Scope::All), "inst-1").unwrap();
        assert_eq!((r1["connection"].as_str(), r1["snapshot"]["event_id"].as_str()), (Some("revoked"), Some("s-3")));
        let h1 = html(Some(Scope::Subject("github|1")));
        assert!(h1.contains("history kept") && !h1.contains("revoke=inst-1"));
        assert!(get(e, "github|1", "/connect?revoke=inst-1&workspace=ws-1", now).2.contains("already revoked"));
        assert!(revoke(e, "github|1", "github|1", "inst-1", "ws-1", now).1.contains("Nothing to revoke"));
        // the local report reads the store only with the host administrator's private key
        #[cfg(unix)]
        {
            std::fs::set_permissions(d.join("dashq.key"), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
            assert_eq!(Enroll::open(&d, HOST).err().unwrap(), "dashq.key: mode must be 0600");
        }
    }
}
