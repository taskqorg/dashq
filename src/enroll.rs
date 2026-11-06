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
/// D28: products that connect only to ship observations.
const SHIPPERS: [&str; 2] = ["assetq", "pilotq"];
/// Operation capabilities a device may request for separate approval (D16), with the product that owns each and
/// the effect the approval page names.
const CAPABILITIES: [(&str, &str, &str); 2] = [("agentq", "agentq.configure", "change AgentQ client integration settings on this host, one reviewed request at a time"), ("agentq", "agent-history.manual", "archive manual sessions on this host, including personal conversation content")];
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
/// AgentQ PR #304 refuses retired routing, optimization and MCP effects with this code.
pub const FEATURE_RETIRED: &str = "feature_retired";
// Keep the wire choices for older enrolled releases; current host operations remain producer-owned.
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
    archive_key: Vec<u8>,
    csrf: hmac::Key,
    public_host: String,
    rng: SystemRandom,
    recent: Mutex<HashMap<String, Vec<i64>>>,
    /// Per-device token buckets: (tokens, last refill).
    buckets: Mutex<HashMap<(String, i64), (f64, i64)>>,
    /// Stage records of each decoded telemetry block by block id (D21 #182): blocks never change.
    stages: Mutex<HashMap<i64, std::sync::Arc<Vec<Value>>>>,
    /// The service is decoding the stored stage window: browser reads do not queue behind it (#260).
    stages_warming: std::sync::atomic::AtomicBool,
    fleet_cache: crate::source::Cache,
    fleet_db: std::path::PathBuf,
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
            archive_key: k.clone(),
            seal: aead::LessSafeKey::new(aead::UnboundKey::new(&aead::AES_256_GCM, &k).map_err(|_| "dashq.key: bad key")?),
            // separate keys for sealing and CSRF, both from the one deployment key
            csrf: hmac::Key::new(hmac::HMAC_SHA256, ring::digest::digest(&ring::digest::SHA256, &[b"dashq csrf\n".as_slice(), &k].concat()).as_ref()),
            public_host: public_host.into(),
            rng: SystemRandom::new(),
            recent: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
            stages: Mutex::new(HashMap::new()),
            stages_warming: Default::default(),
            fleet_cache: Default::default(),
            fleet_db: db,
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
        s.add_member(workspace, subject, "admin").map_err(|e| format!("dashq.db: {e:?}"))?;
        s.write(|tx| Ok(tx.execute("INSERT OR IGNORE INTO archive_owners VALUES (?1,?2)", [workspace,subject]).map(|_|())?)).map_err(|e| format!("dashq.db: {e:?}"))
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

    pub(crate) fn archive_key(&self) -> &[u8] { &self.archive_key }

    pub fn public_host(&self) -> &str { &self.public_host }

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
    pub(crate) fn spend(&self, g: &store::Grant, now: i64) -> Result<(), i64> {
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

    pub fn warm_stages(&self, warming: bool) {
        self.stages_warming.store(warming, std::sync::atomic::Ordering::SeqCst);
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

mod browser;
mod ingestion;
mod projection;
use browser::*;
use ingestion::*;
pub use browser::{anchor, form};
pub use ingestion::{RESOURCES, TELEMETRY};
pub(crate) use ingestion::unbase64;
pub use projection::{SILENT_SECS, RETIRE_SECS, projection, telemetry_block, stage_records, fleet_records, finding_samples, resource_samples, observations, server_inventory, export_observations, parity};

#[cfg(test)]
mod tests;
