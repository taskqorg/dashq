//! dashq: one read-only page for the taskqorg tools (dashq.md).

mod accounting;
mod agent_work;
mod answer;
mod clients;
mod configure;
mod data;
mod enroll;
mod export;
mod optimization;
mod page;
mod policy;
mod profile;
mod resource;
mod source;
mod version;
#[allow(dead_code)] // export forwarding, backup and restore use the rest
mod store;

use coreq::Tool;
use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const ADDR: &str = "127.0.0.1:18882";
const FIXTURE: &str = include_str!("../fixture/input.json");
const FIXTURE_HTML: &str = include_str!("../fixture/page.html");
const USAGE: &str = "usage: dashq serve | report [--period=S] [--host=H] [--agent=A] [--session=ID] | selftest | profile [--window=S] | --snapshot DIR | admin WORKSPACE SUBJECT | answerer WORKSPACE SUBJECT on|off | resources grant WORKSPACE SUBJECT INSTALLATION HOURS [host] [project=ID].. [runtime=ID].. | resources revoke GRANT_ID | update [--check] | service install|uninstall|run | version";

fn tool() -> Tool {
    Tool::new("dashq", "DashQ", "taskqorg/dashq", env!("CARGO_PKG_VERSION"))
}

struct Config {
    /// `(base, auth)`; `None`: Langfuse not configured, its sections read unknown (D11: optional source).
    langfuse: Option<(String, String)>,
    /// `langfuse.forward: true`: forward `agentq-event/1` to that project (D11, C5); off by default.
    forward: bool,
    links: Value,
    /// `None`: GitHub not configured, boards read unknown.
    github_token: Option<String>,
    projects: Vec<String>,
    /// Pinned `taskq` binary for the `taskq-snapshot` adapter; absent: the label read alone (adapters.md S2).
    taskq_bin: Option<String>,
    serverq_report: String,
    serverq_host: String,
    public_host: String,
    /// `None`: no answer route (D13); writes need the dedicated credential and the pinned binary.
    answer: Option<answer::Config>,
    /// `resources: true` enables the typed host-policy routes (D14); off until the target-host adapter is qualified.
    resources: bool,
    /// `agentq_configure: true` enables the guarded AgentQ review/apply routes (D16); off until AgentQ A5 qualifies.
    agentq_configure: bool,
    /// `taskq_logs` `{host: absolute taskq.log path}`: attempt stage records for Agent work (D21); empty: not configured.
    taskq_logs: Vec<(String, String)>,
}

fn config_path(t: &Tool) -> PathBuf {
    std::env::var_os("DASHQ_CONFIG").map(PathBuf::from).unwrap_or_else(|| t.install.join("config.json"))
}

/// Errors name the bad key, never its value (D2).
fn load_config(p: &std::path::Path) -> Result<Config, String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(p).map_err(|_| "config: file unreadable".to_string())?.permissions().mode();
        if mode & 0o077 != 0 {
            return Err("config: mode must be 0600".into());
        }
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(p).map_err(|_| "config: file unreadable")?).map_err(|_| "config: invalid JSON")?;
    let s = |path: &str, ok: &dyn Fn(&str) -> bool| -> Result<String, String> {
        v.pointer(path).and_then(Value::as_str).filter(|x| ok(x)).map(String::from).ok_or(format!("config: bad key {}", path[1..].replace('/', ".")))
    };
    // Langfuse and GitHub are optional (D11): absent sections read `not configured`; a present one must be valid
    let (mut links, mut forward) = (json!({}), false);
    let langfuse = match v.get("langfuse") {
        None => None,
        Some(_) => {
            let base = s("/langfuse/base_url", &|x| x.starts_with("https://") && !x.contains('@') && !x.contains('"') && x[8..].trim_end_matches('/').bytes().all(|b| b != b'/'))?;
            let public = s("/langfuse/public_key", &|x| x.starts_with("pk-lf-") && data::safe_id(x))?;
            let secret = s("/langfuse/secret_key", &|x| x.starts_with("sk-lf-") && data::safe_id(x))?;
            let project = s("/langfuse/project_id", &|x| data::safe_id(x))?;
            let base = base.trim_end_matches('/').to_string();
            let project_url = format!("{base}/project/{project}");
            links = json!({"traces": format!("{project_url}/traces"), "dashboards": format!("{project_url}/dashboards")});
            forward = match v.pointer("/langfuse/forward") {
                None => false,
                Some(x) => x.as_bool().ok_or("config: bad key langfuse.forward")?,
            };
            Some((base, format!("Basic {}", base64(format!("{public}:{secret}").as_bytes()))))
        }
    };
    let (token, projects) = match v.get("github") {
        None => (None, vec![]),
        Some(_) => {
            let token = s("/github/token", &|x| x.len() >= 20 && x.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))?;
            let projects: Vec<String> = v["projects"]
                .as_array()
                .filter(|a| !a.is_empty())
                .and_then(|a| a.iter().map(|x| x.as_str().filter(|r| r.split('/').count() == 2 && r.split('/').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b)))).map(String::from)).collect())
                .ok_or("config: bad key projects")?;
            (Some(token), projects)
        }
    };
    let taskq_bin = match v.pointer("/taskq/binary") {
        None => None,
        Some(_) => Some(s("/taskq/binary", &|x| std::path::Path::new(x).is_absolute())?),
    };
    let answer = match v.get("answer") {
        None => None,
        Some(_) => {
            let token = s("/answer/token", &|x| std::path::Path::new(x).is_absolute())?;
            let boards = v["answer"]["boards"]
                .as_object()
                .filter(|o| !o.is_empty())
                .and_then(|o| o.iter().map(|(b, w)| w.as_str().filter(|w| enroll::id(w, 64) && projects.contains(b)).map(|w| (b.clone(), w.to_string()))).collect())
                .ok_or("config: bad key answer.boards")?;
            if taskq_bin.is_none() {
                return Err("config: bad key taskq.binary".into());
            }
            Some(answer::Config { token: token.into(), boards })
        }
    };
    let resources = match v.get("resources") {
        None => false,
        Some(x) => x.as_bool().ok_or("config: bad key resources")?,
    };
    let agentq_configure = match v.get("agentq_configure") {
        None => false,
        Some(x) => x.as_bool().ok_or("config: bad key agentq_configure")?,
    };
    let (report, host) = if v.get("serverq").is_some() {
        (s("/serverq/report", &|x| std::path::Path::new(x).is_absolute())?,
         s("/serverq/host", &|x| x.split_once('/').is_some_and(|(h, r)| data::safe_id(h) && matches!(r, "native" | "wsl")))?)
    } else { (String::new(), String::new()) };
    let taskq_logs = match v.get("taskq_logs") {
        None => vec![],
        Some(x) => x
            .as_object()
            .filter(|o| !o.is_empty())
            .and_then(|o| o.iter().map(|(h, p)| p.as_str().filter(|p| data::safe_id(h) && std::path::Path::new(p).is_absolute()).map(|p| (h.clone(), p.to_string()))).collect())
            .ok_or("config: bad key taskq_logs")?,
    };
    let public_host = s("/public_host", &|x| safe_host(x))?.to_ascii_lowercase();
    Ok(Config {
        links,
        langfuse,
        forward,
        github_token: token,
        projects,
        taskq_bin,
        serverq_report: report,
        serverq_host: host,
        public_host,
        answer,
        resources,
        agentq_configure,
        taskq_logs,
    })
}

fn safe_host(x: &str) -> bool {
    x.contains('.') && x.len() <= 253 && x.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

pub(crate) fn base64(b: &[u8]) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    b.chunks(3)
        .flat_map(|c| {
            let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
            (0..4).map(move |i| if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' })
        })
        .collect()
}

struct App {
    cfg: Config,
    cache: source::Cache,
    /// `Err` until `dashq admin` sets enrollment up, or when its store is unavailable.
    enroll: Result<enroll::Enroll, String>,
    answers: answer::Answers,
    resources: policy::State,
    export: export::Last,
}

impl App {
    /// Reads every source (cached per D4) and builds the document; `scope` selects the enrolled installations
    /// the actor may see (D11), `None` for none.
    fn doc(&self, period: u64, filters: Value, scope: Option<store::Scope>) -> Value {
        let c = &self.cfg;
        let now = source::now();
        let span = profile::current();
        let (lf, boards, releases, taskq) = std::thread::scope(|sc| {
            let lf = sc.spawn(|| {
                profile::enter(span);
                match &c.langfuse {
                    Some((base, auth)) => self.cache.get(&format!("langfuse:{period}"), 30, || source::langfuse(base, auth, now, period)),
                    None => Value::Null,
                }
            });
            let gh: Vec<_> = c
                .projects
                .iter()
                .filter_map(|r| Some((r, c.github_token.as_deref()?)))
                .map(|(r, token)| {
                    sc.spawn(move || {
                        profile::enter(span);
                        let b =self.cache.get(&format!("board:{r}"), 60, || source::board(r, token));
                        let l = self.cache.get(&format!("release:{r}"), 600, || source::release(r, token));
                        // one single flight per board: callers inside 60 s share one `taskq status --json` run
                        let t = c.taskq_bin.as_ref().map(|bin| self.cache.get(&format!("taskq:{r}"), 60, || source::taskq_status(bin, r, token)));
                        (r.clone(), b, l, t)
                    })
                })
                .collect();
            let (mut boards, mut releases, mut taskq) = (json!({}), json!({}), json!({}));
            for h in gh {
                if let Ok((r, b, l, t)) = h.join() {
                    boards[&r] = b;
                    releases[&r] = l;
                    if let Some(t) = t {
                        taskq[&r] = t;
                    }
                }
            }
            (lf.join().unwrap_or(Value::Null), boards, releases, taskq)
        });
        // Read every time, but retain the last report and its producer time on failure (D19 U2).
        let serverq = if c.serverq_report.is_empty() { Value::Null } else {
            self.cache.get("serverq", 0, || {
                let snap = source::serverq(&c.serverq_report);
                match snap["error"].as_str() {
                    Some(e) => Err((e.into(), None)),
                    None => Ok((snap["value"].clone(), false)),
                }
            })
        };
        // `read.tasklog` (D20): local files, reread every 60 s, last value kept on failure
        let agent_work = if c.taskq_logs.is_empty() { Value::Null } else {
            c.taskq_logs.iter().map(|(h, p)| (h.clone(), self.cache.get(&format!("tasklog:{h}"), 60, || agent_work::read(p)))).collect::<serde_json::Map<_, _>>().into()
        };
        let t0 = std::time::Instant::now();
        let host = filters["host"].as_str().map(String::from);
        // committed snapshots never wait for or depend on the optional sources above (D11)
        let projection = enroll::projection(&self.enroll, scope, host.as_deref(), now);
        let enrolled = projection.as_ref().map_or(json!([]), |p| enroll::resource_samples(&self.enroll, &p["rows"], now.saturating_sub(period) as i64));
        profile::step("enrollment", t0);
        let t0 = std::time::Instant::now();
        let doc = data::build(&json!({
            "now": now, "period": period, "filters": filters, "projects": c.projects, "serverq_host": c.serverq_host,
            "langfuse": lf, "boards": boards, "taskq": taskq, "releases": releases, "serverq": serverq, "enrolled_resources": enrolled,
            "installations": projection, "agent_work": agent_work,
        }));
        profile::step("build", t0);
        let mut doc = doc;
        // the page offers review/apply only where the route is on (D16)
        doc["controls"] = json!({"agentq_configure": c.agentq_configure});
        // outbox counts span every workspace: host-local readers only (D11)
        if let (Some(store::Scope::All), Ok(e)) = (scope, &self.enroll) {
            doc["sources"]["export"] = export::status(Some(e.store()), &self.export, c.langfuse.is_some() && c.forward, now);
        }
        doc
    }
}

fn respond(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8]) {
    respond_with(s, status, ctype, body, "", false)
}

/// `extra` header lines end in CRLF; `forms` allows same-origin form posts (the approval page only, D11).
fn respond_with(s: &mut TcpStream, status: &str, ctype: &str, body: &[u8], extra: &str, forms: bool) {
    let form = if forms { "'self'" } else { "'none'" };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\n{extra}Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: same-origin\r\nContent-Security-Policy: default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action {form}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = s.write_all(head.as_bytes()).and_then(|_| s.write_all(body));
}

fn handle(app: &App, mut s: TcpStream) {
    let _ = s.set_read_timeout(Some(Duration::from_secs(10)));
    // request line and headers together at most 16 KiB, then a body of at most enroll::MAX_BODY
    let Ok(clone) = s.try_clone() else { return };
    let mut r = BufReader::new(std::io::Read::take(clone, (16 << 10) + enroll::MAX_BODY as u64));
    let mut line = String::new();
    if r.read_line(&mut line).is_err() {
        return;
    }
    let (mut h, mut headers, start) = (String::new(), Vec::new(), std::time::Instant::now());
    for _ in 0..100 {
        h.clear();
        if r.read_line(&mut h).map_or(true, |n| n == 0) || h.trim().is_empty() {
            break;
        }
        // a slow drip of header lines gets 10 s in total, not per line
        if start.elapsed() > Duration::from_secs(10) {
            return;
        }
        if let Some((k, v)) = h.split_once(':') {
            headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    let get = |k: &str| headers.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
    let host = get("host").map(str::to_ascii_lowercase);
    let len: usize = match get("content-length").map(str::parse) {
        None => 0,
        Some(Ok(n)) if n <= enroll::MAX_BODY => n,
        _ => return respond(&mut s, "413 Content Too Large", "text/plain", b"body too large"),
    };
    let mut body = vec![0; len];
    if std::io::Read::read_exact(&mut r, &mut body).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let (method, target) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    // DNS rebinding: only the loopback address and the configured public name
    if !host.as_deref().is_some_and(|h| [ADDR, "localhost:18882", app.cfg.public_host.as_str()].contains(&h)) {
        return respond(&mut s, "421 Misdirected Request", "text/plain", b"unknown host");
    }
    let req = enroll::Req { method, path, query, headers: &headers, body: &body };
    let c = &app.cfg;
    let ctx = answer::Ctx { enroll: &app.enroll, cfg: c.answer.as_ref(), bin: c.taskq_bin.as_deref(), public_host: &c.public_host, state: &app.answers };
    let rctx = policy::Ctx { enroll: &app.enroll, enabled: c.resources, public_host: &c.public_host, state: &app.resources };
    let cctx = configure::Ctx { enroll: &app.enroll, enabled: c.agentq_configure, public_host: &c.public_host };
    let t0 = profile::begin();
    let (op, status) = if let Some(x) = configure::route(&cctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.configure", x.status)
    } else if let Some(x) = answer::route(&ctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.answer", x.status)
    } else if let Some(x) = policy::route(&rctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.resources", x.status)
    } else if let Some(x) = enroll::route(&app.enroll, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.enroll", x.status)
    } else {
        let (status, ctype, body) = route(app, method, path, query, actor(host.as_deref(), &app.cfg.public_host, get(enroll::SUBJECT)));
        respond(&mut s, status, ctype, &body);
        (match path {
            "/" => "http.page",
            "/data" => "http.data",
            _ => return, // assets and 404 are not measured
        }, status)
    };
    // page and API latency (D20): a 5xx is a failure named by its code, a 4xx is the caller's
    profile::record(op, t0, status.starts_with('5').then(|| status.split(' ').next().unwrap_or("5xx")));
}

/// Whose installations a page request sees (D11): a request through the public name only its verified subject's
/// (none without one); a loopback request is host-local and trusted like `dashq report`.
fn actor<'a>(host: Option<&str>, public_host: &str, subject: Option<&'a str>) -> Option<store::Scope<'a>> {
    if host != Some(public_host) {
        return Some(store::Scope::All);
    }
    subject.filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control)).map(store::Scope::Subject)
}

fn route(app: &App, method: &str, path: &str, query: &str, scope: Option<store::Scope>) -> (&'static str, &'static str, Vec<u8>) {
    const HTML: &str = "text/html; charset=utf-8";
    const JSON: &str = "application/json";
    if !["/", "/app.js", "/style.css", "/data"].contains(&path) {
        return ("404 Not Found", "text/plain", b"not found".to_vec());
    }
    if method != "GET" {
        return ("405 Method Not Allowed", "text/plain", b"method not allowed".to_vec());
    }
    match path {
        "/app.js" => ("200 OK", "text/javascript; charset=utf-8", app_js().into_bytes()),
        "/style.css" => ("200 OK", "text/css; charset=utf-8", style_css().into_bytes()),
        _ => {
            let Ok((period, filters)) = data::query(query) else {
                return ("400 Bad Request", JSON, br#"{"error":"invalid parameters"}"#.to_vec());
            };
            let doc = app.doc(period, filters, scope);
            if path == "/" {
                return ("200 OK", HTML, page::render(&doc, &app.cfg.links).into_bytes());
            }
            if data::all_failed(&doc) {
                return ("503 Service Unavailable", JSON, br#"{"error":"all sources failed"}"#.to_vec());
            }
            ("200 OK", JSON, doc.to_string().into_bytes())
        }
    }
}

fn serve(t: &Tool, service: bool) -> ! {
    let cfg = load_config(&config_path(t)).unwrap_or_else(|e| t.die(&e));
    // loopback only (D2): the address is a constant, never configured
    let l = TcpListener::bind(ADDR).unwrap_or_else(|e| t.die(&format!("bind {ADDR}: {e}")));
    eprintln!("dashq {} listening on {ADDR}", t.version);
    if service {
        let healthy = t.install.join("releases").join(t.version).join("healthy");
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(60));
            if healthy.parent().is_some_and(|p| p.is_dir()) {
                let _ = std::fs::write(&healthy, "");
            }
        });
        // only the launcher-run release updates itself; a service run from elsewhere just serves
        let exe = |p: PathBuf| std::fs::canonicalize(p).ok();
        if std::env::current_exe().ok().and_then(exe).is_some_and(|e| Some(e) == exe(t.release_exe(t.version))) {
            std::thread::spawn(|| auto_update(tool()));
        } else {
            eprintln!("update: not an installed release; no automatic update");
        }
    }
    let dir = config_path(t).parent().map(PathBuf::from).unwrap_or_default();
    profile::open(&dir);
    let enroll = enroll::Enroll::open(&dir, &cfg.public_host);
    if let Err(e) = &enroll {
        eprintln!("enrollment: {e}");
    }
    let app = Arc::new(App { cfg, cache: source::Cache::default(), enroll, answers: Default::default(), resources: Default::default(), export: Default::default() });
    // the projection pass: committed snapshots become `projected` without waiting for a page view (C1.5, C5 2 s)
    let pass = app.clone();
    std::thread::spawn(move || {
        while let Ok(e) = &pass.enroll {
            if let Err(x) = e.project(store::Scope::All) {
                eprintln!("projection: {x:?}");
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    // forwarding and retention run apart from ingestion and projection: an outage only delays export (C5)
    let fwd = app.clone();
    std::thread::spawn(move || {
        let mut pruned = 0;
        while let Ok(e) = &fwd.enroll {
            let now = source::now() as i64;
            if now - pruned >= 3600 {
                pruned = now;
                if let Err(x) = e.store().prune(now) {
                    eprintln!("prune: {x:?}");
                }
            }
            if let (Some((base, auth)), true) = (&fwd.cfg.langfuse, fwd.cfg.forward) {
                let r = export::pass(e.store(), base, auth, now);
                if let Err(x) = &r {
                    eprintln!("export: {x}");
                }
                *fwd.export.lock().unwrap_or_else(|p| p.into_inner()) = Some((now, r.err()));
            }
            std::thread::sleep(Duration::from_secs(5));
        }
    });
    let open = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for mut s in l.incoming().flatten() {
        use std::sync::atomic::Ordering::SeqCst;
        // ponytail: one thread per connection, capped; a pool if load ever grows
        if open.fetch_add(1, SeqCst) >= 64 {
            open.fetch_sub(1, SeqCst);
            respond(&mut s, "503 Service Unavailable", "text/plain", b"busy");
            continue;
        }
        let (app, open) = (app.clone(), open.clone());
        std::thread::spawn(move || {
            // the slot is freed even when a request panics
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| handle(&app, s)));
            open.fetch_sub(1, SeqCst);
        });
    }
    std::process::exit(1)
}

fn app_js() -> String {
    [include_str!("../assets/taskq/resource-controls.js"), include_str!("../assets/resources.js"), include_str!("../assets/taskq/project-blocks.js"), include_str!("../assets/taskq/answer-card.js"), include_str!("../assets/boards.js"), include_str!("../assets/app.js")].join("\n")
}

fn style_css() -> String {
    [include_str!("../assets/style.css"), include_str!("../assets/taskq/resource-controls.css"), include_str!("../assets/resources.css"), include_str!("../assets/taskq/project-blocks.css"), include_str!("../assets/taskq/answer-card.css")].join("\n")
}

fn fixture_doc() -> Value {
    data::build(&serde_json::from_str(FIXTURE).expect("fixture JSON"))
}

fn fixture_html() -> String {
    let input: Value = serde_json::from_str(FIXTURE).unwrap();
    page::render(&fixture_doc(), &input["links"])
}

/// The fixture page as one file: stylesheet inlined, script dropped (D2 --snapshot).
fn snapshot_html() -> String {
    let css = format!("<style>\n{}</style>", style_css());
    fixture_html().replace("<link rel=\"stylesheet\" href=\"/style.css\">", &css).replace("<script src=\"/app.js\" defer></script>\n", "")
}

fn chrome() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DASHQ_CHROME") {
        return Some(p.into());
    }
    let path = std::env::var_os("PATH").unwrap_or_default();
    ["google-chrome", "chromium", "chromium-browser"]
        .iter()
        .flat_map(|n| std::env::split_paths(&path).map(move |d| d.join(n)))
        .chain([PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome")])
        .find(|p| p.is_file())
}

fn snapshot(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("snapshot: {e}"))?;
    let dir = dir.canonicalize().map_err(|e| format!("snapshot: {e}"))?;
    let (html, png) = (dir.join("page.html"), dir.join("page.png"));
    std::fs::write(&html, snapshot_html()).map_err(|e| format!("snapshot: {e}"))?;
    let _ = std::fs::remove_file(&png);
    let c = chrome().ok_or("snapshot: PNG renderer not found (headless Chrome; set DASHQ_CHROME)")?;
    // ponytail: fixed 1280x4000 window; measure the page height if the fixture outgrows it
    let out = std::process::Command::new(&c)
        .args(["--headless", "--disable-gpu", "--no-sandbox", "--hide-scrollbars", "--window-size=1280,4000"])
        .arg(format!("--screenshot={}", png.display()))
        .arg(format!("file://{}", html.display()))
        .output()
        .map_err(|e| format!("snapshot: run {}: {e}", c.display()))?;
    if !png.is_file() {
        return Err(format!("snapshot: no page.png ({})", out.status));
    }
    println!("snapshot: {} {}", html.display(), png.display());
    Ok(())
}

fn selftest(t: &Tool) -> Result<(), String> {
    if fixture_html() != FIXTURE_HTML {
        return Err("fixture: rendering differs from fixture/page.html".into());
    }
    println!("fixture: ok");
    let p = config_path(t);
    if !p.exists() {
        println!("config: none at DASHQ_CONFIG or {}; source reads skipped", p.display());
        return Ok(());
    }
    let cfg = load_config(&p)?;
    println!("config: ok");
    if std::env::var_os(t.env("SELFTEST_OFFLINE")).is_some() {
        return Ok(());
    }
    let doc = App { cfg, cache: source::Cache::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default() }.doc(1800, json!({}), None);
    for (name, s) in doc["sources"].as_object().unwrap() {
        println!("{name}: {} {}", s["state"].as_str().unwrap_or(""), s["reason"].as_str().unwrap_or(""));
    }
    Ok(())
}

/// The release the coreq launcher rolled back: newer than `running`, never healthy, three starts of `project` (D2).
fn rolled_back(install: &Path, running: &str, project: &str) -> Option<String> {
    let found = std::fs::read_dir(install.join("releases")).ok()?.flatten().filter_map(|e| {
        let (v, d) = (e.file_name().to_string_lossy().into_owned(), e.path());
        let starts: u32 = std::fs::read_to_string(d.join("starts").join(project)).ok()?.trim().parse().ok()?;
        (starts >= 3 && !d.join("healthy").exists() && coreq::update::newer(&v, running)).then_some(v)
    });
    found.reduce(|a, b| if coreq::update::newer(&b, &a) { b } else { a })
}

/// The version an automatic check applies: a newer release, unless it is the one rolled back after failed starts.
fn applies<'a>(check: &'a Result<Option<String>, String>, skip: Option<&str>) -> Option<&'a str> {
    check.as_ref().ok()?.as_deref().filter(|v| Some(*v) != skip)
}

/// Automatic update (D2): an installed `service run` checks after at most 900 s, then every `DASHQ_UPDATE_EVERY` s
/// (default 3600), and applies a newer release by exiting 75 so the unit or launcher starts the new `current`.
fn auto_update(t: Tool) {
    let every = Duration::from_secs(std::env::var("DASHQ_UPDATE_EVERY").ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(3600));
    let project = std::env::current_dir().ok().and_then(|d| Some(d.file_name()?.to_string_lossy().into_owned())).unwrap_or_else(|| "_".into());
    let skip_file = t.install.join("rolled-back");
    std::thread::sleep(every.min(Duration::from_secs(900)));
    loop {
        if let Some(v) = rolled_back(&t.install, t.version, &project) {
            let _ = std::fs::write(&skip_file, &v);
        }
        let skip = std::fs::read_to_string(&skip_file).ok().map(|s| s.trim().to_string());
        let t0 = std::time::Instant::now();
        let check = coreq::update::update(&t, true);
        profile::record("update.check", t0, check.as_ref().err().map(|_| "failed"));
        match (&check, applies(&check, skip.as_deref())) {
            (Err(e), _) => eprintln!("update check: failed: {e}"),
            (Ok(None), _) => eprintln!("update check: up to date ({})", t.version),
            (Ok(Some(v)), None) => eprintln!("update check: {v} available; skipped: rolled back after failed starts"),
            (Ok(Some(_)), Some(v)) => {
                eprintln!("update check: {v} available; applying");
                let t0 = std::time::Instant::now();
                let r = coreq::update::update(&t, false);
                profile::record("update", t0, r.as_ref().err().map(|_| "failed"));
                match r {
                    Ok(Some(v)) => {
                        eprintln!("update: {v} applied; restarting");
                        std::process::exit(75);
                    }
                    Ok(None) => eprintln!("update: up to date ({})", t.version),
                    Err(e) => eprintln!("update: failed: {e}; staying on {}", t.version),
                }
            }
        }
        std::thread::sleep(every);
    }
}

/// Only a release that became current restarts an installed service; check, unchanged and failed updates leave it running.
fn restarts(r: &Result<Option<String>, String>, check: bool, installed: bool) -> bool {
    !check && installed && matches!(r, Ok(Some(_)))
}

fn main() {
    let t = tool();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // the installed launcher runs the current release (coreq launcher)
    if std::env::current_exe().ok() == Some(t.install.join(t.exe_name())) && args.first().map(String::as_str) != Some("update") {
        coreq::launcher::launch(&t, &args);
    }
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["serve"] => serve(&t, false),
        ["service", "run"] => serve(&t, true),
        ["report", rest @ ..] => {
            let q: Vec<String> = rest.iter().map(|x| x.strip_prefix("--").filter(|x| x.contains('=')).map(String::from).unwrap_or_else(|| t.die(USAGE))).collect();
            let (period, filters) = data::query(&q.join("&")).unwrap_or_else(|e| t.die(&e));
            let cfg = load_config(&config_path(&t)).unwrap_or_else(|e| t.die(&e));
            // the host administrator's local report sees every installation (D11), like a loopback page request
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            profile::open(&dir);
            let enroll = enroll::Enroll::open(&dir, &cfg.public_host);
            println!("{}", App { cfg, cache: source::Cache::default(), enroll, answers: Default::default(), resources: Default::default(), export: Default::default() }.doc(period, filters, Some(store::Scope::All)));
        }
        ["admin", workspace, subject] => {
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            enroll::Enroll::admin(&dir, workspace, subject).unwrap_or_else(|e| t.die(&e));
            println!("admin: {subject} administers {workspace}");
        }
        ["answerer", workspace, subject, op @ ("on" | "off")] => {
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            enroll::Enroll::answerer(&dir, workspace, subject, *op == "on").unwrap_or_else(|e| t.die(&e));
            println!("answerer: {subject} {op} in {workspace}");
        }
        ["resources", "grant", workspace, subject, installation, hours, rest @ ..] => {
            let hours: i64 = hours.parse().ok().filter(|h| (1..=24 * 365).contains(h)).unwrap_or_else(|| t.die(USAGE));
            let (mut host, mut projects, mut runtimes) = (false, vec![], vec![]);
            for x in rest {
                match x.split_once('=') {
                    None if *x == "host" => host = true,
                    Some(("project", p)) => projects.push(p.to_string()),
                    Some(("runtime", r)) => runtimes.push(r.to_string()),
                    _ => t.die(USAGE),
                }
            }
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            let e = enroll::Enroll::open(&dir, "").unwrap_or_else(|e| t.die(&e));
            let expires = source::now() as i64 + hours * 3600;
            let (id, actor) = policy::issue(e.store(), workspace, subject, installation, host, &projects, &runtimes, expires).unwrap_or_else(|e| t.die(&e));
            println!("resources grant: {id} for {subject} as {actor} on {installation}");
        }
        ["resources", "revoke", id] => {
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            let e = enroll::Enroll::open(&dir, "").unwrap_or_else(|e| t.die(&e));
            let found = policy::revoke(e.store(), id).unwrap_or_else(|e| t.die(&e));
            println!("resources revoke: {}", if found { "revoked" } else { "no such grant" });
        }
        ["profile", rest @ ..] => {
            let window = match rest {
                [] => 86400,
                [w] => w.strip_prefix("--window=").and_then(|w| w.parse().ok()).filter(|w| *w > 0).unwrap_or_else(|| t.die(USAGE)),
                _ => t.die(USAGE),
            };
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            let text = std::fs::read_to_string(profile::path(&dir)).ok();
            println!("{}", profile::report(text.as_deref(), source::now(), window));
        }
        ["selftest"] => selftest(&t).unwrap_or_else(|e| t.die(&e)),
        ["--snapshot", dir] => snapshot(std::path::Path::new(dir)).unwrap_or_else(|e| t.die(&e)),
        ["update", rest @ ..] if rest.iter().all(|x| *x == "--check") => {
            let check = !rest.is_empty();
            profile::open(&config_path(&t).parent().map(PathBuf::from).unwrap_or_default());
            let t0 = std::time::Instant::now();
            let r = coreq::update::update(&t, check);
            // coreq's error text may name hosts or files: only the outcome is recorded
            profile::record(if check { "update.check" } else { "update" }, t0, r.as_ref().err().map(|_| "failed"));
            let u = coreq::service::unit(&t, std::path::Path::new("."), "dashq");
            let restart = restarts(&r, check, u.path.exists());
            match r {
                Ok(Some(v)) => println!("update: {v}"),
                Ok(None) => println!("update: up to date"),
                Err(e) => t.die(&e),
            }
            // the running service keeps the old release until its unit is reloaded (#83)
            if restart {
                let t0 = std::time::Instant::now();
                let r = coreq::service::run_all(&u.unload, true).and_then(|_| coreq::service::run_all(&u.load, false));
                profile::record("restart", t0, r.as_ref().err().map(|_| "failed"));
                r.unwrap_or_else(|e| t.die(&format!("restart: {e}")));
                println!("service restart: ok ({})", u.path.display());
            }
        }
        ["service", op @ ("install" | "uninstall")] => {
            let root = std::env::current_dir().unwrap_or_else(|e| t.die(&e.to_string()));
            let u = coreq::service::unit(&t, &root, "dashq");
            let r = if *op == "install" {
                coreq::update::install_self(&t).and_then(|_| coreq::service::run_all(&u.unload, true)).and_then(|_| coreq::service::write_unit(&u)).and_then(|_| coreq::service::run_all(&u.load, false))
            } else {
                coreq::service::run_all(&u.unload, true).and_then(|_| std::fs::remove_file(&u.path).map_err(|e| e.to_string()))
            };
            r.unwrap_or_else(|e| t.die(&e));
            println!("service {op}: ok ({})", u.path.display());
        }
        ["version"] => println!("dashq {}", t.version),
        _ => t.die(USAGE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // chain: `dashq update` switches the running service to the new release (#83)
    #[test]
    fn update_restarts_only_onto_a_new_release() {
        let new = Ok(Some("0.7.0".to_string()));
        assert!(restarts(&new, false, true));
        assert!(!restarts(&new, true, true), "--check installs nothing");
        assert!(!restarts(&new, false, false), "no service unit");
        assert!(!restarts(&Ok(None), false, true), "unchanged");
        assert!(!restarts(&Err("selftest failed".into()), false, true), "failed");
    }

    // chain: the installed service applies a newer release on its own, never the one rolled back after failed starts (#130)
    #[test]
    fn auto_update_applies_new_releases_but_not_a_rolled_back_one() {
        let new = Ok(Some("0.9.3".to_string()));
        assert_eq!(applies(&new, None), Some("0.9.3"));
        assert_eq!(applies(&new, Some("0.9.3")), None, "rolled back");
        assert_eq!(applies(&new, Some("0.9.2")), Some("0.9.3"), "a newer release than the rolled-back one");
        assert_eq!(applies(&Ok(None), None), None, "up to date");
        assert_eq!(applies(&Err("timeout".into()), None), None, "failed check");

        let d = std::env::temp_dir().join(format!("dashq-rollback-{}", std::process::id()));
        let release = |v: &str, starts: &str, healthy: bool| {
            let r = d.join("releases").join(v);
            std::fs::create_dir_all(r.join("starts")).unwrap();
            std::fs::write(r.join("starts").join("dashq"), starts).unwrap();
            if healthy {
                std::fs::write(r.join("healthy"), "").unwrap();
            }
        };
        release("0.9.2", "1", true);
        release("0.9.1", "3", false);
        assert_eq!(rolled_back(&d, "0.9.2", "dashq"), None, "older releases are not candidates");
        release("0.9.3", "2", false);
        assert_eq!(rolled_back(&d, "0.9.2", "dashq"), None, "still starting");
        release("0.9.3", "3", false);
        assert_eq!(rolled_back(&d, "0.9.2", "dashq"), Some("0.9.3".into()));
        assert_eq!(rolled_back(&d, "0.9.2", "other"), None, "starts count per project");
        release("0.9.3", "3", true);
        assert_eq!(rolled_back(&d, "0.9.2", "dashq"), None, "healthy");
        let _ = std::fs::remove_dir_all(&d);
    }

    // Chain D13: pinned shared card fixtures -> duplicate/stale/receipt behavior and unmodified assets.
    #[test]
    fn shared_board_conformance() {
        let out = std::process::Command::new("node").arg("tests/boards.cjs").current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("node is required for shared TaskQ conformance");
        assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }

    // Chain D13 live gate (#44): the live board run stops before any write on a failed guard or prerequisite.
    #[test]
    fn answer_live_gate_stops_before_writes() {
        let out = std::process::Command::new("node").arg("tests/answer-live-stub.cjs").current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("node is required for the answer live gate");
        assert!(out.status.success(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn fixture_rendering_is_pinned() {
        let html = fixture_html();
        if std::env::var_os("DASHQ_UPDATE_FIXTURE").is_some() {
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/fixture/page.html"), &html).unwrap();
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/fixture/data.json"), serde_json::to_string_pretty(&fixture_doc()).unwrap() + "\n").unwrap();
        }
        assert_eq!(html, FIXTURE_HTML, "rerun with DASHQ_UPDATE_FIXTURE=1 after an intended change");
        let pinned: Value = serde_json::from_str(include_str!("../fixture/data.json")).unwrap();
        assert_eq!(fixture_doc(), pinned);
    }

    #[test]
    fn snapshot_writes_html_and_png() {
        let d = std::env::temp_dir().join(format!("dashq-snap-{}", std::process::id()));
        let r = snapshot(&d);
        let html = std::fs::read_to_string(d.join("page.html")).unwrap();
        // pinned: fixture/page.html with the stylesheet inlined and the script dropped
        let css = format!("<style>\n{}</style>", style_css());
        assert_eq!(html, FIXTURE_HTML.replace("<link rel=\"stylesheet\" href=\"/style.css\">", &css).replace("<script src=\"/app.js\" defer></script>\n", ""));
        assert!(html.contains(&css) && !html.contains("/style.css") && !html.contains("/app.js"));
        // CI (ubuntu-latest) ships Chrome; a local run without it only checks the HTML
        if chrome().is_none() && std::env::var_os("CI").is_none() {
            return eprintln!("snapshot: no Chrome, PNG not checked");
        }
        r.unwrap();
        assert!(std::fs::read(d.join("page.png")).unwrap().starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    #[test]
    fn routes_and_headers() {
        let app = App { cfg: load_config_fixture(), cache: source::Cache::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default() };
        assert_eq!(route(&app, "GET", "/nope", "", None).0, "404 Not Found");
        assert_eq!(route(&app, "POST", "/data", "", None).0, "405 Method Not Allowed");
        assert_eq!(route(&app, "GET", "/data", "period=5", None).0, "400 Bad Request");
        assert_eq!(route(&app, "GET", "/app.js", "", None).0, "200 OK");
    }

    fn load_config_fixture() -> Config {
        Config { langfuse: None, forward: false, links: json!({}), github_token: None, projects: vec![], taskq_bin: None, serverq_report: String::new(), serverq_host: "fra/native".into(), public_host: "dash.example.com".into(), answer: None, resources: false, agentq_configure: false, taskq_logs: vec![] }
    }

    /// D19 U2: report -> failed refresh -> recovery retains producer age, values and incident evidence.
    #[test]
    fn startup_serverq_retains_last_good() {
        let dir = std::env::temp_dir().join(format!("dashq-startup-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("report.json");
        let observed = source::now() - 120;
        let report = json!({"schema": 1, "at": observed, "results": {"probe": {"status": "warn", "detail": "probe failed"}},
            "incidents": {"probe": {"state": "open", "first_seen": observed, "last_seen": observed, "occurrences": 1}}});
        std::fs::write(&path, report.to_string()).unwrap();
        let mut cfg = load_config_fixture();
        cfg.serverq_report = path.to_string_lossy().into();
        let app = App { cfg, cache: Default::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default() };
        let good = app.doc(1800, json!({}), None);
        assert_eq!(good["sources"]["serverq"]["state"], "ok");
        std::fs::write(&path, "invalid").unwrap();
        let failed = app.doc(1800, json!({}), None);
        assert_eq!(failed["sources"]["serverq"]["state"], "unconfirmed");
        assert_eq!(failed["sources"]["serverq"]["at"], observed);
        assert_eq!(failed["checks"]["results"], good["checks"]["results"]);
        assert_eq!(failed["checks"]["incidents"], good["checks"]["incidents"]);
        assert!(page::render(&failed, &json!({})).contains("current condition unverified"));
        std::fs::write(&path, report.to_string()).unwrap();
        let recovered = app.doc(1800, json!({}), None);
        assert_eq!(recovered["sources"]["serverq"]["state"], "ok");
        assert_eq!(recovered["checks"]["incidents"], good["checks"]["incidents"], "connection recovery is not incident recovery");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn config_errors_name_the_key_never_the_value() {
        let d = std::env::temp_dir().join(format!("dashq-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("config.json");
        let report_path = d.join("report.json");
        let binary_path = d.join("taskq");
        let token_path = d.join("answer-token");
        let good = json!({"langfuse": {"base_url": "https://lf.example.com", "public_key": "pk-lf-1", "secret_key": "sk-lf-SECRET", "project_id": "p1"},
            "github": {"token": "ghp_aaaaaaaaaaaaaaaaaaaaaaaa"}, "projects": ["taskqorg/dashq"], "serverq": {"report": report_path, "host": "fra/native"}, "public_host": "dash.example.com"});
        let write = |v: &Value| {
            std::fs::write(&p, v.to_string()).unwrap();
            #[cfg(unix)]
            std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        };
        write(&good);
        let c = load_config(&p).unwrap();
        assert_eq!(c.langfuse.as_ref().unwrap().1, format!("Basic {}", base64(b"pk-lf-1:sk-lf-SECRET")));
        assert_eq!(c.links["traces"], "https://lf.example.com/project/p1/traces");
        let mut bad = good.clone();
        bad["langfuse"]["secret_key"] = json!("SECRET-but-wrong");
        write(&bad);
        let e = load_config(&p).err().unwrap();
        assert_eq!(e, "config: bad key langfuse.secret_key");
        // forwarding is off unless the flag is a literal true
        assert!(!c.forward);
        let mut f = good.clone();
        f["langfuse"]["forward"] = json!("yes");
        write(&f);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key langfuse.forward");
        #[cfg(unix)]
        {
            write(&good);
            std::fs::set_permissions(&p, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
            assert_eq!(load_config(&p).err().unwrap(), "config: mode must be 0600");
        }
        let mut t = good.clone();
        t["taskq"] = json!({"binary": binary_path});
        write(&t);
        assert_eq!(load_config(&p).unwrap().taskq_bin.as_deref(), binary_path.to_str());
        t["answer"] = json!({"token": token_path, "boards": {"taskqorg/dashq": "ws-1"}});
        write(&t);
        let a = load_config(&p).unwrap().answer.unwrap();
        assert_eq!((a.token.to_str(), a.boards["taskqorg/dashq"].as_str()), (token_path.to_str(), "ws-1"));
        // only configured projects are answerable, and answers need the pinned binary
        t["answer"]["boards"] = json!({"other/repo": "ws-1"});
        write(&t);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key answer.boards");
        t["answer"]["boards"] = json!({"taskqorg/dashq": "ws-1"});
        t["taskq"]["binary"] = json!("taskq");
        write(&t);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key taskq.binary");
        t.as_object_mut().unwrap().remove("taskq");
        write(&t);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key taskq.binary");
        // Agent work logs (D21): host ids and absolute paths only
        let mut l = good.clone();
        l["taskq_logs"] = json!({"mac": report_path});
        write(&l);
        assert_eq!(load_config(&p).unwrap().taskq_logs, [("mac".to_string(), report_path.to_string_lossy().into_owned())]);
        l["taskq_logs"] = json!({"mac": "taskq.log"});
        write(&l);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key taskq_logs");
        write(&json!({"public_host": "dash.example.com"}));
        let first = load_config(&p).unwrap();
        assert!(first.langfuse.is_none() && first.github_token.is_none() && first.serverq_report.is_empty());
        assert_eq!(base64(b"ab"), "YWI=");
    }
}
