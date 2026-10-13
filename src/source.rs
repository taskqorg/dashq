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
}

#[derive(Default)]
pub struct Cache(Mutex<HashMap<String, Arc<Mutex<Slot>>>>);

impl Cache {
    pub fn get(&self, key: &str, ttl: u64, read: impl FnOnce() -> Result<(Value, bool), Fail>) -> Value {
        let slot = self.0.lock().unwrap().entry(key.to_string()).or_default().clone();
        // single flight: the slot lock is held during the read, so waiting callers share its result
        let mut s = slot.lock().unwrap();
        let now = now();
        if !s.snap.is_null() && now < s.next {
            return s.snap.clone();
        }
        match read() {
            Ok((value, incomplete)) => {
                s.snap = json!({"value": value, "at": now, "error": null, "incomplete": incomplete});
                s.next = now + ttl;
            }
            Err((reason, retry)) => {
                if s.snap.is_null() {
                    s.snap = json!({"value": null, "at": null, "error": null, "incomplete": false});
                }
                s.snap["error"] = json!(reason);
                s.next = retry.filter(|r| *r > now).unwrap_or(now + ttl);
            }
        }
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
    let mut cfg = format!("url = \"{}\"\nuser-agent = \"dashq\"\n", url.replace('\\', "\\\\").replace('"', "\\\""));
    for h in headers {
        cfg += &format!("header = \"{}\"\n", h.replace('\\', "\\\\").replace('"', "\\\""));
    }
    let mut child = coreq::quiet(Command::new("curl").args(["-sS", "-i", "--max-time", "10", "--proto", "=https", "-K", "-"]))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| ("curl not found".to_string(), None))?;
    child.stdin.take().unwrap().write_all(cfg.as_bytes()).map_err(|_| ("curl input failed".to_string(), None))?;
    let out = child.wait_with_output().map_err(|_| ("curl failed".to_string(), None))?;
    match out.status.code() {
        Some(0) => {}
        Some(28) => return Err(("timeout".into(), None)),
        c => return Err((format!("curl exit {}", c.unwrap_or(-1)), None)),
    }
    parse_response(&String::from_utf8_lossy(&out.stdout)).ok_or(("invalid response".into(), None))
}

fn parse_response(raw: &str) -> Option<Resp> {
    let mut rest = raw;
    loop {
        let (head, body) = rest.split_once("\r\n\r\n")?;
        let status: u16 = head.split_whitespace().nth(1)?.parse().ok()?;
        // 1xx and a proxy's CONNECT reply precede the real response
        if (100..200).contains(&status) || (status == 200 && body.starts_with("HTTP/")) {
            rest = body;
            continue;
        }
        let header = |name: &str| head.lines().find_map(|l| l.split_once(':').filter(|(k, _)| k.trim().eq_ignore_ascii_case(name)).map(|(_, v)| v.trim().to_string()));
        let body = if body.trim().is_empty() { Value::Null } else { serde_json::from_str(body).ok()? };
        return Some(Resp { status, date: header("date").and_then(|d| http_date(&d)), reset: header("x-ratelimit-reset").and_then(|r| r.parse().ok()), body });
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
    if s.len() < 19 || &s[4..5] != "-" || &s[10..11] != "T" {
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

/// `14:02 UTC`; a time on another day also names the date.
pub fn clock(t: u64, now: u64) -> String {
    let (_, m, d, s) = civil(t);
    let hm = format!("{:02}:{:02} UTC", s / 3600, s / 60 % 60);
    if t / 86400 == now / 86400 { hm } else { format!("{m:02}-{d:02} {hm}") }
}

// ---- Langfuse (D4.1)

/// Event reads of one period: `{health, compression, native_tools, activity, incidents, truncated}`.
pub fn langfuse(base: &str, auth: &str, now: u64, period: u64) -> Result<(Value, bool), Fail> {
    let filter = r#"[{"type":"string","column":"name","operator":"starts with","value":"service.health."}]"#;
    let queries: [(&str, (&str, &str), u64); 5] = [
        ("health", ("filter", filter), 86400),
        ("compression", ("name", "agent-context.compression"), period),
        ("native_tools", ("name", "agent-context.native-tool"), period),
        ("activity", ("type", "GENERATION"), period),
        ("incidents", ("name", "service.incident"), 86400),
    ];
    let reads: Vec<Result<(Vec<Value>, bool), Fail>> = std::thread::scope(|sc| {
        let hs: Vec<_> = queries.iter().map(|(key, q, secs)| sc.spawn(move || observations(base, auth, now, *q, *secs, *key == "activity"))).collect();
        hs.into_iter().map(|h| h.join().unwrap_or(Err(("reader panicked".into(), None)))).collect()
    });
    let mut out = json!({"truncated": []});
    for ((key, _, _), r) in queries.iter().zip(reads) {
        let (rows, capped) = r?;
        out[*key] = Value::Array(rows);
        if capped {
            out["truncated"].as_array_mut().unwrap().push(json!(key));
        }
    }
    let incomplete = !out["truncated"].as_array().unwrap().is_empty();
    Ok((out, incomplete))
}

fn observations(base: &str, auth: &str, now: u64, (k, v): (&str, &str), secs: u64, generation: bool) -> Result<(Vec<Value>, bool), Fail> {
    let fields = if generation { "core,basic,trace_context,metadata,model,usage" } else { "core,basic,trace_context,metadata" };
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
        assert_eq!(clock(1_791_875_139, 1_791_875_139), "07:05 UTC");
        let r = parse_response("HTTP/1.1 100 Continue\r\n\r\nHTTP/2 200\r\ndate: Tue, 13 Oct 2026 07:05:39 GMT\r\n\r\n{\"a\":1}").unwrap();
        assert_eq!((r.status, r.date, r.body["a"].as_i64()), (200, Some(1_791_875_139), Some(1)));
        assert_eq!(dec("fra%2Fnative").as_deref(), Some("fra/native"));
        assert_eq!(enc("a b/c"), "a%20b%2Fc");
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
}
