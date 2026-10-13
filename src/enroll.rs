//! Pairing and device grants (docs/design/enrollment.md C3, coreq wire `coreq-enrollment/1`).
//! Machine routes take JSON and authorize by pairing or device credential only; the browser route `/connect`
//! takes the provider subject from the trusted proxy header and checks workspace membership, origin and CSRF.

use crate::store::{self, Decision, Installation, Poll, Store};
use ring::{aead, hmac, rand::SecureRandom, rand::SystemRandom};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

/// Set by the reverse proxy from the guard's verified subject after stripping any client copy (README, fixture/Caddyfile).
pub const SUBJECT: &str = "x-dashq-subject";
/// Client address set by the reverse proxy; loopback callers have none.
pub const CLIENT: &str = "x-dashq-client";
pub const MAX_BODY: usize = 64 << 10;
const ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const PRODUCTS: [&str; 3] = ["agentq", "serverq", "taskq"];
/// C3: new pairing sessions per source address per minute, and live sessions overall.
const PER_MINUTE: usize = 10;
const MAX_PENDING: i64 = 1000;

pub struct Req<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// Lowercase names.
    pub headers: &'a [(String, String)],
    pub body: &'a [u8],
}

impl Req<'_> {
    fn header(&self, name: &str) -> Option<&str> {
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
        hex(hmac::sign(&self.csrf, format!("{subject}\n{code}").as_bytes()).as_ref())
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

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    s.len().is_multiple_of(2).then(|| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()).flatten()
}

fn b64url(b: &[u8]) -> String {
    crate::base64(b).trim_end_matches('=').replace('+', "-").replace('/', "_")
}

fn sha256(s: &str) -> Vec<u8> {
    ring::digest::digest(&ring::digest::SHA256, s.as_bytes()).as_ref().to_vec()
}

fn id(s: &str, max: usize) -> bool {
    !s.is_empty() && s.len() <= max && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn valid_subject(s: &str) -> bool {
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
    let page = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>DashQ · Connect</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n</head>\n<body>\n<main>\n<h1>Connect a product</h1>\n{body}</main>\n</body>\n</html>\n"
    );
    Resp { status, ctype: "text/html; charset=utf-8", body: page.into_bytes(), headers: String::new(), forms: true }
}

/// Enrollment routes; `None` for every other path. `e` is `Err` when enrollment is not set up or unavailable.
pub fn route(e: &Result<Enroll, String>, r: &Req, now: i64) -> Option<Resp> {
    let machine = ["/connect/device", "/connect/token", "/connect/ack", "/connection/revoke"].contains(&r.path);
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
                    let Some(credential) = open_sealed(e, &device, sealed) else { return error("400 Bad Request", "expired_token") };
                    json(
                        "200 OK",
                        json!({"protocol": 1, "credential": credential, "installation": grant.installation, "generation": grant.generation,
                            "product": product, "workspace": grant.workspace, "schemas": serde_json::from_str::<Value>(&schemas).unwrap_or(json!([]))}),
                    )
                }
                Err(x) => unavailable(x),
            }
        }
        _ => {
            let Some(cred) = r.header("authorization").and_then(|a| a.strip_prefix("Bearer ")).filter(|c| !c.is_empty()) else {
                return error("401 Unauthorized", "invalid_credential");
            };
            let v = sha256(cred);
            let res = if r.path == "/connect/ack" { e.store.ack(&v, now) } else { e.store.revoke_credential(&v, now) };
            match res {
                Ok(None) => error("401 Unauthorized", "invalid_credential"),
                // a revoked credential cannot activate; revoke repeats its reply so offline retries end
                Ok(Some(c)) if c.revoked && r.path == "/connect/ack" => error("401 Unauthorized", "revoked"),
                Ok(Some(c)) => json("200 OK", json!({"protocol": 1, "state": if c.revoked { "revoked" } else { "active" }, "generation": c.grant.generation})),
                Err(x) => unavailable(x),
            }
        }
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
        match e.store.pair(&sha256(&device), &code, &inst, &json!(schemas).to_string(), now, MAX_PENDING) {
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

fn form(body: &[u8]) -> HashMap<String, String> {
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
            Ok(Decision::Approved(g)) => html("200 OK", &format!("{who}<p>Approved: installation <code>{}</code> is connected to workspace <strong>{}</strong>. Return to the command; it finishes on its own.</p>\n", esc(&g.installation), esc(&g.workspace))),
            Ok(Decision::Already) => fail("200 OK", "This code is already approved; nothing new was granted."),
            Ok(Decision::Denied) => fail("200 OK", "Denied: no access was granted."),
            Ok(Decision::Invalid) => fail("400 Bad Request", "This code is unknown, expired or already used. Start the connection again on the host."),
            Ok(Decision::NotMember) => fail("403 Forbidden", "This account is not a member of that workspace."),
            Err(store::Error::Conflict) => fail("409 Conflict", "This installation belongs to another workspace or runtime; moving it is not supported yet."),
            Err(_) => fail("503 Service Unavailable", "The connection store is unavailable; nothing was granted."),
        };
    }
    let code = form(r.query.as_bytes()).get("code").cloned().unwrap_or_default();
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
            "{who}{replaces}<form method=\"post\" action=\"/connect\">\n<dl>\n<dt>Code</dt><dd><strong>{code}</strong>: check that the command shows the same code</dd>\n<dt>Product</dt><dd>{}</dd>\n<dt>Host</dt><dd>{} ({})</dd>\n<dt>Installation</dt><dd><code>{}</code></dd>\n<dt>Data</dt><dd>{}: observation only, no commands or task answers</dd>\n{ws}</dl>\n<input type=\"hidden\" name=\"code\" value=\"{code}\">\n<input type=\"hidden\" name=\"csrf\" value=\"{}\">\n<button name=\"decision\" value=\"approve\">Approve</button>\n<button name=\"decision\" value=\"deny\">Deny</button>\n</form>\n",
            esc(&p.product),
            esc(&p.label),
            esc(&p.runtime),
            esc(&p.installation),
            scopes.join(", "),
            e.token(subject, &code)
        ),
    )
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
        let csrf = e.as_ref().unwrap().token(subject, &user_code(code).unwrap());
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
        assert_eq!(paths, ["/connect/device", "/connect/token", "/connect/ack", "/connection/revoke"]);
        assert!(head.contains("method POST") && !head.contains("/data") && !head.contains('*'));
        assert!(guarded.contains("copy_headers") && guarded.contains(">X-Dashq-Subject") && guarded.contains("reverse_proxy 127.0.0.1:18882"));
    }
}
