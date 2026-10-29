//! dashq: one read-only page for the taskqorg tools (dashq.md).

mod accounting;
mod agent_work;
mod model_use;
mod answer;
mod pm;
mod clients;
mod configure;
mod data;
mod doctor;
mod enroll;
mod explain;
mod export;
mod findings;
mod link;
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
const USAGE: &str = "usage: dashq serve | report [--period=S] [--host=H] [--agent=A] [--session=ID] [--block=ID] [--cursor=N] | selftest | doctor | profile [--window=S] | --snapshot DIR | admin WORKSPACE SUBJECT | answerer WORKSPACE SUBJECT on|off | pm-requester WORKSPACE SUBJECT on|off | controller WORKSPACE SUBJECT on|off | resources grant WORKSPACE SUBJECT INSTALLATION HOURS [host] [project-limits] [project=ID].. [runtime=ID].. | resources revoke GRANT_ID | update [--check] | service install|uninstall|run | version";

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
    task_controls: Option<answer::Config>,
    pm_requests: bool,
    /// `resources: true` enables the typed host-policy routes (D14); off until the target-host adapter is qualified.
    resources: bool,
    /// `agentq_configure: true` enables the guarded AgentQ review/apply routes (D16); off until AgentQ A5 qualifies.
    agentq_configure: bool,
    /// `taskq_logs` `{host: absolute taskq.log path}`: attempt stage records for Agent work (D21); empty: not configured.
    taskq_logs: Vec<(String, String)>,
    /// `telemetry` `{retention_days, max_mib}`: bounds of stored `taskq-telemetry/1` blocks, `max_mib` per installation (D11 #161, #181).
    limits: store::Limits,
    /// `prices`: the owner's model price table for API-equivalent estimates (D24); `null`: estimates read unknown.
    prices: Value,
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
    let write_config = |key: &str| -> Result<Option<answer::Config>, String> {
        if v.get(key).is_none() { return Ok(None); }
        let token = s(&format!("/{key}/token"), &|x| std::path::Path::new(x).is_absolute())?;
        let boards = v[key]["boards"].as_object().filter(|o| !o.is_empty())
            .and_then(|o| o.iter().map(|(b, w)| w.as_str().filter(|w| enroll::id(w, 64) && projects.contains(b)).map(|w| (b.clone(), w.to_string()))).collect())
            .ok_or(format!("config: bad key {key}.boards"))?;
        if taskq_bin.is_none() { return Err("config: bad key taskq.binary".into()); }
        Ok(Some(answer::Config { token: token.into(), boards }))
    };
    let answer = write_config("answer")?;
    let task_controls = write_config("task_controls")?;
    let pm_requests = match v.get("pm_requests") { None => false, Some(v) => v.as_bool().ok_or("config: bad key pm_requests")? };
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
    let n = |k: &str, max: u64| v.pointer(&format!("/telemetry/{k}")).map(|x| x.as_u64().filter(|x| (1..=max).contains(x)).ok_or(format!("config: bad key telemetry.{k}"))).transpose();
    if v.get("telemetry").is_some_and(|x| !x.is_object()) {
        return Err("config: bad key telemetry".into());
    }
    let mut limits = store::LIMITS;
    if let Some(d) = n("retention_days", 90)? {
        limits.telemetry_secs = d as i64 * 86400;
    }
    if let Some(m) = n("max_mib", 1024)? {
        limits.telemetry_bytes = m << 20;
    }
    let prices = v.get("prices").map(model_use::prices).transpose()?.unwrap_or(Value::Null);
    let public_host = s("/public_host", &|x| safe_host(x))?.to_ascii_lowercase();
    Ok(Config {
        limits,
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
        task_controls,
        pm_requests,
        resources,
        agentq_configure,
        taskq_logs,
        prices,
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
    link: link::Relay,
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
        // committed snapshots never wait for or depend on the optional sources above (D11)
        let projection = enroll::projection(&self.enroll, scope, None, now);
        let enrolled = projection.as_ref().map_or(json!([]), |p| enroll::resource_samples(&self.enroll, &p["rows"], now.saturating_sub(period) as i64));
        let findings = projection.as_ref().map_or(json!([]), |p| enroll::finding_samples(&self.enroll, &p["rows"]));
        // D21 (#182): stage records of the TaskQ telemetry in scope; a fixed two days, so every period and an attempt
        // started up to a day before the widest window share one decoded set
        let stages = projection.as_ref().map_or(json!([]), |p| enroll::stage_records(&self.enroll, &p["rows"], now as i64 - 2 * 86400));
        profile::step("enrollment", t0);
        let t0 = std::time::Instant::now();
        let doc = data::build(&json!({
            "now": now, "period": period, "filters": filters, "projects": c.projects, "serverq_host": c.serverq_host,
            "langfuse": lf, "boards": boards, "taskq": taskq, "releases": releases, "serverq": serverq, "enrolled_resources": enrolled,
            "installations": projection, "agent_work": agent_work, "agent_telemetry": stages, "serverq_samples": findings, "prices": c.prices,
        }));
        profile::step("build", t0);
        let mut doc = doc;
        // the same block by event id for the page's caller and `dashq report` alike, inside the projected rows' scope (#181 A6)
        if let Some(id) = doc["filters"]["block"].as_str().map(String::from) {
            doc["telemetry_block"] = enroll::telemetry_block(&self.enroll, projection.as_ref().map_or(&Value::Null, |p| &p["rows"]), &id);
        }
        // the page offers review/apply only where the route is on (D16)
        doc["controls"] = json!({"agentq_configure": c.agentq_configure});
        // outbox counts span every workspace: host-local readers only (D11)
        if let (Some(store::Scope::All), Ok(e)) = (scope, &self.enroll) {
            doc["sources"]["export"] = export::status(Some(e.store()), &self.export, c.langfuse.is_some() && c.forward, now);
        }
        doc
    }
}

/// `dashq report --k=v..` (D2): the period and filters of a `/data` request with the same parameters.
fn report_query(args: &[&str]) -> Result<(u64, Value), String> {
    let q: Vec<&str> = args.iter().map(|x| x.strip_prefix("--").filter(|x| x.contains('=')).ok_or(USAGE)).collect::<Result<_, _>>()?;
    data::query(&q.join("&"))
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
    let ctx = answer::Ctx { enroll: &app.enroll, cfg: if path == "/task/control" { c.task_controls.as_ref() } else { c.answer.as_ref() }, bin: c.taskq_bin.as_deref(), public_host: &c.public_host, state: &app.answers };
    let rctx = policy::Ctx { enroll: &app.enroll, enabled: c.resources, public_host: &c.public_host, state: &app.resources };
    let cctx = configure::Ctx { enroll: &app.enroll, enabled: c.agentq_configure, public_host: &c.public_host };
    let pctx = pm::Ctx { enroll: &app.enroll, enabled: c.pm_requests, public_host: &c.public_host, state: &app.answers };
    let t0 = profile::begin();
    let (op, status) = if let Some(x) = link::route(&app.link, &app.enroll, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.link", x.status)
    } else if let Some(x) = pm::route(&pctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.pm", x.status)
    } else if let Some(x) = configure::route(&cctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        ("http.configure", x.status)
    } else if let Some(x) = answer::route(&ctx, &req, source::now() as i64) {
        respond_with(&mut s, x.status, x.ctype, &x.body, &x.headers, x.forms);
        (if path == "/task/control" { "http.control" } else { "http.answer" }, x.status)
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
    if !["/app.js", "/style.css", "/data"].contains(&path) && !page::VIEWS.iter().any(|(p, _)| *p == path) {
        return ("404 Not Found", HTML, b"<!doctype html><title>View not found</title><h1>View not found</h1><p>The requested path is unsupported.</p><a href=\"/\">Return to overview</a>".to_vec());
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
            if path != "/data" {
                return ("200 OK", HTML, page::render_view(&doc, &app.cfg.links, path).into_bytes());
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
    report_requirements(&doctor::check(Path::new("curl"), cfg.taskq_bin.as_deref().map(Path::new), chrome().as_deref()));
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
    let enroll = enroll::Enroll::open_with(&dir, &cfg.public_host, cfg.limits);
    if let Err(e) = &enroll {
        eprintln!("enrollment: {e}");
    }
    let app = Arc::new(App { cfg, cache: source::Cache::default(), enroll, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() });
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
    [include_str!("../assets/taskq/resource-controls.js"), include_str!("../assets/resources.js"), include_str!("../assets/taskq/project-blocks.js"), include_str!("../assets/taskq/answer-card.js"), include_str!("../assets/taskq/task-controls.js"), include_str!("../assets/taskq/task-workspace.js"), include_str!("../assets/taskq/pm-composer.js"), include_str!("../assets/workspace.js"), include_str!("../assets/boards.js"), include_str!("../assets/app.js")].join("\n")
}

fn style_css() -> String {
    [include_str!("../assets/style.css"), include_str!("../assets/taskq/resource-controls.css"), include_str!("../assets/resources.css"), include_str!("../assets/taskq/project-blocks.css"), include_str!("../assets/taskq/answer-card.css"), include_str!("../assets/taskq/pm-composer.css"), include_str!("../assets/workspace.css"), &format!("@scope (#taskq-workspace) {{{}}}", include_str!("../assets/taskq/task-workspace.css"))].join("\n")
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

/// D2 doctor: this release's host tool requirements; `taskq.binary` from the config when one loads.
fn doctor_lines(t: &Tool) -> Vec<(bool, String)> {
    let p = config_path(t);
    let cfg = if p.exists() { load_config(&p).map(|c| c.taskq_bin) } else { Ok(None) };
    let mut lines = doctor::check(Path::new("curl"), cfg.as_ref().ok().and_then(|b| b.as_deref()).map(Path::new), chrome().as_deref());
    if let Err(e) = cfg {
        lines[1] = (true, format!("taskq: FAILED not checked, {e}; fix: correct the config"));
    }
    lines
}

/// Install, update and service start report each unmet requirement with its fix; `dashq doctor` stays failed (D2).
fn report_requirements(lines: &[(bool, String)]) {
    for l in doctor::failures(lines) {
        eprintln!("requirement: {l}");
    }
}

fn snapshot(dir: &std::path::Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("snapshot: {e}"))?;
    let dir = dir.canonicalize().map_err(|e| format!("snapshot: {e}"))?;
    let (html, png) = (dir.join("page.html"), dir.join("page.png"));
    std::fs::write(&html, snapshot_html()).map_err(|e| format!("snapshot: {e}"))?;
    let _ = std::fs::remove_file(&png);
    let c = chrome().ok_or("snapshot: PNG renderer not found (headless Chrome; set DASHQ_CHROME)")?;
    // Chrome's default profile may be unavailable; own a fresh profile and remove only that directory.
    let profile = std::env::temp_dir().join(format!("dashq-chrome-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
    std::fs::create_dir(&profile).map_err(|e| format!("snapshot: renderer profile: {e}"))?;
    // ponytail: fixed 1280x4000 window; measure the page height if the fixture outgrows it
    let out = std::process::Command::new(&c)
        .args(["--headless", "--disable-gpu", "--no-sandbox", "--hide-scrollbars", "--window-size=1280,4000"])
        .arg(format!("--user-data-dir={}", profile.display()))
        .arg(format!("--screenshot={}", png.display()))
        .arg(format!("file://{}", html.display()))
        .output();
    let _ = std::fs::remove_dir_all(&profile);
    let out = out.map_err(|e| format!("snapshot: run {}: {e}", c.display()))?;
    if !png.is_file() {
        return Err(format!("snapshot: no page.png (`{} --headless --screenshot`: {}: {})", c.display(), out.status, doctor::first_line(&String::from_utf8_lossy(&out.stderr))));
    }
    println!("snapshot: {} {}", html.display(), png.display());
    Ok(())
}

fn selftest(t: &Tool) -> Result<(), String> {
    if fixture_html() != FIXTURE_HTML {
        return Err("fixture: rendering differs from fixture/page.html".into());
    }
    answer::control_fixture()?;
    println!("fixture: ok; task controls: taskq.task-controls/1");
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
    let doc = App { cfg, cache: source::Cache::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() }.doc(1800, json!({}), None);
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
            let (period, filters) = report_query(rest).unwrap_or_else(|e| t.die(&e));
            let cfg = load_config(&config_path(&t)).unwrap_or_else(|e| t.die(&e));
            // the host administrator's local report sees every installation (D11), like a loopback page request
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            profile::open(&dir);
            let enroll = enroll::Enroll::open_with(&dir, &cfg.public_host, cfg.limits);
            println!("{}", App { cfg, cache: source::Cache::default(), enroll, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() }.doc(period, filters, Some(store::Scope::All)));
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
        ["pm-requester", workspace, subject, op @ ("on" | "off")] => {
            if !enroll::id(workspace, 64) || !enroll::valid_subject(subject) { t.die("pm-requester: bad workspace or subject"); }
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            let e = enroll::Enroll::open(&dir, "").unwrap_or_else(|e| t.die(&e));
            e.store().pm_requester(workspace, subject, *op == "on").unwrap_or_else(|e| t.die(&format!("PM grant: {e:?}")));
            println!("PM actor: {} · {op} in {workspace}", pm::actor(subject));
        }
        ["controller", workspace, subject, op @ ("on" | "off")] => {
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            enroll::Enroll::controller(&dir, workspace, subject, *op == "on").unwrap_or_else(|e| t.die(&e));
            println!("controller: {subject} {op} in {workspace}");
        }
        ["resources", "grant", workspace, subject, installation, hours, rest @ ..] => {
            let hours: i64 = hours.parse().ok().filter(|h| (1..=24 * 365).contains(h)).unwrap_or_else(|| t.die(USAGE));
            let (mut host, mut limits, mut projects, mut runtimes) = (false, false, vec![], vec![]);
            for x in rest {
                match x.split_once('=') {
                    None if *x == "host" => host = true,
                    None if *x == "project-limits" => limits = true,
                    Some(("project", p)) => projects.push(p.to_string()),
                    Some(("runtime", r)) => runtimes.push(r.to_string()),
                    _ => t.die(USAGE),
                }
            }
            let dir = config_path(&t).parent().map(PathBuf::from).unwrap_or_default();
            let e = enroll::Enroll::open(&dir, "").unwrap_or_else(|e| t.die(&e));
            let expires = source::now() as i64 + hours * 3600;
            let (id, actor) = policy::issue(e.store(), workspace, subject, installation, host, limits, &projects, &runtimes, expires).unwrap_or_else(|e| t.die(&e));
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
        ["doctor"] => {
            let lines = doctor_lines(&t);
            for (_, l) in &lines {
                println!("{l}");
            }
            let failed = doctor::failures(&lines).len();
            if failed > 0 {
                t.die(&format!("doctor: {failed} requirement(s) failed"));
            }
        }
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
            let applied = r.as_ref().ok().cloned().flatten().filter(|_| !check);
            match r {
                Ok(Some(v)) => println!("update: {v}"),
                Ok(None) => println!("update: up to date"),
                Err(e) => t.die(&e),
            }
            if !check {
                report_requirements(&applied.map_or_else(|| doctor_lines(&t), |v| doctor::release(&t.release_exe(&v))));
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
            if *op == "install" {
                report_requirements(&doctor_lines(&t));
            }
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

    // #186 A2-A4: unchanged release assets and PM identity/receipt fixtures.
    #[test]
    fn shared_workspace_conformance() {
        let out = std::process::Command::new("node").arg("tests/workspace.cjs").current_dir(env!("CARGO_MANIFEST_DIR")).output().expect("node is required for shared TaskQ conformance");
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
        let app = App { cfg: load_config_fixture(), cache: source::Cache::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() };
        assert_eq!(route(&app, "GET", "/nope", "", None).0, "404 Not Found");
        let missing = route(&app, "GET", "/nope", "", None);
        assert!(String::from_utf8(missing.2).unwrap().contains("Return to overview"));
        assert_eq!(route(&app, "GET", "/projects", "period=5", None).0, "400 Bad Request");
        // D22 / F7: direct shared view URLs must resolve, not become 404 or JSON.
        for path in ["/projects", "/hosts", "/agents", "/services", "/checks", "/integrations", "/telemetry", "/reports"] {
            let r = route(&app, "GET", path, "period=7200&host=fixture", None);
            assert_eq!(r.0, "200 OK", "{path}");
            assert!(String::from_utf8(r.2).unwrap().contains(&format!("data-view=\"{path}\"")));
        }
        assert_eq!(route(&app, "POST", "/data", "", None).0, "405 Method Not Allowed");
        assert_eq!(route(&app, "GET", "/data", "period=5", None).0, "400 Bad Request");
        assert_eq!(route(&app, "GET", "/app.js", "", None).0, "200 OK");
    }

    // Chain: stored block -> `/data?block=` and `dashq report --block=` -> one document builder.
    // Risk (#181 A6): the page's caller and the host report reading different blocks, receipts or coverage, or a
    // caller without a subject reading a block by guessing its id.
    #[test]
    fn telemetry_block_reads_back_alike_through_data_and_report() {
        let d = std::env::temp_dir().join(format!("dashq-block-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        enroll::Enroll::admin(&d, "ws", "github|1").unwrap();
        let e = enroll::Enroll::open(&d, "dash.example.com");
        let now = source::now() as i64;
        let s = e.as_ref().unwrap().store();
        s.grant(&store::Installation { id: "tq-1", product: "taskq", runtime: "native", label: "mac" }, "ws", "github|1", r#"["taskq-telemetry/1"]"#, b"v1", now).unwrap();
        let payload = r#"{"schema":1,"first":{"seq":1,"id":"a"},"last":{"seq":2,"id":"b"},"rows":2,"bytes":1,"gzip":"x"}"#;
        s.accept(b"v1", &[store::Event { id: "t-1", schema: "taskq-telemetry/1", observed: now, kind: store::Kind::Telemetry, payload }], now).unwrap();
        let app = App { cfg: load_config_fixture(), cache: source::Cache::default(), enroll: e, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() };
        let (_, _, body) = route(&app, "GET", "/data", "block=t-1", Some(store::Scope::All));
        let data: Value = serde_json::from_slice(&body).unwrap();
        let (period, filters) = report_query(&["--block=t-1"]).unwrap();
        let report = app.doc(period, filters, Some(store::Scope::All));
        let b = &data["telemetry_block"];
        assert_eq!(b, &report["telemetry_block"]);
        // #222 / D23: the page, /data and report consume one scoped route/coverage projection.
        let rows = &data["installations"]["rows"];
        for field in ["route", "telemetry"] { assert_eq!(rows[0][field], report["installations"]["rows"][0][field]); }
        assert_eq!(rows[0]["telemetry"]["evidence"][0]["event_id"], "t-1");
        assert!(rows[0]["telemetry"]["evidence"][0].get("payload").is_none());
        let page = String::from_utf8(route(&app, "GET", "/telemetry", "", Some(store::Scope::All)).2).unwrap();
        assert!(page.contains("block=t-1") && page.contains("data-stage=\"accepted\" data-state=\"known\""));
        let scoped = String::from_utf8(route(&app, "GET", "/telemetry", "", None).2).unwrap();
        assert!(!scoped.contains("block=t-1"), "no subject cannot read sender evidence");
        assert_eq!((&b["state"], &b["blocks"][0]["payload"], &b["blocks"][0]["received_at"], &b["blocks"][0]["coverage"]["retained"]["last_seq"]), (&json!("ok"), &serde_json::from_str::<Value>(payload).unwrap(), &json!(now), &json!(2)));
        assert_eq!(app.doc(1800, json!({"block": "t-1"}), None)["telemetry_block"]["blocks"], json!([]), "no subject: no block");
        assert!(report_query(&["--block=a b"]).is_err() && report_query(&["block=t-1"]).is_err());
    }

    // Chain (#182 A5): stored TaskQ telemetry plus a cutover log -> one document for `/data`, `dashq report` and the
    // page. Risk: the three disagree on attempts, or the cutover host shows a gap or a duplicate there.
    #[test]
    fn agent_work_agrees_across_data_report_and_page() {
        use std::io::Write as _;
        let fx: Value = serde_json::from_str(include_str!("../fixture/taskq-telemetry/stages.json")).unwrap();
        let now = source::now() as i64;
        // replay the fixture one hour before the current clock
        let shift = (now - fx["t0"].as_i64().unwrap()) * 1000;
        let moved = |l: &Value| {
            let mut l = l.clone();
            for k in ["/received_ms", "/observation/started_ms", "/at_ms"] {
                if let Some(v) = l.pointer_mut(k).filter(|v| v.is_i64()) {
                    *v = json!(v.as_i64().unwrap() + shift);
                }
            }
            l
        };
        let d = std::env::temp_dir().join(format!("dashq-stages-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        enroll::Enroll::admin(&d, "ws", "github|1").unwrap();
        let e = enroll::Enroll::open(&d, "dash.example.com");
        let s = e.as_ref().unwrap().store();
        let mut payloads = vec![];
        for (n, inst) in fx["installations"].as_array().unwrap().iter().enumerate() {
            let id = format!("tq-{n}");
            s.grant(&store::Installation { id: &id, product: "taskq", runtime: "native", label: inst["label"].as_str().unwrap() }, "ws", "github|1", r#"["taskq-telemetry/1"]"#, id.as_bytes(), now).unwrap();
            for (i, b) in inst["blocks"].as_array().unwrap().iter().enumerate() {
                let lines: Vec<Value> = b.as_array().unwrap().iter().map(moved).collect();
                let plain: String = lines.iter().map(|l| format!("{l}\n")).collect();
                let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
                z.write_all(plain.as_bytes()).unwrap();
                let p = json!({"schema": 1, "first": lines[0]["seq"], "last": lines[lines.len() - 1]["seq"], "rows": lines.len(), "bytes": plain.len(), "gzip": base64(&z.finish().unwrap())});
                payloads.push((id.clone(), format!("t-{n}-{i}"), p.to_string()));
            }
        }
        for (id, ev, p) in &payloads {
            s.accept(id.as_bytes(), &[store::Event { id: ev, schema: "taskq-telemetry/1", observed: now, kind: store::Kind::Telemetry, payload: p }], now).unwrap();
        }
        let log = d.join("taskq.log");
        let lines: Vec<String> = fx["log"]["mac"].as_array().unwrap().iter().filter_map(|l| {
            let r: Value = serde_json::from_str(l.as_str()?.strip_prefix("taskq-stage ")?).ok()?;
            Some(format!("taskq-stage {}", moved(&r)))
        }).collect();
        std::fs::write(&log, lines.join("\n")).unwrap();
        let mut cfg = load_config_fixture();
        cfg.projects = vec!["taskqorg/dashq".into()];
        cfg.taskq_logs = vec![("mac".into(), log.to_string_lossy().into())];
        let app = App { cfg, cache: source::Cache::default(), enroll: e, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() };
        let (_, _, body) = route(&app, "GET", "/data", "period=7200", Some(store::Scope::All));
        let mut data: Value = serde_json::from_slice(&body).unwrap();
        let (period, filters) = report_query(&["--period=7200"]).unwrap();
        let mut report = app.doc(period, filters, Some(store::Scope::All));
        // Separate requests can cross a second; validate ages/windows before comparing their stable contents.
        for doc in [&mut data, &mut report] {
            let at = doc["generated_at"].as_u64().unwrap();
            for host in doc["agent_work"]["hosts"].as_array_mut().unwrap() {
                let src = &mut host["source"];
                if let Some(observed) = src["at"].as_u64() {
                    assert_eq!(src["age_s"], json!(at.saturating_sub(observed)));
                    src["age_s"] = json!(0);
                }
            }
            for k in ["query", "changes"] {
                assert_eq!((&doc[k]["from"], &doc[k]["to"]), (&json!(at - period), &json!(at)));
                doc[k]["from"] = json!(0);
                doc[k]["to"] = json!(period);
            }
        }
        let (a, r) = (&data["agent_work"], &report["agent_work"]);
        assert_eq!((&a["attempts"], &a["rows"], &a["hosts"]), (&r["attempts"], &r["rows"], &r["hosts"]));
        assert_eq!(a["totals"], r["totals"], "#221: browser and CLI Reports totals agree");
        // D23 A6: the same projection names the same window, cursor, cap, changes and requests
        for k in ["query", "changes"] { assert_eq!(data[k], report[k], "{k}"); }
        assert_eq!((&data["attention"]["requests"], &data["query"]["timezone"], &data["query"]["cap"]), (&report["attention"]["requests"], &json!("UTC"), &json!(200)));
        assert_eq!(data["model_use"], report["model_use"], "#184 A5: the report reproduces the page's model use");
        assert_eq!((a["attempts_total"].as_u64(), data["sources"]["agent_work"]["state"].as_str()), (Some(7), Some("incomplete")));
        let page = String::from_utf8(route(&app, "GET", "/agents", "period=7200", Some(store::Scope::All)).2).unwrap();
        for t in a["attempts"].as_array().unwrap() {
            assert!(page.contains(&format!("data-attempt=\"attempt-{}-{}\"", t["task"], t["attempt"])), "attempt {} on the page", t["attempt"]);
        }
        let _ = std::fs::remove_dir_all(&d);
    }

    /// D23 A6 live (owner host, `DASHQ_LIVE_CONFIG=<config> cargo test -- --ignored live_waiting`): one real board
    /// read through the pinned taskq binary; `/data` and `dashq report` agree, and every waiting row carries TaskQ's
    /// own admission. Volatile times (`generated_at`, ages, window bounds, which move by the second) are dropped before comparing.
    #[test]
    #[ignore]
    fn live_waiting_work_report_matches_data() {
        let p = PathBuf::from(std::env::var("DASHQ_LIVE_CONFIG").expect("DASHQ_LIVE_CONFIG"));
        let cfg = load_config(&p).unwrap();
        let enroll = enroll::Enroll::open_with(p.parent().unwrap(), &cfg.public_host, cfg.limits);
        let app = App { cfg, cache: source::Cache::default(), enroll, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() };
        let (_, _, body) = route(&app, "GET", "/data", "period=86400&cursor=0", Some(store::Scope::All));
        let (period, filters) = report_query(&["--period=86400", "--cursor=0"]).unwrap();
        fn calm(v: &mut Value) {
            match v {
                Value::Object(o) => {
                    for k in ["generated_at", "age_s", "from", "to", "at", "now", "start", "end"] { o.remove(k); }
                    o.values_mut().for_each(calm);
                }
                Value::Array(a) => a.iter_mut().for_each(calm),
                _ => {}
            }
        }
        let (mut data, mut report): (Value, Value) = (serde_json::from_slice(&body).unwrap(), app.doc(period, filters, Some(store::Scope::All)));
        let waiting: Vec<String> = data["boards"].as_array().unwrap().iter().flat_map(|b| b["tasks"].as_array().unwrap().iter().filter(|t| t["admission"].is_object())
            .map(|t| format!("{}#{} {} {} {} observed {}", b["project"].as_str().unwrap(), t["n"], t["state"], t["admission"]["code"], t["admission"]["reason"], t["admission"]["observed"]))).collect();
        eprintln!("{}\nquery {}\nchanges {} rows, comparison {}\nrequests {}", waiting.join("\n"), data["query"], data["changes"]["rows_total"], data["changes"]["comparison"], data["attention"]["requests"].as_array().unwrap().len());
        assert!(!waiting.is_empty() && waiting.iter().all(|w| !w.contains(" \"unknown\" ")), "TaskQ admission on every waiting row");
        calm(&mut data);
        calm(&mut report);
        assert!(data == report, "/data and dashq report differ");
    }

    fn load_config_fixture() -> Config {
        Config { prices: Value::Null, langfuse: None, forward: false, links: json!({}), github_token: None, projects: vec![], taskq_bin: None, serverq_report: String::new(), serverq_host: "fra/native".into(), public_host: "dash.example.com".into(), answer: None, task_controls: None, pm_requests: false, resources: false, agentq_configure: false, taskq_logs: vec![], limits: store::LIMITS }
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
        let app = App { cfg, cache: Default::default(), enroll: Err(String::new()), answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() };
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
        // #161 A3: telemetry bounds come from config, within limits, and name the bad key
        assert_eq!((c.limits.telemetry_secs, c.limits.telemetry_bytes), (7 * 86400, 160 << 20));
        let mut t = good.clone();
        t["telemetry"] = json!({"retention_days": 30, "max_mib": 64});
        write(&t);
        let l = load_config(&p).unwrap().limits;
        assert_eq!((l.telemetry_secs, l.telemetry_bytes), (30 * 86400, 64 << 20));
        for (bad, key) in [(json!({"retention_days": 0}), "telemetry.retention_days"), (json!({"max_mib": 4096}), "telemetry.max_mib"), (json!(5), "telemetry")] {
            t["telemetry"] = bad;
            write(&t);
            assert_eq!(load_config(&p).err().unwrap(), format!("config: bad key {key}"));
        }
        // #186 A1: PM requires a literal boolean opt-in.
        assert!(!c.pm_requests);
        let mut pm = good.clone(); pm["pm_requests"] = json!(true); write(&pm);
        assert!(load_config(&p).unwrap().pm_requests);
        pm["pm_requests"] = json!("yes"); write(&pm);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key pm_requests");
        // Forwarding has its own opt-in.
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
        // #179 A1/A5: controls are separately configured, off by default, and bound to configured boards.
        assert!(load_config(&p).unwrap().task_controls.is_none());
        t["task_controls"] = json!({"token": token_path, "boards": {"taskqorg/dashq": "ws-1"}});
        write(&t);
        assert_eq!(load_config(&p).unwrap().task_controls.unwrap().boards["taskqorg/dashq"], "ws-1");
        t["task_controls"]["boards"] = json!({"other/repo": "ws-1"});
        write(&t);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key task_controls.boards");
        t["task_controls"]["boards"] = json!({"taskqorg/dashq": "ws-1"});
        t["task_controls"]["token"] = json!("relative");
        write(&t);
        assert_eq!(load_config(&p).err().unwrap(), "config: bad key task_controls.token");
        t.as_object_mut().unwrap().remove("task_controls");
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
