//! Guarded AgentQ configuration review and apply (dashq.md D16, docs/design/agentq-settings.md G2/G3, #63). A
//! signed-in actor asks the host for a plan of typed changes, reviews the host's exact diff and applies that plan;
//! the operations travel through the #96 store and `/machine/operations`. DashQ never edits a host config: the plan, the
//! outcome and the readback all come from AgentQ. Authority is the server's (`Enroll::configure`); the browser
//! names only the installation, a fresh operation ID and either the changes or the reviewed plan's operation.

use crate::enroll::{self, Change, Enroll, Plan, Req, Resp};
use crate::store;
use serde_json::{Value, json};

const PREFIX: &str = "/agentq/configure";
const MAX_BODY: usize = 16 << 10;
/// Operations listed per installation, newest first; older ones stay stored and readable by ID.
const LIST: i64 = 16;

pub struct Ctx<'a> {
    pub enroll: &'a Result<Enroll, String>,
    /// Config `agentq_configure: true`; off until AgentQ A5 qualification (D16).
    pub enabled: bool,
    pub public_host: &'a str,
}

fn reply(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

/// A refusal before anything is queued: only `{reason}`, never reflected input.
fn refuse(reason: &str) -> Resp {
    let status = match reason {
        "invalid" | "too_large" => "400 Bad Request",
        "unauthorized" => "401 Unauthorized",
        "authority" | "scope" | "no_grant" | "no_profiles" | "profile_out_of_scope" | "writes_disabled" => "403 Forbidden",
        "unknown_operation" => "404 Not Found",
        "replay_mismatch" | "stale_revision" | "not_planned" | "blocked" => "409 Conflict",
        "expired" => "410 Gone",
        "busy" => "429 Too Many Requests",
        _ => "503 Service Unavailable",
    };
    reply(status, json!({"reason": if reason == "too_large" { "invalid" } else { reason }}))
}

pub fn route(c: &Ctx, r: &Req, now: i64) -> Option<Resp> {
    let rest = r.path.strip_prefix(PREFIX).filter(|x| x.is_empty() || x.starts_with('/'))?;
    Some(match (r.method, rest) {
        ("GET", "") => list(c, r, now),
        ("POST", "") => submit(c, r, now),
        ("GET", id) => one(c, r, &id[1..], now),
        _ => refuse("invalid"),
    })
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
    format!("{subject}\nagentq.configure\n{day}")
}

fn store_error(x: store::Error) -> Resp {
    refuse(if x == store::Error::Busy { "busy" } else { "unavailable" })
}

/// `GET ?installation=I`: the actor's CSRF token and the operations of `I` it may read, newest first, so late
/// outcomes stay visible after a reload.
fn list(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, false, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    let q = enroll::form(r.query.as_bytes());
    let Some(inst) = q.get("installation").filter(|i| enroll::id(i, 128)) else { return refuse("invalid") };
    let ids = match e.store().operation_ids(inst, LIST) {
        Ok(x) => x,
        Err(x) => return store_error(x),
    };
    let mut ops = vec![];
    for id in ids {
        match status(e, subject, &id, now) {
            Ok(Some(s)) => ops.push(s),
            Ok(None) => {}
            Err(x) => return store_error(x),
        }
    }
    reply("200 OK", json!({"csrf": e.token_for(&purpose(subject, now / 86400)), "installation": inst, "operations": ops}))
}

fn one(c: &Ctx, r: &Req, id: &str, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, false, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    match status(e, subject, id, now) {
        Ok(Some(s)) => reply("200 OK", s),
        Ok(None) => refuse("unknown_operation"),
        Err(x) => store_error(x),
    }
}

/// The installation's current snapshot payload with its producer time and freshness (C5 180 s, D6).
fn current(e: &Enroll, installation: &str, now: i64) -> Result<(Value, Option<i64>, bool), store::Error> {
    Ok(match e.store().snapshot(installation)? {
        None => (Value::Null, None, false),
        Some(s) => {
            let p: Value = serde_json::from_str(&s.payload).unwrap_or_default();
            let fresh = !s.clock_uncertain && now - s.observed <= enroll::STALE_SECS as i64 && p["configuration"]["state"] == "fresh";
            (p, Some(s.observed), fresh)
        }
    })
}

fn profile<'a>(snapshot: &'a Value, id: &str) -> Option<&'a Value> {
    snapshot["configuration"]["profiles"].as_array()?.iter().find(|p| p["id"] == id)
}

/// `POST`: `{installation, operation, changes}` asks for a plan; `{installation, operation, plan}` applies the
/// reviewed plan of that earlier operation, with its changes, revision and digest taken from the store and the
/// host's plan, never from the browser. A retry with the same operation and body returns the same operation.
fn submit(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (e, subject) = match browser(c, r, true, now) {
        Ok(x) => x,
        Err(x) => return x,
    };
    let Some(v) = crate::policy::strict(r.body) else { return refuse("invalid") };
    let keys: Vec<&str> = v.as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
    let (Some(inst), Some(op)) = (v["installation"].as_str(), v["operation"].as_str()) else { return refuse("invalid") };
    let apply = keys.contains(&"plan");
    if keys.len() != 3 || !keys.contains(&if apply { "plan" } else { "changes" }) {
        return refuse("invalid");
    }
    let (snapshot, _, _) = match current(e, inst, now) {
        Ok(x) => x,
        Err(x) => return store_error(x),
    };
    // an apply replays the reviewed plan exactly: same actor, installation and changes, a writable unexpired host
    // plan, and a snapshot still at the plan's revision (a newer one invalidates the confirmation)
    let rec;
    let (changes, plan) = if apply {
        let Some(id) = v["plan"].as_str() else { return refuse("invalid") };
        rec = match e.store().operation(id) {
            Ok(Some(x)) if x.actor == enroll::actor_id(subject) && x.op["installation"] == inst && x.op["payload"]["kind"] == "plan" => x,
            Ok(_) => return refuse("unknown_operation"),
            Err(x) => return store_error(x),
        };
        let host = reply_of(rec.result.as_ref().unwrap_or(&Value::Null));
        if host["kind"] != "plan" || host["operation"] != id {
            return refuse("not_planned");
        }
        if host["writable"] != true || host["blocked"].as_array().is_none_or(|b| !b.is_empty()) {
            return refuse("blocked");
        }
        if host["expires_at"].as_i64().is_none_or(|x| now >= x) {
            return refuse("expired");
        }
        if snapshot["configuration"]["revision"] != host["revision"] {
            return refuse("stale_revision");
        }
        let (Some(revision), Some(digest)) = (host["revision"].as_str(), host["digest"].as_str()) else { return refuse("not_planned") };
        (rec.op["payload"]["changes"].as_array().cloned().unwrap_or_default(), Some(Plan { revision, digest }))
    } else {
        (v["changes"].as_array().cloned().unwrap_or_default(), None)
    };
    let mut typed = vec![];
    for x in &changes {
        let ok = x.as_object().is_some_and(|o| o.len() == 3);
        let (Some(profile), Some(field), Some(value), true) = (x["profile"].as_str(), x["field"].as_str(), x["value"].as_str(), ok) else { return refuse("invalid") };
        typed.push(Change { profile, field, value });
    }
    // G1: AgentQ's own write gate per profile (e.g. `not_released` until A5) disables review and apply
    if typed.iter().any(|x| profile(&snapshot, x.profile).is_some_and(|p| p["writes"]["enabled"] != true)) {
        return refuse("writes_disabled");
    }
    match e.configure(subject, inst, op, plan, &typed, now) {
        Ok(_) => match status(e, subject, op, now) {
            Ok(Some(s)) => reply("202 Accepted", s),
            Ok(None) => refuse("unknown_operation"),
            Err(x) => store_error(x),
        },
        Err(why) => refuse(why),
    }
}

/// AgentQ posts `{result, readback}` (CONFIGURATION.md § Enrolled delivery); the envelope is under `result`.
fn reply_of(posted: &Value) -> &Value {
    if posted["result"].is_object() { &posted["result"] } else { posted }
}

/// One operation in G2 words for its actor or the workspace administrator; `None` for anyone else.
/// `status`: plans `planning`, `planned` or `rejected`; applies `accepted` (stored or host-accepted, not yet
/// confirmed), `configured` (host applied and a fresh snapshot after the result shows every intended value),
/// `verified` (configured and AgentQ verified each changed profile), `rejected`, `partial` or `unknown`. `reason`
/// names why; an operation that never reached the host reads `rejected` with `expired` or `revoked`, as does a
/// delivered one revoked before its result.
pub fn status(e: &Enroll, subject: &str, id: &str, now: i64) -> Result<Option<Value>, store::Error> {
    let Some(rb) = e.readback(subject, id, now)? else { return Ok(None) };
    let Some(rec) = e.store().operation(id)? else { return Ok(None) };
    let changes = &rec.op["payload"]["changes"];
    let host = reply_of(rec.result.as_ref().unwrap_or(&Value::Null));
    let plan = rb["kind"] == "plan";
    let word = |x: &Value| x.as_str().map(String::from);
    let (status, reason): (&str, Option<String>) = match rb["state"].as_str().unwrap_or("") {
        "queued" | "delivered" => (if plan { "planning" } else { "accepted" }, None),
        "expired" => ("rejected", Some("expired".into())),
        "revoked" => ("rejected", Some("revoked".into())),
        "resulted" if plan && host["kind"] == "plan" => ("planned", None),
        "resulted" => match host["outcome"].as_str() {
            // the host's posted result is final (A3), so an accepted apply also reconciles from later snapshots
            Some(o @ ("accepted" | "applied")) if !plan => {
                let inst = rec.op["installation"].as_str().unwrap_or("");
                let (snapshot, observed, fresh) = current(e, inst, now)?;
                let after = observed.zip(rec.result_at).is_some_and(|(o, r)| o >= r);
                let each = |f: &dyn Fn(&Value, &Value) -> bool| {
                    changes.as_array().is_some_and(|cs| cs.iter().all(|c| profile(&snapshot, c["profile"].as_str().unwrap_or("")).is_some_and(|p| f(p, c))))
                };
                let matches = each(&|p, c| c["field"].as_str().is_some_and(|f| p[f] == c["value"]));
                if !(fresh && after) || (o == "accepted" && !matches) {
                    ("accepted", (o == "applied").then(|| "awaiting_readback".into()))
                } else if !matches {
                    ("unknown", Some("readback_mismatch".into()))
                } else if each(&|p, _| p["verification"]["state"] == "verified") {
                    ("verified", None)
                } else {
                    ("configured", None)
                }
            }
            Some(o @ ("partial" | "unknown" | "rejected")) => (o, word(&host["reason"])),
            _ => ("unknown", Some("invalid_result".into())),
        },
        // delivered, then revoked before any result: resolved rejected (#147); no result can arrive after the revoke
        _ => ("rejected", Some("revoked".into())),
    };
    let reason = reason.or_else(|| (status == "unknown").then(|| "no_reason".into()));
    Ok(Some(json!({"operation": id, "kind": rb["kind"], "installation": rb["installation"], "state": rb["state"], "status": status, "reason": reason,
        "expires_at": rb["expires_at"], "changes": changes, "plan": rec.op["payload"]["plan"], "host": if rec.result.is_some() { host.clone() } else { Value::Null }})))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Event, Installation, Kind};

    const HOST: &str = "dash.example.com";
    const T0: i64 = 1_800_000_000;
    const CRED: &str = "cred-c1";
    const INST: &str = "inst-c1";

    struct T {
        d: std::path::PathBuf,
        e: Result<Enroll, String>,
        enabled: bool,
    }

    /// A projection snapshot (fixture/agentq) of `INST` with writes enabled unless `released` is false.
    fn snapshot(revision: &str, released: bool, verified: bool, routing: &str) -> Value {
        let mut s: Value = serde_json::from_str::<Value>(include_str!("../fixture/agentq/configuration.json")).unwrap()["cases"]["profile_projection"].clone();
        let c = &mut s["configuration"];
        c["revision"] = json!(revision);
        c["incomplete"] = json!(false);
        for p in c["profiles"].as_array_mut().unwrap() {
            p["writes"] = json!({"enabled": released, "reasons": if released { json!([]) } else { json!(["not_released"]) }});
            if verified {
                p["verification"] = json!({"state": "verified", "reason": null});
            }
        }
        c["profiles"][0]["routing"] = json!(routing);
        s
    }

    impl T {
        /// Workspace ws-1 with administrator github|1 and member github|2; INST approved by github|1 with
        /// `agentq.configure` ticked, acknowledged and holding a released snapshot at revision r1.
        fn new(name: &str) -> T {
            let d = std::env::temp_dir().join(format!("dashq-configure-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Enroll::admin(&d, "ws-1", "github|1").unwrap();
            Enroll::admin(&d, "ws-2", "github|3").unwrap();
            let e = Enroll::open(&d, HOST);
            let s = e.as_ref().unwrap().store();
            s.add_member("ws-1", "github|2", "member").unwrap();
            let inst = Installation { id: INST, product: "agentq", runtime: "native", label: "mac-1" };
            s.grant(&inst, "ws-1", "github|1", "[\"agentq-snapshot/1\"]", &enroll::sha256(CRED), T0).unwrap();
            s.write(|tx| Ok(tx.execute("UPDATE grants SET capabilities = '[\"agentq.configure\"]' WHERE installation = ?1", [INST])?)).unwrap();
            s.ack(&enroll::sha256(CRED), T0).unwrap();
            let t = T { d, e, enabled: true };
            t.snap("s0", T0, snapshot("r1", true, false, "routed"));
            t
        }

        fn snap(&self, id: &str, observed: i64, payload: Value) {
            let p = payload.to_string();
            let ev = Event { id, schema: "agentq-snapshot/1", observed, kind: Kind::Snapshot, payload: &p };
            self.e.as_ref().unwrap().store().accept(&enroll::sha256(CRED), &[ev], observed).unwrap();
        }

        fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str, now: i64) -> (u16, Value) {
            let c = Ctx { enroll: &self.e, enabled: self.enabled, public_host: HOST };
            let h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let (path, query) = path.split_once('?').unwrap_or((path, ""));
            let r = route(&c, &Req { method, path, query, headers: &h, body: body.as_bytes() }, now).expect("configure route");
            (r.status[..3].parse().unwrap(), serde_json::from_slice(&r.body).unwrap_or(Value::Null))
        }

        /// As the browser posts it: same origin, JSON and the actor's CSRF token.
        fn post(&self, subject: &str, body: &Value, now: i64) -> (u16, Value) {
            let csrf = self.e.as_ref().unwrap().token_for(&purpose(subject, now / 86400));
            let h = [(enroll::SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/json"), ("x-dashq-csrf", csrf.as_str())];
            self.call("POST", PREFIX, &h, &body.to_string(), now)
        }

        fn get(&self, subject: &str, path: &str, now: i64) -> (u16, Value) {
            self.call("GET", path, &[(enroll::SUBJECT, subject)], "", now)
        }

        /// The host: pulls pending operations and posts `result` for `id` as AgentQ does (`{result, readback}`).
        fn host(&self, id: &str, result: Value, now: i64) {
            let s = self.e.as_ref().unwrap().store();
            let ops = s.pull(&enroll::sha256(CRED), "agentq.configure", now).unwrap().unwrap();
            let op = ops.iter().find(|o| o["operation"] == id).expect("delivered");
            let posted = json!({"result": result, "readback": {}}).to_string();
            assert_eq!(s.post_result(&enroll::sha256(CRED), id, op["digest"].as_str().unwrap(), &posted, now).unwrap(), None);
        }
    }

    fn op(n: u8) -> String {
        format!("{n:032x}")
    }

    fn plan_body(n: u8) -> Value {
        json!({"installation": INST, "operation": op(n), "changes": [{"profile": "0123456789abcdef", "field": "routing", "value": "direct"}]})
    }

    /// AgentQ's published plan reply (fixture/agentq/configuration-fixtures.json, first case) for operation `n` at
    /// `revision`, with `blocked` conflicts.
    fn host_plan(n: u8, revision: &str, blocked: &[&str], expires: i64) -> Value {
        let fx: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration-fixtures.json")).unwrap();
        let mut p = fx["cases"][0]["expect"].clone();
        for (k, v) in [("operation", json!(op(n))), ("installation", json!(INST)), ("generation", json!(1)), ("revision", json!(revision)), ("expires_at", json!(expires)),
            ("writable", json!(blocked.is_empty())), ("blocked", json!(blocked)), ("digest", json!(format!("{n:064x}"))), ("schema", json!(1))] {
            p[k] = v;
        }
        p
    }

    /// AgentQ's published result of `outcome` (fixture results) for operation `n`.
    fn result(n: u8, outcome: &str, reason: Value) -> Value {
        let fx: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration-fixtures.json")).unwrap();
        let mut r = fx["results"].as_array().unwrap().iter().find(|r| r["valid"] == true && r["result"]["outcome"] == outcome).map_or(fx["results"][0]["result"].clone(), |r| r["result"].clone());
        (r["operation"], r["outcome"], r["reason"]) = (json!(op(n)), json!(outcome), reason);
        r
    }

    // chain: G2/G3 review and apply, a guarded plan reaches the host, the host's exact diff is reviewed, the apply
    // carries that plan and every G2 outcome stays distinct, from accepted through configured and verified
    #[test]
    fn plan_review_apply_and_every_outcome() {
        let t = T::new("chain");
        let st = |n: u8, now| t.get("github|1", &format!("{PREFIX}/{}", op(n)), now).1;
        let words = |v: &Value| (v["status"].as_str().unwrap_or("").to_string(), v["reason"].as_str().unwrap_or("").to_string());
        let (s, v) = t.post("github|1", &plan_body(1), T0);
        assert_eq!((s, words(&v)), (202, ("planning".into(), "".into())));
        // a lost receipt: the same request returns the same operation, any change is refused
        assert_eq!(t.post("github|1", &plan_body(1), T0 + 1).1["operation"], op(1));
        let mut changed = plan_body(1);
        changed["changes"][0]["value"] = json!("routed");
        assert_eq!(t.post("github|1", &changed, T0 + 1), (409, json!({"reason": "replay_mismatch"})));
        // no apply before the host planned it
        let apply = |n: u8, plan: u8| json!({"installation": INST, "operation": op(n), "plan": op(plan)});
        assert_eq!(t.post("github|1", &apply(2, 1), T0 + 2), (409, json!({"reason": "not_planned"})));
        t.host(&op(1), host_plan(1, "r1", &[], T0 + 300), T0 + 3);
        let v = st(1, T0 + 3);
        assert_eq!((words(&v), &v["host"]["profiles"][0]["aliases"]), (("planned".into(), "".into()), &json!(["codex-cli", "codex-desktop"])));
        // apply: changes, revision and digest come from the stored plan, never from the browser
        let (s, v) = t.post("github|1", &apply(2, 1), T0 + 4);
        assert_eq!((s, words(&v)), (202, ("accepted".into(), "".into())));
        assert_eq!((&v["changes"], &v["plan"]), (&plan_body(1)["changes"], &json!({"revision": "r1", "digest": format!("{:064x}", 1)})));
        // one unresolved mutation per installation
        t.post("github|1", &plan_body(3), T0 + 5);
        t.host(&op(3), host_plan(3, "r1", &[], T0 + 305), T0 + 6);
        assert_eq!(t.post("github|1", &apply(4, 3), T0 + 6), (429, json!({"reason": "busy"})));
        // host accepted (its result is final): configured only once a fresh snapshot newer than it shows the values
        t.host(&op(2), result(2, "accepted", Value::Null), T0 + 7);
        assert_eq!(words(&st(2, T0 + 7)), ("accepted".into(), "".into()));
        // host applied: still only accepted until that readback arrives
        t.post("github|1", &apply(5, 3), T0 + 8);
        t.host(&op(5), result(5, "applied", Value::Null), T0 + 9);
        assert_eq!(words(&st(5, T0 + 9)), ("accepted".into(), "awaiting_readback".into()));
        t.snap("s1", T0 + 10, snapshot("r2", true, false, "direct"));
        assert_eq!((words(&st(5, T0 + 10)), words(&st(2, T0 + 10))), (("configured".into(), "".into()), ("configured".into(), "".into())));
        // stale readback is not proof; a verified profile is
        assert_eq!(words(&st(5, T0 + 10 + 181)), ("accepted".into(), "awaiting_readback".into()));
        t.snap("s2", T0 + 200, snapshot("r2", true, true, "direct"));
        assert_eq!(words(&st(5, T0 + 200)), ("verified".into(), "".into()));
        // a later out-of-band edit: the host said applied but the readback disagrees
        t.snap("s3", T0 + 210, snapshot("r3", true, false, "routed"));
        assert_eq!(words(&st(5, T0 + 210)), ("unknown".into(), "readback_mismatch".into()));
        // the reviewed plan's revision is gone: its confirmation is invalid; a new plan is required
        t.post("github|1", &plan_body(6), T0 + 211);
        t.host(&op(6), host_plan(6, "r2", &[], T0 + 500), T0 + 212);
        assert_eq!(t.post("github|1", &apply(7, 6), T0 + 213), (409, json!({"reason": "stale_revision"})));
        // partial, unknown and rejected results keep their reasons
        for (n, outcome, reason) in [(8u8, "partial", "write_failed"), (10, "unknown", "interrupted"), (12, "rejected", "stale_revision")] {
            t.post("github|1", &plan_body(n), T0 + 220 + n as i64);
            t.host(&op(n), host_plan(n, "r3", &[], T0 + 500), T0 + 220 + n as i64);
            assert_eq!(t.post("github|1", &apply(n + 1, n), T0 + 221 + n as i64).0, 202);
            t.host(&op(n + 1), result(n + 1, outcome, json!(reason)), T0 + 221 + n as i64);
            assert_eq!(words(&st(n + 1, T0 + 240)), (outcome.into(), reason.into()));
        }
        // a blocked or expired plan cannot be applied
        t.post("github|1", &plan_body(20), T0 + 250);
        t.host(&op(20), host_plan(20, "r3", &["project_override"], T0 + 500), T0 + 250);
        assert_eq!(t.post("github|1", &apply(21, 20), T0 + 251), (409, json!({"reason": "blocked"})));
        assert_eq!(t.post("github|1", &apply(21, 12), T0 + 600), (410, json!({"reason": "expired"})));
        // the list keeps late outcomes visible, newest first, and hands out the CSRF token
        let (s, v) = t.get("github|1", &format!("{PREFIX}?installation={INST}"), T0 + 260);
        assert_eq!((s, v["operations"][0]["operation"].clone(), v["operations"].as_array().unwrap().len()), (200, json!(op(20)), 12));
        assert_eq!(v["csrf"], t.e.as_ref().unwrap().token_for(&purpose("github|1", (T0 + 260) / 86400)));
        // operations and results survive a restart
        let id = op(5);
        let T { d, e, .. } = t;
        drop(e);
        let t = T { e: Enroll::open(&d, HOST), d, enabled: true };
        assert_eq!(t.get("github|1", &format!("{PREFIX}/{id}"), T0 + 260).1["status"], "unknown");
    }

    // chain: G3 guard, only the signed-in workspace administrator or approving member with the agentq.configure
    // grant reaches the host, through this dashboard's origin and CSRF, within the snapshot's profiles and AgentQ's
    // write gate; everyone else, and every malformed or extra field, is refused before anything is queued
    #[test]
    fn guard_scope_and_refusals_queue_nothing() {
        let mut t = T::new("guard");
        let body = plan_body(1);
        let csrf = t.e.as_ref().unwrap().token_for(&purpose("github|1", T0 / 86400));
        let h = |o: &'static str, ct: &'static str, c: &str| -> Vec<(String, String)> {
            vec![(enroll::SUBJECT.into(), "github|1".into()), ("origin".into(), o.into()), ("content-type".into(), ct.into()), ("x-dashq-csrf".into(), c.into())]
        };
        let raw = |h: Vec<(String, String)>, b: &str| {
            let hs: Vec<(&str, &str)> = h.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            t.call("POST", PREFIX, &hs, b, T0).0
        };
        // session, Origin, CSRF and JSON are each required
        assert_eq!(t.call("POST", PREFIX, &[], &body.to_string(), T0).0, 401);
        assert_eq!(raw(h("https://evil.example", "application/json", &csrf), &body.to_string()), 401);
        assert_eq!(raw(h("https://dash.example.com", "application/json", "00"), &body.to_string()), 401);
        assert_eq!(raw(h("https://dash.example.com", "text/plain", &csrf), &body.to_string()), 401);
        assert_eq!(raw(h("https://dash.example.com", "application/json", &csrf), &"x".repeat(MAX_BODY + 1)), 400);
        // another subject's token is not this subject's
        let other = t.e.as_ref().unwrap().token_for(&purpose("github|2", T0 / 86400));
        assert_eq!(raw(h("https://dash.example.com", "application/json", &other), &body.to_string()), 401);
        // typed fields only: a path, command, file target, provider or extra key never queues
        for b in [
            json!({"installation": INST, "operation": op(1), "changes": [{"profile": "0123456789abcdef", "field": "routing", "value": "direct", "file": "/x"}]}),
            json!({"installation": INST, "operation": op(1), "changes": [{"profile": "0123456789abcdef", "field": "provider", "value": "openai"}]}),
            json!({"installation": INST, "operation": op(1), "changes": [{"profile": "0123456789abcdef", "field": "mcp", "value": "/bin/sh"}]}),
            json!({"installation": INST, "operation": op(1), "changes": [], "command": "sh"}),
            json!({"installation": INST, "operation": "op-1", "changes": body["changes"]}),
            json!({"installation": INST, "operation": op(1), "changes": body["changes"], "plan": op(2)}),
        ] {
            assert_eq!(t.post("github|1", &b, T0).1["reason"], "invalid", "{b}");
        }
        assert_eq!(raw(h("https://dash.example.com", "application/json", &csrf), r#"{"installation":"a","installation":"b"}"#), 400);
        // scope: a member who did not approve, another workspace's admin, a non-member, an unknown profile
        assert_eq!(t.post("github|2", &body, T0), (403, json!({"reason": "authority"})));
        assert_eq!(t.post("github|3", &body, T0), (403, json!({"reason": "authority"})));
        assert_eq!(t.post("github|9", &body, T0), (403, json!({"reason": "authority"})));
        let mut unknown = body.clone();
        unknown["changes"][0]["profile"] = json!("9999999999999999");
        assert_eq!(t.post("github|1", &unknown, T0), (403, json!({"reason": "profile_out_of_scope"})));
        // AgentQ's write gate (not_released until A5) disables review
        t.snap("s1", T0 + 1, snapshot("r1", false, false, "routed"));
        assert_eq!(t.post("github|1", &body, T0 + 1), (403, json!({"reason": "writes_disabled"})));
        t.snap("s2", T0 + 2, snapshot("r1", true, false, "routed"));
        // nothing was queued by any refusal; the actor's own request still works
        assert!(t.e.as_ref().unwrap().store().operation_ids(INST, 16).unwrap().is_empty());
        assert_eq!(t.post("github|1", &body, T0 + 2).0, 202);
        // reads are scoped: another member or workspace reads nothing, the workspace administrator reads it
        assert_eq!(t.get("github|2", &format!("{PREFIX}/{}", op(1)), T0 + 2), (404, json!({"reason": "unknown_operation"})));
        assert_eq!(t.get("github|3", &format!("{PREFIX}?installation={INST}"), T0 + 2).1["operations"], json!([]));
        assert_eq!(t.get("github|1", &format!("{PREFIX}?installation={INST}"), T0 + 2).1["operations"][0]["status"], "planning");
        assert_eq!(t.get("github|1", &format!("{PREFIX}?installation=../x"), T0 + 2).0, 400);
        // an apply of another actor's plan is unknown to this actor
        t.e.as_ref().unwrap().store().add_member("ws-1", "github|4", "admin").unwrap();
        t.host(&op(1), host_plan(1, "r1", &[], T0 + 300), T0 + 3);
        assert_eq!(t.post("github|4", &json!({"installation": INST, "operation": op(2), "plan": op(1)}), T0 + 4), (404, json!({"reason": "unknown_operation"})));
        // switched off: every route is unavailable
        t.enabled = false;
        assert_eq!(t.get("github|1", &format!("{PREFIX}/{}", op(1)), T0 + 4), (503, json!({"reason": "unavailable"})));
        assert!(route(&Ctx { enroll: &t.e, enabled: true, public_host: HOST }, &Req { method: "GET", path: "/agentq/configurex", query: "", headers: &[], body: b"" }, T0).is_none());
    }

    // chain: A6 lifecycle, expiry, revocation, a lost host reply, restart and storage failure read as their actual
    // outcome and never as success; a revoked grant queues nothing more
    #[test]
    fn expiry_revocation_restart_and_storage_failure() {
        let t = T::new("life");
        let st = |t: &T, n: u8, now| {
            let v = t.get("github|1", &format!("{PREFIX}/{}", op(n)), now).1;
            format!("{}:{}", v["status"].as_str().unwrap_or(""), v["reason"].as_str().unwrap_or(""))
        };
        let apply = |n: u8, plan: u8| json!({"installation": INST, "operation": op(n), "plan": op(plan)});
        // a plan never delivered before expiry: rejected, expired; the browser must plan again
        t.post("github|1", &plan_body(1), T0);
        assert_eq!(st(&t, 1, T0 + 300), "rejected:expired");
        // an apply delivered, then revoked before any result: rejected, never success
        t.post("github|1", &plan_body(2), T0 + 300);
        t.host(&op(2), host_plan(2, "r1", &[], T0 + 600), T0 + 301);
        t.post("github|1", &apply(3, 2), T0 + 302);
        let s = t.e.as_ref().unwrap().store();
        s.pull(&enroll::sha256(CRED), "agentq.configure", T0 + 303).unwrap();
        assert_eq!(st(&t, 3, T0 + 303), "accepted:");
        // restart: the delivered apply still reads accepted (host has not answered), not lost
        let T { d, e, .. } = t;
        drop(e);
        let t = T { e: Enroll::open(&d, HOST), d, enabled: true };
        assert_eq!(st(&t, 3, T0 + 304), "accepted:");
        assert!(t.e.as_ref().unwrap().store().revoke(INST, T0 + 305).unwrap());
        assert_eq!(st(&t, 3, T0 + 306), "rejected:revoked");
        assert_eq!(t.post("github|1", &plan_body(4), T0 + 306), (403, json!({"reason": "no_grant"})));
        // storage unavailable: the store refuses, nothing is reported as accepted
        let t2 = T::new("broken");
        let broken = T { e: Err("dashq.db: unavailable".into()), d: t2.d.clone(), enabled: true };
        assert_eq!(broken.get("github|1", &format!("{PREFIX}/{}", op(1)), T0), (503, json!({"reason": "unavailable"})));
        // the live process with an unusable store answers unavailable, never accepted
        let s = t2.e.as_ref().unwrap().store();
        s.write(|tx| Ok(tx.execute_batch("DROP TABLE operations")?)).unwrap();
        assert_eq!(t2.post("github|1", &plan_body(1), T0), (503, json!({"reason": "unavailable"})));
    }
}
