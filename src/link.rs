//! Host link relay (D25, #234): connected TaskQ installations of one workspace exchange end-to-end encrypted
//! messages through DashQ. The relay is blind: it routes opaque base64 bodies it never decodes, holds them in
//! memory only and writes nothing about them to the store or a log.

use crate::enroll::{self, Enroll, Req, Resp};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const SEND: &str = "/machine/link/v1/send";
pub const RECV: &str = "/machine/link/v1/recv";
/// One message body in decoded bytes.
const MAX_MESSAGE: usize = 256 << 10;
/// Pending bytes per installation, separately for messages addressed to it and messages it sent.
const CAP: usize = 4 << 20;
/// A message not received within this is dropped; the sender asks again.
const TTL: i64 = 60;
/// Longest receive wait, well inside Cloudflare's 100 s proxy limit.
const WAIT: u64 = 25;
/// Per-installation requests a second and burst, apart from the C5 ingest budget so a live page never starves it.
const RATE: f64 = 5.0;
const BURST: f64 = 20.0;

struct Msg {
    from: String,
    to: String,
    id: String,
    body: String,
    size: usize,
    expires: i64,
}

#[derive(Default)]
struct State {
    seq: u64,
    /// Pending messages in send order.
    pending: BTreeMap<u64, Msg>,
    /// Receive ticket per installation: a newer receive ends an older wait, so a host holds one thread.
    waiters: HashMap<String, u64>,
    buckets: HashMap<String, (f64, i64)>,
}

impl State {
    /// Bytes pending that match `f`, except the message `skip` a resend replaces.
    fn held(&self, skip: Option<u64>, f: impl Fn(&Msg) -> bool) -> usize {
        self.pending.iter().filter(|(k, m)| Some(**k) != skip && f(m)).map(|(_, m)| m.size).sum()
    }
}

#[derive(Default)]
pub struct Relay {
    state: Mutex<State>,
    wake: Condvar,
}

fn fail(status: &'static str, code: &str) -> Resp {
    reply(status, json!({"protocol": 1, "error": code}))
}

fn reply(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

/// Decoded length of standard padded base64; `None` when `s` is empty or not base64. The body is never decoded.
fn decoded_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let pad = b.iter().rev().take_while(|c| **c == b'=').count();
    let ok = !b.is_empty() && b.len() % 4 == 0 && pad <= 2 && b[..b.len() - pad].iter().all(|c| c.is_ascii_alphanumeric() || b"+/".contains(c));
    ok.then(|| b.len() / 4 * 3 - pad)
}

/// The caller's installation and workspace, or the refusal: only a live TaskQ connection may use the link.
fn caller(e: &Enroll, verifier: &[u8]) -> Result<(String, String), Resp> {
    let c = match e.store().credential(verifier) {
        Ok(Some(c)) => c,
        Ok(None) => return Err(fail("401 Unauthorized", "invalid_credential")),
        Err(_) => return Err(fail("503 Service Unavailable", "unavailable")),
    };
    if c.revoked {
        return Err(fail("401 Unauthorized", "revoked"));
    }
    if !c.active {
        return Err(fail("403 Forbidden", "not_active"));
    }
    match e.store().link_workspace(&c.grant.installation) {
        Ok(Some(ws)) if ws == c.grant.workspace => Ok((c.grant.installation, ws)),
        Ok(_) => Err(fail("403 Forbidden", "scope")),
        Err(_) => Err(fail("503 Service Unavailable", "unavailable")),
    }
}

/// Link routes; `None` for every other path.
pub fn route(relay: &Relay, e: &Result<Enroll, String>, r: &Req, now: i64) -> Option<Resp> {
    if ![SEND, RECV].contains(&r.path) {
        return None;
    }
    if r.method != "POST" {
        return Some(fail("405 Method Not Allowed", "method_not_allowed"));
    }
    let Ok(e) = e else { return Some(fail("503 Service Unavailable", "unavailable")) };
    let Ok(body) = serde_json::from_slice::<Value>(r.body) else { return Some(fail("400 Bad Request", "invalid_request")) };
    if body["protocol"] != 1 {
        return Some(fail("400 Bad Request", "unsupported_protocol"));
    }
    let Some(cred) = r.header("authorization").and_then(|a| a.strip_prefix("Bearer ")).filter(|c| !c.is_empty()) else {
        return Some(fail("401 Unauthorized", "invalid_credential"));
    };
    let verifier = enroll::sha256(cred);
    let me = match caller(e, &verifier) {
        Ok(me) => me,
        Err(x) => return Some(x),
    };
    if relay.spend(&me.0, now).is_err() {
        let mut x = fail("429 Too Many Requests", "rate_limited");
        x.headers = "Retry-After: 1\r\n".into();
        return Some(x);
    }
    Some(if r.path == SEND { relay.send(e, &me, &body, now) } else { relay.recv(e, &verifier, &me, &body, now) })
}

impl Relay {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn spend(&self, installation: &str, now: i64) -> Result<(), ()> {
        let mut s = self.lock();
        if s.buckets.len() > 10_000 {
            s.buckets.retain(|_, (_, t)| now - *t < 60);
        }
        let (tokens, last) = s.buckets.entry(installation.to_string()).or_insert((BURST, now));
        *tokens = (*tokens + (now - *last).max(0) as f64 * RATE).min(BURST);
        *last = now;
        if *tokens < 1.0 {
            return Err(());
        }
        *tokens -= 1.0;
        Ok(())
    }

    /// `{to, id, body}`: one pending message per sender, receiver and id; a resend replaces it.
    fn send(&self, e: &Enroll, (from, ws): &(String, String), b: &Value, now: i64) -> Resp {
        let field = |k: &str| b[k].as_str().filter(|x| enroll::id(x, 128));
        let (Some(to), Some(id), Some(size)) = (field("to"), field("id"), b["body"].as_str().and_then(decoded_len)) else {
            return fail("400 Bad Request", "invalid_request");
        };
        if size > MAX_MESSAGE {
            return fail("413 Content Too Large", "too_large");
        }
        // an unknown, disconnected or foreign receiver reads the same, so a sender learns nothing about others
        match e.store().link_workspace(to) {
            Ok(Some(w)) if w == *ws => {}
            Ok(_) => return fail("403 Forbidden", "scope"),
            Err(_) => return fail("503 Service Unavailable", "unavailable"),
        }
        let mut s = self.lock();
        s.pending.retain(|_, m| m.expires > now);
        let old = s.pending.iter().find(|(_, m)| m.from == *from && m.to == to && m.id == id).map(|(k, _)| *k);
        if s.held(old, |m| m.to == to) + size > CAP || s.held(old, |m| m.from == *from) + size > CAP {
            return fail("429 Too Many Requests", "cap");
        }
        if let Some(k) = old {
            s.pending.remove(&k);
        }
        s.seq += 1;
        let k = s.seq;
        s.pending.insert(k, Msg { from: from.clone(), to: to.into(), id: id.into(), body: b["body"].as_str().unwrap_or_default().into(), size, expires: now + TTL });
        drop(s);
        self.wake.notify_all();
        reply("200 OK", json!({"protocol": 1, "state": "queued", "expires_at": now + TTL}))
    }

    /// `{wait?}`: the messages addressed to the caller, taken once, waiting up to `wait` (default and most 25) s.
    fn recv(&self, e: &Enroll, verifier: &[u8], (me, ws): &(String, String), b: &Value, now: i64) -> Resp {
        let wait = match &b["wait"] {
            Value::Null => WAIT,
            w => match w.as_u64().filter(|w| *w <= WAIT) {
                Some(w) => w,
                None => return fail("400 Bad Request", "invalid_request"),
            },
        };
        let start = Instant::now();
        let mut s = self.lock();
        s.seq += 1;
        let ticket = s.seq;
        s.waiters.insert(me.clone(), ticket);
        self.wake.notify_all();
        let taken = loop {
            // a newer receive of the same installation takes over; this one ends empty
            if s.waiters.get(me) != Some(&ticket) {
                return reply("200 OK", json!({"protocol": 1, "messages": []}));
            }
            let t = now + start.elapsed().as_secs() as i64;
            s.pending.retain(|_, m| m.expires > t);
            let mine: Vec<u64> = s.pending.iter().filter(|(_, m)| m.to == *me).map(|(k, _)| *k).collect();
            let left = Duration::from_secs(wait).saturating_sub(start.elapsed());
            if !mine.is_empty() || left.is_zero() {
                s.waiters.remove(me);
                break mine.iter().filter_map(|k| s.pending.remove(k)).collect::<Vec<Msg>>();
            }
            s = self.wake.wait_timeout(s, left).unwrap_or_else(|p| p.into_inner()).0;
        };
        drop(s);
        if taken.is_empty() {
            return reply("200 OK", json!({"protocol": 1, "messages": []}));
        }
        // a revoke during the wait ends delivery: to the caller, and from every sender no longer connected here
        match caller(e, verifier) {
            Ok((_, w)) if w == *ws => {}
            Ok(_) => return fail("403 Forbidden", "scope"),
            Err(x) => return x,
        }
        let mut live: HashMap<String, bool> = HashMap::new();
        let mut out = Vec::new();
        for m in taken {
            let ok = match live.get(&m.from) {
                Some(ok) => *ok,
                None => match e.store().link_workspace(&m.from) {
                    Ok(w) => *live.entry(m.from.clone()).or_insert(w.as_deref() == Some(ws.as_str())),
                    Err(_) => return fail("503 Service Unavailable", "unavailable"),
                },
            };
            if ok {
                out.push(json!({"from": m.from, "id": m.id, "body": m.body}));
            }
        }
        reply("200 OK", json!({"protocol": 1, "messages": out}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Installation;

    const NOW: i64 = 1_800_000_000;
    const SUBJECT: &str = "github|invented";

    struct T {
        e: Result<Enroll, String>,
        relay: Relay,
    }

    impl T {
        fn new(name: &str) -> T {
            let dir = std::env::temp_dir().join(format!("dashq-link-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Enroll::admin(&dir, "ws-1", SUBJECT).unwrap();
            let e = Enroll::open(&dir, "dash.example.com");
            e.as_ref().unwrap().store().add_member("ws-2", SUBJECT, "admin").unwrap();
            T { e, relay: Relay::default() }
        }

        /// An approved and acknowledged connection of `inst` (`product`) in `ws`; its credential is `cred-<inst>`.
        fn connect(&self, inst: &str, product: &str, ws: &str, generation: i64) -> String {
            let s = self.e.as_ref().unwrap().store();
            let cred = format!("cred-{inst}-{generation}");
            let v = enroll::sha256(&cred);
            let code = format!("{inst}-{generation}");
            let device = format!("device-{inst}-{generation}");
            s.pair(device.as_bytes(), &code, &Installation { id: inst, product, runtime: "native", label: inst }, "[\"taskq-board/1\"]", None, None, NOW, 100).unwrap();
            s.decide(&code, true, ws, SUBJECT, &[], &v, &|_| vec![], NOW).unwrap();
            s.ack(&v, NOW).unwrap();
            cred
        }

        fn call(&self, path: &str, cred: &str, body: Value, now: i64) -> (String, Value) {
            let h = vec![("authorization".to_string(), format!("Bearer {cred}"))];
            let b = body.to_string();
            let r = route(&self.relay, &self.e, &Req { method: "POST", path, query: "", headers: &h, body: b.as_bytes() }, now).unwrap();
            (r.status.to_string(), serde_json::from_slice(&r.body).unwrap())
        }

        fn send(&self, cred: &str, to: &str, id: &str, body: &str, now: i64) -> (String, Value) {
            self.call(SEND, cred, json!({"protocol": 1, "to": to, "id": id, "body": body}), now)
        }

        fn recv(&self, cred: &str, wait: u64, now: i64) -> (String, Value) {
            self.call(RECV, cred, json!({"protocol": 1, "wait": wait}), now)
        }
    }

    fn body(n: usize) -> String {
        crate::base64(&vec![7u8; n])
    }

    #[test]
    fn routes_opaque_bodies_within_one_workspace() {
        let t = T::new("routes");
        let (a, b) = (t.connect("host-a", "taskq", "ws-1", 1), t.connect("host-b", "taskq", "ws-1", 1));
        assert_eq!(t.send(&a, "host-b", "m1", "c2VhbGVk", NOW), ("200 OK".into(), json!({"protocol": 1, "state": "queued", "expires_at": NOW + TTL})));
        // a resend of the same id replaces the pending message; nothing reaches the sender
        assert_eq!(t.send(&a, "host-b", "m1", "c2VhbGVkMg==", NOW).0, "200 OK");
        assert_eq!(t.send(&a, "host-b", "m2", "AA==", NOW).0, "200 OK");
        assert_eq!(t.recv(&a, 0, NOW).1["messages"], json!([]));
        assert_eq!(
            t.recv(&b, 0, NOW).1["messages"],
            json!([{"from": "host-a", "id": "m1", "body": "c2VhbGVkMg=="}, {"from": "host-a", "id": "m2", "body": "AA=="}])
        );
        // each message is taken once
        assert_eq!(t.recv(&b, 0, NOW).1["messages"], json!([]));
        // the body stays opaque: only its base64 form and size are checked
        for bad in [json!({"protocol": 1, "to": "host-b", "id": "m3", "body": "not base64!"}), json!({"protocol": 1, "to": "host-b", "id": "m3", "body": ""}), json!({"protocol": 1, "to": "host-b", "id": "bad id", "body": "AA=="}), json!({"protocol": 1, "to": "host-b", "id": "m3"})] {
            assert_eq!(t.call(SEND, &a, bad.clone(), NOW).0, "400 Bad Request", "{bad}");
        }
        assert_eq!(t.send(&a, "host-b", "big", &body(MAX_MESSAGE + 1), NOW), ("413 Content Too Large".into(), json!({"protocol": 1, "error": "too_large"})));
        assert_eq!(t.send(&a, "host-b", "max", &body(MAX_MESSAGE), NOW).0, "200 OK");
    }

    #[test]
    fn long_poll_wakes_on_send_and_times_out_empty() {
        let t = T::new("poll");
        let (a, b) = (t.connect("host-a", "taskq", "ws-1", 1), t.connect("host-b", "taskq", "ws-1", 1));
        let started = Instant::now();
        assert_eq!(t.recv(&b, 1, NOW), ("200 OK".into(), json!({"protocol": 1, "messages": []})));
        assert!(started.elapsed() >= Duration::from_secs(1));
        let got = std::thread::scope(|s| {
            let waiter = s.spawn(|| t.recv(&b, 10, NOW));
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(t.send(&a, "host-b", "m1", "AA==", NOW).0, "200 OK");
            waiter.join().unwrap()
        });
        assert_eq!(got.1["messages"], json!([{"from": "host-a", "id": "m1", "body": "AA=="}]));
        assert!(started.elapsed() < Duration::from_secs(5), "woke on send, not at the timeout");
        // a newer receive of the same host ends the older wait, so one host holds one thread
        let (older, newer) = std::thread::scope(|s| {
            let older = s.spawn(|| t.recv(&b, 10, NOW));
            std::thread::sleep(Duration::from_millis(200));
            let newer = s.spawn(|| t.recv(&b, 10, NOW));
            std::thread::sleep(Duration::from_millis(200));
            t.send(&a, "host-b", "m2", "AA==", NOW);
            (older.join().unwrap(), newer.join().unwrap())
        });
        assert_eq!(older.1["messages"], json!([]));
        assert_eq!(newer.1["messages"][0]["id"], "m2");
        assert_eq!(t.call(RECV, &b, json!({"protocol": 1, "wait": 26}), NOW).0, "400 Bad Request");
    }

    #[test]
    fn cap_and_ttl_bound_memory() {
        let t = T::new("cap");
        let (a, b, c) = (t.connect("host-a", "taskq", "ws-1", 1), t.connect("host-b", "taskq", "ws-1", 1), t.connect("host-c", "taskq", "ws-1", 1));
        let n = CAP / MAX_MESSAGE;
        for i in 0..n {
            assert_eq!(t.send(&a, "host-b", &format!("m{i}"), &body(MAX_MESSAGE), NOW).0, "200 OK");
        }
        // inbound to host-b is full, whoever sends; outbound of host-a is full, whoever receives
        assert_eq!(t.send(&c, "host-b", "x", "AA==", NOW), ("429 Too Many Requests".into(), json!({"protocol": 1, "error": "cap"})));
        assert_eq!(t.send(&a, "host-c", "x", "AA==", NOW).1["error"], "cap");
        // replacing a pending message does not count it twice
        assert_eq!(t.send(&a, "host-b", "m0", &body(MAX_MESSAGE), NOW).0, "200 OK");
        assert_eq!(t.recv(&b, 0, NOW).1["messages"].as_array().unwrap().len(), n);
        assert_eq!(t.send(&c, "host-b", "x", "AA==", NOW).0, "200 OK");
        // a message older than the TTL is dropped unread and frees its bytes
        assert_eq!(t.send(&a, "host-c", "old", "AA==", NOW).0, "200 OK");
        assert_eq!(t.recv(&b, 0, NOW + TTL - 1).1["messages"][0]["id"], "x");
        assert_eq!(t.recv(&c, 0, NOW + TTL).1["messages"], json!([]));
        let r = &t.relay.lock();
        assert!(r.pending.is_empty());
    }

    #[test]
    fn revoked_disconnected_and_foreign_installations_are_refused() {
        let t = T::new("refuse");
        let (a, b) = (t.connect("host-a", "taskq", "ws-1", 1), t.connect("host-b", "taskq", "ws-1", 1));
        let other = t.connect("host-x", "taskq", "ws-2", 1);
        let agent = t.connect("agent-1", "agentq", "ws-1", 1);
        // sender and receiver outside one workspace
        assert_eq!(t.send(&a, "host-x", "m1", "AA==", NOW), ("403 Forbidden".into(), json!({"protocol": 1, "error": "scope"})));
        assert_eq!(t.send(&other, "host-a", "m1", "AA==", NOW).1["error"], "scope");
        // an unknown installation reads the same as a foreign one
        assert_eq!(t.send(&a, "nobody", "m1", "AA==", NOW).1["error"], "scope");
        // only TaskQ connections use the link
        assert_eq!(t.send(&agent, "host-a", "m1", "AA==", NOW).1["error"], "scope");
        assert_eq!(t.send(&a, "agent-1", "m1", "AA==", NOW).1["error"], "scope");
        assert_eq!(t.recv("forged", 0, NOW).1["error"], "invalid_credential");
        // a pending, never acknowledged connection is disconnected
        let s = t.e.as_ref().unwrap().store();
        let v = enroll::sha256("cred-pending");
        s.pair(b"device-p", "PQRS-TUVW", &Installation { id: "host-p", product: "taskq", runtime: "native", label: "p" }, "[\"taskq-board/1\"]", None, None, NOW, 100).unwrap();
        s.decide("PQRS-TUVW", true, "ws-1", SUBJECT, &[], &v, &|_| vec![], NOW).unwrap();
        assert_eq!(t.send("cred-pending", "host-a", "m1", "AA==", NOW).1["error"], "not_active");
        assert_eq!(t.send(&a, "host-p", "m1", "AA==", NOW).1["error"], "scope");
        // a message from a sender revoked before delivery is dropped; a revoked receiver gets nothing
        assert_eq!(t.send(&a, "host-b", "m1", "AA==", NOW).0, "200 OK");
        assert_eq!(t.send(&b, "host-a", "m2", "AA==", NOW).0, "200 OK");
        s.revoke_credential(&enroll::sha256(&a), NOW).unwrap();
        assert_eq!(t.recv(&b, 0, NOW).1["messages"], json!([]));
        assert_eq!(t.recv(&a, 0, NOW), ("401 Unauthorized".into(), json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(t.send(&a, "host-b", "m3", "AA==", NOW).1["error"], "revoked");
        assert_eq!(t.send(&b, "host-a", "m3", "AA==", NOW).1["error"], "scope");
        // re-pairing the same host continues under its new credential
        let a2 = t.connect("host-a", "taskq", "ws-1", 2);
        assert_eq!(t.send(&b, "host-a", "m4", "AA==", NOW).0, "200 OK");
        let ids: Vec<Value> = t.recv(&a2, 0, NOW).1["messages"].as_array().unwrap().iter().map(|m| m["id"].clone()).collect();
        assert_eq!(ids, [json!("m2"), json!("m4")], "pending messages wait for the host within their TTL");
    }

    #[test]
    fn methods_protocol_rate_and_setup() {
        let t = T::new("misc");
        let a = t.connect("host-a", "taskq", "ws-1", 1);
        let h = vec![("authorization".to_string(), format!("Bearer {a}"))];
        let get = |e: &Result<Enroll, String>, path: &str| route(&t.relay, e, &Req { method: "GET", path, query: "", headers: &h, body: b"" }, NOW).map(|r| r.status);
        assert_eq!(get(&t.e, SEND), Some("405 Method Not Allowed"));
        assert_eq!(get(&t.e, "/machine/link/v2/send"), None);
        let none: Result<Enroll, String> = Err(String::new());
        let post = route(&t.relay, &none, &Req { method: "POST", path: RECV, query: "", headers: &h, body: b"{}" }, NOW).unwrap();
        assert_eq!(post.status, "503 Service Unavailable");
        assert_eq!(t.call(RECV, &a, json!({"protocol": 2}), NOW).1["error"], "unsupported_protocol");
        for _ in 0..BURST as usize {
            assert_eq!(t.recv(&a, 0, NOW + 100).0, "200 OK");
        }
        assert_eq!(t.recv(&a, 0, NOW + 100), ("429 Too Many Requests".into(), json!({"protocol": 1, "error": "rate_limited"})));
        assert_eq!(t.recv(&a, 0, NOW + 101).0, "200 OK");
        assert_eq!(decoded_len("AAAA"), Some(3));
        assert_eq!(decoded_len("AAA="), Some(2));
        assert_eq!(decoded_len("AA=="), Some(1));
        assert_eq!(decoded_len("A==="), None);
        assert_eq!(decoded_len("AA=A"), None);
    }
}
