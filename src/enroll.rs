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
/// Operation capabilities a device may request for separate approval (D16), with the product that owns each and
/// the effect the approval page names.
const CAPABILITIES: [(&str, &str, &str); 1] = [("agentq", "agentq.configure", "change AgentQ client integration settings on this host, one reviewed request at a time")];
/// C3: new pairing sessions per source address per minute, and live sessions overall.
const PER_MINUTE: usize = 10;
const MAX_PENDING: i64 = 1000;
/// C5 per-device bucket: 60 requests a minute, burst 10.
const BURST: f64 = 10.0;
/// C5 heartbeat: a snapshot older than this is stale.
pub const STALE_SECS: u64 = 180;
/// ServerQ sends no heartbeat between its hourly checks: its report is stale after the serverq limit (D4.3, D6).
pub const SERVERQ_STALE_SECS: u64 = 7200;

/// Snapshots a dashboard view renders: only these receipts become `projected` (C4). Installations renders every
/// `agentq-snapshot/1` and the TaskQ board summary of a schema 1 `taskq-board/1`; Server checks renders only the
/// ServerQ payload shape data.rs `serverq_enrolled` accepts, anything else reads unknown there and stays `accepted`.
pub fn renders(product: &str, s: &store::Snapshot) -> bool {
    match (product, s.schema.as_str()) {
        ("agentq", "agentq-snapshot/1") => true,
        ("serverq", "serverq-report/1") => serverq_payload(&serde_json::from_str(&s.payload).unwrap_or(Value::Null)),
        ("taskq", "taskq-board/1") => taskq_payload(&serde_json::from_str(&s.payload).unwrap_or(Value::Null)),
        _ => false,
    }
}

/// serverq.md § 3 projection, schema 1: the only ServerQ payload Server checks renders.
pub fn serverq_payload(p: &Value) -> bool {
    p["schema"] == 1 && p["product"] == "serverq" && p["rules"].is_object()
}

/// taskq enrollment.md Observation, schema 1: the only TaskQ payload Connections renders as a board summary.
pub fn taskq_payload(p: &Value) -> bool {
    p["schema"] == 1 && p["board"]["repo"].is_string() && p["tasks"].is_array()
}

/// Producer-time freshness limit of a snapshot schema.
pub fn stale_secs(schema: &str) -> u64 {
    if schema == "serverq-report/1" { SERVERQ_STALE_SECS } else { STALE_SECS }
}
/// C5 resources: a trend needs at least this many samples; fewer is history warmup.
pub const TREND_SAMPLES: i64 = 3;
/// C5: first-connection progress polls every 2 s for at most 60 s.
const WAIT_SECS: i64 = 60;
const MACHINE: [&str; 8] = ["/machine/connect/device", "/machine/connect/token", "/machine/connect/ack", "/machine/ingest", "/machine/connection/receipt", "/machine/connection/revoke", "/machine/operations", "/machine/operations/result"];
/// AgentQ configure fields and values (agentq CONFIGURATION.md A3 request `changes`); no removal value exists.
pub const CHANGES: [(&str, &[&str]); 5] = [("routing", &["routed", "direct"]), ("optimization", &["enabled", "disabled"]), ("mcp", &["registered"]), ("hooks", &["registered"]), ("plugin", &["enabled"])];

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
    /// Stage records of each decoded telemetry block by block id (D21 #182): blocks never change.
    stages: Mutex<HashMap<i64, std::sync::Arc<Vec<Value>>>>,
}

/// The deployment unit: `dashq.db` and its key `dashq.key` (0600) in the config folder (C4).
fn paths(dir: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    (dir.join("dashq.db"), dir.join("dashq.key"))
}

impl Enroll {
    /// Opens a set-up deployment; `Err` when `dashq admin` never ran there or the key is unusable.
    pub fn open(dir: &Path, public_host: &str) -> Result<Enroll, String> {
        Self::open_with(dir, public_host, store::LIMITS)
    }

    /// `open` with the configured telemetry bounds (config `telemetry`).
    pub fn open_with(dir: &Path, public_host: &str, limits: store::Limits) -> Result<Enroll, String> {
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
        let store = Store::open(&db, limits).map_err(|e| format!("dashq.db: {e:?}"))?;
        Ok(Enroll {
            store,
            seal: aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &k).map_err(|_| "dashq.key: bad key")?),
            // separate keys for sealing and CSRF, both from the one deployment key
            csrf: hmac::Key::new(hmac::HMAC_SHA256, ring::digest::digest(&ring::digest::SHA256, &[b"dashq csrf\n".as_slice(), &k].concat()).as_ref()),
            public_host: public_host.into(),
            rng: SystemRandom::new(),
            recent: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
            stages: Mutex::new(HashMap::new()),
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

    /// Host-administrator-only task-control grant, separate from answering and observation.
    pub fn controller(dir: &Path, workspace: &str, subject: &str, allow: bool) -> Result<(), String> {
        if !id(workspace, 64) || !valid_subject(subject) { return Err("controller: bad workspace or subject".into()); }
        let e = Enroll::open(dir, "")?;
        e.store.set_controller(workspace, subject, allow).map_err(|e| format!("dashq.db: {e:?}"))
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
        self.store.project(scope, renders)
    }

    /// Typed AgentQ configure enqueue for the guarded browser adapter (#63; D16). The server sets everything with
    /// authority: actor from the verified `subject`, workspace and generation from the installation's live grant,
    /// profiles from its current snapshot; the caller names only the operation ID, the changes and, for an apply,
    /// the confirmed plan. Returns the immutable queued operation; the same ID and content again returns it
    /// unchanged. Errors are safe codes; nothing is queued.
    pub fn configure(&self, subject: &str, installation: &str, operation: &str, plan: Option<Plan>, changes: &[Change], now: i64) -> Result<Value, &'static str> {
        let hex = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        let mut seen = std::collections::HashSet::new();
        let valid = hex(operation, 32)
            && (1..=64).contains(&changes.len())
            && changes.iter().all(|c| id(c.profile, 64) && CHANGES.iter().any(|(f, vs)| *f == c.field && vs.contains(&c.value)) && seen.insert((c.profile, c.field)))
            && plan.as_ref().is_none_or(|p| id(p.revision, 64) && hex(p.digest, 64));
        if !valid {
            return Err("invalid");
        }
        let kind = if plan.is_some() { "apply" } else { "plan" };
        let wanted = |op: &Value| {
            let p = &op["payload"];
            op["installation"] == installation && p["kind"] == kind && p["actor"] == actor_id(subject).as_str() && p["changes"] == json!(changes.iter().map(Change::wire).collect::<Vec<_>>())
                && plan.as_ref().is_none_or(|x| p["plan"] == json!({"revision": x.revision, "digest": x.digest}))
        };
        let unavailable = |x| if x == store::Error::Busy { "busy" } else { "unavailable" };
        if let Some(rec) = self.store.operation(operation).map_err(unavailable)? {
            return if wanted(&rec.op) { Ok(rec.op) } else { Err("replay_mismatch") };
        }
        let Some((g, snapshot)) = self.store.live(installation).map_err(unavailable)? else { return Err("no_grant") };
        let role = self.store.role(&g.workspace, subject).map_err(unavailable)?;
        if !(role.as_deref() == Some("admin") || (role.is_some() && g.account == subject)) {
            return Err("authority");
        }
        // every profile of the current snapshot, as AgentQ reports it; no snapshot yet authorizes none
        let snapshot: Value = snapshot.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        let mut profiles: Vec<&str> = vec![];
        for p in snapshot["configuration"]["profiles"].as_array().into_iter().flatten().filter_map(|p| p["id"].as_str()).filter(|p| id(p, 64)) {
            if !profiles.contains(&p) {
                profiles.push(p);
            }
        }
        if profiles.is_empty() || profiles.len() > 64 {
            return Err("no_profiles");
        }
        if !changes.iter().all(|c| profiles.contains(&c.profile)) {
            return Err("profile_out_of_scope");
        }
        let mut payload = json!({"schema": 1, "operation": operation, "kind": kind, "actor": actor_id(subject), "workspace": g.workspace, "installation": installation,
            "generation": g.generation, "issued_at": now, "expires_at": now + store::OPERATION_SECS, "changes": changes.iter().map(Change::wire).collect::<Vec<_>>()});
        if let Some(p) = plan {
            payload["plan"] = json!({"revision": p.revision, "digest": p.digest});
        }
        let op = json!({"operation": operation, "capability": "agentq.configure", "installation": installation, "generation": g.generation, "expires_at": now + store::OPERATION_SECS,
            "digest": coreq::enroll::digest(&payload), "payload": payload, "authority": {"actor": actor_id(subject), "workspace": g.workspace, "profiles": profiles}});
        match self.store.queue(subject, &op, now).map_err(unavailable)? {
            store::Queued::Queued | store::Queued::Same => Ok(op),
            store::Queued::Refused(why) => Err(why),
        }
    }

    /// Readback of one queued operation for its actor or the workspace administrator; `None` for anyone else.
    /// `state`: `queued`, `delivered` (pulled, no result yet), `resulted` (with the host's result), `expired` or
    /// `revoked` (never delivered, so never run), `unknown` (delivered, then revoked before a result).
    pub fn readback(&self, subject: &str, operation: &str, now: i64) -> Result<Option<Value>, store::Error> {
        let Some(rec) = self.store.operation(operation)? else { return Ok(None) };
        if rec.actor != actor_id(subject) && self.store.role(&rec.workspace, subject)?.as_deref() != Some("admin") {
            return Ok(None);
        }
        let state = match (&rec.result, rec.delivered, rec.revoked) {
            (Some(_), _, _) => "resulted",
            (None, true, true) => "unknown",
            (None, true, false) => "delivered",
            (None, false, true) => "revoked",
            (None, false, false) if rec.op["expires_at"].as_i64().is_none_or(|x| now >= x) => "expired",
            (None, false, false) => "queued",
        };
        let op = &rec.op;
        Ok(Some(json!({"operation": op["operation"], "installation": op["installation"], "generation": op["generation"], "kind": op["payload"]["kind"],
            "expires_at": op["expires_at"], "digest": op["digest"], "authority": op["authority"], "state": state, "result": rec.result})))
    }
}

/// One AgentQ configure change: `profile` (its stable ID), `field` and `value` from `CHANGES`.
pub struct Change<'a> {
    pub profile: &'a str,
    pub field: &'a str,
    pub value: &'a str,
}

impl Change<'_> {
    fn wire(&self) -> Value {
        json!({"profile": self.profile, "field": self.field, "value": self.value})
    }
}

/// The confirmed AgentQ plan an apply carries.
pub struct Plan<'a> {
    pub revision: &'a str,
    pub digest: &'a str,
}

/// Opaque operation actor of a verified subject (coreq authority IDs are `[A-Za-z0-9._:-]`, 1-64): the subject
/// itself when it already is one, else `actor-` and 24 hex of its SHA-256.
pub fn actor_id(subject: &str) -> String {
    if subject.len() <= 64 && !subject.is_empty() && subject.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)) {
        return subject.into();
    }
    format!("actor-{}", hex(&sha256(subject)[..12]))
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
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n{head}<title>DashQ · Connect</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n</head>\n<body>\n<main class=\"connections connect-page\">\n<h1>Connect a product</h1>\n{body}</main>\n</body>\n</html>\n"
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
fn telemetry(p: &Value) -> Option<Option<Value>> {
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
fn telemetry_lines(p: &Value) -> Option<(String, Vec<Value>)> {
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

fn telemetry_envelope(p: &Value) -> bool {
    let Some(o) = p.as_object() else { return false };
    let n = |k: &str| o.get(k).and_then(Value::as_u64);
    let status = o.get("status").is_none_or(|s| {
        s.as_object().is_some_and(|s| s.iter().all(|(k, v)| ["backlog_rows", "backlog_bytes", "gap_rows"].contains(&k.as_str()) && (v.is_null() || v.is_u64())))
    });
    let (Some(first), Some(last)) = (cursor(&p["first"]), cursor(&p["last"])) else { return false };
    let (Some(rows), Some(bytes), Some(_)) = (n("rows"), n("bytes"), o.get("gzip").and_then(Value::as_str)) else { return false };
    !(p["schema"] != 1 || !status || o.keys().any(|k| !["schema", "first", "last", "rows", "bytes", "gzip", "status"].contains(&k.as_str())) || first.0 > last.0 || rows == 0 || bytes > TELEMETRY_BYTES)
}

/// TaskQ's allowlisted resource projection (taskq shared-resource-controls.md, Project overlay: Enrollment record).
pub const RESOURCES: &str = "taskq-resources/1";
/// The record is resent only with the board (every 300 s) while unchanged, so its status time ages that long between
/// sends. ponytail: two send intervals; take a producer-stated limit once taskq publishes one.
const RESOURCES_STALE_SECS: u64 = 600;

/// A `taskq-resources/1` payload: a `taskq.resources/1` projection whose every target belongs to the sending
/// installation and generation (else `scope`), metadata only (else `payload`).
fn resources(p: &Value, g: &store::Grant) -> Result<(), &'static str> {
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
fn project_limits(r: &store::Row, now: u64) -> Value {
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
fn limit_groups(r: &store::Row, p: &Value) -> Value {
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
fn unbase64(s: &str) -> Option<Vec<u8>> {
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
fn metadata(v: &Value, depth: usize) -> bool {
    metadata_keys(v, depth, field)
}

/// A telemetry line's key (#181, owner-approved): TaskQ request counters name `method:route` (`GET:issues.N`), so
/// `:` may join segments that each pass [`field`]; the deny list applies to every segment.
fn counter_field(k: &str) -> bool {
    k.len() <= 128 && k.split(':').all(field)
}

fn metadata_keys(v: &Value, depth: usize, key: fn(&str) -> bool) -> bool {
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
fn snapshot_metadata(schema: &str, p: &Value) -> bool {
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
fn strip_line(schema: &str, p: &mut Value) {
    if schema != "taskq-board/1" {
        return;
    }
    for t in p.get_mut("tasks").and_then(Value::as_array_mut).into_iter().flatten() {
        if let Some(s) = t.get_mut("session").and_then(Value::as_object_mut) {
            s.remove("line");
        }
    }
}

fn field(k: &str) -> bool {
    const DENY: [&str; 18] = [
        "prompt", "command", "cmd", "args", "stdout", "stderr", "output", "input", "content", "text", "message", "path", "cwd", "env", "token", "secret",
        "password", "credential",
    ];
    let k = k.to_ascii_lowercase();
    id(&k, 64) && !DENY.iter().any(|d| k == *d || k.ends_with(&format!("_{d}")))
}

/// An operation result (D16) is metadata one level deeper than a snapshot: AgentQ posts `{result, readback}`, and a
/// plan nests per-profile `changes` objects under `result`.
fn result_metadata(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.len() <= 256 && o.iter().all(|(k, x)| field(k) && metadata(x, if x.is_object() { 0 } else { 1 })))
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
            json!({"installation": r.installation, "product": r.product, "runtime": r.runtime, "host_label": r.label, "workspace": r.workspace, "telemetry": telemetry,
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

/// An installation row's `telemetry` (D11 #161, #181 A3): DashQ's retained range and its known loss, and the
/// sender's own counters from its newest block. DashQ never infers the sender's counters; missing ones are unknown.
fn telemetry_json(t: &store::Telemetry) -> Value {
    if t.blocks == 0 && t.pruned.is_empty() {
        return json!({"state": "unknown", "blocks": 0, "last_synced": null, "backlog_rows": null, "backlog_bytes": null, "gap_rows": null, "reason": "no telemetry block yet",
            "sender": {"state": "unknown", "reason": "no telemetry block yet"}, "retained": null, "pruned": [], "holes": [], "holes_total": 0, "hole_rows": 0, "restarts": 0});
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
        "holes_total": t.holes_total, "hole_rows": t.hole_rows, "restarts": t.restarts})
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
pub fn stage_records(e: &Result<Enroll, String>, rows: &Value, since: i64) -> Value {
    let Ok(e) = e else { return json!([]) };
    let mut memo = e.stages.lock().unwrap_or_else(|p| p.into_inner());
    let mut oldest = i64::MAX;
    let out: Vec<Value> = rows
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["product"] == "taskq" && r["scopes"].as_array().is_some_and(|s| s.iter().any(|x| x == TELEMETRY)))
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

/// The optional operation capabilities of a device request (coreq v0.11.0): 1-4 distinct names `product` owns, as
/// a JSON array; `Ok(None)` when absent, `Err` when malformed or another product's.
fn capabilities(b: &Value, product: &str) -> Result<Option<String>, ()> {
    let c = &b["capabilities"];
    if c.is_null() {
        return Ok(None);
    }
    let names: Vec<&str> = c.as_array().filter(|a| (1..=4).contains(&a.len())).and_then(|a| a.iter().map(Value::as_str).collect()).ok_or(())?;
    let known = |n: &&str| CAPABILITIES.iter().any(|(p, k, _)| *p == product && k == n);
    if !names.iter().all(known) || (1..names.len()).any(|i| names[..i].contains(&names[i])) {
        return Err(());
    }
    Ok(Some(json!(names).to_string()))
}

/// A request after approval may repeat its scope but never change it (A2); one without a scope is unchanged.
fn same_board(b: &Value, g: &store::Grant) -> bool {
    b["scope"].is_null() || board(b).ok().flatten().is_some_and(|x| g.board.as_deref() == Some(x.as_str()))
}

/// A `taskq-board/1` record names the granted board (`<kind>:<host>/<repo>`, taskq enrollment); without a granted
/// board it is outside the grant. Other schemas carry no board.
fn in_board(schema: &str, p: &Value, g: &store::Grant) -> bool {
    const BOARD: &str = "taskq-board/1";
    if schema != BOARD {
        return true;
    }
    let b = &p["board"];
    let (Some(kind), Some(host), Some(repo)) = (b["kind"].as_str(), b["host"].as_str(), b["repo"].as_str()) else { return false };
    g.board.as_deref().and_then(|x| serde_json::from_str::<Value>(x).ok()).is_some_and(|x| x == json!({"name": BOARD, "value": format!("{kind}:{host}/{repo}")}))
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
    let Ok(caps) = capabilities(b, product) else { return error("400 Bad Request", "invalid_request") };
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
        match e.store.pair(&sha256(&device), &code, &inst, &json!(schemas).to_string(), board.as_deref(), caps.as_deref(), now, MAX_PENDING) {
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
    let who = format!("<p class=\"detail\">Destination <strong>https://{}</strong></p><p>Signed in as <strong>{}</strong>.</p>\n", esc(&e.public_host), esc(subject));
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
        // each capability is its own consent: only a ticked box grants it (D16)
        let granted: Vec<&str> = CAPABILITIES.iter().map(|(_, k, _)| *k).filter(|k| get(&format!("capability.{k}")) == "grant").collect();
        let credential = b64url(&e.random(32));
        let sealer = |device: &[u8]| seal(e, device, &credential);
        return match e.store.decide(&code, approve, get("workspace"), subject, &granted, &sha256(&credential), &sealer, now) {
            Ok(Decision::Approved(g)) => {
                // C1.5: this page follows the first projected snapshot of this generation to the overview
                let wait = format!("/connect?wait={}&workspace={}&generation={}&since={now}", g.installation, g.workspace, g.generation);
                html_head(
                    "200 OK",
                    &format!("<meta http-equiv=\"refresh\" content=\"2; url={}\">\n", esc(&wait)),
                    &format!("{who}<p>Approved: installation <code>{}</code> is approved for workspace <strong>{}</strong>. Waiting for credential activation, then a durable snapshot and dashboard readback. Return to the command; it finishes on its own.</p>\n<p><a href=\"{}\">Waiting for its first snapshot…</a></p>\n", esc(&g.installation), esc(&g.workspace), esc(&wait)),
                )
            }
            Ok(Decision::Already) => html("200 OK", &format!("{who}<p>This code is already approved; nothing new was granted. Keep the existing host command running.</p><p><a href=\"/\">Open the overview and check pending readback</a></p>")),
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
    let entry = format!("<form method=\"get\" action=\"/connect\">\n<label>Code shown by the command <input name=\"code\" autocomplete=\"off\" autofocus required></label>\n<button>Continue</button>\n</form>\n");
    if code.is_empty() {
        let commands: String = PRODUCTS.iter().map(|p| format!("<p><code>{p} enroll https://{}</code></p>", esc(&e.public_host))).collect();
        return html("200 OK", &format!("{who}<h2>Add service / reconnect</h2><p>Run the matching command on the existing installation:</p><div class=\"connect-panel\">{commands}</div><p>Reuse the existing service and identity. A headless host prints the same approval link and code. A revoked connection needs new browser approval; lost identity creates a separate installation, never a hostname merge.</p><p>Retry an interrupted enrollment on that host with the same destination; do not start another approval while readback is pending.</p>{entry}<p><a href=\"/\">Back to overview</a></p>"));
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
    // requested capabilities are separate effects, unticked by default: approving the connection alone grants none
    let caps: String = p.capabilities.as_deref().and_then(|c| serde_json::from_str::<Vec<String>>(c).ok()).unwrap_or_default().iter().map(|c| {
        let effect = CAPABILITIES.iter().find(|(_, k, _)| k == c).map_or("", |(_, _, e)| e);
        format!("<dt>Also requested</dt><dd><label><input type=\"checkbox\" name=\"capability.{0}\" value=\"grant\"> Allow <code>{0}</code>: {1}</label></dd>\n", esc(c), esc(effect))
    }).collect();
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
            "{who}{replaces}<form method=\"post\" action=\"/connect\">\n<dl>\n<dt>Code</dt><dd><strong>{code}</strong>: check that the command shows the same code</dd>\n<dt>Product</dt><dd>{}</dd>\n<dt>Host</dt><dd>{} ({})</dd>\n<dt>Installation</dt><dd><code>{}</code></dd>\n<dt>Data</dt><dd>{}: observation only, no commands or task answers</dd>\n{scope}{caps}{ws}</dl>\n<input type=\"hidden\" name=\"code\" value=\"{code}\">\n<input type=\"hidden\" name=\"csrf\" value=\"{}\">\n<button name=\"decision\" value=\"approve\">Approve</button>\n<button name=\"decision\" value=\"deny\">Deny</button>\n</form>\n",
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
    if row.snapshot.as_ref().is_some_and(|s| s.generation == generation && renders(&row.product, s)) {
        let mut x = html("303 See Other", &format!("<p><a href=\"/#{to}\">Open the overview</a></p>\n"));
        x.headers = format!("Location: /#{to}\r\n");
        return x;
    }
    let stage = if row.connection == "pending" { "Waiting for credential activation" } else { "Waiting for the first snapshot" };
    let since: i64 = get("since").parse().unwrap_or(0);
    if (0..WAIT_SECS).contains(&(now - since)) {
        let again = format!("/connect?wait={inst}&workspace={ws}&generation={generation}&since={since}");
        return html_head(
            "200 OK",
            &format!("<meta http-equiv=\"refresh\" content=\"2; url={}\">\n", esc(&again)),
            &format!("{who}<p role=\"status\">Approved. {stage} from <strong>{} ({})</strong>; checked every 2 s for up to 60 s.</p>\n<p><a href=\"/#{to}\">Open the overview now</a></p>\n", esc(&row.label), esc(&row.runtime)),
        );
    }
    let retry = format!("/connect?wait={inst}&workspace={ws}&generation={generation}&since={now}");
    html("200 OK", &format!("{who}<p role=\"status\">No snapshot arrived within 60 s. {stage}; dashboard readback not confirmed.</p><p>Recovery uses the same connection. No new grant is needed; keep the host command running or retry it with the same destination.</p><p><a href=\"{}\">Check again for 60 s</a> · <a href=\"/#{to}\">Open the overview</a></p>", esc(&retry)))
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
        let (s, v) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"]}), now);
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
        /// Also ticks every requested operation capability at approval.
        tick: bool,
    }

    impl coreq::enroll::Wire for Wire<'_> {
        fn post(&mut self, destination: &str, route: &str, auth: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            assert_eq!(destination, "https://dash.example.com/machine");
            let route = &format!("/machine{route}");
            self.calls.push(route.into());
            Ok(machine(self.e, route, auth, body.clone(), self.now))
        }
        fn now(&self) -> u64 {
            self.now as u64
        }
        fn sleep(&mut self, secs: u64) {
            self.now += secs as i64;
            if let Some(code) = self.code.take() {
                let decision = if self.tick { "approve&capability.agentq.configure=grant" } else { "approve" };
                assert!(decide(self.e, "github|1", &code, decision, "ws-1", self.now).1.contains("Approved"));
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
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!((&a["generation"], &a["workspace"], &a["product"]), (&json!(1), &json!("ws-1"), &json!("agentq")));
        assert_eq!(w.calls, ["/machine/connect/device", "/machine/connect/token", "/machine/connect/token", "/machine/connect/ack"]);
        // repeating enrollment returns the active connection without new requests or grants
        coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(w.calls.len(), 4);
        let cred = a["credential"].as_str().unwrap();
        assert!(cred.len() >= 43, "256-bit credential");
        drop(e);
        let e = Enroll::open(&d, HOST);
        let c = e.as_ref().unwrap().store.credential(&sha256(cred)).unwrap().unwrap();
        assert!(c.active && !c.revoked && c.grant.account == "github|1");
        // revoke survives restart; the reply repeats so offline retries end
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 60), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        drop(e);
        let e = Enroll::open(&d, HOST);
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 61), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some("forged"), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "invalid_credential"})));
        assert_eq!(machine(&e, "/machine/connect/ack", None, json!({"protocol": 1}), T0 + 62).0, 401);
        assert!(e.as_ref().unwrap().store.authorize(&sha256(cred)).unwrap().is_none());
    }

    /// A `taskq-telemetry/1` block (taskq#1353 S3) of `lines`, with optional sender `status`.
    fn block(first: u64, lines: &[Value], status: Option<Value>) -> Value {
        use std::io::Write;
        let plain: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(plain.as_bytes()).unwrap();
        let mut p = json!({"schema": 1, "first": first, "last": first + lines.len() as u64 - 1, "rows": lines.len(), "bytes": plain.len(), "gzip": crate::base64(&z.finish().unwrap())});
        if let Some(s) = status {
            p["status"] = s;
        }
        p
    }

    /// #181: TaskQ `requests` rows count `method:route` keys; `:` passes in telemetry lines only, every segment
    /// still allowlisted, and stays refused in snapshot payloads.
    #[test]
    fn telemetry_counter_keys_allow_colon_only_in_lines() {
        let row = |k: &str| json!({"at_ms": 1, "op": "requests", "counts": {k: 3}});
        assert_eq!(telemetry(&block(1, &[row("GET:issues.N")], None)), Some(None));
        // taskq 4.6.31 still sends `GET:compare.:`; each bad line is dropped and counted, the block is kept
        for k in ["GET:text", "command:GET", "GET::x", ":GET", "GET:a/b", "GET:compare.:"] {
            let kept = telemetry(&block(1, &[row(k), row("GET:issues.N")], None)).flatten().unwrap_or_else(|| panic!("{k}"));
            assert_eq!((&kept["rows"], &kept["dropped"], &kept["last"]), (&json!(1), &json!(1), &json!(2)), "{k}");
        }
        assert!(!metadata(&json!({"counts": {"GET:issues.N": 3}}), 0));
    }

    /// The `/ingest` records taskq v4.6.30's own `telemetry_sync` posted (generator: fixture/taskq-telemetry/generate.rs).
    const TASKQ_BLOCKS: &str = include_str!("../fixture/taskq-telemetry/telemetry-sync-v4.6.30.json");

    // Chain: taskq's wire records -> /machine/ingest -> replay -> changed replay -> forged cursor -> grant without
    // telemetry -> projection -> readback by event id.
    // Risk (#181 A1-A3, A6): every real block refused for its `{seq, id}` cursors, a replay stored twice, a cursor
    // that misnames its rows accepted, rows the sender never counted as a gap left invisible, or a readback that
    // differs from what was accepted.
    #[test]
    fn taskq_v4_6_30_blocks_conform() {
        let f: Value = serde_json::from_str(TASKQ_BLOCKS).unwrap();
        let recs = f["records"].as_array().unwrap().clone();
        let now = recs[2]["observed_at"].as_i64().unwrap() + 60;
        let (_d, e) = setup("tq-wire");
        let mut w = Wire { e: &e, now, code: None, calls: vec![], tick: false };
        let pair = |w: &mut Wire, label: &str, schemas: &[&str]| {
            let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: label, schemas, scope: None, capabilities: &[] };
            coreq::enroll::pair(&dir(label), "https://dash.example.com/machine", &req, w).unwrap()
        };
        let a = pair(&mut w, "tq-wire-host", &[TELEMETRY]);
        let cred = a["credential"].as_str().unwrap();
        let (code, got) = ingest(&e, cred, recs.clone(), now);
        assert_eq!((code, states(&got)), (200, vec!["accepted".to_string(); 3]), "{got}");
        assert_eq!(states(&ingest(&e, cred, recs.clone(), now + 1).1), ["accepted"; 3], "a replay returns the stored receipts");
        let mut changed = recs[0].clone();
        changed["payload"] = recs[1]["payload"].clone();
        let mut forged = recs[1].clone();
        forged["event_id"] = json!("t-forged");
        forged["payload"]["first"]["id"] = json!("0".repeat(32));
        assert_eq!(states(&ingest(&e, cred, vec![changed, forged], now + 2).1), ["rejected:replay_mismatch", "rejected:payload"]);
        let off = pair(&mut w, "tq-wire-off", &["taskq-board/1"]);
        assert_eq!(states(&ingest(&e, off["credential"].as_str().unwrap(), vec![recs[0].clone()], now).1), ["rejected:scope"]);
        let p = projection(&e, Some(Scope::All), None, now as u64 + 3).unwrap();
        let t = p["rows"].as_array().unwrap().iter().find(|r| r["installation"] == a["installation"]).unwrap()["telemetry"].clone();
        assert_eq!((&t["state"], &t["retained"]["first_seq"], &t["retained"]["last_seq"], &t["retained"]["rows"], &t["retained"]["blocks"]), (&json!("ok"), &json!(1), &json!(9), &json!(7), &json!(3)));
        // taskq pruned seq 6-7 after its cursor yet reported gap 0: only DashQ's hole shows them
        assert_eq!((&t["holes"], &t["hole_rows"], &f["status_after"][2]["gap_rows"]), (&json!([{"from_seq": 6, "to_seq": 7, "rows": 2}]), &json!(2), &json!(0)));
        assert_eq!((&t["sender"]["state"], &t["sender"]["reason"]), (&json!("unknown"), &json!("status not reported by taskq")), "v4.6.30 sends no status");
        let id = recs[2]["event_id"].as_str().unwrap();
        let b = telemetry_block(&e, &p["rows"], id);
        assert_eq!((&b["state"], &b["blocks"][0]["payload"], &b["blocks"][0]["received_at"], &b["blocks"][0]["receipt"]), (&json!("ok"), &recs[2]["payload"], &json!(now), &json!("accepted")));
        assert_eq!(b["blocks"][0]["coverage"], t);
        assert_eq!(telemetry_block(&e, &json!([]), id)["state"], "unknown", "outside the caller's rows nothing is returned");
        assert_eq!(telemetry_block(&e, &p["rows"], "t-none")["reason"], "no stored block with this event id");
    }

    /// The coreq client whose next `lose` replies are lost after the server committed them.
    struct Lossy<'a, 'b> {
        w: &'b mut Wire<'a>,
        lose: usize,
    }

    impl coreq::enroll::Wire for Lossy<'_, '_> {
        fn post(&mut self, d: &str, route: &str, auth: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            let r = self.w.post(d, route, auth, body);
            if self.lose > 0 {
                self.lose -= 1;
                return Err("timeout".into());
            }
            r
        }
        fn now(&self) -> u64 {
            self.w.now()
        }
        fn sleep(&mut self, secs: u64) {
            self.w.sleep(secs)
        }
        fn show(&mut self, uri: &str, code: &str) {
            self.w.show(uri, code)
        }
    }

    /// #161 A2/A4/A5 with the coreq reference client: a granted block is stored once across a lost reply and a
    /// restart, replays return the stored receipt, blocks outside the grant or the allowlist are refused, the
    /// projection shows taskq's sync counters (unknown with a reason when missing), and a revoke stops delivery.
    #[test]
    fn taskq_telemetry_blocks_through_the_coreq_client() {
        let (d, e) = setup("telemetry");
        let host = dir("telemetry-host");
        let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: "mac-1", schemas: &[TELEMETRY], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let inst = a["installation"].as_str().unwrap().to_string();
        let row = |e: &Result<Enroll, String>| projection(e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap()["telemetry"].clone();
        let t = row(&e);
        assert_eq!((&t["state"], &t["blocks"], &t["backlog_rows"], &t["reason"], &t["sender"]["state"], &t["retained"]), (&json!("unknown"), &json!(0), &Value::Null, &json!("no telemetry block yet"), &json!("unknown"), &Value::Null));
        let stored = |e: &Result<Enroll, String>| e.as_ref().unwrap().store.write(|tx| Ok(tx.query_row("SELECT COUNT(*) FROM telemetry", [], |r| r.get::<_, i64>(0))?)).unwrap();
        let line = |n: u64| json!({"at_ms": n, "op": "service.pass", "ms": 12, "outcome": "ok"});
        let status = json!({"backlog_rows": 5, "backlog_bytes": 900, "gap_rows": 2});
        let r1 = coreq::enroll::stamp(&host, TELEMETRY, "t-1", T0 as u64, block(1, &[line(1), line(2)], Some(status.clone()))).unwrap();
        // lost reply: the server committed, the client retries the same record and gets the stored receipt
        let got = coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut Lossy { w: &mut w, lose: 1 });
        assert!(matches!(got, Err(coreq::enroll::Delivery::Retry(_))), "{got:?}");
        let ok = json!([{"event_id": "t-1", "state": "accepted"}]);
        assert_eq!(json!(coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut w).unwrap()), ok);
        assert_eq!(stored(&e), 1);
        // restart of both sides: the client resends from its cursor, nothing is stored twice
        drop(w);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let mut w = Wire { e: &e, now: T0 + 10, code: None, calls: vec![], tick: false };
        assert_eq!(json!(coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut w).unwrap()), ok);
        assert_eq!(coreq::enroll::receipt(&host, &r1, &mut w).unwrap(), "accepted");
        let r2 = coreq::enroll::stamp(&host, TELEMETRY, "t-2", T0 as u64 + 10, block(3, &[line(3)], None)).unwrap();
        // allowlist inside the block: a line with a content field is dropped alone and counted (#181); a bad block
        // and a changed replay are refused, never stored
        let bad = [
            coreq::enroll::stamp(&host, TELEMETRY, "t-cmd", T0 as u64, block(4, &[json!({"op": "x", "command": "rm"}), line(5)], None)).unwrap(),
            coreq::enroll::stamp(&host, TELEMETRY, "t-rows", T0 as u64, {
                let mut p = block(4, &[line(4)], None);
                p["rows"] = json!(2);
                p
            })
            .unwrap(),
            coreq::enroll::stamp(&host, TELEMETRY, "t-1", T0 as u64, block(1, &[line(9)], None)).unwrap(),
        ];
        let got: Vec<Value> = coreq::enroll::deliver(&host, &[vec![r2.clone()], bad.to_vec()].concat(), &mut Default::default(), &mut w).unwrap();
        assert_eq!(json!(got.iter().map(|r| format!("{}{}", r["state"].as_str().unwrap(), r["reason"].as_str().map(|x| format!(":{x}")).unwrap_or_default())).collect::<Vec<_>>()),
            json!(["accepted", "accepted", "rejected:payload", "rejected:replay_mismatch"]));
        assert_eq!(stored(&e), 3);
        let kept = telemetry_block(&e, &projection(&e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"], "t-cmd")["blocks"][0]["payload"].clone();
        assert_eq!((&kept["rows"], &kept["dropped"], &kept["first"], &kept["last"]), (&json!(1), &json!(1), &json!(4), &json!(5)));
        assert!(!kept.to_string().contains("rm") && unbase64(kept["gzip"].as_str().unwrap()).is_some());
        // the newest block's counters are taskq's own; a block without status leaves them unknown with the reason
        let t = row(&e);
        assert_eq!((&t["state"], &t["blocks"], &t["last_synced"], &t["backlog_rows"], &t["reason"]), (&json!("ok"), &json!(3), &json!(T0 + 10), &Value::Null, &json!("status not reported by taskq")));
        assert_eq!(t["pruned"], json!([{"cause": "allowlist", "blocks": 1, "rows": 1, "through_seq": 5}]));
        let r3 = coreq::enroll::stamp(&host, TELEMETRY, "t-3", T0 as u64 + 20, block(4, &[line(4)], Some(status))).unwrap();
        coreq::enroll::deliver(&host, std::slice::from_ref(&r3), &mut Default::default(), &mut w).unwrap();
        let t = row(&e);
        assert_eq!((&t["backlog_rows"], &t["backlog_bytes"], &t["gap_rows"], &t["reason"]), (&json!(5), &json!(900), &json!(2), &Value::Null));
        // outside the grant: another taskq connection without the schema
        let host2 = dir("telemetry-host2");
        let req2 = coreq::enroll::Request { installation: None, product: "taskq", host_label: "mac-2", schemas: &["taskq-board/1"], scope: None, capabilities: &[] };
        let a2 = coreq::enroll::pair(&host2, "https://dash.example.com/machine", &req2, &mut w).unwrap();
        let out = coreq::enroll::stamp(&host2, TELEMETRY, "t-x", T0 as u64, block(1, &[line(1)], None)).unwrap();
        assert_eq!(json!(coreq::enroll::deliver(&host2, &[out], &mut Default::default(), &mut w).unwrap()), json!([{"event_id": "t-x", "state": "rejected", "reason": "scope"}]));
        let off = projection(&e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"].as_array().unwrap().iter().find(|r| r["installation"] == a2["installation"]).unwrap()["telemetry"].clone();
        assert_eq!(off, json!({"state": "off", "reason": "telemetry not in grant"}));
        // revoke: the client stops at once, the old credential stores nothing more
        coreq::enroll::disconnect(&host, &mut w).unwrap();
        let r4 = json!({"schema": TELEMETRY, "event_id": "t-4", "observed_at": T0 + 30, "payload": block(5, &[line(5)], None)});
        assert!(matches!(coreq::enroll::deliver(&host, std::slice::from_ref(&r4), &mut Default::default(), &mut w), Err(coreq::enroll::Delivery::Stopped(_))));
        assert_eq!(ingest(&e, a["credential"].as_str().unwrap(), vec![r4], T0 + 30), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(stored(&e), 4);
    }

    /// Chain (#182 A1–A6): TaskQ stage rows in telemetry blocks -> coreq client -> /machine/ingest allowlist -> store
    /// -> [`stage_records`] -> `/data.agent_work` with a cutover log. Risk: stage rows dropped at ingest, nested stages
    /// or a log/telemetry overlap counted twice, elapsed time shown for open, adopted or gapped attempts, a
    /// cross-host attempt split in two, an unlinked record linked by guess.
    #[test]
    fn taskq_stage_replay_through_ingest() {
        let fx: Value = serde_json::from_str(include_str!("../fixture/taskq-telemetry/stages.json")).unwrap();
        let t0 = fx["t0"].as_i64().unwrap();
        let (_d, e) = setup("stages");
        let mut w = Wire { e: &e, now: t0, code: None, calls: vec![], tick: false };
        for (n, inst) in fx["installations"].as_array().unwrap().iter().enumerate() {
            let host = dir(&format!("stages-host{n}"));
            let label = inst["label"].as_str().unwrap();
            let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: label, schemas: &[TELEMETRY], scope: None, capabilities: &[] };
            coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
            let recs: Vec<Value> = inst["blocks"].as_array().unwrap().iter().enumerate().map(|(i, b)| {
                let lines = b.as_array().unwrap();
                let mut p = block(lines[0]["seq"].as_u64().unwrap(), lines, None);
                p["last"] = lines[lines.len() - 1]["seq"].clone();
                coreq::enroll::stamp(&host, TELEMETRY, &format!("t-{label}-{i}"), t0 as u64, p).unwrap()
            }).collect();
            let got = coreq::enroll::deliver(&host, &recs, &mut Default::default(), &mut w).unwrap();
            assert!(got.iter().all(|r| r["state"] == "accepted"), "every stage row passes the allowlist: {got:?}");
        }
        let now = t0 as u64 + 60;
        let p = projection(&e, Some(Scope::All), None, now).unwrap();
        let stages = stage_records(&e, &p["rows"], t0 - 2 * 86400);
        assert_eq!(stages.as_array().unwrap().iter().map(|s| s["records"].as_array().unwrap().len()).sum::<usize>(), 19, "18 stages and one queue transition");
        // decoded once: a second read is served from the per-block memo, identical
        assert_eq!(stage_records(&e, &p["rows"], t0 - 2 * 86400), stages);
        let log: Vec<Value> = fx["log"]["mac"].as_array().unwrap().iter().filter_map(|l| serde_json::from_str(l.as_str()?.strip_prefix("taskq-stage ")?).ok()).collect();
        let snap = json!({"value": {"records": log, "corrupt": 0}, "at": now, "error": null, "incomplete": false});
        let inp = json!({"now": now, "period": 7200, "filters": {}, "projects": ["taskqorg/dashq"], "agent_telemetry": stages, "agent_work": {"mac": snap}});
        let a = crate::agent_work::build(&inp);
        let hosts: Vec<(&str, &str)> = a["hosts"].as_array().unwrap().iter().map(|h| (h["host"].as_str().unwrap(), h["source"]["state"].as_str().unwrap())).collect();
        assert_eq!(hosts, [("mac", "incomplete"), ("win", "ok")], "{}", a["hosts"]);
        assert_eq!((&a["hosts"][0]["source"]["reason"], &a["hosts"][0]["provenance"]), (&json!("2 telemetry rows not received"), &json!(["log", "telemetry"])));
        let by = |task: u64| a["attempts"].as_array().unwrap().iter().find(|t| t["task"] == task).unwrap().clone();
        let seen = |task: u64| { let t = by(task); (t["state"].as_str().unwrap().to_string(), t["reason"].as_str().map(String::from), t["elapsed_ms"].as_f64()) };
        assert_eq!(a["attempts_total"], 7);
        assert_eq!(seen(101), ("complete".into(), None, Some(670_000.0)), "nested start inside build counts once (stage sum 672 s)");
        assert_eq!(seen(102), ("partial".into(), Some("rows not received".into()), None));
        assert_eq!(seen(103), ("open".into(), Some("open-stage".into()), None));
        assert_eq!(seen(201), ("adopted".into(), Some("adopted".into()), None));
        assert_eq!(seen(301), ("complete".into(), None, Some(603_000.0)));
        assert_eq!((&by(301)["hosts"], &by(301)["runtimes"]), (&json!(["mac", "win"]), &json!(["codex"])), "one cross-host attempt, one row");
        let (t101, t401) = (by(101), by(401));
        assert_eq!((&t101["link"], &t101["project"], &t101["attempt"]), (&json!("https://github.com/taskqorg/dashq/issues/101"), &json!("taskqorg/dashq"), &json!(1001)));
        assert_eq!((&t401["link"], &t401["project"], &t401["hosts"], &t401["elapsed_ms"]), (&Value::Null, &Value::Null, &json!(["mac"]), &json!(101_000.0)), "no project: unlinked; no host: label");
        assert_eq!(seen(90), ("complete".into(), None, Some(100_000.0)), "the log fills the cutover");
        // #184 A2: wall time, summed overlapping execution and the queue wait stay apart; a missing wait says why
        assert_eq!((&t101["execution_ms"], &t101["queue_wait_s"], &t101["queue_wait_reason"]), (&json!(672_000.0), &json!(240), &Value::Null));
        assert_eq!((&by(102)["execution_ms"], &by(102)["queue_wait_s"], &by(102)["queue_wait_reason"]), (&Value::Null, &Value::Null, &json!("ready transition not retained")));
        assert_eq!((&t101["models"]["ids"], &t101["output_tokens"]["sum"], &t101["estimate"]["amounts"]), (&json!({"claude-opus-5-5": 1}), &json!(7000), &Value::Null), "stage tokens carry no cache split: no estimate");
        assert_eq!((&t101["tokens"], &by(102)["tokens"]["reasons"]), (&json!({"sum": 900_000, "known": 1, "of": 1, "reasons": []}), &json!(["no-usage"])));
        let row = |host: &str, rt: &str| a["rows"].as_array().unwrap().iter().find(|r| r["host"] == host && r["runtime"] == rt).unwrap().clone();
        let mc = row("mac", "claude");
        let claim = mc["stages"].as_array().unwrap().iter().find(|s| s["stage"] == "claim").unwrap().clone();
        assert_eq!(claim["count"], 5, "the claim of 1001 in both log and telemetry counts once");
        assert_eq!((&mc["time_ms"], &mc["elapsed"]), (&json!(871_000.0), &json!({"known": 3, "of": 5, "reasons": ["open-stage", "rows not received"]})));
        assert_eq!(mc["tokens"]["sum"], 908_000, "context tokens (cache included) summed once per build record");
        assert_eq!(row("win", "codex")["elapsed"], json!({"known": 1, "of": 2, "reasons": ["adopted"]}));
        let mut html = String::new();
        crate::agent_work::render(&mut html, &json!({"agent_work": a, "generated_at": now}));
        assert!(html.contains("<a href=\"https://github.com/taskqorg/dashq/issues/101\">taskqorg/dashq#101</a>") && html.contains("title=\"unknown: adopted\">—</span> <small>adopted</small>") && html.contains("partial: rows not received") && html.contains(">#401<"), "{html}");
        let solo = crate::agent_work::build(&json!({"now": now, "period": 7200, "filters": {"host": "win"}, "agent_telemetry": stages}));
        assert_eq!((solo["hosts"].as_array().unwrap().len(), &solo["source"]["state"]), (1, &json!("ok")), "host filter; telemetry alone configures the block");
    }

    #[test]
    fn polling_lost_replies_and_expiry() {
        let (d, e) = setup("poll");
        assert_eq!(machine(&e, "/machine/connect/device", None, json!({"protocol": 2}), T0), (400, json!({"protocol": 1, "error": "unsupported_protocol"})));
        let (device, code) = start(&e, "inst-1", T0);
        let tok = |e: &Result<Enroll, String>, now| machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now);
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
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 41), (200, json!({"protocol": 1, "state": "active", "generation": 1})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 42).0, 200);
        assert_eq!(tok(&e, T0 + 43).1["error"], "invalid_grant");
        let sealed: Option<Vec<u8>> = rusqlite::Connection::open(d.join("dashq.db")).unwrap().query_row("SELECT sealed FROM pairings", [], |r| r.get(0)).unwrap();
        assert_eq!(sealed, None, "erased at acknowledgement");
        // approved but never acknowledged: expiry erases the token response
        let (device2, code2) = start(&e, "inst-2", T0 + 100);
        decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 110);
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 700).1["error"], "expired_token");
        let (_, code3) = start(&e, "inst-3", T0 + 800);
        assert!(decide(&e, "github|1", &code3, "approve", "ws-1", T0 + 1400).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": "nope"}), T0).1["error"], "invalid_grant");
    }

    #[test]
    fn configure_capability_needs_its_own_approval_and_never_widens() {
        // D16 / coreq v0.11.0: a requested operation capability is a separate, unticked effect on the approval page;
        // the token names only the approved subset, and the coreq client keeps only what it asked for and got
        let (_d, e) = setup("capability");
        let req = |inst: &str, product: &str, caps: Value| json!({"protocol": 1, "product": product, "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": [format!("{product}-snapshot/1")], "capabilities": caps});
        for (product, caps) in [("serverq", json!(["agentq.configure"])), ("agentq", json!(["agentq.run"])), ("agentq", json!(["agentq.configure", "agentq.configure"])), ("agentq", json!([])), ("agentq", json!("agentq.configure"))] {
            assert_eq!(machine(&e, "/machine/connect/device", None, req("inst-x", product, caps.clone()), T0), (400, json!({"protocol": 1, "error": "invalid_request"})), "{product} {caps}");
        }
        let token = |inst: &str, ticks: &str| {
            let (_, v) = machine(&e, "/machine/connect/device", None, req(inst, "agentq", json!(["agentq.configure"])), T0);
            let (device, code) = (v["device_code"].as_str().unwrap().to_string(), v["user_code"].as_str().unwrap().to_string());
            let p = page(&e, "github|1", &code, T0).1;
            assert!(p.contains("<input type=\"checkbox\" name=\"capability.agentq.configure\" value=\"grant\"> Allow <code>agentq.configure</code>"), "{p}");
            assert!(decide(&e, "github|1", &code, &format!("approve{ticks}"), "ws-1", T0 + 1).1.contains("Approved"));
            machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 10).1
        };
        // approving the connection alone grants no capability; a forged extra capability is ignored
        assert_eq!(token("inst-1", "&capability.agentq.run=grant")["capabilities"], json!([]));
        assert_eq!(token("inst-2", "&capability.agentq.configure=grant")["capabilities"], json!(["agentq.configure"]));
        // a request without capabilities keeps today's reply shape and page
        let (device, code) = start(&e, "inst-3", T0);
        assert!(!page(&e, "github|1", &code, T0).1.contains("Also requested"));
        decide(&e, "github|1", &code, "approve&capability.agentq.configure=grant", "ws-1", T0 + 1);
        assert!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 10).1.get("capabilities").is_none());
        // the shared client: approval without the tick leaves the active grant without the capability
        let host = dir("capability-host");
        let r = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &["agentq.configure"] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        assert_eq!(coreq::enroll::pair(&host, "https://dash.example.com/machine", &r, &mut w).unwrap()["capabilities"], json!([]));
    }

    #[test]
    fn board_scope_is_shown_granted_named_and_never_changed() {
        let (_d, e) = setup("board");
        let scope = json!({"name": "taskq-board/1", "value": "taskqorg/taskq"});
        let req = |inst: &str, scope: Value| json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"], "scope": scope});
        for bad in [json!({"name": "taskq-board"}), json!({"name": "board", "value": "x"}), json!({"name": "taskq-board/1", "value": "a\nb"}), json!("taskq-board/1")] {
            assert_eq!(machine(&e, "/machine/connect/device", None, req("inst-x", bad.clone()), T0).1["error"], "invalid_request", "{bad}");
        }
        // A1: the page names the scope the device requested; a request without one renders as before
        let (_, v) = machine(&e, "/machine/connect/device", None, req("inst-1", scope.clone()), T0);
        let (device, code) = (v["device_code"].as_str().unwrap(), v["user_code"].as_str().unwrap());
        assert!(page(&e, "github|1", code, T0).1.contains("<dt>Board scope</dt><dd><code>taskq-board/1</code>: <code>taskqorg/taskq</code></dd>"));
        assert!(!page(&e, "github|1", &start(&e, "inst-2", T0).1, T0).1.contains("Board scope"));
        // A2: the grant keeps the approved scope and token, status and receipt replies name it
        assert!(decide(&e, "github|1", code, "approve", "ws-1", T0 + 5).1.contains("Approved"));
        let tok = |scope: Value| machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device, "scope": scope}), T0 + 10);
        assert_eq!(tok(json!({"name": "taskq-board/1", "value": "taskqorg/other"})), (403, json!({"protocol": 1, "error": "scope"})));
        let (s, t) = tok(scope.clone());
        assert_eq!((s, &t["scope"]), (200, &scope));
        let cred = t["credential"].as_str().unwrap();
        let (s, a) = machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 11);
        assert_eq!((s, &a["scope"]), (200, &scope));
        let other = json!({"protocol": 1, "scope": {"name": "taskq-board/1", "value": "taskqorg/other"}, "records": [rec("e1", T0, json!({"n": 1}))]});
        assert_eq!(machine(&e, "/machine/ingest", Some(cred), other, T0 + 12), (403, json!({"protocol": 1, "error": "scope"})));
        assert_eq!(ingest(&e, cred, vec![rec("e1", T0, json!({"n": 1}))], T0 + 12).0, 200);
        let (s, r) = machine(&e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": "e1", "scope": scope}), T0 + 13);
        assert_eq!((s, &r["scope"]), (200, &scope));
        // revoke always ends the connection
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1, "scope": {"name": "x/1", "value": "y"}}), T0 + 14).1["state"], "revoked");
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
        let forged = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 5);
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
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 20).1["error"], "access_denied");
        let (device2, code2) = start(&e, "inst-2", T0);
        for _ in 0..5 {
            decide(&e, "github|2", &code2, "approve", "ws-1", T0 + 10);
        }
        assert!(decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 11).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 20).1["error"], "invalid_grant");
        assert!(s.credential(b"none").unwrap().is_none());
    }

    #[test]
    fn setup_limits_and_methods() {
        let d = dir("setup");
        let none = Enroll::open(&d, HOST);
        assert!(none.is_err());
        assert_eq!(call(&none, "POST", "/machine/connect/device", &[], "{}", T0).0, "503 Service Unavailable");
        assert!(Enroll::admin(&d, "ws 1", "x").is_err() && Enroll::admin(&d, "ws", "a\nb").is_err());
        let (d, e) = setup("limits");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(d.join("dashq.key")).unwrap().permissions()) & 0o777, 0o600);
        assert_eq!(call(&e, "GET", "/machine/connect/token", &[], "", T0).0, "405 Method Not Allowed");
        assert!(route(&e, &Req { method: "GET", path: "/data", query: "", headers: &[], body: b"" }, T0).is_none());
        // the old unprefixed machine paths are gone, no aliases (#110): the page router answers 404
        for p in MACHINE.map(|p| p.strip_prefix("/machine").unwrap()) {
            assert!(route(&e, &Req { method: "POST", path: p, query: "", headers: &[], body: b"{}" }, T0).is_none(), "{p}");
        }
        let dev = |ip: &str, inst: &str| {
            let h = [(CLIENT, ip)];
            call(&e, "POST", "/machine/connect/device", &h, &json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "wsl", "host_label": "pc", "schemas": ["agentq-snapshot/1"]}).to_string(), T0)
        };
        for i in 0..10 {
            assert_eq!(dev("203.0.113.1", &format!("i{i}")).0, "200 OK");
        }
        let (s, h, v) = dev("203.0.113.1", "i10");
        assert_eq!((s.as_str(), h.as_str(), &v["error"]), ("429 Too Many Requests", "Retry-After: 60\r\n", &json!("rate_limited")));
        assert_eq!(dev("203.0.113.2", "i11").0, "200 OK");
        // scope: a product requests only its own schemas
        let bad = json!({"protocol": 1, "product": "serverq", "installation": "s1", "runtime": "native", "host_label": "pc", "schemas": ["agentq-snapshot/1"]});
        assert_eq!(machine(&e, "/machine/connect/device", None, bad, T0).1["error"], "invalid_request");
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
        // one prefix for every machine route, the resource exchange (D14) included (#110)
        assert_eq!(paths, ["/machine/*"]);
        assert!(MACHINE.iter().chain(&[crate::policy::EXCHANGE]).all(|p| p.starts_with("/machine/")));
        assert!(head.contains("method POST") && !head.contains("/data"));
        assert!(guarded.contains("copy_headers") && guarded.contains(">X-Dashq-Subject") && guarded.contains("reverse_proxy 127.0.0.1:18882"));
    }

    /// Approved and acknowledged connection of `inst` in `ws` as `subject`; returns the credential.
    fn connect(e: &Result<Enroll, String>, inst: &str, subject: &str, ws: &str, now: i64) -> String {
        let (device, code) = start(e, inst, now);
        assert!(decide(e, subject, &code, "approve", ws, now).1.contains("Approved"));
        let cred = machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        cred
    }

    fn rec(id: &str, observed: i64, payload: Value) -> Value {
        json!({"schema": "agentq-snapshot/1", "event_id": id, "observed_at": observed, "payload": payload})
    }

    fn ingest(e: &Result<Enroll, String>, cred: &str, records: Vec<Value>, now: i64) -> (u16, Value) {
        machine(e, "/machine/ingest", Some(cred), json!({"protocol": 1, "records": records}), now)
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

    /// coreq wire v1 conformance traces (fixture/enrollment: coreq v0.7.0 tests/enrollment, unchanged; agentq-operations
    /// from v0.11.0) run against these handlers; `{"server": "project"}` is the dashboard projection pass and
    /// `{"server": "queue"}` queues through the store's own authority check.
    #[test]
    fn coreq_conformance_traces() {
        let traces = [
            include_str!("../fixture/enrollment/agentq-connect.json"),
            include_str!("../fixture/enrollment/agentq-pairing.json"),
            include_str!("../fixture/enrollment/agentq-reject.json"),
            include_str!("../fixture/enrollment/serverq-connect.json"),
            include_str!("../fixture/enrollment/agentq-operations.json"),
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
            let mut now = 0;
            for (i, step) in trace["steps"].as_array().unwrap().iter().enumerate() {
                let at_s = format!("{} step {i}", trace["about"]);
                now = step["at"].as_i64().unwrap_or(now);
                if step["server"] == "project" {
                    e.as_ref().unwrap().project(Scope::All).unwrap();
                } else if step["server"] == "queue" {
                    // the queuing account is the authority's actor; DashQ checks it against its own grants and members
                    let op = &step["operation"];
                    let got = e.as_ref().unwrap().store.queue(op["authority"]["actor"].as_str().unwrap(), op, now).unwrap();
                    assert_eq!(if got == store::Queued::Queued { "queued" } else { "refused" }, step["result"], "{at_s}: {got:?}");
                } else if let Some(action) = step["browser"].as_str() {
                    let s = subst(step, &vars);
                    // a trace approval approves what the device requested: here that is ticking each capability
                    let action = if action == "approve" { "approve&capability.agentq.configure=grant" } else { action };
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

    /// AgentQ's published snapshots, with and without the profile projection (agentq#101), pass the allowlist whole.
    #[test]
    fn agentq_profile_snapshots_are_accepted() {
        let (_d, e) = setup("agentq-profiles");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", T0);
        let cases: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        let records: Vec<Value> = cases["cases"].as_object().unwrap().iter().map(|(k, p)| rec(k, T0, p.clone())).collect();
        let n = records.len();
        assert_eq!(states(&ingest(&e, &cred, records, T0).1), vec!["accepted"; n]);
    }

    #[test]
    fn agentq_events_enter_the_outbox_beside_snapshots() {
        let (_d, e) = setup("events");
        let (s, v) = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": "inst-1", "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1", "agentq-event/1"]}), T0);
        assert_eq!(s, 200, "{v}");
        assert!(decide(&e, "github|1", v["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": v["device_code"]}), T0).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
        let ev = |id: &str, p: Value| json!({"schema": "agentq-event/1", "event_id": id, "observed_at": T0, "payload": p});
        let records = vec![
            ev("r1", json!({"name": "agentq.run", "metadata": {"exit": 0}})),
            ev("r2", json!({"name": "agentq.unknown", "metadata": {}})),
            ev("r3", json!({"name": "agentq.run", "metadata": {"prompt": "hi"}})),
            rec("s1", T0, json!({"version": "1.0.0"})),
            // OTLP cannot time it: refused before acknowledgement, never an overflow in the forwarder (#37)
            json!({"schema": "agentq-event/1", "event_id": "r4", "observed_at": i64::MAX, "payload": {"name": "agentq.run", "metadata": {}}}),
        ];
        assert_eq!(states(&ingest(&e, &cred, records, T0).1), ["accepted", "rejected:payload", "rejected:payload", "accepted", "rejected:payload"]);
        let store = &e.as_ref().unwrap().store;
        assert_eq!(store.outbox(9).unwrap().iter().map(|r| r.event_id.as_str()).collect::<Vec<_>>(), ["r1"]);
        // an export never becomes the snapshot or a history sample
        assert_eq!(store.project(Scope::All, renders).unwrap()[0].snapshot.as_ref().unwrap().event_id, "s1");
        assert_eq!(store.history("inst-1").unwrap().len(), 1);
    }

    #[test]
    fn board_records_outside_the_granted_board_are_refused() {
        // Chain: taskq board grant -> ingest -> projection. Risk: a record of another board (taskq#913 probe:
        // taskqorg/coreq under a taskqorg/taskq grant) is stored and shown under this grant.
        let (_d, e) = setup("board-records");
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/taskq"});
        let req = |inst: &str, scope: Value| json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["taskq-board/1"], "scope": scope});
        let join = |inst: &str, scope: Value| {
            let v = machine(&e, "/machine/connect/device", None, req(inst, scope), T0).1;
            assert!(decide(&e, "github|1", v["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
            let t = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": v["device_code"]}), T0).1;
            let cred = t["credential"].as_str().unwrap().to_string();
            assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
            cred
        };
        let board = |id: &str, repo: &str| json!({"schema": "taskq-board/1", "event_id": id, "observed_at": T0, "payload": {"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": repo}}});
        let cred = join("inst-t", scope);
        let (s, v) = ingest(&e, &cred, vec![board("in", "taskqorg/taskq"), board("out", "taskqorg/coreq"), json!({"schema": "taskq-board/1", "event_id": "none", "observed_at": T0, "payload": {"n": 1}})], T0);
        assert_eq!((s, states(&v)), (200, vec!["accepted".to_string(), "rejected:scope".into(), "rejected:scope".into()]), "{v}");
        let rows = e.as_ref().unwrap().project(Scope::All).unwrap();
        assert_eq!(rows.iter().map(|r| r.snapshot.as_ref().map(|s| s.event_id.as_str())).collect::<Vec<_>>(), [Some("in")]);
        // a grant without a board covers no board record
        let bare = join("inst-u", Value::Null);
        assert_eq!(states(&ingest(&e, &bare, vec![board("x", "taskqorg/taskq")], T0).1), ["rejected:scope"]);
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
            json!({"a": {"b": {"c": {"d": {"e": [[1]]}}}}}),
            json!({"a": {"b": {"c": {"d": {"e": [{"f": 1}]}}}}}),
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
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1}), T0 + 6).0, 400);
        // per-device rate: burst 10, then 429 with Retry-After
        let codes: Vec<u16> = (0..12).map(|_| ingest(&e, &cred, vec![], T0 + 100).0).collect();
        assert_eq!(codes, [[200; 10].as_slice(), &[429, 429]].concat());
        let (s, h, _) = call(&e, "POST", "/machine/ingest", &[("authorization", &format!("Bearer {cred}"))], r#"{"protocol": 1, "records": []}"#, T0 + 100);
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
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&other), json!({"protocol": 1, "event_id": "a-1"}), T0).0, 404);
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&admin), json!({"protocol": 1, "event_id": "a-1"}), T0).1["state"], "projected");
        // revoked: kept as revoked, not dropped; a new approval into another workspace starts empty there
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(&admin), json!({"protocol": 1}), T0).0, 200);
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
        let cfg = crate::Config { prices: Value::Null, langfuse, forward: false, links: json!({}), github_token: None, projects: vec![], taskq_bin: None, serverq_report: "/nonexistent/report.json".into(), serverq_host: "fra/native".into(), public_host: HOST.into(), answer: None, task_controls: None, resources: false, agentq_configure: false, taskq_logs: vec![], limits: crate::store::LIMITS };
        crate::App { cfg, cache: crate::source::Cache::default(), enroll: e, answers: Default::default(), resources: Default::default(), export: Default::default() }
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
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now, code: None, calls: vec![], tick: false };
        let conn = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
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
        let mut w = Wire { e: &app.enroll, now, code: None, calls: vec![], tick: false };
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

    /// Approved and acknowledged connection of a standalone product installation; returns the credential.
    fn join(e: &Result<Enroll, String>, product: &str, inst: &str, runtime: &str, label: &str, schema: &str, now: i64) -> String {
        let (s, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": product, "installation": inst, "runtime": runtime, "host_label": label, "schemas": [schema]}), now);
        assert_eq!(s, 200, "{d}");
        assert!(decide(e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        cred
    }

    /// Enrollment E1/E6 for TaskQ (#125): a schema 1 `taskq-board/1` observation, ask-card choice text included, is a
    /// supported versions row, its board summary renders in Connections and its receipt reads `projected`; another
    /// payload schema stays `accepted` and its versions row unsupported. Risk: the D11 choice-text exception widens to
    /// other fields or schemas, or refuses the canonical TaskQ observation.
    #[test]
    fn taskq_board_snapshots_render_and_unknown_schemas_stay_unsupported() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("taskq");
        let schema = "taskq-board/1";
        let join_board = |inst: &str| {
            let scope = json!({"name": schema, "value": "github:github.com/taskqorg/taskq"});
            let d = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "win", "schemas": [schema], "scope": scope}), now).1;
            assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
            let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
            assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
            cred
        };
        let (good, v2) = (join_board("tq-win"), join_board("tq-v2"));
        let board = json!({"kind": "github", "host": "github.com", "repo": "taskqorg/taskq"});
        let choices = json!([{"number": 1, "text": "Approve narrowly scoped choice content in D11 ★", "recommended": true}, {"number": 2, "text": "Defer choice support", "recommended": false}]);
        let observation = |tasks: Value| json!({"schema": 1, "installation": "tq-win", "board": board, "observed": now, "complete": true, "reason": null, "totals": {"doing": 2, "ask": 1}, "tasks": tasks, "observations": [
            {"host":"win","kind":"online","state":"online","producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+290,"complete":true,"reason":null},
            {"host":"win","kind":"login","state":"logged in","claude":"logged in","codex":"logged in","producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+590,"complete":true,"reason":null},
            {"host":"win","kind":"queue","state":"busy","running":1,"queued":2,"producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+290,"complete":true,"reason":null}]});
        let record = |id: &str, payload: Value| json!({"schema": schema, "event_id": id, "observed_at": now, "payload": payload});
        // text outside `tasks[].choices[].text`, or choice text that is not short single-line metadata, stays refused
        let refused = [
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "text": "raw card", "choices": choices}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "message": "x", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "two
lines", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "/etc/passwd", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "x".repeat(257), "recommended": true}]}])),
            json!({"schema": 1, "board": board, "text": "free text", "tasks": []}),
        ];
        let records = refused.into_iter().enumerate().map(|(i, p)| record(&format!("bad-{i}"), p)).collect();
        assert_eq!(states(&ingest(&e, &good, records, now).1), vec!["rejected:payload"; 6]);
        for (cred, id, payload) in [
            (&good, "b-1", observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": choices}, {"n": 8, "state": "doing", "revision": 4, "choices": []}]))),
            (&v2, "b-2", json!({"schema": 2, "board": board, "tasks": []})),
        ] {
            let (s, v) = ingest(&e, cred, vec![record(id, payload)], now);
            assert_eq!((s, states(&v)), (200, vec!["accepted".to_string()]), "{v}");
        }
        // the exception is TaskQ's: the same choice shape in another schema is refused
        let agent = join(&e, "agentq", "aq-1", "native", "mac", "agentq-snapshot/1", now);
        let (_, v) = ingest(&e, &agent, vec![json!({"schema": "agentq-snapshot/1", "event_id": "a-1", "observed_at": now, "payload": {"tasks": [{"choices": choices}]}})], now);
        assert_eq!(states(&v), ["rejected:payload"]);
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let receipt = |cred: &str, id: &str| machine(&app.enroll, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        assert_eq!((receipt(&good, "b-1"), receipt(&v2, "b-2")), (json!("projected"), json!("accepted")));
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "tq-win").unwrap().clone();
        assert_eq!(row["snapshot"]["payload"]["tasks"][0]["choices"], choices);
        let host_observations = |d: &Value| d["hosts"].as_array().unwrap().iter().find(|h|h["id"]=="win").unwrap()["taskq"]["observations"].clone();
        let obs = host_observations(&doc);
        assert_eq!(obs.as_array().unwrap().len(),3,"stored observations reach /data without optional sources");
        assert!(obs.as_array().unwrap().iter().any(|o|o["kind"]=="online" && o["state"]=="online" && o["valid_until"]==now+290));
        let report = app.doc(1800,json!({}),Some(Scope::All));
        assert_eq!(host_observations(&report),obs,"local report and HTTP share the projection");
        let scoped = data(&app,"host=win",Some(Scope::All)).1;
        assert_eq!(host_observations(&scoped),obs,"host filter retains board observations");
        assert!(data(&app,"",None).1["hosts"].as_array().unwrap().iter().all(|h|h["taskq"]["observations"].as_array().unwrap().is_empty()),"no observation escapes browser scope");
        let html = crate::page::render(&doc,&json!({}));
        assert!(html.contains("valid until") && html.contains("aaaaaaaaaaaa") && html.contains("logged in"));
        if let Some(dir) = std::env::var_os("DASHQ_HOST_OBSERVATIONS_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("page.html"),html).unwrap();
        }

        let ver = |inst: &str| doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap().clone();
        assert_eq!((ver("tq-win")["provenance"].as_str(), ver("tq-win")["observation"]["state"].as_str(), ver("tq-win")["reason"].as_str()), (Some("enrollment"), Some("ok"), None));
        assert_eq!((ver("tq-v2")["provenance"].as_str(), ver("tq-v2")["reason"].as_str()), (Some("unsupported"), Some("unsupported snapshot schema taskq-board/1")));
        let html = crate::page::render(&doc, &json!({}));
        assert!(html.contains("Board github:github.com/taskqorg/taskq · doing 2 · ask 1 · 2 tasks observed"), "board summary");
    }

    /// #180 A2/A3: a `taskq-board/1` task `session` keeps host, runtime, stage and time; its worker `line` is dropped
    /// before storing, a retry of the same record keeps its receipt, and a snapshot stored with a line before #180
    /// projects without it. Risk: worker text reaches the store, `/data`, `dashq report` or the page.
    #[test]
    fn taskq_session_line_is_never_stored_or_projected() {
        let now = crate::source::now() as i64;
        let (d, e) = setup("taskq-line");
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/taskq"});
        let dev = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": "tq-1", "runtime": "native", "host_label": "win", "schemas": ["taskq-board/1"], "scope": scope}), now).1;
        assert!(decide(&e, "github|1", dev["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": dev["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let session = json!({"host": "win", "runtime": "codex", "stage": "doing", "at": now});
        let mut with_line = session.clone();
        with_line["line"] = json!("Running tests in the worker");
        let payload = |s: &Value| json!({"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": "taskqorg/taskq"}, "complete": true, "totals": {"doing": 1},
            "tasks": [{"n": 7, "state": "doing", "revision": 1, "session": s}]});
        let rec = json!({"schema": "taskq-board/1", "event_id": "b-1", "observed_at": now, "payload": payload(&with_line)});
        assert_eq!(states(&ingest(&e, &cred, vec![rec.clone()], now).1), ["accepted"]);
        let db = rusqlite::Connection::open(d.join("dashq.db")).unwrap();
        let stored: String = db.query_row("SELECT payload FROM snapshots WHERE event_id = 'b-1'", [], |r| r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), payload(&session));
        assert_eq!(states(&ingest(&e, &cred, vec![rec], now).1), ["accepted"], "a retry is the same content");
        // a snapshot stored before the change keeps its line until retention ages it out; no view projects it
        db.execute("UPDATE snapshots SET payload = ?1 WHERE event_id = 'b-1'", [payload(&with_line).to_string()]).unwrap();
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "tq-1").unwrap().clone();
        assert_eq!(row["snapshot"]["payload"], payload(&session));
        assert!(!doc.to_string().contains("Running tests"), "no line in /data or dashq report");
        assert!(!crate::page::render(&doc, &json!({})).contains("Running tests"), "no line on the page");
    }

    /// #160 A1/A2: TaskQ sends its board and the release fixture `enrollment_record` (taskq v4.6.25
    /// resource-policy-fixtures, pinned in fixture/taskq-resources) in one batch. Both are accepted, the board stays
    /// the snapshot so the connection reads Connected and versions stay supported, and the projection shows as project
    /// limits aged by its status time. Risk: a resources record replacing the board snapshot (`unsupported snapshot
    /// schema`, pending readback), or a target of another installation or generation being stored.
    #[test]
    fn taskq_resources_record_shows_project_limits_beside_the_board() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("resources");
        let fixture: Value = serde_json::from_str(include_str!("../fixture/taskq-resources/resource-policy-v1.json")).unwrap();
        let mut res = fixture["enrollment_record"].clone();
        let inst = res["payload"]["targets"][0]["target"]["installation_id"].as_str().unwrap().to_string();
        // move the fixture to now, keeping the status readback 61 s older than the send
        let shift = now - res["observed_at"].as_i64().unwrap();
        res["observed_at"] = json!(now);
        res["payload"]["observed_at"] = json!(res["payload"]["observed_at"].as_i64().unwrap() + shift);
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/dashq"});
        let d = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac", "schemas": ["taskq-board/1", RESOURCES], "scope": scope}), now).1;
        assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let board = json!({"schema": "taskq-board/1", "event_id": "b-1", "observed_at": now - 1, "payload": {"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": "taskqorg/dashq"}, "complete": true, "totals": {"doing": 1}, "tasks": []}});
        let variant = |id: &str, f: &dyn Fn(&mut Value)| {
            let mut r = res.clone();
            r["event_id"] = json!(id);
            f(&mut r);
            r
        };
        let records = vec![
            board,
            res.clone(),
            variant("other-inst", &|r| r["payload"]["targets"][0]["target"]["installation_id"] = json!("b".repeat(64))),
            variant("other-gen", &|r| r["payload"]["targets"][0]["target"]["generation"] = json!(2)),
            variant("raw", &|r| r["payload"]["targets"][0]["sample"] = json!({"path": "/etc"})),
            variant("schema", &|r| r["payload"]["schema"] = json!("taskq.resources/2")),
        ];
        let (s, v) = ingest(&e, &cred, records, now);
        assert_eq!((s, states(&v)), (200, vec!["accepted", "accepted", "rejected:scope", "rejected:scope", "rejected:payload", "rejected:payload"].into_iter().map(String::from).collect::<Vec<_>>()), "{v}");
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap().clone();
        assert_eq!((&row["projected"], &row["snapshot"]["schema"]), (&json!(true), &json!("taskq-board/1")));
        let l = &row["project_limits"];
        // age from the document's own clock: the route reads the wall clock again, maybe one second after `now`
        let age = doc["generated_at"].as_i64().unwrap() - res["payload"]["observed_at"].as_i64().unwrap();
        assert!((61..=62).contains(&age), "{age}");
        assert_eq!((&l["state"], &l["age_s"], &l["reason"]), (&json!("ok"), &json!(age), &Value::Null), "{l}");
        assert_eq!(l["targets"], json!([{"project_id": "project-1", "runtime_id": "codex", "policy_schemas": ["taskq.project-limits/1"], "current_revision": "1".repeat(64), "limit": 1,
            "effective_revision": null, "effective": null, "blockers": [{"code": "sample", "knowledge": "unknown"}], "completeness": "unknown"}]));
        let ver = doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap().clone();
        assert_eq!((ver["provenance"].as_str(), ver["reason"].as_str()), (Some("enrollment"), None));
        let html = crate::page::render(&doc, &json!({}));
        assert!(html.contains("✓ Connected") && !html.contains("unsupported snapshot schema"), "{html}");
        assert!(html.contains(&format!("Project limits: observed {age}s ago")), "{html}");
        assert!(html.contains("project-1 · codex: limit 1 · effective <span class=\"unknown\" title=\"unknown: sample unknown\">—</span> · unknown"), "{html}");
    }

    /// A ServerQ projection fixture (serverq.md § 3) moved so its report time is `at`; every time keeps its offset.
    fn serverq_report(fixture: &str, at: i64) -> Value {
        let mut p: Value = serde_json::from_str(fixture).unwrap();
        let shift = at - p["observed_at"].as_i64().unwrap();
        fn walk(v: &mut Value, shift: i64) {
            match v {
                Value::Object(o) => o.iter_mut().for_each(|(k, x)| match x.as_i64() {
                    Some(t) if ["observed_at", "first_seen", "last_seen", "verified_at"].contains(&k.as_str()) => *x = json!(t + shift),
                    _ => walk(x, shift),
                }),
                _ => {}
            }
        }
        walk(&mut p, shift);
        p
    }

    /// Enrollment E1/E8 for ServerQ (#122): ServerQ-only installations on Linux (WSL) and Windows under one host label
    /// fill their own hosts rows and Server checks, stay fresh by producer time within the serverq limit, and their
    /// receipts read `projected` only after a view rendered them; an unsupported schema never does.
    #[test]
    fn serverq_snapshots_render_per_installation_and_drive_projected_receipts() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("serverq");
        let schema = "serverq-report/1";
        let (linux, windows) = (include_str!("../fixture/serverq/linux.json"), include_str!("../fixture/serverq/windows.json"));
        let wsl = join(&e, "serverq", "sq-wsl", "wsl", "DESKTOP", schema, now);
        let native = join(&e, "serverq", "sq-native", "native", "DESKTOP", schema, now);
        // a lost identity on the same label and runtime: an older, stale report of its own
        let lost = join(&e, "serverq", "sq-lost", "native", "DESKTOP", schema, now);
        let other = join(&e, "taskq", "tq-1", "native", "DESKTOP", "taskq-snapshot/1", now);
        // the right schema name around a payload the view cannot render: another payload schema, no rules
        let v2 = join(&e, "serverq", "sq-v2", "native", "other", schema, now);
        let bare = join(&e, "serverq", "sq-bare", "native", "other", schema, now);
        let send = |cred: &str, id: &str, schema: &str, at: i64, payload: Value| {
            let (s, v) = ingest(&e, cred, vec![json!({"schema": schema, "event_id": id, "observed_at": at, "payload": payload})], now);
            assert_eq!((s, states(&v)), (200, vec!["accepted".to_string()]), "{v}");
        };
        send(&wsl, "r-wsl", schema, now - 180, serverq_report(linux, now - 180));
        // ServerQ sends no heartbeat between hourly checks: 7000 s is fresh, 7300 s is stale
        send(&native, "r-native", schema, now - 7000, serverq_report(windows, now - 7000));
        send(&lost, "r-lost", schema, now - 7300, serverq_report(linux, now - 7300));
        send(&other, "t-1", "taskq-snapshot/1", now, json!({"schema": 1}));
        send(&v2, "r-v2", schema, now, json!({"schema": 2, "product": "serverq", "rules": {}}));
        send(&bare, "r-bare", schema, now, json!({"schema": 1, "product": "serverq"}));
        let receipt = |cred: &str, id: &str| machine(&e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        assert_eq!(receipt(&wsl, "r-wsl"), "accepted", "no view has read it yet");

        let mut app = app(e, None);
        app.cfg.serverq_report = String::new();
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let e = &app.enroll;
        let receipt = |cred: &str, id: &str| machine(e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        for (cred, id) in [(&wsl, "r-wsl"), (&native, "r-native"), (&lost, "r-lost")] {
            assert_eq!(receipt(cred, id), "projected", "{id}");
        }
        assert_eq!(receipt(&other, "t-1"), "accepted", "no view renders taskq-snapshot/1");
        assert_eq!((receipt(&v2, "r-v2"), receipt(&bare, "r-bare")), (json!("accepted"), json!("accepted")), "an unsupported payload never reads projected");
        let rows = doc["installations"]["rows"].as_array().unwrap();
        let row = |inst: &str| rows.iter().find(|r| r["installation"] == inst).unwrap().clone();
        let projected = |inst: &str| row(inst)["projected"].clone();
        assert_eq!([projected("sq-native"), projected("tq-1"), projected("sq-v2"), projected("sq-bare")], [true, false, false, false].map(|b| json!(b)));
        assert_eq!((row("sq-native")["observation"]["state"].as_str(), row("sq-lost")["observation"]["state"].as_str()), (Some("ok"), Some("stale")));

        // hosts: one row per label and runtime; the newest observation owns a shared row
        let host = |id: &str| doc["hosts"].as_array().unwrap().iter().find(|h| h["id"] == id).unwrap()["serverq"].clone();
        let (h_wsl, h_native) = (host("DESKTOP/wsl"), host("DESKTOP/native"));
        assert_eq!((h_wsl["status"].as_str(), h_wsl["state"].as_str(), h_wsl["installation"].as_str(), h_wsl["at"].as_i64()), (Some("warn"), Some("ok"), Some("sq-wsl"), Some(now - 180)));
        // age is read at /data time, a second or more after `now` on a slow host
        assert!(h_wsl["age_s"].as_i64().is_some_and(|a| (180..190).contains(&a)), "{h_wsl}");
        assert_eq!((h_native["status"].as_str(), h_native["state"].as_str(), h_native["installation"].as_str()), (Some("unknown"), Some("ok"), Some("sq-native")));

        // Server checks: each installation apart, per-rule status, reason and producer time
        let checks = doc["checks"]["installations"].as_array().unwrap();
        assert_eq!(checks.len(), 5);
        for inst in ["sq-v2", "sq-bare"] {
            let c = checks.iter().find(|c| c["installation"] == inst).unwrap();
            assert_eq!((c["source"]["state"].as_str(), c["source"]["reason"].as_str()), (Some("unknown"), Some("unsupported serverq projection (schema 1 expected)")), "{c}");
        }
        let check = |inst: &str| checks.iter().find(|c| c["installation"] == inst).unwrap().clone();
        let win = check("sq-native");
        assert_eq!((win["host"].as_str(), win["version"].as_str(), win["source"]["state"].as_str()), (Some("DESKTOP/native"), Some("1.3.0"), Some("ok")));
        assert_eq!(win["results"].as_array().unwrap().len(), 5);
        for r in win["results"].as_array().unwrap() {
            assert_eq!((r["status"].as_str(), r["reason"].as_str(), r["observed_at"].as_i64()), (Some("unknown"), Some("unsupported-os"), Some(now - 7000)), "{r}");
        }
        let manual: Vec<&str> = win["manual_rules"].as_array().unwrap().iter().map(|m| m["status"].as_str().unwrap()).collect();
        assert_eq!(manual, ["not verified"; 4], "manual rules are never verified");
        let lin = check("sq-wsl");
        let rule = |name: &str| lin["results"].as_array().unwrap().iter().find(|r| r["check"] == name).unwrap().clone();
        let mut failed = rule("failed-services");
        let source = failed.as_object_mut().unwrap().remove("source").unwrap();
        assert_eq!(source["state"], "ok");
        assert_eq!(source["at"], now - 180);
        assert_eq!(failed, json!({"check": "failed-services", "status": "warn", "reason": "failed-units", "observed_at": now - 180, "evidence": {"units": 1}}));
        assert_eq!(rule("bounded-journal")["observed_at"], now - 180 - 3600, "a cached rule keeps its own observation time");
        assert_eq!(lin["incidents"], json!([{"check": "failed-services", "state": "observed", "first_seen": now - 180 - 7200, "last_seen": now - 180, "occurrences": 3}]));
        assert_eq!(check("sq-lost")["source"]["state"], "stale");

        // Attention: the warning is current, unknown rules are gaps, each naming its installation
        let att = &doc["attention"];
        assert!(att["current"].as_array().unwrap().iter().any(|i| i["installation"] == "sq-wsl" && i["what"] == "failed-services: warn" && i["host"] == "DESKTOP/wsl"));
        assert_eq!(att["gaps"].as_array().unwrap().iter().filter(|i| i["installation"] == "sq-native" && i["detail"] == "unsupported-os").count(), 5);
        // versions: the enrolled ServerQ version, an unknown schema stays unsupported
        let ver = |inst: &str| doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap().clone();
        assert_eq!((ver("sq-native")["provenance"].as_str(), ver("sq-native")["running"]["version"].as_str()), (Some("enrollment"), Some("1.3.0")));
        assert_eq!(ver("tq-1")["provenance"], "unsupported");

        // the page renders what /data says; equal host labels stay apart
        let html = crate::page::render(&doc, &json!({}));
        let section = &html[html.find("<section id=\"checks\">").unwrap()..];
        for s in ["DESKTOP/wsl · ServerQ 1.3.0 · <code>sq-wsl</code>", "DESKTOP/native · ServerQ 1.3.0 · <code>sq-native</code>", "<code>sq-lost</code>", "unsupported-os", "failed-units", "Last known ok", "not verified"] {
            assert!(section.contains(s), "{s}");
        }
        assert!(html.contains("<section id=\"host-grid\">") && !html.contains("no serverq report for this host"));
    }

    /// H1/H2 chain: ingest -> retained store -> scoped projection -> Attention/page, without any task write.
    #[test]
    fn serverq_episode_chain_keeps_identity_recovery_and_recurrence() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("finding-40");
        let schema = "serverq-report/1";
        let a = join(&e, "serverq", "sq-a", "native", "same", schema, now);
        let b = join(&e, "serverq", "sq-b", "native", "same", schema, now);
        let mut app = app(e, None);
        app.cfg.serverq_report = String::new();
        let e = &app.enroll;
        let report = |at: i64, status: &str, first: i64, verified: Option<i64>| json!({
            "schema": 1, "product": "serverq", "catalog": crate::findings::catalog_hash(), "observed_at": at,
            "rules": {"disk-headroom": {"status": status, "observed_at": at, "evidence": {"available": 20, "total": 100}}},
            "incidents": {"disk-headroom": {"state": if verified.is_some() { "verified" } else { "observed" }, "first_seen": first, "last_seen": first, "verified_at": verified, "occurrences": 1}}
        });
        let send = |cred: &str, id: &str, at: i64, p: Value| {
            assert_eq!(ingest(e, cred, vec![json!({"schema": schema, "event_id": id, "observed_at": at, "payload": p})], now).0, 200);
        };
        let first = now - 500;
        send(&a, "warn-a", now - 400, report(now - 400, "warn", first, None));
        send(&b, "warn-b", now - 400, report(now - 400, "warn", first, None));
        // A newer incomplete frame must not erase the earlier open episode.
        send(&a, "missing", now - 300, json!({"schema": 1, "product": "serverq", "rules": {}, "incidents": {}, "complete": false}));
        let check = |doc: &Value, inst: &str| doc["checks"]["installations"].as_array().unwrap().iter().find(|c| c["installation"] == inst).unwrap().clone();
        let doc = data(&app, "", Some(Scope::All)).1;
        assert_eq!(check(&doc, "sq-a")["findings"][0]["state"], "open");
        assert!(doc["attention"]["gaps"].as_array().unwrap().iter().any(|g| g["installation"] == "sq-a" && g["episode"]["first_seen"] == first));
        let mut unknown = report(now - 200, "unknown", first, None);
        send(&a, "unknown", now - 200, unknown.clone());
        unknown["rules"]["disk-headroom"]["observed_at"] = json!(now - 8000);
        unknown["rules"]["disk-headroom"]["status"] = json!("ok");
        unknown["incidents"]["disk-headroom"]["state"] = json!("verified");
        unknown["incidents"]["disk-headroom"]["verified_at"] = json!(now - 100);
        send(&a, "cached-pass", now - 100, unknown);
        let cached = check(&data(&app, "", Some(Scope::All)).1, "sq-a");
        assert_eq!(cached["findings"][0]["state"], "open");
        assert_eq!(cached["results"][0]["source"]["state"], "stale");
        // The full page also exposes the stale rule as last known, never a fresh green pass.
        let fragment = crate::page::render(&data(&app, "", Some(Scope::All)).1, &json!({}));
        assert!(fragment.contains("Last known ok") && fragment.contains("rule observation stale"));
        send(&a, "pass", now - 50, report(now - 50, "ok", first, Some(now - 50)));
        // Received later, observed earlier: cannot undo the matching pass.
        send(&a, "late-warn", now - 350, report(now - 350, "warn", first, None));
        let doc = data(&app, "", Some(Scope::All)).1;
        assert_eq!(check(&doc, "sq-a")["findings"][0]["state"], "verified");
        assert_eq!(check(&doc, "sq-b")["findings"][0]["state"], "open");
        // A3/H2: reconstruct history through the real retained-store boundary after expiry.
        let aged = projection(e, Some(Scope::All), None, (now + 7201) as u64).unwrap();
        let samples = finding_samples(e, &aged["rows"]);
        for inst in ["sq-a", "sq-b"] {
            let row = aged["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap();
            let retained = samples.as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap();
            let findings = crate::findings::build(row, retained, (now + 7201) as u64);
            assert_eq!(findings["episodes"][0]["state"], if inst == "sq-a" { "verified" } else { "open" });
            assert_eq!(findings["episodes"][0]["source"]["state"], "stale");
        }
        send(&a, "recurrence", now - 10, report(now - 10, "warn", now - 10, None));
        let doc = data(&app, "", Some(Scope::All)).1;
        let c = check(&doc, "sq-a");
        assert_eq!(c["findings"].as_array().unwrap().len(), 2);
        assert_eq!(c["findings"][0]["state"], "verified");
        assert_eq!(c["findings"][1]["state"], "open");
        let html = crate::page::render(&doc, &json!({}));
        for expected in ["verified", "Catalog matched", "minimum_percent: 15", "installation <code>sq-a</code>", "installation <code>sq-b</code>", "available 20"] {
            assert!(html.contains(expected), "{expected}");
        }
        if let Some(dir) = std::env::var_os("DASHQ_FINDINGS_EVIDENCE") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("page.html"), html.replace("<link rel=\"stylesheet\" href=\"/style.css\">", &format!("<style>{}</style>", crate::style_css())).replace("<script src=\"/app.js\" defer></script>", "")).unwrap();
            std::fs::write(dir.join("data.json"), serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
        }
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
        assert_eq!(machine(e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-0"}), now + 2).1["state"], "accepted");
        assert_eq!(machine(e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 2).1["state"], "projected");
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
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let (s, _, b) = get(&e, "github|1", &wait, now + 2);
        assert!(s == "200 OK" && b.contains("Waiting for the first snapshot") && b.contains("http-equiv=\"refresh\""), "{b}");
        // another account cannot follow or even see it
        assert_eq!(get(&e, "github|2", &wait, now + 2).0, "404 Not Found");
        assert_eq!(states(&ingest(&e, &cred, vec![rec("s-1", now, json!({"v": 1}))], now + 3).1), ["accepted"]);
        let (s, h, _) = get(&e, "github|1", &wait, now + 4);
        assert_eq!((s.as_str(), h.as_str()), ("303 See Other", "Location: /#inst-inst-1-ws-1\r\n"));
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 4).1["state"], "projected");
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
    /// L1-L3: approval -> activation -> durable snapshot -> matching projection; receipt age is not
    /// producer freshness, reconnect cannot borrow an old generation, and lost identity never merges.
    #[test]
    fn connections_lifecycle_reference_chain() {
        let now = crate::source::now() as i64;
        let (_, e) = setup("connections-reference");
        let emit = |name: &str, html: &str| {
            if let Some(dir) = std::env::var_os("DASHQ_CONNECTION_EVIDENCE") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(Path::new(&dir).join(format!("{name}.html")), html).unwrap();
            }
        };
        emit("entry", &get(&e, "github|1", "/connect", now).2);
        let (device, code) = start(&e, "inst-reference", now);
        let review = page(&e, "github|1", &code, now).1;
        emit("approval", &review);
        let (_, approved) = decide(&e, "github|1", &code, "approve", "ws-1", now);
        assert!(!approved.contains("is connected"), "approval alone is not connection: {approved}");
        assert!(review.contains("https://dash.example.com") && review.contains("observation only"));
        emit("approved", &approved);
        let wait = format!("/connect?wait=inst-reference&workspace=ws-1&generation=1&since={now}");
        let pending = get(&e, "github|1", &wait, now + 2).2;
        assert!(pending.contains("credential activation"));
        emit("activation", &pending);
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now + 5).1["credential"].as_str().unwrap().to_string();
        machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now + 5);
        let pending = get(&e, "github|1", &wait, now + 6).2;
        assert!(pending.contains("Waiting for the first snapshot"));
        emit("snapshot", &pending);
        let timeout = get(&e, "github|1", &wait, now + 61).2;
        assert!(timeout.contains("Check again for 60 s") && !timeout.contains("http-equiv=\"refresh\""));
        emit("timeout", &timeout);
        let payload = json!({"version": "1.2.3", "agentq.resource.cpu_unknown": "warmup"});
        ingest(&e, &cred, vec![rec("reference-1", now - 1000, payload.clone())], now);
        // Same delivery retry retains one installation and snapshot; its original observation time stays old.
        ingest(&e, &cred, vec![rec("reference-1", now - 1000, payload)], now + 11);
        assert_eq!(get(&e, "github|1", &wait, now + 12).0, "303 See Other");
        let p = projection(&e, Some(Scope::Subject("github|1")), None, (now + 12) as u64).unwrap();
        let r = &p["rows"][0];
        assert_eq!(p["rows"].as_array().unwrap().len(), 1);
        assert_eq!((&r["projected"], &r["observation"]["state"], &r["export"]["state"]), (&json!(true), &json!("stale"), &json!("unknown")));
        assert_eq!(r["received_at"], now);
        assert_eq!(r["history"]["samples"], 1);
        let app = app(e, None);
        let e = &app.enroll;
        let render = || String::from_utf8(crate::route(&app, "GET", "/", "", Some(Scope::Subject("github|1"))).2).unwrap();
        let stale = render();
        assert!(stale.contains("✓ Connected") && stale.contains("stale") && stale.contains("export forwarding not qualified"));
        emit("stale", &stale);
        emit("revoke", &get(e, "github|1", "/connect?revoke=inst-reference&workspace=ws-1", now).2);
        revoke(e, "github|1", "github|1", "inst-reference", "ws-1", now + 13);
        emit("revoked", &render());
        assert_eq!(ingest(e, &cred, vec![rec("old-credential", now, json!({}))], now + 14).0, 401);
        let next = connect(e, "inst-reference", "github|1", "ws-1", now + 20);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 21) as u64).unwrap();
        assert_eq!(p["rows"].as_array().unwrap().len(), 1);
        assert_eq!((&p["rows"][0]["generation"], &p["rows"][0]["projected"]), (&json!(2), &json!(false)));
        assert!(!render().contains("✓ Connected"), "new generation cannot inherit the previous readback");
        let wait2 = format!("/connect?wait=inst-reference&workspace=ws-1&generation=2&since={now}");
        assert_eq!(get(e, "github|1", &wait2, now + 21).0, "200 OK", "old history is not new readback");
        ingest(e, &next, vec![rec("reference-2", now, json!({"version": "1.2.3", "agentq.resource.cpu_unknown": "warmup"}))], now);
        assert_eq!(get(e, "github|1", &wait2, now + 22).0, "303 See Other");
        emit("connected", &render());
        connect(e, "inst-lost-identity", "github|1", "ws-1", now + 25);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 26) as u64).unwrap();
        assert_eq!(p["rows"].as_array().unwrap().len(), 2, "equal host labels keep distinct identities");
        emit("identity", &render());
        // Standalone products do not require AgentQ or Langfuse and keep their own scope.
        for product in ["serverq", "taskq"] {
            let schema = format!("{product}-snapshot/1");
            let (_, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": product, "installation": product, "runtime": "wsl", "host_label": "same-long-host-label", "schemas": [schema], "scope": {"name": "board/1", "value": "example/project"}}), now + 30);
            decide(e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now + 31);
        }
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 32) as u64).unwrap();
        for r in p["rows"].as_array().unwrap().iter().filter(|r| r["product"] != "agentq") {
            assert_eq!(r["export"]["state"], "not_applicable");
            assert_eq!(r["scope"]["value"], "example/project");
        }
        emit("list", &render());
        let mut quiet = app.doc(1800, json!({}), Some(Scope::Subject("github|1")));
        quiet["generated_at"] = json!(now + 400);
        quiet["installations"] = projection(e, Some(Scope::Subject("github|1")), None, (now + 400) as u64).unwrap();
        let quiet = crate::page::render(&quiet, &json!({}));
        assert!(quiet.contains("No recent receipt") && quiet.contains("Host activity unknown"));
        emit("no-contact", &quiet);
        let store = &e.as_ref().unwrap().store;
        store.accept(&sha256(&next), &[Event { id: "export-gap", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        store.prune(now + 7 * 86400 + 1).unwrap();
        let gap = render();
        assert!(gap.contains("gap · events expired before export: 1") && gap.contains("✓ Connected"));
        emit("export-gap", &gap);
        // Separately approved operations from D16 stay separate from the observation scope.
        let (_, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": "inst-scoped", "runtime": "native", "host_label": "scoped-host", "schemas": ["agentq-snapshot/1"], "capabilities": ["agentq.configure"]}), now + 40);
        let code = d["user_code"].as_str().unwrap();
        emit("approval-capability", &page(e, "github|1", code, now + 41).1);
        decide(e, "github|1", code, "approve&capability.agentq.configure=grant", "ws-1", now + 41);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 42) as u64).unwrap();
        let r = p["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-scoped").unwrap();
        assert_eq!(r["capabilities"], json!(["agentq.configure"]));
        assert!(render().contains("Separately approved operations</dt><dd>agentq.configure"));
    }

    /// L3/E7 owner boundary: the real coreq client retains a revoke-only request offline. DashQ
    /// cannot know that local state; acknowledgement, restart and new approval preserve one identity.
    #[test]
    fn connections_offline_revoke_host_and_server_chain() {
        struct Offline;
        impl coreq::enroll::Wire for Offline {
            fn post(&mut self, _: &str, route: &str, _: Option<&str>, _: &Value) -> Result<(u16, Value), String> {
                assert_eq!(route, "/connection/revoke");
                Err("offline".into())
            }
            fn now(&self) -> u64 { T0 as u64 }
            fn sleep(&mut self, _: u64) {}
            fn show(&mut self, _: &str, _: &str) { panic!("no new approval") }
        }
        let (d, e) = setup("offline-life");
        let host = dir("offline-life-host");
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "same-host", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let grant = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(coreq::enroll::disconnect(&host, &mut Offline), Err("1 revoke pending".into()));
        assert!(coreq::enroll::active(&host).unwrap().is_none());
        assert_eq!(coreq::enroll::stamp(&host, "agentq-snapshot/1", "off", T0 as u64, json!({})), Err("disconnected".into()));
        assert_eq!(projection(&e, Some(Scope::All), None, T0 as u64).unwrap()["rows"][0]["connection"], "active");
        coreq::enroll::retry_revokes(&host, &mut w).unwrap();
        coreq::enroll::retry_revokes(&host, &mut w).unwrap();
        assert_eq!(w.calls.iter().filter(|s| *s == "/machine/connection/revoke").count(), 1);
        drop(e);
        let e = Enroll::open(&d, HOST);
        assert_eq!(projection(&e, Some(Scope::All), None, T0 as u64).unwrap()["rows"][0]["connection"], "revoked");
        let mut w = Wire { e: &e, now: T0 + 60, code: None, calls: vec![], tick: false };
        let new = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(new["installation"], grant["installation"]);
        assert_eq!(new["generation"], 2);
        assert_eq!(e.as_ref().unwrap().project(Scope::All).unwrap().len(), 1);
    }

    /// An AgentQ host paired through the coreq client with `agentq.configure` ticked and one profile snapshot
    /// (profiles 0123456789abcdef, 1111111111111111, 3333333333333333); returns its client folder.
    fn configurable(e: &Result<Enroll, String>, inst: &str, now: i64) -> PathBuf {
        let host = dir(&format!("{inst}-host"));
        let req = coreq::enroll::Request { installation: Some(inst), product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &["agentq.configure"] };
        let mut w = Wire { e, now, code: None, calls: vec![], tick: true };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(a["capabilities"], json!(["agentq.configure"]));
        let snap: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        assert_eq!(states(&ingest(e, a["credential"].as_str().unwrap(), vec![rec("s1", now, snap["cases"]["profile_projection"].clone())], now).1), ["accepted"]);
        host
    }

    fn op_id(n: u8) -> String {
        format!("{n:032x}")
    }

    const ROUTING: Change = Change { profile: "0123456789abcdef", field: "routing", value: "direct" };
    const PLAN: Plan = Plan { revision: "00112233445566ff", digest: "9d029e2b923a7a5b63e1b3e2c306898c454f27df8026fd6e061b3421b274d937" };

    // chain: D16 A1/A2, a typed configure request is queued only under server-set authority (actor, workspace,
    // profiles), reaches the AgentQ host through the shared coreq client, and its result commits before the receipt
    // and repeats after a lost reply; AgentQ's published envelopes fit the transport
    #[test]
    fn configure_operations_carry_server_authority_to_the_host() {
        let (d, e) = setup("configure");
        let x = e.as_ref().unwrap();
        let host = configurable(&e, "inst-c1", T0);
        let fx: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration-fixtures.json")).unwrap();
        let c = |profile, field, value| Change { profile, field, value };
        // malformed requests never queue: id, empty, unknown value, duplicate target, bad plan
        let bad_plan = Plan { revision: "r1", digest: "nothex" };
        for (op, plan, changes) in [("XYZ".to_string(), None, vec![ROUTING]), (op_id(1), None, vec![]), (op_id(1), None, vec![c("0123456789abcdef", "routing", "uninstall")]),
            (op_id(1), None, vec![ROUTING, ROUTING]), (op_id(1), Some(bad_plan), vec![ROUTING])] {
            assert_eq!(x.configure("github|1", "inst-c1", &op, plan, &changes, T0), Err("invalid"));
        }
        // authority is the server's: no grant, a non-member, a member who did not approve, a profile not in the snapshot
        x.store.add_member("ws-1", "github|2", "member").unwrap();
        assert_eq!(x.configure("github|1", "inst-none", &op_id(1), None, &[ROUTING], T0), Err("no_grant"));
        assert_eq!(x.configure("github|9", "inst-c1", &op_id(1), None, &[ROUTING], T0), Err("authority"));
        assert_eq!(x.configure("github|2", "inst-c1", &op_id(1), None, &[ROUTING], T0), Err("authority"));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[c("9999999999999999", "routing", "direct")], T0), Err("profile_out_of_scope"));
        // an observation grant carries no configure authority, with or without a snapshot
        let observe = connect(&e, "inst-o1", "github|1", "ws-1", T0);
        assert_eq!(x.configure("github|1", "inst-o1", &op_id(1), None, &[ROUTING], T0), Err("no_profiles"));
        let snap: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        ingest(&e, &observe, vec![rec("o1", T0, snap["cases"]["profile_projection"].clone())], T0);
        assert_eq!(x.configure("github|1", "inst-o1", &op_id(1), None, &[ROUTING], T0), Err("scope"));
        // the plan: AgentQ's request envelope with authority from the grant and snapshot, digest per coreq
        let plan = x.configure("github|1", "inst-c1", &op_id(1), None, &[ROUTING], T0).unwrap();
        let actor = actor_id("github|1");
        assert!(actor.starts_with("actor-") && actor.len() == 30 && actor_id("acct-1") == "acct-1");
        assert_eq!(plan["authority"], json!({"actor": actor, "workspace": "ws-1", "profiles": ["0123456789abcdef", "1111111111111111", "3333333333333333"]}));
        let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys(&plan["payload"]), keys(&fx["requests"]["plan"]));
        assert_eq!((&plan["payload"]["expires_at"], &plan["digest"]), (&json!(T0 + 300), &json!(coreq::enroll::digest(&plan["payload"]))));
        // immutable identity: a retry returns the stored operation, changed content or another actor is refused
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[ROUTING], T0 + 5), Ok(plan.clone()));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[c("0123456789abcdef", "routing", "routed")], T0 + 5), Err("replay_mismatch"));
        assert_eq!(x.configure("acct-1", "inst-c1", &op_id(1), None, &[ROUTING], T0 + 5), Err("replay_mismatch"));
        // one unresolved mutation per installation; plans are not mutations
        let apply = x.configure("github|1", "inst-c1", &op_id(2), Some(PLAN), &[ROUTING], T0).unwrap();
        assert_eq!(keys(&apply["payload"]), keys(&fx["requests"]["apply"]));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(3), Some(PLAN), &[ROUTING], T0), Err("busy"));
        x.configure("github|1", "inst-c1", &op_id(4), None, &[ROUTING], T0).unwrap();
        assert_eq!(x.readback("github|1", &op_id(2), T0).unwrap().unwrap()["state"], "queued");
        // the host pulls through the coreq client; a lost pull reply pulls the same operations again
        let mut w = Wire { e: &e, now: T0 + 10, code: None, calls: vec![], tick: false };
        let mut b = coreq::enroll::Backoff::default();
        let got = coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap();
        assert_eq!((got.operations.iter().map(|o| o["operation"].clone()).collect::<Vec<_>>(), got.rejected.len()), (vec![json!(op_id(1)), json!(op_id(2)), json!(op_id(4))], 0));
        assert_eq!(coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap(), got);
        let op = &got.operations[1];
        // payload claims never add authority: only the authority's actor, workspace and profiles pass
        assert_eq!(coreq::enroll::admit(op, Some(&actor), Some("ws-1"), &["0123456789abcdef"]), Ok(()));
        assert_eq!(coreq::enroll::admit(op, Some("acct-1"), None, &["0123456789abcdef"]), Err("wrong_actor"));
        assert_eq!(coreq::enroll::admit(op, None, Some("ws-2"), &["0123456789abcdef"]), Err("wrong_workspace"));
        assert_eq!(coreq::enroll::admit(op, None, None, &["9999999999999999"]), Err("profile_out_of_scope"));
        assert_eq!(x.readback("github|1", &op_id(2), T0 + 10).unwrap().unwrap()["state"], "delivered");
        // every published AgentQ plan and result envelope is storable metadata
        for r in fx["results"].as_array().unwrap().iter().filter(|r| r["valid"] == true).map(|r| &r["result"]).chain(fx["cases"].as_array().unwrap().iter().filter(|c| c["expect"]["kind"] == "plan").map(|c| &c["expect"])) {
            assert!(result_metadata(&json!({"result": r})), "{r}");
        }
        // the result commits before its receipt; a lost receipt repeats it; a changed result is refused
        let result = json!({"result": fx["results"][0]["result"]});
        let receipt = json!({"protocol": 1, "operation": op_id(2), "state": "accepted"});
        assert_eq!(coreq::enroll::post_result(&host, op, result.clone(), &mut b, &mut w).unwrap(), receipt);
        assert_eq!(coreq::enroll::post_result(&host, op, result.clone(), &mut b, &mut w).unwrap(), receipt);
        let changed = coreq::enroll::post_result(&host, op, json!({"result": fx["results"][1]["result"]}), &mut b, &mut w).unwrap();
        assert_eq!(changed, json!({"protocol": 1, "operation": op_id(2), "state": "rejected", "reason": "replay_mismatch"}));
        // readback: the actor and the workspace administrator; another member reads nothing
        let back = x.readback("github|1", &op_id(2), T0 + 11).unwrap().unwrap();
        assert_eq!((&back["state"], &back["result"], &back["authority"]), (&json!("resulted"), &result, &apply["authority"]));
        assert_eq!(x.readback("github|2", &op_id(2), T0 + 11).unwrap(), None);
        // resolved: the next mutation queues; the resulted one is no longer pending
        x.configure("github|1", "inst-c1", &op_id(5), Some(PLAN), &[ROUTING], T0 + 11).unwrap();
        let ids = |p: &coreq::enroll::Pulled| p.operations.iter().map(|o| o["operation"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap()), [op_id(1), op_id(4), op_id(5)]);
        // a restart keeps operations and results
        drop(w);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let mut w = Wire { e: &e, now: T0 + 12, code: None, calls: vec![], tick: false };
        assert_eq!(ids(&coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap()), [op_id(1), op_id(4), op_id(5)]);
        assert_eq!(e.as_ref().unwrap().readback("github|1", &op_id(2), T0 + 12).unwrap().unwrap()["result"], result);
    }

    // chain: D16 A3, expiry, revocation and results commit in either order with the actual outcome: an undelivered
    // operation never runs or blocks after expiry or revoke, a delivered one stays unresolved (across restart) until
    // its result or a revoke, and a committed result is never undone by a later revoke
    #[test]
    fn operation_expiry_revocation_and_restart_orderings() {
        let (d, e) = setup("op-races");
        let host = configurable(&e, "inst-r1", T0);
        let cred = coreq::enroll::active(&host).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        let pull = |e: &Result<Enroll, String>, cred: &str, now| machine(e, "/machine/operations", Some(cred), json!({"protocol": 1, "capability": "agentq.configure"}), now);
        let post = |e: &Result<Enroll, String>, cred: &str, op: &Value, now| {
            machine(e, "/machine/operations/result", Some(cred), json!({"protocol": 1, "operation": op["operation"], "digest": op["digest"], "result": {"outcome": "applied"}}), now)
        };
        let state = |e: &Result<Enroll, String>, id: &str, now| e.as_ref().unwrap().readback("github|1", id, now).unwrap().unwrap()["state"].clone();
        let x = e.as_ref().unwrap();
        // expiry before delivery: never delivered, never blocks the next mutation
        x.configure("github|1", "inst-r1", &op_id(1), Some(PLAN), &[ROUTING], T0).unwrap();
        assert_eq!(pull(&e, &cred, T0 + 300), (200, json!({"protocol": 1, "operations": []})));
        assert_eq!(state(&e, &op_id(1), T0 + 300), "expired");
        let op2 = x.configure("github|1", "inst-r1", &op_id(2), Some(PLAN), &[ROUTING], T0 + 300).unwrap();
        // delivery before expiry: unresolved past expiry, also after a restart, until its result arrives
        assert_eq!(pull(&e, &cred, T0 + 301).1["operations"], json!([op2]));
        assert_eq!(x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 700), Err("busy"));
        drop(e);
        let e = Enroll::open(&d, HOST);
        let x = e.as_ref().unwrap();
        assert_eq!((state(&e, &op_id(2), T0 + 700), x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 700)), (json!("delivered"), Err("busy")));
        assert_eq!(post(&e, &cred, &op2, T0 + 701).1["state"], "accepted", "a late result of a delivered operation is its actual outcome");
        let op3 = x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 701).unwrap();
        // result then revoke: the committed result stays; the revoked credential can neither pull nor post
        assert_eq!(pull(&e, &cred, T0 + 702).1["operations"], json!([op3]));
        assert_eq!(post(&e, &cred, &op3, T0 + 703).1["state"], "accepted");
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(&cred), json!({"protocol": 1}), T0 + 704).0, 200);
        assert_eq!(state(&e, &op_id(3), T0 + 705), "resulted");
        assert_eq!(pull(&e, &cred, T0 + 705), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(post(&e, &cred, &op3, T0 + 705).0, 401);
        assert_eq!(x.configure("github|1", "inst-r1", &op_id(4), None, &[ROUTING], T0 + 705), Err("no_grant"));
        // revoke then result: the delivered operation reads unknown, the undelivered one revoked; neither is undone
        let host2 = configurable(&e, "inst-r2", T0 + 800);
        let cred2 = coreq::enroll::active(&host2).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        let op5 = x.configure("github|1", "inst-r2", &op_id(5), Some(PLAN), &[ROUTING], T0 + 800).unwrap();
        assert_eq!(pull(&e, &cred2, T0 + 801).1["operations"], json!([op5]));
        x.configure("github|1", "inst-r2", &op_id(6), None, &[ROUTING], T0 + 802).unwrap();
        assert!(x.store.revoke("inst-r2", T0 + 803).unwrap());
        assert_eq!(post(&e, &cred2, &op5, T0 + 804), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!((state(&e, &op_id(5), T0 + 804), state(&e, &op_id(6), T0 + 804)), (json!("unknown"), json!("revoked")));
        // reconnecting the installation: the revoke resolved the delivered apply (#147), so a new apply is accepted,
        // also after restart
        configurable(&e, "inst-r2", T0 + 805);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let x = e.as_ref().unwrap();
        assert!(x.configure("github|1", "inst-r2", &op_id(7), Some(PLAN), &[ROUTING], T0 + 807).is_ok());
        // an undelivered apply of a revoked generation never runs, so it does not block the reconnected installation
        configurable(&e, "inst-r4", T0 + 810);
        x.configure("github|1", "inst-r4", &op_id(8), Some(PLAN), &[ROUTING], T0 + 810).unwrap();
        assert!(x.store.revoke("inst-r4", T0 + 811).unwrap());
        configurable(&e, "inst-r4", T0 + 812);
        assert!(x.configure("github|1", "inst-r4", &op_id(9), Some(PLAN), &[ROUTING], T0 + 813).is_ok());
        // another installation's credential never reaches these operations
        let host3 = configurable(&e, "inst-r3", T0 + 900);
        let cred3 = coreq::enroll::active(&host3).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        assert_eq!(post(&e, &cred3, &op5, T0 + 901).1["reason"], "unknown_operation");
        assert_eq!(pull(&e, &cred3, T0 + 901).1["operations"], json!([]));
        assert_eq!(machine(&e, "/machine/operations/result", Some(&cred3), json!({"protocol": 1, "operation": op_id(5), "digest": op5["digest"], "result": {"path": "/etc"}}), T0 + 902).0, 400);
    }

    /// #159 A1/A2 chain: TaskQ's `enrollment_record` fixture becomes the per-project editor groups of an active
    /// connection, with the current and effective revisions the write is reviewed against and applied by; an older
    /// record never replaces the view, and a revoked connection shows no editor. Risk: an editor for a foreign or
    /// revoked target, or a stale record winning. Writes `DASHQ_LIMITS_EVIDENCE` for tests/limits-browser.cjs.
    #[test]
    fn taskq_resources_record_becomes_project_limit_groups() {
        let f: Value = serde_json::from_str(include_str!("../fixture/taskq-resources/resource-policy-v1.json")).unwrap();
        let inst = f["enrollment_record"]["payload"]["targets"][0]["target"]["installation_id"].as_str().unwrap().to_string();
        let (_d, e) = setup("limit-groups");
        let board = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/dashq"});
        let (_, d) = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["taskq-board/1", RESOURCES], "scope": board}), T0);
        assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), T0).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
        let record = |id: &str, observed: i64, payload: Value| json!({"schema": RESOURCES, "event_id": id, "observed_at": observed, "payload": payload});
        let mut payload = f["enrollment_record"]["payload"].clone();
        payload["observed_at"] = json!(T0 - 5);
        let mut old = payload.clone();
        old["targets"][0]["current_revision"] = json!("0".repeat(64));
        assert_eq!(states(&ingest(&e, &cred, vec![record("r-1", T0, payload.clone()), record("r-0", T0 - 60, old)], T0).1), vec!["accepted", "accepted"]);
        let doc = projection(&e, Some(Scope::All), None, T0 as u64).unwrap();
        let l = &doc["rows"][0]["project_limits"];
        assert_eq!(l["targets"][0]["current_revision"], "1".repeat(64), "an older record never replaces the view");
        assert_eq!(l["groups"], json!([{"key": format!("{inst}:ws-1:project-1"), "observed_at": T0 - 5, "labels": {"host": "mac-1", "project": "project-1"}, "targets": [payload["targets"][0]]}]));
        let html = crate::page::render(&json!({"generated_at": T0, "installations": doc, "resources": {"rows": []}}), &json!({}));
        assert!(html.contains(" · unknown · revision <code title=\"1111111111111111111111111111111111111111111111111111111111111111\">111111111111</code> · effective revision unknown</p>"), "{html}");
        assert!(html.contains("class=\"node-policy\" data-taskq-groups=\"[{&quot;key&quot;:&quot;"), "{html}");
        if let Ok(out) = std::env::var("DASHQ_LIMITS_EVIDENCE") {
            std::fs::write(out, &html).unwrap();
        }
        // revoked: the readback stays as history, the editor goes
        e.as_ref().unwrap().store().revoke(&inst, T0 + 1).unwrap();
        let doc = projection(&e, Some(Scope::All), None, T0 as u64 + 2).unwrap();
        assert_eq!(doc["rows"][0]["project_limits"]["groups"], json!([]));
        let html = crate::page::render(&json!({"generated_at": T0 + 2, "installations": doc, "resources": {"rows": []}}), &json!({}));
        assert!(!html.contains("node-policy"));
    }
}
