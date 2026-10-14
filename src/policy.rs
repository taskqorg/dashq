//! Typed host-policy operations (dashq.md D14, docs/design/resource-controls.md § Route) on the TaskQ wire
//! `taskq.resources/1`. A signed-in actor holding an explicit resource grant submits, looks up and cancels one
//! typed change per target; the target's enrolled TaskQ service fetches them through `exchange` and reports
//! receipts. DashQ never edits a host config: `applied` comes only from a fresh matching effective readback.
//! No shell, path or generic command field exists anywhere in this module.

use crate::enroll::{self, Enroll, Req, Resp};
use crate::store::{self, Store};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::Mutex;

pub const SCHEMA: &str = "taskq.resources/1";
const PREFIX: &str = "/api/taskq/resources/v1/";
/// The device-credential route lives under the machine prefix with the enrollment routes (#110).
pub const EXCHANGE: &str = "/machine/resources/v1/exchange";
/// v1 bounds (TaskQ shared-resource-controls.md § Bounds and receipts).
const MAX_BODY: usize = 16 << 10;
const PENDING_INSTALLATION: i64 = 32;
const PENDING_ACTOR: i64 = 8;
const EXCHANGE_ITEMS: usize = 8;
const EXCHANGE_SPACING: i64 = 5;
const LIFETIME: i64 = 300;
const AHEAD: i64 = 30;
const RETAIN: i64 = 86400;
const KEEP: i64 = 4096;
const FRESH: i64 = 60;
pub const RUNTIMES: [&str; 3] = ["claude", "codex", "codex-app"];
const REASONS: [&str; 9] = ["unauthorized", "revoked", "scope", "unsupported", "invalid", "conflict", "replay_mismatch", "busy", "clock_unknown"];

/// Per-process guard: last exchange per installation.
// ponytail: in-memory spacing, reset by a restart; the host loop keeps its own 5 s floor
#[derive(Default)]
pub struct State {
    exchanged: Mutex<HashMap<String, i64>>,
}

pub struct Ctx<'a> {
    pub enroll: &'a Result<Enroll, String>,
    /// Config `resources: true`; off until the target-host adapter is qualified (taskq#931).
    pub enabled: bool,
    pub public_host: &'a str,
    pub state: &'a State,
}

fn reply(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

/// A refusal before acceptance: only `{schema, reason}`, never reflected input.
fn refuse(reason: &str) -> Resp {
    let status = match reason {
        "invalid" => "400 Bad Request",
        "unauthorized" => "401 Unauthorized",
        "revoked" | "scope" => "403 Forbidden",
        "unknown_operation" => "404 Not Found",
        "conflict" | "replay_mismatch" => "409 Conflict",
        "expired" => "410 Gone",
        "too_large" => "413 Content Too Large",
        "unsupported" => "422 Unprocessable Content",
        "busy" => "429 Too Many Requests",
        _ => "503 Service Unavailable",
    };
    reply(status, json!({"schema": SCHEMA, "reason": if reason == "too_large" { "invalid" } else { reason }}))
}

fn store_error(e: store::Error) -> Resp {
    refuse(if e == store::Error::Busy { "busy" } else { "unavailable" })
}

pub fn route(c: &Ctx, r: &Req, now: i64) -> Option<Resp> {
    if r.path == EXCHANGE {
        return Some(if r.method == "POST" { exchange(c, r, now) } else { reply("404 Not Found", json!({"schema": SCHEMA, "reason": "invalid"})) });
    }
    let rest = r.path.strip_prefix(PREFIX)?;
    let op = rest.strip_prefix("operations/");
    let x = match (r.method, rest, op.map(|o| o.split_once('/'))) {
        ("GET", "grants", _) => grants(c, r, now),
        ("POST", "operations", _) => submit(c, r, now),
        ("GET", _, Some(None)) => lookup(c, r, op.unwrap_or(""), now),
        ("POST", _, Some(Some((id, "cancel")))) => cancel(c, r, id, now),
        _ => return Some(reply("404 Not Found", json!({"schema": SCHEMA, "reason": "invalid"}))),
    };
    Some(x)
}

/// JSON that refuses duplicate object keys at any depth (serde_json keeps the last one silently).
pub fn strict(b: &[u8]) -> Option<Value> {
    struct Strict(Value);
    struct V;
    impl<'de> serde::de::Visitor<'de> for V {
        type Value = Value;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("JSON")
        }
        fn visit_bool<E>(self, b: bool) -> Result<Value, E> {
            Ok(b.into())
        }
        fn visit_i64<E>(self, n: i64) -> Result<Value, E> {
            Ok(n.into())
        }
        fn visit_u64<E>(self, n: u64) -> Result<Value, E> {
            Ok(n.into())
        }
        fn visit_f64<E>(self, n: f64) -> Result<Value, E> {
            Ok(n.into())
        }
        fn visit_str<E>(self, s: &str) -> Result<Value, E> {
            Ok(s.into())
        }
        fn visit_unit<E>(self) -> Result<Value, E> {
            Ok(Value::Null)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
            let mut v = Vec::new();
            while let Some(Strict(x)) = a.next_element()? {
                v.push(x);
            }
            Ok(Value::Array(v))
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
            let mut m = Map::new();
            while let Some(k) = a.next_key::<String>()? {
                if m.contains_key(&k) {
                    return Err(serde::de::Error::custom("duplicate key"));
                }
                let Strict(x) = a.next_value()?;
                m.insert(k, x);
            }
            Ok(Value::Object(m))
        }
    }
    impl<'de> serde::Deserialize<'de> for Strict {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            d.deserialize_any(V).map(Strict)
        }
    }
    serde_json::from_slice::<Strict>(b).ok().map(|s| s.0)
}

/// Canonical JSON: keys sorted at every depth, so key order never makes a replay look changed.
fn canon(v: &Value) -> String {
    fn sort(v: &Value) -> Value {
        match v {
            Value::Object(o) => {
                let mut k: Vec<&String> = o.keys().collect();
                k.sort();
                Value::Object(k.into_iter().map(|k| (k.clone(), sort(&o[k]))).collect())
            }
            Value::Array(a) => Value::Array(a.iter().map(sort).collect()),
            x => x.clone(),
        }
    }
    sort(v).to_string()
}

/// Exactly these keys, none missing, none extra.
fn keys(v: &Value, want: &[&str]) -> bool {
    v.as_object().is_some_and(|o| o.len() == want.len() && want.iter().all(|k| o.contains_key(*k)))
}

pub fn rid(v: &Value) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}

fn int(v: &Value) -> Option<i64> {
    v.as_u64().and_then(|n| i64::try_from(n).ok())
}

fn u32_(v: &Value) -> bool {
    v.as_u64().is_some_and(|n| n <= u32::MAX as u64)
}

fn target(t: &Value) -> bool {
    keys(t, &["installation_id", "generation", "project_id", "runtime_id"])
        && t["installation_id"].as_str().is_some_and(|i| i.len() == 64 && i.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
        && int(&t["generation"]).is_some_and(|g| g >= 1)
        && (t["project_id"].is_null() || rid(&t["project_id"]))
        && (t["runtime_id"].is_null() || t["runtime_id"].as_str().is_some_and(|r| RUNTIMES.contains(&r)) && !t["project_id"].is_null())
}

/// Typed changes for one target under `schema`: `invalid` for a malformed value or a field at the wrong scope,
/// `unsupported` for a field the legacy capacity schema lacks.
fn changes(c: &Value, schema: &str, t: &Value) -> Result<(), &'static str> {
    let Some(o) = c.as_object().filter(|o| !o.is_empty()) else { return Err("invalid") };
    let scope = match (t["project_id"].is_null(), t["runtime_id"].is_null()) {
        (true, _) => "host",
        (false, true) => "project",
        _ => "runtime",
    };
    let admission = schema == "taskq.admission/1";
    let gate = |g: &Value| {
        keys(g, &["enabled", "cpu_max", "memory_min"])
            && g["enabled"].is_boolean()
            && int(&g["cpu_max"]).is_some_and(|n| (10..=100).contains(&n))
            && int(&g["memory_min"]).is_some_and(|n| (1..=95).contains(&n))
    };
    for (k, x) in o {
        let (ok, at, needs_admission) = match k.as_str() {
            "mode" => (matches!(x.as_str(), Some("auto" | "fixed")), scope != "runtime", true),
            "maximum" => (x.is_null() || u32_(x), scope != "runtime", true),
            // legacy limits have no null: an absent runtime limit stays disabled
            "runtime_limit" => (u32_(x) || (admission && x.is_null()), scope == "runtime", false),
            "load_gate" => (gate(x), scope == "project", false),
            _ => return Err("invalid"),
        };
        if !ok || !at {
            return Err("invalid");
        }
        if needs_admission && !admission {
            return Err("unsupported");
        }
    }
    // Auto cannot disable the gate
    if o.get("mode").is_some_and(|m| m == "auto") && o.get("load_gate").is_some_and(|g| g["enabled"] == false) {
        return Err("invalid");
    }
    Ok(())
}

const SUBMIT: [&str; 10] = ["schema", "op_id", "actor_id", "grant_id", "target", "policy_schema", "expected_revision", "issued_at", "expires_at", "changes"];
const CANCEL: [&str; 5] = ["schema", "op_id", "actor_id", "grant_id", "target"];
const RECEIPT: [&str; 8] = ["schema", "op_id", "target", "state", "reason", "saved_revision", "effective_revision", "observed_at"];

fn check_submit(v: &Value) -> Result<(), &'static str> {
    let ok = keys(v, &SUBMIT)
        && v["schema"] == SCHEMA
        && ["op_id", "actor_id", "grant_id", "expected_revision"].iter().all(|k| rid(&v[*k]))
        && target(&v["target"])
        && v["policy_schema"].is_string()
        && int(&v["issued_at"]).is_some()
        && int(&v["expires_at"]).is_some();
    if !ok {
        return Err("invalid");
    }
    let schema = v["policy_schema"].as_str().unwrap_or("");
    if !["taskq.capacity/1", "taskq.admission/1"].contains(&schema) {
        return Err("unsupported");
    }
    changes(&v["changes"], schema, &v["target"])
}

/// A resource grant with what makes it usable now: not revoked or expired, its subject still a workspace member,
/// its enrollment generation still live in that workspace.
struct Grant {
    actor: String,
    subject: String,
    installation: String,
    generation: i64,
    projects: Vec<String>,
    runtimes: Vec<String>,
    host: bool,
    expires: i64,
    revoked: bool,
    member: bool,
    live: bool,
}

impl Grant {
    fn active(&self, now: i64) -> bool {
        !self.revoked && self.expires > now && self.member && self.live
    }

    /// Host-wide needs `host_policy`; a project its membership; a runtime both memberships.
    fn covers(&self, t: &Value) -> bool {
        let has = |l: &[String], v: &Value| v.as_str().is_some_and(|x| l.iter().any(|y| y == x));
        t["installation_id"] == self.installation.as_str()
            && t["generation"] == self.generation
            && match (t["project_id"].is_null(), t["runtime_id"].is_null()) {
                (true, _) => self.host,
                (false, true) => has(&self.projects, &t["project_id"]),
                _ => has(&self.projects, &t["project_id"]) && has(&self.runtimes, &t["runtime_id"]),
            }
    }
}

fn grant(tx: &Transaction, id: &str) -> Result<Option<Grant>, store::Error> {
    Ok(tx
        .query_row(
            "SELECT g.actor_id, g.subject, g.installation, g.generation, g.projects, g.runtimes, g.host_policy, g.expires_at, g.revoked,
               EXISTS (SELECT 1 FROM members m WHERE m.workspace = g.workspace AND m.subject = g.subject),
               EXISTS (SELECT 1 FROM grants e WHERE e.installation = g.installation AND e.generation = g.generation AND e.workspace = g.workspace
                 AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = e.installation AND r.generation = e.generation))
             FROM resource_grants g WHERE g.grant_id = ?1",
            [id],
            |r| {
                let list = |s: String| serde_json::from_str::<Vec<String>>(&s).unwrap_or_default();
                Ok(Grant {
                    actor: r.get(0)?,
                    subject: r.get(1)?,
                    installation: r.get(2)?,
                    generation: r.get(3)?,
                    projects: list(r.get(4)?),
                    runtimes: list(r.get(5)?),
                    host: r.get(6)?,
                    expires: r.get(7)?,
                    revoked: r.get(8)?,
                    member: r.get(9)?,
                    live: r.get(10)?,
                })
            },
        )
        .optional()?)
}

/// The verified actor's grant named by a request; claimed actor and grant fields never authorize themselves.
fn authorize(tx: &Transaction, subject: &str, v: &Value, now: i64) -> Result<Result<Grant, &'static str>, store::Error> {
    let g = grant(tx, v["grant_id"].as_str().unwrap_or(""))?;
    Ok(match g {
        Some(g) if g.subject == subject && v["actor_id"] == g.actor.as_str() => {
            if g.revoked || g.expires <= now || !g.live {
                Err("revoked")
            } else if !g.member || !g.covers(&v["target"]) {
                Err("scope")
            } else {
                Ok(g)
            }
        }
        _ => Err("unauthorized"),
    })
}

struct Op {
    id: String,
    generation: i64,
    actor: String,
    grant: String,
    submit: String,
    expires: i64,
    delivered: bool,
    cancel: bool,
    state: String,
    receipt: Value,
}

fn load(tx: &Transaction, id: &str) -> Result<Option<Op>, store::Error> {
    Ok(tx
        .query_row(
            "SELECT op_id, generation, actor_id, grant_id, submit, expires_at, delivered, cancel, state, receipt FROM resource_ops WHERE op_id = ?1",
            [id],
            |r| {
                Ok(Op {
                    id: r.get(0)?,
                    generation: r.get(1)?,
                    actor: r.get(2)?,
                    grant: r.get(3)?,
                    submit: r.get(4)?,
                    expires: r.get(5)?,
                    delivered: r.get(6)?,
                    cancel: r.get(7)?,
                    state: r.get(8)?,
                    receipt: serde_json::from_str(&r.get::<_, String>(9)?).unwrap_or(Value::Null),
                })
            },
        )
        .optional()?)
}

fn set(tx: &Transaction, op: &mut Op, state: &str, reason: Value, now: i64) -> Result<(), store::Error> {
    op.state = state.into();
    op.receipt["state"] = json!(state);
    op.receipt["reason"] = reason;
    tx.execute(
        "UPDATE resource_ops SET state = ?2, receipt = ?3, cancel = ?4, delivered = ?5, at = ?6 WHERE op_id = ?1",
        params![op.id, state, op.receipt.to_string(), op.cancel, op.delivered, now],
    )?;
    Ok(())
}

/// An undelivered pending operation ends here without any write: cancelled, expired at its deadline, or
/// rejected once its grant stops being active.
fn settle(tx: &Transaction, op: &mut Op, now: i64) -> Result<(), store::Error> {
    if op.state != "pending" || op.delivered {
        return Ok(());
    }
    let g = grant(tx, &op.grant)?;
    if op.cancel {
        set(tx, op, "cancelled", Value::Null, now)
    } else if now >= op.expires {
        set(tx, op, "expired", Value::Null, now)
    } else if !g.is_some_and(|g| g.active(now) && g.generation == op.generation) {
        set(tx, op, "rejected", json!("revoked"), now)
    } else {
        Ok(())
    }
}

/// Signed-in browser actor; a mutation also needs this dashboard's Origin, JSON and a CSRF token for the actor.
fn browser<'a>(c: &'a Ctx, r: &'a Req, mutation: bool, now: i64) -> Result<(&'a Enroll, &'a str), Resp> {
    let (true, Ok(e)) = (c.enabled, c.enroll) else { return Err(refuse("unavailable")) };
    let Some(subject) = r.header(enroll::SUBJECT).filter(|s| enroll::valid_subject(s)) else { return Err(refuse("unauthorized")) };
    if mutation {
        let origin = format!("https://{}", c.public_host);
        let csrf = r.header("x-dashq-csrf").unwrap_or("");
        if r.header("origin") != Some(origin.as_str())
            || !r.header("content-type").is_some_and(|t| t.starts_with("application/json"))
            || ![now / 86400, now / 86400 - 1].iter().any(|d| e.token_ok(&purpose(subject, *d), csrf))
        {
            return Err(refuse("unauthorized"));
        }
        if r.body.len() > MAX_BODY {
            return Err(refuse("too_large"));
        }
    }
    Ok((e, subject))
}

fn purpose(subject: &str, day: i64) -> String {
    format!("{subject}\nresources\n{day}")
}

fn write(e: &Enroll, f: impl FnOnce(&Transaction) -> Result<Resp, store::Error>) -> Resp {
    e.store().write(f).unwrap_or_else(store_error)
}

/// `GET grants`: the actor's CSRF token and its usable grants, so the browser can name them in a submit.
fn grants(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, false, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    let csrf = e.token_for(&purpose(subject, now / 86400));
    write(e, |tx| {
        let ids: Vec<String> = tx.prepare("SELECT grant_id FROM resource_grants WHERE subject = ?1 ORDER BY grant_id")?.query_map([subject], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(g) = grant(tx, &id)?.filter(|g| g.active(now)) {
                out.push(json!({"grant_id": id, "actor_id": g.actor, "installation_id": g.installation, "generation": g.generation,
                    "project_ids": g.projects, "runtime_ids": g.runtimes, "host_policy": g.host, "expires_at": g.expires, "revoked": false}));
            }
        }
        Ok(reply("200 OK", json!({"schema": SCHEMA, "csrf": csrf, "grants": out})))
    })
}

fn submit(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, true, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    let Some(v) = strict(r.body) else { return refuse("invalid") };
    if let Err(why) = check_submit(&v) {
        return refuse(why);
    }
    let body = canon(&v);
    write(e, |tx| {
        let g = match authorize(tx, subject, &v, now)? {
            Ok(g) => g,
            Err(why) => return Ok(refuse(why)),
        };
        let id = v["op_id"].as_str().unwrap_or("");
        // the op ID binds the whole submit: the same object repeats its latest receipt, any change is refused
        if let Some(mut op) = load(tx, id)? {
            if op.actor != g.actor || op.submit != body {
                return Ok(refuse("replay_mismatch"));
            }
            settle(tx, &mut op, now)?;
            return Ok(reply("200 OK", op.receipt));
        }
        let (issued, expires) = (int(&v["issued_at"]).unwrap_or(0), int(&v["expires_at"]).unwrap_or(0));
        if issued > now + AHEAD || !(1..=LIFETIME).contains(&(expires - issued)) {
            return Ok(refuse("invalid"));
        }
        if expires <= now {
            return Ok(refuse("expired"));
        }
        tx.execute("DELETE FROM resource_ops WHERE expires_at + ?1 < ?2 AND state != 'pending'", params![RETAIN, now])?;
        let count = |sql: &str, p: &[&dyn rusqlite::ToSql]| tx.query_row(sql, p, |r| r.get::<_, i64>(0));
        let inst = g.installation.as_str();
        let pending = count("SELECT COUNT(*) FROM resource_ops WHERE installation = ?1 AND state = 'pending'", &[&inst])?;
        let mine = count("SELECT COUNT(*) FROM resource_ops WHERE installation = ?1 AND actor_id = ?2 AND state = 'pending'", &[&inst, &g.actor])?;
        let kept = count("SELECT COUNT(*) FROM resource_ops WHERE installation = ?1", &[&inst])?;
        // never evict an unexpired binding: a full table refuses new IDs
        if pending >= PENDING_INSTALLATION || mine >= PENDING_ACTOR || kept >= KEEP {
            return Ok(refuse("busy"));
        }
        let receipt = json!({"schema": SCHEMA, "op_id": id, "target": v["target"], "state": "pending", "reason": null, "saved_revision": null, "effective_revision": null, "observed_at": null});
        tx.execute(
            "INSERT INTO resource_ops VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 0, 'pending', ?8, ?9)",
            params![id, inst, g.generation, g.actor, v["grant_id"].as_str(), body, expires, receipt.to_string(), now],
        )?;
        Ok(reply("202 Accepted", receipt))
    })
}

/// The actor's own operation, from an active grant; another actor's reads as unknown.
fn lookup(c: &Ctx, r: &Req, id: &str, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, false, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    write(e, |tx| {
        let Some(mut op) = load(tx, id)? else { return Ok(refuse("unknown_operation")) };
        match grant(tx, &op.grant)? {
            Some(g) if g.subject == subject && g.actor == op.actor => {
                if !g.active(now) {
                    return Ok(refuse("revoked"));
                }
            }
            _ => return Ok(refuse("unknown_operation")),
        }
        settle(tx, &mut op, now)?;
        Ok(reply("200 OK", op.receipt))
    })
}

/// Cancel before delivery ends the operation; after delivery the host serializes it with its write, and a
/// completed write is reported as such, never as a rollback.
fn cancel(c: &Ctx, r: &Req, id: &str, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, true, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    let Some(v) = strict(r.body).filter(|v| keys(v, &CANCEL) && v["schema"] == SCHEMA && v["op_id"] == id && rid(&v["actor_id"]) && rid(&v["grant_id"]) && target(&v["target"])) else {
        return refuse("invalid");
    };
    write(e, |tx| {
        if let Err(why) = authorize(tx, subject, &v, now)? {
            return Ok(refuse(why));
        }
        let Some(mut op) = load(tx, id)? else { return Ok(refuse("unknown_operation")) };
        let submitted: Value = serde_json::from_str(&op.submit).unwrap_or(Value::Null);
        if v["actor_id"] != op.actor.as_str() || v["grant_id"] != op.grant.as_str() || v["target"] != submitted["target"] {
            return Ok(refuse("replay_mismatch"));
        }
        if op.state == "pending" {
            op.cancel = true;
            tx.execute("UPDATE resource_ops SET cancel = 1 WHERE op_id = ?1", [id])?;
            settle(tx, &mut op, now)?;
        }
        Ok(reply("200 OK", op.receipt))
    })
}

/// A host receipt for one of its operations. Only a fresh effective readback equal to the saved revision makes
/// `applied`; a stale or future readback is unknown, another revision leaves it saved.
fn take_receipt(tx: &Transaction, installation: &str, generation: i64, rc: &Value, now: i64) -> Result<(), store::Error> {
    let Some(mut op) = load(tx, rc["op_id"].as_str().unwrap_or(""))? else { return Ok(()) };
    if op.receipt["target"] != rc["target"] || rc["target"]["installation_id"] != installation || op.generation != generation {
        return Ok(());
    }
    if ["applied", "rejected", "expired", "cancelled"].contains(&op.state.as_str()) {
        return Ok(());
    }
    let host = rc["state"].as_str().unwrap_or("");
    let saved = if rc["saved_revision"].is_null() { op.receipt["saved_revision"].clone() } else { rc["saved_revision"].clone() };
    let (state, reason) = match host {
        "pending" => return Ok(()),
        // expiry or cancel cannot undo a completed write
        "rejected" | "expired" | "cancelled" if op.state == "saved" => return Ok(()),
        "rejected" => ("rejected", rc["reason"].clone()),
        "expired" | "cancelled" => (host, Value::Null),
        _ if saved.is_null() => ("unknown", Value::Null),
        _ if rc["effective_revision"] == saved => {
            let fresh = int(&rc["observed_at"]).is_some_and(|t| t <= now && now - t <= FRESH);
            (if fresh { "applied" } else { "unknown" }, Value::Null)
        }
        "unknown" => ("unknown", Value::Null),
        _ => ("saved", Value::Null),
    };
    op.delivered = true;
    for k in ["saved_revision", "effective_revision", "observed_at"] {
        op.receipt[k] = if k == "saved_revision" { saved.clone() } else { rc[k].clone() };
    }
    set(tx, &mut op, state, reason, now)
}

fn receipt_ok(rc: &Value) -> bool {
    let rev = |v: &Value| v.is_null() || rid(v);
    keys(rc, &RECEIPT)
        && rc["schema"] == SCHEMA
        && rid(&rc["op_id"])
        && target(&rc["target"])
        && matches!(rc["state"].as_str(), Some("pending" | "saved" | "applied" | "rejected" | "expired" | "cancelled" | "unknown"))
        && (rc["reason"].is_null() || rc["reason"].as_str().is_some_and(|r| REASONS.contains(&r)))
        && (rc["state"] != "rejected" || !rc["reason"].is_null())
        && rev(&rc["saved_revision"])
        && rev(&rc["effective_revision"])
        && (rc["observed_at"].is_null() || int(&rc["observed_at"]).is_some())
}

/// `POST exchange`: the enrolled TaskQ service of one installation and generation reports receipts and receives
/// its pending submits and cancellations. Its credential authenticates the installation only; it cannot mint or
/// widen a write grant.
fn exchange(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (true, Ok(e)) = (c.enabled, c.enroll) else { return refuse("unavailable") };
    if r.body.len() > MAX_BODY {
        return refuse("too_large");
    }
    let Some(token) = r.header("authorization").and_then(|a| a.strip_prefix("Bearer ")).filter(|t| !t.is_empty()) else { return refuse("unauthorized") };
    let cred = match e.store().credential(&enroll::sha256(token)) {
        Ok(Some(c)) if !c.revoked && c.active => c.grant,
        Ok(_) => return refuse("unauthorized"),
        Err(x) => return store_error(x),
    };
    let Some(v) = strict(r.body) else { return refuse("invalid") };
    let receipts = v["receipts"].as_array().filter(|a| a.len() <= EXCHANGE_ITEMS && a.iter().all(receipt_ok));
    let (true, Some(receipts)) = (keys(&v, &["schema", "installation_id", "generation", "receipts"]) && v["schema"] == SCHEMA, receipts) else { return refuse("invalid") };
    if v["installation_id"] != cred.installation.as_str() || v["generation"] != cred.generation {
        return refuse("scope");
    }
    {
        let mut m = c.state.exchanged.lock().unwrap_or_else(|p| p.into_inner());
        if m.get(&cred.installation).is_some_and(|t| now - t < EXCHANGE_SPACING && now >= *t) {
            return refuse("busy");
        }
        m.insert(cred.installation.clone(), now);
    }
    write(e, |tx| {
        for rc in receipts {
            take_receipt(tx, &cred.installation, cred.generation, rc, now)?;
        }
        let ids: Vec<String> = tx
            .prepare("SELECT op_id FROM resource_ops WHERE installation = ?1 AND generation = ?2 AND state = 'pending' ORDER BY at, op_id")?
            .query_map(params![cred.installation, cred.generation], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let (mut ops, mut cancels) = (Vec::new(), Vec::new());
        for id in ids {
            let Some(mut op) = load(tx, &id)? else { continue };
            settle(tx, &mut op, now)?;
            if op.state != "pending" {
                continue;
            }
            // a grant revoked after delivery asks the host to drop the write it may still hold
            if op.delivered && !op.cancel && !grant(tx, &op.grant)?.is_some_and(|g| g.active(now)) {
                op.cancel = true;
                tx.execute("UPDATE resource_ops SET cancel = 1 WHERE op_id = ?1", [&op.id])?;
            }
            let s: Value = serde_json::from_str(&op.submit).unwrap_or(Value::Null);
            if op.cancel && cancels.len() < EXCHANGE_ITEMS {
                cancels.push(json!({"schema": SCHEMA, "op_id": op.id, "actor_id": op.actor, "grant_id": op.grant, "target": s["target"]}));
            } else if !op.cancel && now < op.expires && ops.len() < EXCHANGE_ITEMS {
                // resent until a receipt arrives; the host deduplicates by op ID
                tx.execute("UPDATE resource_ops SET delivered = 1 WHERE op_id = ?1", [&op.id])?;
                ops.push(s);
            }
        }
        Ok(reply("200 OK", json!({"schema": SCHEMA, "server_at": now, "operations": ops, "cancellations": cancels})))
    })
}

/// `dashq resources grant`: the host administrator issues a scoped write grant to a workspace member for one
/// enrolled installation's live generation. Returns `(grant_id, actor_id)`.
pub fn issue(s: &Store, workspace: &str, subject: &str, installation: &str, host: bool, projects: &[String], runtimes: &[String], expires: i64) -> Result<(String, String), String> {
    let ok_ids = projects.iter().all(|p| rid(&json!(p))) && runtimes.iter().all(|r| RUNTIMES.contains(&r.as_str()));
    if !enroll::id(workspace, 64) || !enroll::valid_subject(subject) || !ok_ids || (!host && projects.is_empty()) {
        return Err("resources grant: bad workspace, subject, project or runtime".into());
    }
    let mut b = [0u8; 16];
    ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut b).map_err(|_| "resources grant: no randomness")?;
    let id = format!("g-{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>());
    let actor = format!("actor-{}", enroll::sha256(subject)[..12].iter().map(|x| format!("{x:02x}")).collect::<String>());
    s.write(|tx| {
        let member: bool = tx.query_row("SELECT EXISTS (SELECT 1 FROM members WHERE workspace = ?1 AND subject = ?2)", [workspace, subject], |r| r.get(0))?;
        let generation: Option<i64> = tx
            .query_row(
                "SELECT MAX(g.generation) FROM grants g WHERE g.installation = ?1 AND g.workspace = ?2
                   AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)",
                [installation, workspace],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let Some(generation) = generation.filter(|_| member) else { return Ok(None) };
        tx.execute(
            "INSERT INTO resource_grants VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 0)",
            params![id, actor, subject, workspace, installation, generation, json!(projects).to_string(), json!(runtimes).to_string(), host, expires],
        )?;
        Ok(Some(()))
    })
    .map_err(|e| format!("dashq.db: {e:?}"))?
    .ok_or_else(|| "resources grant: subject is not a member, or the installation has no live connection in that workspace".to_string())?;
    Ok((id, actor))
}

/// Revokes a grant at once: undelivered operations under it never reach the host.
pub fn revoke(s: &Store, grant_id: &str) -> Result<bool, String> {
    s.write(|tx| Ok(tx.execute("UPDATE resource_grants SET revoked = 1 WHERE grant_id = ?1", [grant_id])? > 0)).map_err(|e| format!("dashq.db: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const HOST: &str = "dash.example.com";
    const FIXTURE: &str = include_str!("../fixture/taskq-resources/resource-policy-v1.json");
    const INST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CRED: &str = "device-credential-0123456789";

    struct T {
        d: PathBuf,
        e: Result<Enroll, String>,
        enabled: bool,
    }

    impl T {
        /// One workspace with operator `operator-1` holding the fixture grant over an enrolled, active installation.
        fn new(name: &str) -> T {
            let d = std::env::temp_dir().join(format!("dashq-policy-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Enroll::admin(&d, "ws-1", "github|1").unwrap();
            Enroll::admin(&d, "ws-2", "github|2").unwrap();
            Enroll::answerer(&d, "ws-1", "github|2", true).unwrap();
            let e = Enroll::open(&d, HOST);
            let s = e.as_ref().unwrap().store();
            let inst = store::Installation { id: INST, product: "taskq", runtime: "native", label: "mac-1" };
            let g = s.grant(&inst, "ws-1", "github|1", "[\"taskq-snapshot/1\"]", &enroll::sha256(CRED), 1).unwrap();
            assert_eq!(g.generation, 1);
            s.ack(&enroll::sha256(CRED), 1).unwrap();
            let f: Value = serde_json::from_str(FIXTURE).unwrap();
            let fg = &f["grant"];
            s.write(|tx| {
                tx.execute(
                    "INSERT INTO resource_grants VALUES (?1, ?2, 'github|1', 'ws-1', ?3, 1, ?4, ?5, 0, ?6, 0)",
                    params![fg["grant_id"].as_str(), fg["actor_id"].as_str(), INST, fg["project_ids"].to_string(), fg["runtime_ids"].to_string(), fg["expires_at"].as_i64()],
                )?;
                Ok(())
            })
            .unwrap();
            T { d, e, enabled: true }
        }

        fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str, now: i64) -> (u16, Value) {
            let state = State::default();
            let c = Ctx { enroll: &self.e, enabled: self.enabled, public_host: HOST, state: &state };
            let h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let r = route(&c, &Req { method, path, query: "", headers: &h, body: body.as_bytes() }, now).expect("resource route");
            (r.status[..3].parse().unwrap(), serde_json::from_slice(&r.body).unwrap_or(Value::Null))
        }

        /// As the browser posts it: same origin, JSON and the actor's CSRF token.
        fn post(&self, subject: &str, path: &str, body: &Value, now: i64) -> (u16, Value) {
            let csrf = self.e.as_ref().unwrap().token_for(&purpose(subject, now / 86400));
            let h = [(enroll::SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/json"), ("x-dashq-csrf", csrf.as_str())];
            self.call("POST", path, &h, &body.to_string(), now)
        }

        fn exchange(&self, receipts: Value, now: i64) -> (u16, Value) {
            let b = json!({"schema": SCHEMA, "installation_id": INST, "generation": 1, "receipts": receipts});
            self.call("POST", EXCHANGE, &[("authorization", &format!("Bearer {CRED}"))], &b.to_string(), now)
        }

        fn stored(&self, id: &str) -> Value {
            let c = rusqlite::Connection::open(self.d.join("dashq.db")).unwrap();
            c.query_row("SELECT receipt FROM resource_ops WHERE op_id = ?1", [id], |r| r.get::<_, String>(0)).map_or(Value::Null, |s| serde_json::from_str(&s).unwrap())
        }
    }

    /// What a trace step observes: a refusal's reason, a rejected receipt's reason, else the receipt state.
    fn outcome(v: &Value) -> String {
        match (v["state"].as_str(), v["reason"].as_str()) {
            (Some("rejected"), Some(r)) | (None, Some(r)) => r.into(),
            (Some(s), _) => s.into(),
            _ => "none".into(),
        }
    }

    // chain: TaskQ's shared `taskq.resources/1` traces replayed against DashQ's operation store with a simulated
    // target host (A2). Review, edit and pre-confirm cancel are browser-component steps and post nothing here.
    #[test]
    fn shared_traces_hold_against_the_dashq_authority() {
        let f: Value = serde_json::from_str(FIXTURE).unwrap();
        let submit = f["submit"].clone();
        let id = submit["op_id"].as_str().unwrap();
        let ops = format!("{PREFIX}operations");
        let host_receipt = |state: &str, reason: Value, eff: &str, at: i64| {
            json!({"schema": SCHEMA, "op_id": id, "target": submit["target"], "state": state, "reason": reason, "saved_revision": if state == "rejected" { Value::Null } else { json!("r2") }, "effective_revision": eff, "observed_at": at})
        };
        for (i, trace) in f["traces"].as_array().unwrap().iter().enumerate() {
            let t = T::new(&format!("trace{i}"));
            let name = trace["name"].as_str().unwrap();
            let (mut now, mut reviewed, mut confirmed, mut external_edit) = (100i64, false, false, false);
            for step in trace["steps"].as_array().unwrap() {
                let want = step["expect"].as_str().unwrap();
                let got = match step["action"].as_str().unwrap() {
                    "review" => {
                        reviewed = true;
                        "reviewed".into()
                    }
                    "edit" => {
                        reviewed = false;
                        "draft".into()
                    }
                    "confirm" if !reviewed => "draft".into(),
                    "cancel" if !confirmed => "cancelled".into(),
                    "revoke" => {
                        revoke(t.e.as_ref().unwrap().store(), "grant-1").unwrap();
                        if confirmed { outcome(&t.stored(id)) } else { "draft".into() }
                    }
                    "time" => {
                        now = step["at"].as_i64().unwrap();
                        outcome(&t.stored(id))
                    }
                    "external_edit" => {
                        external_edit = true;
                        outcome(&t.stored(id))
                    }
                    "confirm" | "retry" => {
                        let (_, v) = t.post("github|1", &ops, &submit, now);
                        confirmed = true;
                        outcome(&v)
                    }
                    "mismatch" => {
                        let mut m = submit.clone();
                        m["changes"]["runtime_limit"] = json!(1);
                        let (s, v) = t.post("github|1", &ops, &m, now);
                        assert_eq!(s, 409, "{name}");
                        outcome(&v)
                    }
                    "lose_reply" => "unknown".into(),
                    "cancel" => {
                        let (_, v) = t.post("github|1", &format!("{ops}/{id}/cancel"), &f["cancel"], now);
                        outcome(&v)
                    }
                    "apply" => {
                        // the host takes what DashQ delivers and writes it, unless its revision moved
                        let (s, x) = t.exchange(json!([]), now);
                        assert_eq!(s, 200, "{name}");
                        if let Some(op) = x["operations"].as_array().unwrap().first() {
                            assert_eq!(op, &submit, "{name}: delivered unchanged");
                            let rc = if external_edit { host_receipt("rejected", json!("conflict"), "r1", now) } else { host_receipt("saved", Value::Null, "r1", now) };
                            assert_eq!(t.exchange(json!([rc]), now + 5).0, 200);
                        }
                        outcome(&t.stored(id))
                    }
                    "readback" => {
                        let rc = host_receipt("applied", Value::Null, step["revision"].as_str().unwrap(), step["at"].as_i64().unwrap());
                        assert_eq!(t.exchange(json!([rc]), now).0, 200);
                        outcome(&t.stored(id))
                    }
                    a => panic!("{name}: unknown action {a}"),
                };
                assert_eq!(got, want, "{name}: {step}");
            }
        }
    }

    // chain: A1, the write route accepts only a signed-in browser actor holding an active, scoped resource grant;
    // observation credentials, answer capability, cross-site posts and a disabled deployment write nothing.
    #[test]
    fn only_a_scoped_resource_grant_authorizes() {
        let f: Value = serde_json::from_str(FIXTURE).unwrap();
        let mut t = T::new("auth");
        let ops = format!("{PREFIX}operations");
        let s = f["submit"].clone();
        let with = |k: &str, v: Value| {
            let mut x = s.clone();
            x[k] = v;
            x
        };
        let with2 = |a: (&str, Value), b: (&str, Value)| {
            let mut x = with(a.0, a.1);
            x[b.0] = b.1;
            x
        };
        // the grant listing gives the actor's grants and its token; another subject sees none
        let (st, g) = t.call("GET", &format!("{PREFIX}grants"), &[(enroll::SUBJECT, "operator-1")], "", 100);
        assert_eq!((st, g["grants"].as_array().unwrap().len()), (200, 0), "grants belong to the subject that holds them");
        let (st, g) = t.call("GET", &format!("{PREFIX}grants"), &[(enroll::SUBJECT, "github|1")], "", 100);
        assert_eq!((st, &g["grants"][0]["grant_id"]), (200, &json!("grant-1")));
        // the fixture's actor_id belongs to subject github|1; nobody else can use it
        assert_eq!(t.post("github|2", &ops, &s, 100), (401, json!({"schema": SCHEMA, "reason": "unauthorized"})), "answer capability is no resource grant");
        assert_eq!(t.post("github|1", &ops, &with("actor_id", json!("operator-2")), 100).0, 401);
        assert_eq!(t.post("github|1", &ops, &with("grant_id", json!("grant-9")), 100).0, 401);
        // scope: project and runtime membership, host policy, installation and generation
        let mut tg = s["target"].clone();
        tg["runtime_id"] = json!("claude");
        assert_eq!(t.post("github|1", &ops, &with("target", tg.clone()), 100).1["reason"], "scope");
        tg["runtime_id"] = Value::Null;
        tg["project_id"] = Value::Null;
        assert_eq!(t.post("github|1", &ops, &with2(("target", tg), ("changes", json!({"mode": "auto"}))), 100).1["reason"], "scope", "host-wide needs host_policy");
        let mut tg = s["target"].clone();
        tg["generation"] = json!(2);
        assert_eq!(t.post("github|1", &ops, &with("target", tg), 100).1["reason"], "scope");
        // cross-site, wrong type, missing token, a device credential instead of a browser identity
        let csrf = t.e.as_ref().unwrap().token_for(&purpose("github|1", 0));
        for h in [
            vec![(enroll::SUBJECT, "github|1"), ("origin", "https://evil.example"), ("content-type", "application/json"), ("x-dashq-csrf", csrf.as_str())],
            vec![(enroll::SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "text/plain"), ("x-dashq-csrf", csrf.as_str())],
            vec![(enroll::SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "application/json")],
            vec![("authorization", "Bearer device-credential-0123456789"), ("origin", "https://dash.example.com"), ("content-type", "application/json"), ("x-dashq-csrf", csrf.as_str())],
        ] {
            assert_eq!(t.call("POST", &ops, &h, &s.to_string(), 100).0, 401);
        }
        // the device credential cannot fetch an operation either way but through its own exchange
        assert_eq!(t.call("GET", &format!("{ops}/op-1"), &[("authorization", &format!("Bearer {CRED}"))], "", 100).0, 401);
        // typed fields only: no paths, commands or extra keys; duplicate keys and legacy aggregate scope refused
        assert_eq!(t.post("github|1", &ops, &with("command", json!("rm -rf /")), 100).0, 400);
        assert_eq!(t.post("github|1", &ops, &with("changes", json!({"path": "/etc/taskq.json"})), 100).0, 400);
        assert_eq!(t.post("github|1", &ops, &with("changes", json!({"mode": "auto"})), 100).0, 400, "mode is not a runtime field");
        assert_eq!(t.post("github|1", &ops, &with2(("policy_schema", json!("taskq.capacity/1")), ("changes", json!({"runtime_limit": null}))), 100).0, 400, "legacy has no null limit");
        assert_eq!(t.post("github|1", &ops, &with("policy_schema", json!("taskq.other/9")), 100).0, 422);
        for raw in f["regressions"]["duplicate_json"].as_array().unwrap() {
            let csrf = t.e.as_ref().unwrap().token_for(&purpose("github|1", 0));
            let h = [(enroll::SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "application/json"), ("x-dashq-csrf", csrf.as_str())];
            assert_eq!(t.call("POST", &ops, &h, raw.as_str().unwrap(), 100).0, 400);
        }
        for case in f["regressions"]["legacy_runtime_scope"].as_array().unwrap() {
            let mut tg = s["target"].clone();
            tg["project_id"] = case["project_id"].clone();
            tg["runtime_id"] = case["runtime_id"].clone();
            let (st, v) = t.post("github|1", &ops, &with2(("policy_schema", json!("taskq.capacity/1")), ("target", tg)), 100);
            assert_eq!(outcome(&v), case["expect"].as_str().unwrap(), "{case} ({st})");
        }
        // lifetime: 1..300 s, issued at most 30 s ahead, never accepted at or after its deadline
        assert_eq!(t.post("github|1", &ops, &with2(("op_id", json!("op-2")), ("expires_at", json!(401))), 100).0, 400);
        assert_eq!(t.post("github|1", &ops, &with("op_id", json!("op-3")), 400).0, 410);
        // a disabled deployment answers 503 and writes nothing
        t.enabled = false;
        assert_eq!(t.post("github|1", &ops, &with("op_id", json!("op-4")), 100).0, 503);
        assert_eq!(t.exchange(json!([]), 100).0, 503);
        // a revoked enrollment ends the grant with it
        t.enabled = true;
        t.e.as_ref().unwrap().store().revoke(INST, 50).unwrap();
        assert_eq!(t.post("github|1", &ops, &with("op_id", json!("op-5")), 100).1["reason"], "revoked");
        assert_eq!(t.exchange(json!([]), 100).0, 401);
    }

    // chain: A2 bounds and delivery, per-actor pending cap, exchange scope and spacing, and the host receipt
    // checks that keep `applied` tied to a fresh matching readback of the same target.
    #[test]
    fn delivery_is_bounded_scoped_and_applied_needs_matching_readback() {
        let f: Value = serde_json::from_str(FIXTURE).unwrap();
        let t = T::new("bounds");
        let ops = format!("{PREFIX}operations");
        for i in 0..PENDING_ACTOR {
            let mut s = f["submit"].clone();
            s["op_id"] = json!(format!("op-{i}"));
            assert_eq!(t.post("github|1", &ops, &s, 100).0, 202);
        }
        let mut s = f["submit"].clone();
        s["op_id"] = json!("op-9");
        assert_eq!(t.post("github|1", &ops, &s, 100).1["reason"], "busy");
        // exchange: at most 8 per body; another installation or generation is refused
        let (_, x) = t.exchange(json!([]), 100);
        assert_eq!(x["operations"].as_array().unwrap().len(), EXCHANGE_ITEMS);
        let b = json!({"schema": SCHEMA, "installation_id": INST, "generation": 2, "receipts": []});
        assert_eq!(t.call("POST", EXCHANGE, &[("authorization", &format!("Bearer {CRED}"))], &b.to_string(), 100).1["reason"], "scope");
        let state = State::default();
        let c = Ctx { enroll: &t.e, enabled: true, public_host: HOST, state: &state };
        let h = vec![("authorization".to_string(), format!("Bearer {CRED}"))];
        let body = json!({"schema": SCHEMA, "installation_id": INST, "generation": 1, "receipts": []}).to_string();
        let req = Req { method: "POST", path: EXCHANGE, query: "", headers: &h, body: body.as_bytes() };
        assert_eq!(route(&c, &req, 100).unwrap().status, "200 OK");
        assert_eq!(route(&c, &req, 103).unwrap().status, "429 Too Many Requests", "5 s between exchanges");
        // the old browser-prefixed exchange path is gone, no alias (#110)
        assert_eq!(route(&c, &Req { path: "/api/taskq/resources/v1/exchange", ..req }, 200).unwrap().status, "404 Not Found");
        // a readback for another target cannot satisfy the operation
        let mut rc = f["receipt"].clone();
        rc["op_id"] = json!("op-0");
        rc["target"]["runtime_id"] = json!("claude");
        rc["state"] = json!("applied");
        rc["effective_revision"] = json!("r2");
        t.exchange(json!([rc]), 100);
        assert_eq!(t.stored("op-0")["state"], "pending");
        // future readback is unknown; a saved write is never undone by a later cancel report
        let mut rc = f["receipt"].clone();
        rc["op_id"] = json!("op-1");
        rc["effective_revision"] = json!("r2");
        rc["observed_at"] = json!(160);
        t.exchange(json!([rc.clone()]), 100);
        assert_eq!((t.stored("op-1")["state"].as_str(), t.stored("op-1")["saved_revision"].as_str()), (Some("unknown"), Some("r2")));
        rc["observed_at"] = json!(100);
        t.exchange(json!([rc]), 100);
        assert_eq!(t.stored("op-1")["state"], "applied");
        // lookup belongs to the actor; another subject reads unknown
        assert_eq!(t.call("GET", &format!("{ops}/op-1"), &[(enroll::SUBJECT, "github|1")], "", 100).1["state"], "applied");
        assert_eq!(t.call("GET", &format!("{ops}/op-1"), &[(enroll::SUBJECT, "github|2")], "", 100).0, 404);
        // a malformed receipt refuses the whole exchange
        assert_eq!(t.exchange(json!([{"op_id": "op-2", "state": "applied"}]), 100).0, 400);
        // revoking after delivery sends a cancellation for what the host may still hold
        revoke(t.e.as_ref().unwrap().store(), "grant-1").unwrap();
        let (_, x) = t.exchange(json!([]), 110);
        assert!(x["operations"].as_array().unwrap().is_empty());
        assert_eq!(x["cancellations"].as_array().unwrap().len(), EXCHANGE_ITEMS - 1);
    }
}
