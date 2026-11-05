use super::*;

/// The optional board scope of a device request (coreq#36): `{"name": "<id>/<version>", "value": "<short line>"}`;
/// `Ok(None)` when absent, `Err` when malformed.
pub(super) fn board(b: &Value) -> Result<Option<String>, ()> {
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
pub(super) fn capabilities(b: &Value, product: &str) -> Result<Option<String>, ()> {
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
pub(super) fn same_board(b: &Value, g: &store::Grant) -> bool {
    b["scope"].is_null() || board(b).ok().flatten().is_some_and(|x| g.board.as_deref() == Some(x.as_str()))
}

/// A `taskq-board/1` record names the granted board (`<kind>:<host>/<repo>`, taskq enrollment); without a granted
/// board it is outside the grant. Other schemas carry no board.
pub(super) fn in_board(schema: &str, p: &Value, g: &store::Grant) -> bool {
    const BOARD: &str = "taskq-board/1";
    if schema != BOARD {
        return true;
    }
    let b = &p["board"];
    let (Some(kind), Some(host), Some(repo)) = (b["kind"].as_str(), b["host"].as_str(), b["repo"].as_str()) else { return false };
    g.board.as_deref().and_then(|x| serde_json::from_str::<Value>(x).ok()).is_some_and(|x| x == json!({"name": BOARD, "value": format!("{kind}:{host}/{repo}")}))
}

/// Names the approved scope in token, status and receipt replies; replies without one keep today's shape.
pub(super) fn with_board(v: &mut Value, g: &store::Grant) {
    if let Some(b) = g.board.as_deref().and_then(|b| serde_json::from_str::<Value>(b).ok()) {
        v["scope"] = b;
    }
}

pub(super) fn open_sealed(e: &Enroll, device: &[u8], mut sealed: Vec<u8>) -> Option<String> {
    if sealed.len() < 12 {
        return None;
    }
    let ct = sealed.split_off(12);
    let mut ct = ct;
    let nonce = aead::Nonce::try_assume_unique_for_key(&sealed).ok()?;
    let plain = e.seal.open_in_place(nonce, aead::Aad::from(device), &mut ct).ok()?;
    String::from_utf8(plain.to_vec()).ok()
}

pub(super) fn seal(e: &Enroll, device: &[u8], credential: &str) -> Vec<u8> {
    let nonce = e.random(12);
    let mut buf = credential.as_bytes().to_vec();
    e.seal.seal_in_place_append_tag(aead::Nonce::try_assume_unique_for_key(&nonce).expect("12 bytes"), aead::Aad::from(device), &mut buf).expect("seal");
    [nonce, buf].concat()
}

pub(super) fn device(e: &Enroll, r: &Req, b: &Value, now: i64) -> Resp {
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

pub(super) fn browser(e: &Enroll, r: &Req, now: i64) -> Resp {
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

pub(super) fn revoke_purpose(subject: &str, installation: &str, workspace: &str) -> String {
    format!("{subject}\nrevoke\n{installation}\n{workspace}")
}

/// `GET /connect?wait=` (first-connection progress) and `?revoke=` (confirmation) for the signed-in subject; both
/// read through the same scoped projection as the overview, so an account sees only its own installations.
pub(super) fn lifecycle(e: &Enroll, subject: &str, who: &str, q: &HashMap<String, String>, now: i64) -> Resp {
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
