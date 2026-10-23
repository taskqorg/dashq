//! D13/#186: a bounded PM request outbox, never a conversation or native launcher.
use crate::{
    enroll::{self, Enroll, Req, Resp},
    store::{self, Store},
};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

pub const SCHEMA: &str = "taskq.pm/1";
const BROWSER: &str = "/pm/request";
const EXCHANGE: &str = "/machine/pm/v1/exchange";

pub struct Ctx<'a> {
    pub enroll: &'a Result<Enroll, String>,
    pub enabled: bool,
    pub public_host: &'a str,
    pub state: &'a crate::answer::Answers,
}
fn reply(status: &'static str, v: Value) -> Resp {
    Resp {
        status,
        ctype: "application/json",
        body: v.to_string().into_bytes(),
        headers: String::new(),
        forms: false,
    }
}
fn fail(status: &'static str, reason: &str) -> Resp {
    reply(status, json!({"reason":reason}))
}
pub fn actor(subject: &str) -> String {
    enroll::sha256(subject)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn id(v: &Value) -> bool {
    v.as_str().is_some_and(|s| {
        !s.is_empty()
            && s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    })
}
fn keys(v: &Value, names: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == names.len() && names.iter().all(|k| o.contains_key(*k)))
}
fn purpose(subject: &str, day: i64) -> String {
    format!("{subject}\npm.request\n{day}")
}
fn receipt(r: &Value, reason: &str) -> Value {
    json!({"schema":SCHEMA,"actor_id":r["actor_id"],"op_id":r["op_id"],"revision":r["revision"],"outcome":"unknown","reason":reason})
}
fn typed(r: &Value) -> bool {
    keys(
        r,
        &[
            "schema",
            "actor_id",
            "op_id",
            "grant_id",
            "revision",
            "target",
            "issued_at",
            "expires_at",
            "text",
        ],
    ) && r["schema"] == SCHEMA
        && ["actor_id", "op_id", "grant_id", "revision"]
            .iter()
            .all(|k| id(&r[*k]))
        && keys(
            &r["target"],
            &["installation_id", "generation", "project_id", "runtime_id"],
        )
        && ["installation_id", "project_id", "runtime_id"]
            .iter()
            .all(|k| id(&r["target"][*k]))
        && r["target"]["generation"].as_i64().is_some_and(|n| n > 0)
        && r["issued_at"].as_i64().is_some_and(|n| n >= 0)
        && r["expires_at"]
            .as_i64()
            .zip(r["issued_at"].as_i64())
            .is_some_and(|(e, i)| e > i && e - i <= 300)
        && r["text"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty() && s.len() <= 65536)
}
fn targets(v: &Value) -> bool {
    v.as_array().is_some_and(|a| {
        a.len() <= 256
            && a.iter().all(|t| {
                keys(
                    t,
                    &[
                        "project_id",
                        "revision",
                        "runtimes",
                        "grant_id",
                        "actor_id",
                        "expires_at",
                    ],
                ) && ["project_id", "revision", "grant_id", "actor_id"]
                    .iter()
                    .all(|k| id(&t[*k]))
                    && t["expires_at"].as_i64().is_some()
                    && t["runtimes"]
                        .as_array()
                        .is_some_and(|a| !a.is_empty() && a.len() <= 8 && a.iter().all(id))
            })
    })
}
// A receipt is metadata only. Never retain host-supplied prose or off-host codex:// links.
fn clean_receipt(v: &Value) -> Option<Value> {
    if v["schema"] != SCHEMA
        || !["actor_id", "op_id", "revision"].iter().all(|k| id(&v[*k]))
        || !["accepted", "rejected", "stale", "unknown"]
            .iter()
            .any(|o| v["outcome"] == *o)
    {
        return None;
    }
    let mut out = json!({"schema":SCHEMA,"actor_id":v["actor_id"],"op_id":v["op_id"],"revision":v["revision"],"outcome":v["outcome"],
        "reason":match v["outcome"].as_str() {Some("rejected")=>json!("PM request denied or expired"),Some("stale")=>json!("PM project revision changed"),Some("unknown")=>json!("Native request readback unavailable; check the same request ID"),_=>Value::Null}});
    if v["outcome"] == "accepted" {
        let request = v["request_id"].as_str().filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        })?;
        let n = &v["native"];
        let safe = |k: &str| {
            n[k].as_str()
                .filter(|s| {
                    !s.is_empty()
                        && s.len() <= 160
                        && s.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
                })
                .map(str::to_string)
        };
        let (host, runtime) = (safe("host")?, safe("runtime")?);
        let state = safe("state")
            .filter(|s| {
                [
                    "pending",
                    "launching",
                    "launched",
                    "failed",
                    "unknown",
                    "closed",
                    "done",
                ]
                .contains(&s.as_str())
            })
            .unwrap_or_else(|| "unknown".into());
        let url = n["url"].as_str().filter(|s| {
            s.strip_prefix("https://claude.ai/code/session_")
                .is_some_and(|x| !x.is_empty() && x.bytes().all(|b| b.is_ascii_alphanumeric()))
        });
        out["request_id"] = json!(request);
        out["native"] = json!({"host":host,"runtime":runtime,"state":state,"session":safe("session"),"url":url});
    }
    Some(out)
}

pub fn route(c: &Ctx, r: &Req, now: i64) -> Option<Resp> {
    if ![BROWSER, EXCHANGE].contains(&r.path) {
        return None;
    }
    let (true, Ok(e)) = (c.enabled, c.enroll) else {
        return Some(fail(
            "503 Service Unavailable",
            "PM requests are not enabled on this dashboard",
        ));
    };
    Some(if r.path == EXCHANGE {
        if r.method != "POST" {
            return Some(fail("405 Method Not Allowed", "POST required"));
        }
        let Some(token) = r
            .header("authorization")
            .and_then(|s| s.strip_prefix("Bearer "))
        else {
            return Some(fail("401 Unauthorized", "invalid_credential"));
        };
        if !r
            .header("content-type")
            .is_some_and(|s| s.starts_with("application/json"))
            || r.body.len() > 1 << 20
        {
            return Some(fail("400 Bad Request", "invalid_request"));
        }
        let Ok(v) = serde_json::from_slice::<Value>(r.body) else {
            return Some(fail("400 Bad Request", "invalid_request"));
        };
        if !keys(
            &v,
            &[
                "schema",
                "installation_id",
                "generation",
                "targets",
                "receipts",
            ],
        ) || v["schema"] != SCHEMA
            || !targets(&v["targets"])
            || !v["receipts"]
                .as_array()
                .is_some_and(|a| a.len() <= 8 && a.iter().all(|r| clean_receipt(r).is_some()))
        {
            return Some(fail("400 Bad Request", "invalid_request"));
        }
        let verifier = enroll::sha256(token);
        let g = match e.store().credential(&verifier) {
            Ok(Some(g)) if g.active && !g.revoked => g.grant,
            _ => return Some(fail("403 Forbidden", "PM connection denied")),
        };
        if e.spend(&g, now).is_err() {
            return Some(fail("429 Too Many Requests", "rate_limited"));
        }
        match e.store().pm_exchange(&verifier, &v, now) {
            Ok(v) => reply("200 OK", v),
            Err(store::Error::Denied) => fail("403 Forbidden", "PM connection denied"),
            Err(_) => fail(
                "503 Service Unavailable",
                "PM store unavailable; nothing acknowledged",
            ),
        }
    } else {
        let Some(subject) = r
            .header(enroll::SUBJECT)
            .filter(|s| enroll::valid_subject(s))
        else {
            return Some(fail("401 Unauthorized", "Sign-in required"));
        };
        let actor = actor(subject);
        if r.method == "GET" {
            match e.store().pm_targets(subject, &actor, now) {
                Ok(t) => reply(
                    "200 OK",
                    json!({"actor_id":actor,"csrf":e.token_for(&purpose(subject, now/86400)),"targets":t}),
                ),
                Err(_) => fail("503 Service Unavailable", "PM store unavailable"),
            }
        } else if r.method == "POST" {
            let origin = format!("https://{}", c.public_host);
            if r.header("origin") != Some(origin.as_str())
                || !r
                    .header("content-type")
                    .is_some_and(|s| s.starts_with("application/json"))
                || ![now / 86400, now / 86400 - 1].iter().any(|d| {
                    e.token_ok(
                        &purpose(subject, *d),
                        r.header("x-dashq-csrf").unwrap_or(""),
                    )
                })
            {
                return Some(fail(
                    "403 Forbidden",
                    "Request refused: wrong Origin or expired form",
                ));
            }
            if r.body.len() > (96 << 10) {
                return Some(fail("413 Content Too Large", "Request too large"));
            }
            let Ok(v) = serde_json::from_slice::<Value>(r.body) else {
                return Some(fail("400 Bad Request", "Malformed PM request"));
            };
            if !typed(&v) || v["actor_id"] != actor {
                return Some(fail("400 Bad Request", "Invalid PM identity or request"));
            }
            if crate::answer::admit(c.state, subject, now).is_err() {
                return Some(fail(
                    "429 Too Many Requests",
                    "Too many operations; wait a minute",
                ));
            }
            match e.store().pm_submit(subject, &v, now) {
                Ok(v) => reply("200 OK", v),
                Err(store::Error::Denied) => {
                    fail("403 Forbidden", "PM grant or target unavailable")
                }
                Err(store::Error::Conflict) => {
                    fail("409 Conflict", "Operation ID already binds different input")
                }
                Err(_) => fail(
                    "503 Service Unavailable",
                    "PM store unavailable; check the same request ID",
                ),
            }
        } else {
            fail("405 Method Not Allowed", "GET or POST required")
        }
    })
}

fn allowed(db: &rusqlite::Connection, ws: &str, subject: &str) -> Result<bool, store::Error> {
    Ok(db.query_row("SELECT EXISTS(SELECT 1 FROM pm_requesters p JOIN members m USING(workspace,subject) WHERE p.workspace=?1 AND p.subject=?2 AND m.role='admin')",[ws,subject],|r|r.get(0))?)
}
impl Store {
    pub fn pm_requester(&self, ws: &str, subject: &str, on: bool) -> Result<(), store::Error> {
        self.write(|tx| {
            if on {
                tx.execute(
                    "INSERT OR IGNORE INTO pm_requesters VALUES (?1,?2)",
                    [ws, subject],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM pm_requesters WHERE workspace=?1 AND subject=?2",
                    [ws, subject],
                )?;
            }
            Ok(())
        })
    }
    pub fn pm_targets(
        &self,
        subject: &str,
        actor: &str,
        now: i64,
    ) -> Result<Vec<Value>, store::Error> {
        let db = self.lock()?;
        let mut stmt = db.prepare("SELECT h.installation,h.generation,h.workspace,h.targets FROM pm_hosts h JOIN grants g ON g.installation=h.installation AND g.generation=h.generation WHERE h.at>=?1 AND NOT EXISTS(SELECT 1 FROM revocations r WHERE r.installation=h.installation AND r.generation=h.generation)")?;
        let rows = stmt.query_map([now - 90], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
            ))
        })?;
        let mut out = vec![];
        for row in rows {
            let (inst, gen_, ws, payload) = row?;
            if !allowed(&db, &ws, subject)? {
                continue;
            }
            let ts: Value = serde_json::from_str(&payload).map_err(|_| store::Error::Corrupt)?;
            for t in ts
                .as_array()
                .ok_or(store::Error::Corrupt)?
                .iter()
                .filter(|t| {
                    t["actor_id"] == actor && t["expires_at"].as_i64().is_some_and(|e| e > now)
                })
            {
                for runtime in t["runtimes"].as_array().ok_or(store::Error::Corrupt)? {
                    out.push(json!({"actor_id":actor,"grant_id":t["grant_id"],"revision":t["revision"],"target":{"installation_id":inst,"generation":gen_,"project_id":t["project_id"],"runtime_id":runtime}}));
                }
            }
        }
        Ok(out)
    }
    pub fn pm_submit(&self, subject: &str, r: &Value, now: i64) -> Result<Value, store::Error> {
        // Target advertisement is authority only while fresh. A retry may use an expired target, never a revoked actor.
        let targets = self.pm_targets(subject, r["actor_id"].as_str().unwrap(), now)?;
        self.write(|tx| {
            tx.execute("UPDATE pm_ops SET request=NULL WHERE expires<=?1",[now])?;
            let installation = r["target"]["installation_id"].as_str().unwrap();
            let generation = r["target"]["generation"].as_i64().unwrap();
            let ws: Option<String> = tx.query_row("SELECT workspace FROM grants g WHERE installation=?1 AND generation=?2 AND NOT EXISTS(SELECT 1 FROM revocations r WHERE r.installation=g.installation AND r.generation=g.generation)",params![installation,generation],|r|r.get(0)).optional()?;
            let ws = ws.ok_or(store::Error::Denied)?;
            if !allowed(tx,&ws,subject)? { return Err(store::Error::Denied); }
            let digest = enroll::sha256(&r.to_string());
            let old: Option<(Vec<u8>,String,Option<String>)> = tx.query_row("SELECT digest,subject,receipt FROM pm_ops WHERE actor=?1 AND op=?2",params![r["actor_id"].as_str(),r["op_id"].as_str()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            if let Some((d,s,receipt_)) = old {
                if d != digest || s != subject { return Err(store::Error::Conflict); }
                return receipt_.map(|s|serde_json::from_str(&s).map_err(|_|store::Error::Corrupt)).unwrap_or_else(||Ok(receipt(r,"Delivery unknown; check the same request ID")));
            }
            if r["issued_at"].as_i64().unwrap() > now+30 || r["expires_at"].as_i64().unwrap() <= now
                || !targets.iter().any(|t| t["grant_id"]==r["grant_id"] && t["revision"]==r["revision"] && t["target"]==r["target"]) { return Err(store::Error::Denied); }
            if store::used(tx, &Self::path(tx))? + self.limits.reserve_bytes >= self.limits.store_bytes { return Err(store::Error::Full); }
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM pm_ops",[],|r|r.get(0))?;
            if count>=4096 { return Err(store::Error::Full); }
            tx.execute("INSERT INTO pm_ops(actor,op,subject,workspace,installation,generation,revision,digest,request,expires,delivered) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,0)",params![r["actor_id"].as_str(),r["op_id"].as_str(),subject,ws,installation,generation,r["revision"].as_str(),digest,r.to_string(),r["expires_at"].as_i64()])?;
            Ok(receipt(r,"Queued for TaskQ; native handoff not yet observed"))
        })
    }
    fn pm_exchange(&self, verifier: &[u8], v: &Value, now: i64) -> Result<Value, store::Error> {
        self.write(|tx| {
            let g = store::authorize(tx,verifier)?.ok_or(store::Error::Denied)?;
            let active: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM activations a JOIN installations i ON i.id=a.installation WHERE a.installation=?1 AND a.generation=?2 AND i.product='taskq')",params![g.installation,g.generation],|r|r.get(0))?;
            if !active || v["installation_id"]!=g.installation || v["generation"]!=g.generation { return Err(store::Error::Denied); }
            tx.execute("INSERT INTO pm_hosts VALUES (?1,?2,?3,?4,?5) ON CONFLICT(installation) DO UPDATE SET generation=excluded.generation,workspace=excluded.workspace,targets=excluded.targets,at=excluded.at",params![g.installation,g.generation,g.workspace,v["targets"].to_string(),now])?;
            for r in v["receipts"].as_array().unwrap() {
                let r = clean_receipt(r).ok_or(store::Error::Denied)?;
                tx.execute("UPDATE pm_ops SET receipt=?1,request=CASE WHEN ?2 IN ('accepted','rejected','stale') THEN NULL ELSE request END WHERE installation=?3 AND generation=?4 AND actor=?5 AND op=?6 AND revision=?7",params![r.to_string(),r["outcome"].as_str(),g.installation,g.generation,r["actor_id"].as_str(),r["op_id"].as_str(),r["revision"].as_str()])?;
            }
            tx.execute("UPDATE pm_ops SET request=NULL WHERE expires<=?1",[now])?;
            let requests: Vec<(String,String,String)> = {
                let mut stmt = tx.prepare("SELECT actor,op,request FROM pm_ops p WHERE installation=?1 AND generation=?2 AND request IS NOT NULL AND expires>?3 AND EXISTS(SELECT 1 FROM pm_requesters a JOIN members m USING(workspace,subject) WHERE a.workspace=p.workspace AND a.subject=p.subject AND m.role='admin') ORDER BY rowid LIMIT 8")?;
                stmt.query_map(params![g.installation,g.generation,now],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?.collect::<Result<_,_>>()?
            };
            let mut out = vec![];
            for (actor, op, request) in requests {
                let r: Value = serde_json::from_str(&request).map_err(|_|store::Error::Corrupt)?;
                // The host's current advertisement must still authorize an undispatched request.
                let offered = v["targets"].as_array().unwrap().iter().any(|t| t["actor_id"]==r["actor_id"] && t["grant_id"]==r["grant_id"] && t["project_id"]==r["target"]["project_id"] && t["revision"]==r["revision"] && t["expires_at"].as_i64().is_some_and(|e|e>now) && t["runtimes"].as_array().is_some_and(|a|a.contains(&r["target"]["runtime_id"])));
                if offered { out.push(r); tx.execute("UPDATE pm_ops SET delivered=1 WHERE actor=?1 AND op=?2",[actor,op])?; }
            }
            Ok(json!({"schema":SCHEMA,"server_at":now,"requests":out}))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const NOW: i64 = 1_800_000_000;
    const SUBJECT: &str = "github|invented";
    const HOST: &str = "dash.example.com";
    struct T {
        dir: std::path::PathBuf,
        e: Result<Enroll, String>,
        state: crate::answer::Answers,
        request: Value,
    }
    impl T {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("dashq-pm-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Enroll::admin(&dir, "invented-workspace", SUBJECT).unwrap();
            let e = Enroll::open(&dir, HOST);
            let s = e.as_ref().unwrap().store();
            let v = enroll::sha256("invented-credential");
            s.pair(
                b"device",
                "ABCD-EFGH",
                &store::Installation {
                    id: "invented-installation",
                    product: "taskq",
                    runtime: "native",
                    label: "invented",
                },
                "[\"taskq-board/1\"]",
                None,
                None,
                NOW,
                10,
            )
            .unwrap();
            s.decide(
                "ABCD-EFGH",
                true,
                "invented-workspace",
                SUBJECT,
                &[],
                &v,
                &|_| vec![],
                NOW,
            )
            .unwrap();
            s.ack(&v, NOW).unwrap();
            let mut request: Value =
                serde_json::from_str(include_str!("../assets/taskq/pm-composer-fixtures.json"))
                    .unwrap();
            request = request["request"].clone();
            request["actor_id"] = json!(actor(SUBJECT));
            request["issued_at"] = json!(NOW);
            request["expires_at"] = json!(NOW + 300);
            Self {
                dir,
                e,
                state: Default::default(),
                request,
            }
        }
        fn call(
            &self,
            enabled: bool,
            method: &str,
            path: &str,
            subject: Option<&str>,
            v: &Value,
        ) -> (String, Value) {
            let e = self.e.as_ref().unwrap();
            let mut h = vec![
                ("content-type".into(), "application/json".into()),
                ("origin".into(), format!("https://{HOST}")),
                ("authorization".into(), "Bearer invented-credential".into()),
            ];
            if let Some(s) = subject {
                h.push((enroll::SUBJECT.into(), s.into()));
                h.push(("x-dashq-csrf".into(), e.token_for(&purpose(s, NOW / 86400))));
            }
            let body = v.to_string();
            let r = route(
                &Ctx {
                    enroll: &self.e,
                    enabled,
                    public_host: HOST,
                    state: &self.state,
                },
                &Req {
                    method,
                    path,
                    query: "",
                    headers: &h,
                    body: body.as_bytes(),
                },
                NOW,
            )
            .unwrap();
            (r.status.into(), serde_json::from_slice(&r.body).unwrap())
        }
        fn exchange(&self, receipts: Vec<Value>) -> Value {
            let r = &self.request;
            json!({"schema":SCHEMA,"installation_id":"invented-installation","generation":1,"targets":[{"project_id":r["target"]["project_id"],"revision":r["revision"],"runtimes":["claude"],"grant_id":r["grant_id"],"actor_id":r["actor_id"],"expires_at":NOW+3600}],"receipts":receipts})
        }
    }
    impl Drop for T {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    // A1/A3/A4: metadata exchange -> durable input -> same-ID retry -> matching native receipt.
    #[test]
    fn pm_request_exchange_receipts_and_restart() {
        let t = T::new("exchange");
        let s = t.e.as_ref().unwrap().store();
        s.pm_requester("invented-workspace", SUBJECT, true).unwrap();
        assert_eq!(
            t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![])).0,
            "200 OK"
        );
        assert_eq!(
            s.pm_targets(SUBJECT, &actor(SUBJECT), NOW).unwrap().len(),
            1
        );
        let r = &t.request;
        let first = t.call(true, "POST", BROWSER, Some(SUBJECT), r);
        assert_eq!(first.1["outcome"], "unknown");
        let mut changed = r.clone();
        changed["text"] = json!("Different request");
        assert_eq!(
            t.call(true, "POST", BROWSER, Some(SUBJECT), &changed).0,
            "409 Conflict"
        );
        for _ in 0..2 {
            assert_eq!(
                t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![])).1["requests"],
                json!([r])
            );
        }
        let accepted = json!({"schema":SCHEMA,"actor_id":r["actor_id"],"op_id":r["op_id"],"revision":r["revision"],"outcome":"accepted","request_id":"a".repeat(64),"text":"must not be stored","native":{"host":"invented-host","runtime":"claude","state":"launched","session":"invented-session","url":"https://claude.ai/code/session_invented"}});
        let mut foreign = accepted.clone();
        foreign["revision"] = json!("foreign");
        assert_eq!(
            t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![foreign]))
                .1["requests"],
            json!([r])
        );
        assert_eq!(
            t.call(
                true,
                "POST",
                EXCHANGE,
                None,
                &t.exchange(vec![accepted.clone()])
            )
            .1["requests"],
            json!([])
        );
        let e = Enroll::open(&t.dir, HOST).unwrap();
        let out = e.store().pm_submit(SUBJECT, r, NOW + 1).unwrap();
        assert_eq!(out["native"]["session"], "invented-session");
        assert!(out.get("text").is_none());
        let db = s.lock().unwrap();
        let (count, text): (i64, Option<String>) = db
            .query_row("SELECT COUNT(*),request FROM pm_ops", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((count, text), (1, None));
    }
    // A1/A4/A5: observation/answer grants, stale targets, expired input and revoked generations cannot launch.
    #[test]
    fn pm_request_guards_and_expiry() {
        let t = T::new("guards");
        let s = t.e.as_ref().unwrap().store();
        let r = &t.request;
        assert_eq!(
            t.call(false, "GET", BROWSER, Some(SUBJECT), &json!({})).0,
            "503 Service Unavailable"
        );
        assert_eq!(t.call(true, "POST", BROWSER, None, r).0, "401 Unauthorized");
        let body = r.to_string();
        for (origin, csrf, ctype) in [
            ("https://elsewhere.example", "bad", "application/json"),
            ("https://dash.example.com", "bad", "application/json"),
            ("https://dash.example.com", "bad", "text/plain"),
        ] {
            let h = vec![
                (enroll::SUBJECT.into(), SUBJECT.into()),
                ("origin".into(), origin.into()),
                ("content-type".into(), ctype.into()),
                ("x-dashq-csrf".into(), csrf.into()),
            ];
            let x = route(
                &Ctx {
                    enroll: &t.e,
                    enabled: true,
                    public_host: HOST,
                    state: &t.state,
                },
                &Req {
                    method: "POST",
                    path: BROWSER,
                    query: "",
                    headers: &h,
                    body: body.as_bytes(),
                },
                NOW,
            )
            .unwrap();
            assert_eq!(x.status, "403 Forbidden");
        }
        t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![]));
        Enroll::answerer(&t.dir, "invented-workspace", SUBJECT, true).unwrap();
        assert_eq!(
            t.call(true, "POST", BROWSER, Some(SUBJECT), r).0,
            "403 Forbidden"
        );
        s.pm_requester("invented-workspace", SUBJECT, true).unwrap();
        let mut accepted = json!({"schema":SCHEMA,"actor_id":r["actor_id"],"op_id":r["op_id"],"revision":r["revision"],"outcome":"accepted","request_id":"b".repeat(64),"native":{"host":"host","runtime":"claude","state":"launched","session":"session","url":"codex://threads/local-only"}});
        assert!(clean_receipt(&accepted).unwrap()["native"]["url"].is_null());
        accepted["native"]["host"] = json!("private/path");
        assert!(clean_receipt(&accepted).is_none());
        assert!(
            s.pm_targets(SUBJECT, &actor(SUBJECT), NOW + 91)
                .unwrap()
                .is_empty()
        );
        let full = Enroll::open_with(
            &t.dir,
            HOST,
            store::Limits {
                store_bytes: 1 << 20,
                reserve_bytes: 1 << 20,
                ..store::LIMITS
            },
        )
        .unwrap();
        let mut new = r.clone();
        new["op_id"] = json!("full-store");
        assert_eq!(
            full.store().pm_submit(SUBJECT, &new, NOW),
            Err(store::Error::Full)
        );
        drop(full);
        let mut invalid = r.clone();
        invalid["actor_id"] = json!("someone-else");
        assert_eq!(
            t.call(true, "POST", BROWSER, Some(SUBJECT), &invalid).0,
            "400 Bad Request"
        );
        invalid = r.clone();
        invalid["target"]["generation"] = json!(2);
        assert_eq!(
            t.call(true, "POST", BROWSER, Some(SUBJECT), &invalid).0,
            "403 Forbidden"
        );
        invalid = r.clone();
        invalid["revision"] = json!("stale");
        assert_eq!(
            t.call(true, "POST", BROWSER, Some(SUBJECT), &invalid).0,
            "403 Forbidden"
        );
        assert_eq!(t.call(true, "POST", BROWSER, Some(SUBJECT), r).0, "200 OK");
        s.pm_requester("invented-workspace", SUBJECT, false)
            .unwrap();
        assert_eq!(
            t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![])).1["requests"],
            json!([])
        );
        s.pm_requester("invented-workspace", SUBJECT, true).unwrap();
        s.pm_exchange(
            &enroll::sha256("invented-credential"),
            &t.exchange(vec![]),
            NOW + 301,
        )
        .unwrap();
        assert!(
            s.lock()
                .unwrap()
                .query_row("SELECT request IS NULL FROM pm_ops", [], |r| r
                    .get::<_, bool>(0))
                .unwrap()
        );
        assert_eq!(
            s.pm_submit(SUBJECT, r, NOW + 302).unwrap()["outcome"],
            "unknown"
        );
        s.revoke_credential(&enroll::sha256("invented-credential"), NOW + 303)
            .unwrap();
        assert!(
            s.pm_targets(SUBJECT, &actor(SUBJECT), NOW)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            t.call(true, "POST", EXCHANGE, None, &t.exchange(vec![])).0,
            "403 Forbidden"
        );
    }
}
