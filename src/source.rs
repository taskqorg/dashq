//! Source reads (dashq.md D4): HTTP through the OS `curl`, one cache slot per key with single flight.

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

pub const CAP: usize = 10_000;

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A failed read: short reason (never a secret or a raw error) and, for rate limits, when to retry.
pub type Fail = (String, Option<u64>);

/// One cached source read: `{value, at, error, incomplete}`. A failed read keeps the previous value and time.
#[derive(Default)]
struct Slot {
    snap: Value,
    next: u64,
    reading: bool,
}

impl Slot {
    fn finish(&mut self, at: u64, ttl: u64, result: Result<(Value, bool), Fail>) {
        self.next = now() + ttl;
        self.reading = false;
        match result {
            Ok((value, incomplete)) => self.snap = json!({"value": value, "at": at, "error": null, "incomplete": incomplete}),
            Err((reason, retry)) => {
                if self.snap.is_null() {
                    self.snap = json!({"value": null, "at": null, "incomplete": false});
                }
                self.snap["error"] = json!(reason);
                if let Some(retry) = retry.filter(|r| *r > now()) { self.next = retry; }
            }
        }
    }
}

#[derive(Default)]
pub struct Cache(Mutex<HashMap<String, Arc<Mutex<Slot>>>>);

impl Cache {
    /// One background read per key; pending callers get the last-good snapshot, marked unconfirmed (D4/D6).
    pub fn refresh(&self, key: &str, ttl: u64, read: impl FnOnce() -> Result<(Value, bool), Fail> + Send + 'static) -> Value {
        self.background(key, ttl, true, read)
    }

    /// The same background read for a source with its own freshness limit: its last value keeps its state and age
    /// while the next read runs; a source with no value yet is unknown (D4/D6).
    pub fn revalidate(&self, key: &str, ttl: u64, read: impl FnOnce() -> Result<(Value, bool), Fail> + Send + 'static) -> Value {
        self.background(key, ttl, false, read)
    }

    fn background(&self, key: &str, ttl: u64, mark: bool, read: impl FnOnce() -> Result<(Value, bool), Fail> + Send + 'static) -> Value {
        let slot = self.0.lock().unwrap().entry(key.to_string()).or_default().clone();
        let mut s = slot.lock().unwrap();
        if s.reading || (!s.snap.is_null() && now() < s.next) { return s.snap.clone(); }
        s.reading = true;
        let cold = s.snap.is_null();
        if cold {
            s.snap = json!({"value": null, "at": null, "incomplete": false});
        }
        if cold || mark {
            s.snap["error"] = json!("refresh in progress");
        }
        let (target, key, at) = (slot.clone(), key.to_string(), now());
        let span = crate::profile::current();
        if std::thread::Builder::new().spawn(move || {
            crate::profile::enter(span);
            let t0 = std::time::Instant::now();
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(read))
                .unwrap_or_else(|_| Err(("read panicked".into(), None)));
            crate::profile::record(&format!("read.{}", key.split(':').next().unwrap_or(&key)), t0, result.as_ref().err().map(|e| e.0.as_str()));
            target.lock().unwrap().finish(at, ttl, result);
        }).is_err() {
            s.finish(at, ttl, Err(("refresh unavailable".into(), None)));
        }
        s.snap.clone()
    }

    pub fn get(&self, key: &str, ttl: u64, read: impl FnOnce() -> Result<(Value, bool), Fail>) -> Value {
        let slot = self.0.lock().unwrap().entry(key.to_string()).or_default().clone();
        // single flight: the slot lock is held during the read, so waiting callers share its result
        let mut s = slot.lock().unwrap();
        let now = now();
        if s.reading || (!s.snap.is_null() && now < s.next) {
            return s.snap.clone();
        }
        let t0 = std::time::Instant::now();
        let r = read();
        // `langfuse:1800` is `read.langfuse` (D20)
        crate::profile::record(&format!("read.{}", key.split(':').next().unwrap_or(key)), t0, r.as_ref().err().map(|e| e.0.as_str()));
        s.finish(now, ttl, r);
        s.snap.clone()
    }
}

pub struct Resp {
    pub status: u16,
    pub date: Option<u64>,
    pub reset: Option<u64>,
    pub body: Value,
}

/// GET with a 10 s timeout. URL and headers go to curl on stdin: no secret in the process list.
pub fn get(url: &str, headers: &[String]) -> Result<Resp, Fail> {
    send(url, headers, None)
}

/// GET, or POST of a JSON `body` when given. HTTPS only.
pub fn send(url: &str, headers: &[String], body: Option<&str>) -> Result<Resp, Fail> {
    parse_response(&exchange(url, headers, body)?).ok_or_else(|| {
        eprintln!("curl failed: unreadable response");
        ("invalid response".into(), None)
    })
}

/// POST to a receiver that answers in plain text: its status alone, the reply is never read.
pub fn post_status(url: &str, headers: &[String], body: &str) -> Result<u16, Fail> {
    response(&exchange(url, headers, Some(body))?).map(|(status, ..)| status).ok_or(("invalid response".into(), None))
}

fn exchange(url: &str, headers: &[String], body: Option<&str>) -> Result<String, Fail> {
    let q = |x: &str| format!("\"{}\"", x.replace('\\', "\\\\").replace('"', "\\\""));
    let mut cfg = format!("url = {}\nuser-agent = \"dashq\"\n", q(url));
    for h in headers {
        cfg += &format!("header = {}\n", q(h));
    }
    if let Some(b) = body {
        cfg += &format!("data-binary = {}\n", q(b));
    }
    // plain HTTP only for in-process test receivers; production config accepts only https:// bases
    let proto = if cfg!(test) && url.starts_with("http://127.0.0.1:") { "=http" } else { "=https" };
    // URL, headers and body go on stdin, so the logged command never holds a secret
    let cmd = format!("curl -sS -i --max-time 10 --proto {proto} -K -");
    let log = |e: &str| eprintln!("{cmd} failed: {e}");
    let mut child = coreq::quiet(Command::new("curl").args(["-sS", "-i", "--max-time", "10", "--proto", proto, "-K", "-"]))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            log(&e.to_string());
            ("curl not found".to_string(), None)
        })?;
    child.stdin.take().unwrap().write_all(cfg.as_bytes()).map_err(|e| {
        log(&e.to_string());
        ("curl input failed".to_string(), None)
    })?;
    let out = child.wait_with_output().map_err(|e| {
        log(&e.to_string());
        ("curl failed".to_string(), None)
    })?;
    if !out.status.success() {
        log(&format!("{}: {}", out.status, crate::doctor::first_line(&String::from_utf8_lossy(&out.stderr))));
    }
    match out.status.code() {
        Some(0) => {}
        Some(28) => return Err(("timeout".into(), None)),
        c => return Err((format!("curl exit {}", c.unwrap_or(-1)), None)),
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn parse_response(raw: &str) -> Option<Resp> {
    let (status, head, body) = response(raw)?;
    let header = |name: &str| head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()));
    let body = if body.trim().is_empty() { Value::Null } else { serde_json::from_str(body).ok()? };
    Some(Resp { status, date: header("date").and_then(|d| http_date(&d)), reset: header("x-ratelimit-reset").and_then(|r| r.parse().ok()), body })
}

/// Status, headers and body of the final response in curl's `-i` output.
fn response(raw: &str) -> Option<(u16, &str, &str)> {
    let mut rest = raw;
    loop {
        let (head, body) = rest.split_once("\r\n\r\n")?;
        let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
        // 1xx and a proxy's CONNECT reply precede the real response
        if (100..200).contains(&status) || (status == 200 && body.starts_with("HTTP/")) {
            rest = body;
            continue;
        }
        return Some((status, head, body));
    }
}

pub fn enc(s: &str) -> String {
    s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect()
}

pub fn dec(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' => {
                out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

// ---- time: UTC only, no clock but the caller's `now`

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

fn civil(t: u64) -> (i64, i64, i64, u64) {
    let z = (t / 86400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d, t % 86400)
}

/// `2026-10-13T07:05:39Z`, fractions ignored. `ponytail:` UTC only; Langfuse and GitHub send `Z`.
pub fn iso_secs(s: &str) -> Option<u64> {
    let n = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    if s.get(4..5) != Some("-") || s.get(10..11) != Some("T") {
        return None;
    }
    let t = days_from_civil(n(0..4)?, n(5..7)?, n(8..10)?) * 86400 + n(11..13)? * 3600 + n(14..16)? * 60 + n(17..19)?;
    u64::try_from(t).ok()
}

pub fn iso(t: u64) -> String {
    let (y, m, d, s) = civil(t);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

/// `Tue, 13 Oct 2026 07:05:39 GMT`.
fn http_date(s: &str) -> Option<u64> {
    let p: Vec<&str> = s.split_whitespace().collect();
    let mon = *p.get(2)?;
    let m = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"].iter().position(|x| *x == mon)? + 1;
    iso_secs(&format!("{}-{m:02}-{:0>2}T{}Z", p.get(3)?, p.get(1)?, p.get(4)?))
}

/// Exact cross-host time (specq Time); display forms are never parsed back.
pub fn clock(t: u64, _now: u64) -> String {
    let (y, m, d, s) = civil(t);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", s / 3600, s / 60 % 60)
}

// ---- Langfuse (D4.1)

/// Reads of one period: `{activity, agentq.*, truncated}`. The agentq events key by their span name; `activity` is
/// every model call (Langfuse `GENERATION` observations), whatever its producer.
pub fn langfuse(base: &str, auth: &str, now: u64, period: u64) -> Result<(Value, bool), Fail> {
    let queries: [(&str, (&str, &str), u64); 10] = [
        ("activity", ("type", "GENERATION"), period),
        // fleet, versions, last seen and health: the hourly heartbeat, health window
        ("agentq.installation", ("name", "agentq.installation"), 86400),
        ("agentq.run", ("name", "agentq.run"), period),
        ("agentq.compress", ("name", "agentq.compress"), period),
        ("agentq.retrieval", ("name", "agentq.retrieval"), period),
        // per-call model use (agentq#161) for API-equivalent estimates (D24)
        ("agentq.model", ("name", "agentq.model"), period),
        ("agentq.incident", ("name", "agentq.incident"), 86400),
        // resource samples (D4.4): the window is the period
        ("agentq.resource", ("name", "agentq.resource"), period),
        // per-client use (D8.4, #131) always counts 24 h; the 24 h period reuses its own reads
        ("agentq.compress.24h", ("name", "agentq.compress"), 86400),
        ("agentq.retrieval.24h", ("name", "agentq.retrieval"), 86400),
    ];
    let reused = |key: &str| period == 86400 && key.ends_with(".24h");
    let reads: Vec<Result<(Vec<Value>, bool), Fail>> = std::thread::scope(|sc| {
        let hs: Vec<_> = queries.iter().map(|(key, q, secs)| sc.spawn(move || if reused(key) { Ok((Vec::new(), false)) } else { observations(base, auth, now, *q, *secs, fields(key)) })).collect();
        hs.into_iter().map(|h| h.join().unwrap_or(Err(("reader panicked".into(), None)))).collect()
    });
    let mut out = json!({"truncated": []});
    for ((key, _, _), r) in queries.iter().zip(reads) {
        let (rows, capped) = r?;
        let base_key = key.trim_end_matches(".24h");
        let capped = if reused(key) { out["truncated"].as_array().unwrap().iter().any(|t| t == base_key) } else { capped };
        out[*key] = if reused(key) { out[base_key].clone() } else { Value::Array(rows) };
        if capped {
            out["truncated"].as_array_mut().unwrap().push(json!(key));
        }
    }
    let incomplete = !out["truncated"].as_array().unwrap().is_empty();
    Ok((out, incomplete))
}

/// Field groups per read; without `io` Langfuse cuts string metadata such as `agentq.resource.disks` at 200 characters.
fn fields(key: &str) -> &'static str {
    match key {
        "activity" => "core,basic,trace_context,metadata,model,usage",
        // the heartbeat's `agentq.routing` and `agentq.hooks` JSON strings exceed that cut (#131)
        "agentq.resource" | "agentq.installation" => "core,basic,trace_context,metadata,io",
        _ => "core,basic,trace_context,metadata",
    }
}

fn observations(base: &str, auth: &str, now: u64, (k, v): (&str, &str), secs: u64, fields: &str) -> Result<(Vec<Value>, bool), Fail> {
    let (mut rows, mut cursor, mut seen) = (Vec::new(), None::<String>, std::collections::HashSet::new());
    loop {
        let mut url = format!(
            "{}/api/public/v2/observations?{k}={}&fields={}&fromStartTime={}&toStartTime={}&limit={}",
            base.trim_end_matches('/'), enc(v), enc(fields), enc(&iso(now - secs)), enc(&iso(now)), 1000.min(CAP - rows.len())
        );
        if let Some(c) = &cursor {
            url += &format!("&cursor={}", enc(c));
        }
        let r = get(&url, &[format!("Authorization: {auth}")])?;
        if r.status != 200 {
            return Err((format!("HTTP {}", r.status), None));
        }
        let data = r.body["data"].as_array().ok_or(("invalid response".to_string(), None))?;
        rows.extend(data.iter().take(CAP - rows.len()).cloned());
        cursor = r.body["meta"]["cursor"].as_str().or(r.body["meta"]["nextCursor"].as_str()).filter(|c| !c.is_empty()).map(String::from);
        match &cursor {
            Some(c) if !seen.insert(c.clone()) => return Err(("repeated cursor".into(), None)),
            Some(_) if rows.len() < CAP => {}
            _ => return Ok((rows, cursor.is_some())),
        }
    }
}

// ---- GitHub (D4.2)

fn github(path: &str, token: &str) -> Result<Resp, Fail> {
    let r = get(&format!("https://api.github.com{path}"), &[format!("Authorization: Bearer {token}"), "Accept: application/vnd.github+json".into()])?;
    match r.status {
        200 | 404 => Ok(r),
        403 | 429 => Err(("rate limited".into(), r.reset)),
        s => Err((format!("HTTP {s}"), None)),
    }
}

/// Open `taskq` issues and `taskq-host` heartbeats of one repo; `date` is the response `Date`.
pub fn board(repo: &str, token: &str) -> Result<(Value, bool), Fail> {
    let list = |label: &str| -> Result<(Vec<Value>, Option<u64>), Fail> {
        let r = github(&format!("/repos/{repo}/issues?labels={label}&state=open&per_page=100"), token)?;
        let items = r.body.as_array().filter(|_| r.status == 200).ok_or((format!("HTTP {}", r.status), None))?;
        Ok((items.iter().filter(|i| i.get("pull_request").is_none()).cloned().collect(), r.date))
    };
    let (issues, _) = list("taskq")?;
    let (hosts, date) = list("taskq-host")?;
    let incomplete = issues.len() >= 100;
    let issues: Vec<Value> = issues
        .iter()
        .map(|i| json!({"number": i["number"], "title": i["title"], "url": i["html_url"], "labels": i["labels"].as_array().map(|l| l.iter().map(|x| x["name"].clone()).collect::<Vec<_>>())}))
        .collect();
    let hosts: Vec<Value> = hosts.iter().map(|i| json!({"title": i["title"], "body": i["body"], "updated_at": i["updated_at"]})).collect();
    Ok((json!({"date": date, "issues": issues, "hosts": hosts}), incomplete))
}

/// Latest release version without `v`; `null` when the repo has none.
pub fn release(repo: &str, token: &str) -> Result<(Value, bool), Fail> {
    let r = github(&format!("/repos/{repo}/releases/latest"), token)?;
    Ok((r.body["tag_name"].as_str().filter(|_| r.status == 200).map(|t| json!(t.trim_start_matches('v'))).unwrap_or(Value::Null), false))
}

// ---- taskq snapshot (adapters.md S3 R1)

/// `taskq status --json` of one board through the pinned binary: no project checkout, only a fresh scratch folder
/// with `taskq.json` and a scratch install whose `token` is the read-only token (taskq reads its token only from
/// `$TASKQ_INSTALL/token`), 10 s timeout. The folder is removed after the run. An incomplete snapshot is a failed read.
pub fn taskq_status(bin: &str, repo: &str, token: &str) -> Result<(Value, bool), Fail> {
    let (status, text) = taskq(bin, repo, token, &["status", "--json"], 10)?;
    if !status.success() {
        return Err((format!("taskq exit {}", status.code().unwrap_or(-1)), None));
    }
    let log = |e: &str| eprintln!("{} failed: {e}", taskq_cmd(bin, &["status", "--json"]));
    let v: Value = serde_json::from_str(&text).map_err(|_| {
        log("output invalid");
        (String::from("taskq output invalid"), None)
    })?;
    if v["schema"] != 1 || !v["tasks"].is_array() {
        log("snapshot schema unsupported");
        return Err(("taskq snapshot schema unsupported".into(), None));
    }
    if v["complete"] != true {
        // taskq's reason is a short board-read outcome; cap it so no stray output reaches the page
        let r: String = v["reason"].as_str().unwrap_or("no reason").chars().take(160).collect();
        return Err((format!("taskq incomplete: {r}"), None));
    }
    Ok((v, false))
}

/// One pinned taskq run for `repo` with the argument array `args` (no shell) in a fresh scratch folder holding
/// `taskq.json` and a scratch install whose `token` is `token`: exit status and stdout. `Err` only when the run
/// did not finish: never started (nothing written) or killed after `secs` (`taskq timeout`, outcome unknown).
pub fn taskq(bin: &str, repo: &str, token: &str, args: &[&str], secs: u64) -> Result<(std::process::ExitStatus, String), Fail> {
    static RUN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = RUN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("dashq-taskq-{}-{n}", std::process::id()));
    // create, not create_all: a folder planted by another user fails the read instead of receiving the token
    let mut b = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
    b.create(&dir).map_err(|_| (String::from("scratch folder unwritable"), None))?;
    let r = taskq_run(bin, repo, token, args, secs, &dir);
    let _ = std::fs::remove_dir_all(&dir);
    r
}

/// A taskq command for the service log: answer text is user content, so `--text` shows no value.
pub fn taskq_cmd(bin: &str, args: &[&str]) -> String {
    let shown: Vec<&str> = args.iter().enumerate().map(|(i, a)| if i > 0 && args[i - 1] == "--text" { "<text>" } else { a }).collect();
    format!("{bin} {}", shown.join(" "))
}

fn taskq_run(bin: &str, repo: &str, token: &str, args: &[&str], secs: u64, dir: &std::path::Path) -> Result<(std::process::ExitStatus, String), Fail> {
    let fail = |e: &str| (e.to_string(), None);
    let install = dir.join("install");
    std::fs::create_dir(&install).map_err(|_| fail("scratch folder unwritable"))?;
    std::fs::write(install.join("token"), token).map_err(|_| fail("scratch folder unwritable"))?;
    std::fs::write(dir.join("taskq.json"), json!({"board": "github", "repo": repo}).to_string()).map_err(|_| fail("scratch folder unwritable"))?;
    let log = |e: &str| eprintln!("{} failed: {e}", taskq_cmd(bin, args));
    let mut child = coreq::quiet(Command::new(bin).args(args))
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("TASKQ_INSTALL", &install)
        .env("TASKQ_INCIDENT_REPORTS", "off")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            log(&e.to_string());
            fail("taskq binary not found")
        })?;
    // read in threads: a full pipe must not stall the timeout loop
    let read = |mut p: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = p.read_to_string(&mut s);
            s
        })
    };
    let (reader, errors) = (read(Box::new(child.stdout.take().unwrap())), read(Box::new(child.stderr.take().unwrap())));
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| {
            log(&e.to_string());
            fail("taskq wait failed")
        })? {
            break s;
        }
        if start.elapsed() > std::time::Duration::from_secs(secs) {
            let _ = child.kill();
            let _ = child.wait();
            log(&format!("timeout after {secs} s"));
            return Err(fail("taskq timeout"));
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    if !status.success() {
        log(&format!("{status}: {}", crate::doctor::first_line(&errors.join().unwrap_or_default())));
    }
    Ok((status, reader.join().unwrap_or_default()))
}

// ---- serverq (D4.3)

/// Read on each refresh; no cache beyond the file.
pub fn serverq(path: &str) -> Value {
    let fail = |e: &str| json!({"value": null, "at": null, "error": e, "incomplete": false});
    let Ok(text) = std::fs::read_to_string(path) else { return fail("report missing") };
    let Ok(v) = serde_json::from_str::<Value>(&text) else { return fail("invalid JSON") };
    if v["schema"] != 1 || !v["at"].is_number() || !v["results"].is_object() {
        return fail("unsupported schema");
    }
    json!({"value": v, "at": v["at"].as_f64().map(|a| a as u64), "error": null, "incomplete": false})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_and_responses_parse() {
        assert_eq!(iso_secs("2026-10-13T07:05:39.120Z"), Some(1_791_875_139));
        assert_eq!(iso(1_791_875_139), "2026-10-13T07:05:39Z");
        assert_eq!(http_date("Tue, 13 Oct 2026 07:05:39 GMT"), Some(1_791_875_139));
        assert_eq!(clock(1_791_875_139, 1_791_875_139), "2026-10-13 07:05 UTC");
        assert_eq!(clock(iso_secs("2025-12-31T23:59:59Z").unwrap(), 1_791_875_139), "2025-12-31 23:59 UTC");
        assert_eq!(clock(iso_secs("2026-01-01T00:00:00Z").unwrap(), 1_791_875_139), "2026-01-01 00:00 UTC");
        let r = parse_response("HTTP/1.1 100 Continue\r\n\r\nHTTP/2 200\r\ndate: Tue, 13 Oct 2026 07:05:39 GMT\r\n\r\n{\"a\":1}").unwrap();
        assert_eq!((r.status, r.date, r.body["a"].as_i64()), (200, Some(1_791_875_139), Some(1)));
        assert_eq!(dec("fra%2Fnative").as_deref(), Some("fra/native"));
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
        assert_eq!(iso_secs("2026-1€-13T07:05:39Z"), None, "non-ASCII never panics");
    }

    #[test]
    fn logged_taskq_command_hides_answer_text() {
        assert_eq!(taskq_cmd("tq", &["answer", "5", "--text", "private words", "--json"]), "tq answer 5 --text <text> --json");
    }

    #[test]
    fn failed_read_keeps_the_last_value() {
        let c = Cache::default();
        let a = c.get("k", 0, || Ok((json!(1), false)));
        assert_eq!((a["value"].as_i64(), a["error"].is_null()), (Some(1), true));
        let b = c.get("k", 0, || Err(("timeout".into(), None)));
        assert_eq!((b["value"].as_i64(), b["error"].as_str()), (Some(1), Some("timeout")));
        let fresh = c.get("k2", 60, || Ok((json!(2), false)));
        assert_eq!(c.get("k2", 60, || panic!("cached")), fresh);
    }

    /// #260 A2 / D4,D6: a blocked refresh must share one read without holding callers or losing the last-good time.
    /// #260 A3. Defect: a source read at cache expiry held the page behind it, past 5 s through the public name.
    /// Protection: last value keeps its time and no failure reason while one read runs, and its result replaces it.
    #[test]
    fn revalidate_keeps_the_last_value_unmarked_during_one_read() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let c = Cache::default();
        let original = c.get("board:test", 0, || Ok((json!({"n": 1}), false)));
        let (started, running) = channel();
        let (release, wait) = channel::<()>();
        let (done, finished) = channel();
        let response = c.revalidate("board:test", 0, move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
            done.send(()).unwrap();
            Ok((json!({"n": 2}), false))
        });
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(response, original, "no reason is added to a value whose age speaks for it");
        assert_eq!(c.revalidate("board:test", 0, || panic!("duplicate read")), original);
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        let replaced = (0..500).find_map(|_| {
            let snap = c.revalidate("board:test", 60, || panic!("duplicate read"));
            if snap["value"]["n"] == 2 { Some(snap) } else { std::thread::sleep(Duration::from_millis(10)); None }
        });
        assert_eq!(replaced.expect("the finished read replaced the value")["error"], Value::Null);
    }

    #[test]
    fn fleet_refresh_does_not_hold_callers() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let c = Arc::new(Cache::default());
        let original = c.get("fleet:test", 0, || Ok((json!({"records": [1]}), true)));
        let (started, running) = channel();
        let (release, wait) = channel();
        let (returned, result) = channel();
        let worker = c.clone();
        let request = std::thread::spawn(move || {
            returned.send(worker.refresh("fleet:test", 30, move || {
                started.send(()).unwrap();
                wait.recv().unwrap();
                Err(("store unavailable".into(), None))
            })).unwrap();
        });
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let response = result.recv_timeout(Duration::from_secs(1));
        let (returned, result) = channel();
        let caller = c.clone();
        let second = std::thread::spawn(move || {
            returned.send(caller.refresh("fleet:test", 30, || panic!("duplicate read"))).unwrap();
        });
        let shared = result.recv_timeout(Duration::from_secs(1));
        release.send(()).unwrap();
        request.join().unwrap();
        second.join().unwrap();
        let response = response.expect("Fleet held the caller behind its read");
        assert_eq!(response["value"], original["value"]);
        assert_eq!(response["at"], original["at"]);
        assert_eq!(response["incomplete"], true);
        assert_eq!(response["error"], "refresh in progress");
        assert_eq!(shared.expect("Fleet held the second caller"), response);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let failed = c.refresh("fleet:test", 30, || panic!("retry before backoff"));
            if failed["error"] == "store unavailable" {
                assert_eq!(failed["value"], original["value"]);
                assert_eq!(failed["at"], original["at"]);
                break;
            }
            assert!(std::time::Instant::now() < deadline, "refresh never completed");
            std::thread::yield_now();
        }
        let (release, wait) = channel();
        let cold = c.refresh("fleet:cold", 30, move || {
            wait.recv().unwrap();
            Ok((json!(2), false))
        });
        assert!(cold["value"].is_null() && cold["at"].is_null());
        assert_eq!(cold["error"], "refresh in progress");
        release.send(()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let fresh = c.refresh("fleet:cold", 30, || panic!("duplicate cold read"));
            if fresh["error"].is_null() {
                assert_eq!(fresh["value"], 2);
                assert!(fresh["at"].is_u64());
                break;
            }
            assert!(std::time::Instant::now() < deadline, "cold refresh never completed");
            std::thread::yield_now();
        }
    }

    #[cfg(unix)]
    #[test]
    fn taskq_status_runs_without_checkout_and_fails_with_a_reason() {
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("dashq-tq-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let fake = |name: &str, script: &str| {
            let p = d.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{script}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p.to_string_lossy().into_owned()
        };
        // the fake checks its arguments, the scratch config, the token where real taskq reads it and no HOME
        let ok = fake("ok", &format!(r#"pwd > {}/cwd; [ "$1 $2" = "status --json" ] && [ "$(cat "$TASKQ_INSTALL/token")" = tok ] && [ -z "$HOME" ] && grep -q '"repo":"o/r"' taskq.json && echo '{{"schema":1,"complete":true,"tasks":[]}}'"#, d.display()));
        assert_eq!(taskq_status(&ok, "o/r", "tok").unwrap().0["schema"], 1);
        let cwd = std::fs::read_to_string(d.join("cwd")).unwrap();
        assert!(!std::path::Path::new(cwd.trim()).exists(), "the scratch folder with the token is removed");
        let cases = [
            ("exit", "exit 3", "taskq exit 3"),
            ("bad", "echo nope", "taskq output invalid"),
            ("schema", r#"echo '{"schema":2,"tasks":[]}'"#, "taskq snapshot schema unsupported"),
            ("part", r#"echo '{"schema":1,"complete":false,"reason":"HTTP 401","tasks":[]}'"#, "taskq incomplete: HTTP 401"),
            ("slow", "sleep 30", "taskq timeout"),
        ];
        for (name, script, reason) in cases {
            assert_eq!(taskq_status(&fake(name, script), "o/r", "tok").unwrap_err().0, reason);
        }
        assert_eq!(taskq_status("/nonexistent/taskq", "o/r", "tok").unwrap_err().0, "taskq binary not found");
    }
}

/// D26 explicit upstream CLI release sources; no repository discovery or board authority.
pub fn cli_release(repo: &str, token: Option<&str>) -> Result<(Value, bool), Fail> {
    let mut headers = vec!["Accept: application/vnd.github+json".into()];
    if let Some(t) = token { headers.push(format!("Authorization: Bearer {t}")); }
    let r = get(&format!("https://api.github.com/repos/{repo}/releases/latest"), &headers)?;
    if r.status != 200 { return Err((format!("HTTP {}", r.status), r.reset)); }
    let tag = r.body["tag_name"].as_str().ok_or(("release version missing".into(), None))?;
    let version = tag.strip_prefix("rust-v").unwrap_or(tag.trim_start_matches('v'));
    let version = crate::data::semver(&json!(version));
    if version.is_null() || r.body["prerelease"] != false || r.body["draft"] != false {
        return Err(("unsupported stable release".into(), None));
    }
    Ok((version, false))
}
