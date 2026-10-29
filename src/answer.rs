//! Guarded owner answers and separately granted task controls (dashq.md D13, docs/design/answers.md). `GET /answer?board=` gives a signed-in answerer
//! its CSRF token; `POST /answer` writes one ordinary TaskQ answer through the pinned dispatcher with the expected
//! revision and an operation ID. `/task/control` uses the same journal and dispatcher for the published fixed
//! control table, with a separate controller grant, config, CSRF purpose and operation namespace.

use crate::enroll::{self, Enroll, Req, Resp};
use crate::store::AnswerOp;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

/// answers.md: request body bound and answers per actor a minute.
pub const MAX_BODY: usize = 4096;
const PER_MINUTE: usize = 10;
/// taskq § 5 `answer --text`: at most 4 lines of at most 240 characters.
const TEXT_LINES: usize = 4;
const LINE_CHARS: usize = 240;
/// One canonical write with its readback; a run past it has an unknown outcome.
const WRITE_SECS: u64 = 60;

pub struct Config {
    /// Dedicated answer credential file (0600), read on every write; never the read token.
    pub token: PathBuf,
    /// Answerable board → the workspace that owns it.
    pub boards: HashMap<String, String>,
}

/// Per-process guards: rate windows per actor and operation IDs being dispatched now.
#[derive(Default)]
pub struct Answers {
    recent: Mutex<HashMap<String, Vec<i64>>>,
    inflight: Mutex<HashSet<String>>,
}

pub struct Ctx<'a> {
    pub enroll: &'a Result<Enroll, String>,
    pub cfg: Option<&'a Config>,
    /// Pinned `taskq` binary.
    pub bin: Option<&'a str>,
    pub public_host: &'a str,
    pub state: &'a Answers,
}

/// TaskQ runs `close` for an option that starts with `close` (taskq § 2): such choices stay on the board.
pub fn closes(text: &str) -> bool {
    text.trim_start().get(..5).is_some_and(|x| x.eq_ignore_ascii_case("close"))
}

/// The reason a choice cannot be answered here, for `/data`; `null` when it can.
pub fn unavailable(text: &Value) -> Value {
    if text.as_str().is_some_and(closes) { json!("Close and publication are answered on the board") } else { Value::Null }
}

fn reply(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

/// A refusal before any write, in the receipt shape.
fn refuse(status: &'static str, reason: &str, next: Option<&str>) -> Resp {
    reply(status, json!({"task": null, "op_id": null, "outcome": "rejected", "state": null, "revision": null, "reason": reason, "next": next}))
}

fn status_of(outcome: &str) -> &'static str {
    match outcome {
        "accepted" => "200 OK",
        "stale" => "409 Conflict",
        "rejected" => "422 Unprocessable Content",
        _ => "202 Accepted",
    }
}

pub fn route(c: &Ctx, r: &Req, now: i64) -> Option<Resp> {
    if !["/answer", "/task/control"].contains(&r.path) {
        return None;
    }
    Some(match r.method {
        "GET" => {
            let q = enroll::form(r.query.as_bytes());
            let board = q.get("board").map(String::as_str).unwrap_or("");
            match guard(c, r, board) {
                Ok(g) => reply("200 OK", json!({"csrf": g.e.token_for(&purpose_for(g.subject, board, now / 86400, g.controls))})),
                Err(x) => x,
            }
        }
        "POST" => post(c, r, now),
        _ => Resp { status: "405 Method Not Allowed", ctype: "text/plain", body: b"method not allowed".to_vec(), headers: String::new(), forms: false },
    })
}

/// CSRF purpose: bound to the verified account, this route, the board and the day.
#[cfg(test)]
fn purpose(subject: &str, board: &str, day: i64) -> String {
    purpose_for(subject, board, day, false)
}

fn purpose_for(subject: &str, board: &str, day: i64, controls: bool) -> String {
    let route = if controls { "task/control" } else { "answer" };
    format!("{subject}\n{route}\n{board}\n{day}")
}

#[derive(Clone, Copy)]
enum Write<'a> { Answer(Result<u32, &'a str>), Control(&'a str) }

/// The published TaskQ table is the only control-to-command mapping.
fn control(action: &str) -> Option<Value> {
    let f: Value = serde_json::from_str(include_str!("../assets/taskq/task-controls.json")).ok()?;
    f["controls"].as_array()?.iter().find(|c| c["action"] == action).cloned()
}

pub fn control_fixture() -> Result<(), String> {
    let f: Value = serde_json::from_str(include_str!("../assets/taskq/task-controls.json")).map_err(|_| "task controls: invalid fixture")?;
    if f["version"] != "taskq.task-controls/1" || f["close"] != "board" { return Err("task controls: unsupported fixture".into()); }
    for case in f["cases"].as_array().ok_or("task controls: missing cases")? {
        let action = case["op"]["action"].as_str().ok_or("task controls: missing action")?;
        let c = control(action).ok_or("task controls: unknown action")?;
        let args = control_args(&c, &case["op"]);
        if json!(args) != case["argv"] { return Err("task controls: argv differs from release fixture".into()); }
    }
    Ok(())
}

fn control_args(c: &Value, op: &Value) -> Vec<String> {
    let mut args = vec![c["command"].as_str().unwrap().into(), op["task"].to_string()];
    if let Some(text) = c["text"].as_str().filter(|s| !s.is_empty()) { args.extend(["--text".into(), text.into()]); }
    args.extend(["--json".into(), "--op-id".into(), op["id"].as_str().unwrap().into(), "--revision".into(), op["revision"].to_string()]);
    args
}

struct Guarded<'a> {
    e: &'a Enroll,
    cfg: &'a Config,
    bin: &'a str,
    subject: &'a str,
    workspace: &'a str,
    controls: bool,
}

/// Every request: answers enabled, a verified subject, a configured board, and the subject administers the board's
/// workspace with the explicit answer capability. Read from the store each time, so a revoke applies at once.
fn guard<'a>(c: &'a Ctx, r: &'a Req, board: &str) -> Result<Guarded<'a>, Resp> {
    let controls = r.path == "/task/control";
    let (Ok(e), Some(cfg), Some(bin)) = (c.enroll, c.cfg, c.bin) else {
        return Err(refuse("503 Service Unavailable", if controls { "Task controls are not enabled on this dashboard" } else { "Answers are not enabled on this dashboard" }, None));
    };
    let Some(subject) = r.header(enroll::SUBJECT).filter(|s| enroll::valid_subject(s)) else {
        return Err(refuse("401 Unauthorized", "Sign-in required: open the dashboard address", None));
    };
    let Some(workspace) = cfg.boards.get(board) else {
        return Err(refuse("404 Not Found", "This board is not answerable here", None));
    };
    if controls && credential(&cfg.token).is_err() {
        return Err(refuse("503 Service Unavailable", "The task-control credential is unavailable; nothing was written", None));
    }
    match if controls { e.store().can_control(workspace, subject) } else { e.store().can_answer(workspace, subject) } {
        Ok(true) => Ok(Guarded { e, cfg, bin, subject, workspace, controls }),
        Ok(false) => Err(refuse("403 Forbidden", if controls { "This account may not control tasks on this board" } else { "This account may not answer on this board" }, None)),
        Err(_) => Err(refuse("503 Service Unavailable", "The store is unavailable; nothing was written", None)),
    }
}

/// Form fields; `None` when malformed or a field repeats.
fn fields(body: &[u8]) -> Option<HashMap<String, String>> {
    let mut f = HashMap::new();
    for p in std::str::from_utf8(body).ok()?.split('&').filter(|p| !p.is_empty()) {
        let (k, v) = p.split_once('=')?;
        if f.insert(crate::source::dec(k)?, crate::source::dec(v)?).is_some() {
            return None;
        }
    }
    Some(f)
}

/// taskq § 5 limits; no control characters but newlines; no leading `-`, so the value is never read as a flag.
fn valid_text(t: &str) -> bool {
    let lines: Vec<&str> = t.lines().collect();
    !t.trim().is_empty() && !t.starts_with('-') && lines.len() <= TEXT_LINES && lines.iter().all(|l| l.chars().count() <= LINE_CHARS && !l.chars().any(char::is_control))
}

fn post(c: &Ctx, r: &Req, now: i64) -> Resp {
    if r.body.len() > MAX_BODY {
        return refuse("413 Content Too Large", "Request too large", None);
    }
    let origin = format!("https://{}", c.public_host);
    if r.header("origin") != Some(origin.as_str()) || !r.header("content-type").is_some_and(|t| t.starts_with("application/x-www-form-urlencoded")) {
        return refuse("403 Forbidden", "Request refused: it did not come from this dashboard's own page", None);
    }
    let Some(f) = fields(r.body) else {
        return refuse("400 Bad Request", "Malformed request", None);
    };
    let get = |k: &str| f.get(k).map(String::as_str);
    let board = get("board").unwrap_or("");
    let g = match guard(c, r, board) {
        Ok(g) => g,
        Err(x) => return x,
    };
    let csrf = get("csrf").unwrap_or("");
    if ![now / 86400, now / 86400 - 1].iter().any(|d| g.e.token_ok(&purpose_for(g.subject, board, *d, g.controls), csrf)) {
        return refuse("403 Forbidden", "Request refused: the form expired or belongs to another account; reload the page", None);
    }
    let task = get("task").and_then(|t| t.parse::<i64>().ok()).filter(|t| *t > 0);
    let revision = get("revision").and_then(|t| t.parse::<i64>().ok()).filter(|t| *t >= 0);
    let op = get("op_id").filter(|o| enroll::id(o, 64) && !o.starts_with('-'));
    let choice = get("choice").map(|k| k.parse::<u32>().ok().filter(|k| (1..=99).contains(k)));
    let text = get("text");
    let (Some(task), Some(revision), Some(op)) = (task, revision, op) else {
        return refuse("400 Bad Request", "Malformed request: board, task, revision and op_id are required", None);
    };
    let answer = if g.controls {
        if f.keys().any(|k| !["board", "csrf", "task", "revision", "op_id", "action"].contains(&k.as_str())) {
            return refuse("400 Bad Request", "Controls carry no answer or free text", None);
        }
        let Some(action) = get("action").filter(|a| control(a).is_some()) else {
            return refuse("400 Bad Request", "Unknown task control; Close stays on the board", None);
        };
        Write::Control(action)
    } else { Write::Answer(match (choice, text) {
        (Some(Some(k)), None) => Ok(k),
        (None, Some(t)) if valid_text(t) => Err(t),
        (None, Some(_)) => {
            return refuse("400 Bad Request", "Answer text: 1 to 4 lines of at most 240 characters, not starting with -", None);
        }
        _ => {
            return refuse("400 Bad Request", "Malformed request: send exactly one of choice or text", None);
        }
    }) };
    if let Err(wait) = admit(c.state, g.subject, now) {
        let mut x = refuse("429 Too Many Requests", "Too many operations; wait a minute", None);
        x.headers = format!("Retry-After: {wait}\r\n");
        return x;
    }
    let payload = match answer {
        Write::Control(a) => enroll::sha256(&format!("control\n{a}")),
        Write::Answer(Ok(k)) => enroll::sha256(&format!("choice\n{k}")),
        Write::Answer(Err(t)) => enroll::sha256(&format!("text\n{t}")),
    };
    let op = AnswerOp { op_id: if g.controls { format!("dashq.control.{op}") } else { format!("dashq.{op}") }, subject: g.subject.into(), workspace: g.workspace.into(), board: board.into(), task, revision, payload, state: "pending".into(), receipt: None };
    // a double click or a retry while the first run is still writing never starts a second run
    if !c.state.inflight.lock().unwrap_or_else(|p| p.into_inner()).insert(op.op_id.clone()) {
        return reply("202 Accepted", receipt_of(&op, "unknown", "This operation is still being written", "Retry with the same operation ID"));
    }
    let r = answer_once(&g, &op, answer, now);
    c.state.inflight.lock().unwrap_or_else(|p| p.into_inner()).remove(&op.op_id);
    r
}

/// True when another answer is allowed for `subject` now; else the seconds to wait.
// ponytail: in-memory window, reset by a restart
pub(crate) fn admit(a: &Answers, subject: &str, now: i64) -> Result<(), i64> {
    let mut m = a.recent.lock().unwrap_or_else(|p| p.into_inner());
    let v = m.entry(subject.into()).or_default();
    v.retain(|t| now - t < 60);
    if v.len() >= PER_MINUTE {
        return Err(60 - (now - v[0]));
    }
    v.push(now);
    Ok(())
}

fn receipt_of(op: &AnswerOp, outcome: &str, reason: &str, next: &str) -> Value {
    json!({"task": op.task, "op_id": op.op_id, "outcome": outcome, "state": null, "revision": op.revision, "reason": reason, "next": next})
}

fn answer_once(g: &Guarded, op: &AnswerOp, answer: Write<'_>, now: i64) -> Resp {
    let store = g.e.store();
    let Ok(token) = credential(&g.cfg.token) else {
        return refuse("503 Service Unavailable", "The answer credential is unavailable; nothing was written", None);
    };
    // A failed precheck must never leave a pending control that a retry could dispatch unchecked.
    if g.controls {
        match store.has_answer(&op.op_id) {
            Ok(false) => if let Err(v) = precheck(g, op, answer, &token) {
                return reply(status_of(v["outcome"].as_str().unwrap_or("rejected")), v);
            },
            Ok(true) => {},
            Err(_) => return refuse("503 Service Unavailable", "The store is unavailable; nothing was written", None),
        }
    }
    let seen = match store.begin_answer(op, now) {
        Ok(s) => s,
        Err(_) => {
            return refuse("503 Service Unavailable", "The store is unavailable; nothing was written", None);
        }
    };
    match seen {
        Some(s) if (&s.subject, &s.workspace, &s.board, s.task, s.revision, &s.payload) != (&op.subject, &op.workspace, &op.board, op.task, op.revision, &op.payload) => {
            return refuse("409 Conflict", if g.controls { "This operation ID belongs to another task control" } else { "This operation ID belongs to another answer" }, Some("Start a new answer"));
        }
        // the first outcome repeats; nothing is written again
        Some(s) if ["accepted", "rejected", "stale"].contains(&s.state.as_str()) => {
            let v: Value = s.receipt.as_deref().and_then(|r| serde_json::from_str(r).ok()).unwrap_or_else(|| receipt_of(op, &s.state, "", ""));
            return reply(status_of(&s.state), v);
        }
        // pending after a restart or unknown: reconcile through TaskQ by the same operation ID, payload and revision;
        // TaskQ returns the first receipt for a seen ID and refuses a moved revision, so no second entry appears
        Some(_) => {}
        None if !g.controls => {
            if let Err(v) = precheck(g, op, answer, &token) {
                let outcome = v["outcome"].as_str().unwrap_or("rejected").to_string();
                let _ = store.finish_answer(&op.op_id, &outcome, &v.to_string(), now);
                return reply(status_of(&outcome), v);
            }
        }
        None => {}
    }
    let v = dispatch(g, op, answer, &token);
    let outcome = v["outcome"].as_str().unwrap_or("unknown").to_string();
    if store.finish_answer(&op.op_id, &outcome, &v.to_string(), now).is_err() {
        // TaskQ holds the outcome; the record stays pending and a retry reconciles it
        return reply("202 Accepted", receipt_of(op, "unknown", "The receipt could not be recorded", "Retry with the same operation ID"));
    }
    reply(status_of(&outcome), v)
}

/// The dedicated credential: an owner-only file holding one token.
fn credential(p: &std::path::Path) -> Result<String, ()> {
    #[cfg(unix)]
    if std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(p).map_err(|_| ())?.permissions()) & 0o077 != 0 {
        return Err(());
    }
    let t = std::fs::read_to_string(p).map_err(|_| ())?.trim().to_string();
    (t.len() >= 20 && t.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')).then_some(t).ok_or(())
}

/// Fresh canonical read before any write: the card must still be open at the expected revision and the answer
/// must fit it. An incomplete read disables writing.
fn precheck(g: &Guarded, op: &AnswerOp, answer: Write<'_>, token: &str) -> Result<(), Value> {
    let out = |outcome: &str, reason: String, revision: Value, next: Value| {
        Err(json!({"task": op.task, "op_id": op.op_id, "outcome": outcome, "state": null, "revision": revision, "reason": reason, "next": next}))
    };
    let snap = match crate::source::taskq_status(g.bin, &op.board, token) {
        Ok((v, _)) => v,
        Err((why, _)) => {
            return out("rejected", format!("Board read failed ({why}); nothing was written"), Value::Null, json!("Try again later"));
        }
    };
    let Some(t) = snap["tasks"].as_array().into_iter().flatten().find(|t| t["n"].as_i64() == Some(op.task)) else {
        return out("stale", "This task is no longer open".into(), Value::Null, json!("Review the board"));
    };
    if let Write::Control(action) = answer {
        if t["revision"].as_i64() != Some(op.revision) {
            return out("stale", "The task changed; review its current controls".into(), t["revision"].clone(), t["url"].clone());
        }
        if !t["controls"].as_array().is_some_and(|a| a.iter().any(|v| v == action)) {
            return out("rejected", "This row does not offer that control".into(), t["revision"].clone(), t["url"].clone());
        }
        return Ok(());
    }
    let Write::Answer(answer) = answer else { unreachable!() };
    let url = t["url"].clone();
    let state = t["state"].as_str().unwrap_or("");
    if !["ask", "review"].contains(&state) {
        return out("stale", format!("This question is no longer open (state {state})"), t["revision"].clone(), url);
    }
    if t["revision"].as_i64() != Some(op.revision) {
        return out("stale", "The question changed; review the new question".into(), t["revision"].clone(), url);
    }
    let q = &t["question"];
    match answer {
        Ok(k) => {
            if q["parsed"] != true {
                return out("rejected", "This card does not parse; answer on the board".into(), t["revision"].clone(), url);
            }
            let Some(c) = q["choices"].as_array().into_iter().flatten().find(|c| c["id"].as_u64() == Some(k as u64)) else {
                return out("rejected", format!("This card has no option {k}"), t["revision"].clone(), url);
            };
            if c["text"].as_str().is_some_and(closes) {
                return out("rejected", "Close and publication are answered on the board".into(), t["revision"].clone(), url);
            }
        }
        Err(_) if state != "ask" => {
            return out("rejected", "Free text answers only an open question".into(), t["revision"].clone(), url);
        }
        Err(_) => {}
    }
    Ok(())
}

/// The pinned dispatcher with argument arrays: `taskq answer N.K|N --text T --revision R --op-id ID --json`.
fn dispatch(g: &Guarded, op: &AnswerOp, answer: Write<'_>, token: &str) -> Value {
    let args: Vec<String> = match answer {
        Write::Control(action) => control_args(&control(action).unwrap(), &json!({"task": op.task, "revision": op.revision, "id": op.op_id})),
        Write::Answer(answer) => {
            let target = match answer { Ok(k) => format!("{}.{k}", op.task), Err(_) => op.task.to_string() };
            let mut args = vec!["answer".into(), target];
            if let Err(t) = answer { args.extend(["--text".into(), t.into()]); }
            args.extend(["--revision".into(), op.revision.to_string(), "--op-id".into(), op.op_id.clone(), "--json".into()]);
            args
        }
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match crate::source::taskq(g.bin, &op.board, token, &args, WRITE_SECS) {
        Ok((_, stdout)) => match serde_json::from_str::<Value>(stdout.trim()) {
            Ok(v) => sanitize(&v, op, g.controls),
            Err(_) => {
                eprintln!("{} failed: no readable receipt", crate::source::taskq_cmd(g.bin, &args));
                receipt_of(op, "unknown", "TaskQ returned no readable receipt", "Retry with the same operation ID")
            }
        },
        Err((why, _)) if ["taskq timeout", "taskq wait failed"].contains(&why.as_str()) => receipt_of(op, "unknown", "TaskQ did not finish; the operation may be written", "Retry with the same operation ID"),
        // never started: nothing was written
        Err((why, _)) => receipt_of(op, "rejected", &format!("TaskQ could not run ({why}); nothing was written"), "Try again later"),
    }
}

/// Keeps the receipt fields only, as short single-line strings; a receipt for another operation is unknown.
fn sanitize(v: &Value, op: &AnswerOp, controls: bool) -> Value {
    let s = |k: &str| v[k].as_str().map(|x| x.chars().filter(|c| !c.is_control()).take(LINE_CHARS).collect::<String>());
    let same = |k: &str, want: &str| v[k].as_str() == Some(want) || v[k].as_i64().is_some_and(|n| n.to_string() == want);
    if !same("task", &op.task.to_string()) || !same("op_id", &op.op_id) {
        return receipt_of(op, "unknown", "TaskQ returned a receipt for another operation", "Retry with the same operation ID");
    }
    let outcome = s("outcome").filter(|o| ["accepted", "rejected", "stale", "unknown"].contains(&o.as_str())).unwrap_or_else(|| "unknown".into());
    if controls && outcome == "accepted" && (v["revision"].as_i64().or_else(|| v["revision"].as_str()?.parse::<i64>().ok()).is_none_or(|n| n < 0)
        || !v["state"].as_str().is_some_and(|s| ["held", "ready", "waiting", "doing", "review", "ask", "failed", "later", "closed"].contains(&s))) {
        return receipt_of(op, "unknown", "TaskQ returned an incomplete accepted receipt", "Retry with the same operation ID");
    }
    let revision = if v["revision"].is_i64() { v["revision"].clone() } else { json!(s("revision")) };
    json!({"task": op.task, "op_id": op.op_id, "outcome": outcome, "state": s("state"), "revision": revision, "reason": s("reason"), "next": s("next")})
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const HOST: &str = "dash.example.com";
    const NOW: i64 = 1_800_000_000;
    const TOKEN: &str = "github_pat_answer_only_0123456789";

    struct T {
        d: PathBuf,
        e: Result<Enroll, String>,
        cfg: Config,
        bin: String,
        state: Answers,
    }

    impl T {
        fn new(name: &str) -> T {
            let d = std::env::temp_dir().join(format!("dashq-answer-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(&d).unwrap();
            Enroll::admin(&d, "ws-1", "github|1").unwrap();
            Enroll::admin(&d, "ws-1", "github|3").unwrap();
            Enroll::admin(&d, "ws-2", "github|2").unwrap();
            Enroll::answerer(&d, "ws-1", "github|1", true).unwrap();
            Enroll::answerer(&d, "ws-2", "github|2", true).unwrap();
            let token = d.join("answer-token");
            std::fs::write(&token, TOKEN).unwrap();
            std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
            // the fake checks the dedicated credential where real taskq reads it, and no inherited environment
            let bin = d.join("taskq");
            let script = format!(
                "#!/bin/sh\nD={0}\n[ -z \"$HOME\" ] || exit 9\n[ \"$(cat \"$TASKQ_INSTALL/token\")\" = {TOKEN} ] || exit 8\nif [ \"$1 $2\" = \"status --json\" ]; then cat $D/snap.json; exit 0; fi\nprintf '%s\\n' \"$*\" >> $D/calls\ncat $D/receipt.json\n",
                d.display()
            );
            std::fs::write(&bin, script).unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            let t = T {
                e: Enroll::open(&d, HOST),
                cfg: Config { token, boards: HashMap::from([("o/r".to_string(), "ws-1".to_string())]) },
                bin: bin.to_string_lossy().into(),
                d,
                state: Answers::default(),
            };
            t.snap("ask", 700);
            t.receipt(json!({"task": 12, "op_id": "dashq.op-1", "outcome": "accepted", "state": "ready", "revision": 701, "reason": null, "next": null}));
            t
        }

        fn snap(&self, state: &str, revision: i64) {
            let s = json!({"schema": 1, "complete": true, "tasks": [{"n": 12, "url": "https://github.com/o/r/issues/12", "state": state, "revision": revision,
                "question": {"parsed": true, "choices": [{"id": 1, "text": "close: merge now", "recommended": false}, {"id": 3, "text": "Files", "recommended": true}]}}]});
            std::fs::write(self.d.join("snap.json"), s.to_string()).unwrap();
        }

        fn receipt(&self, v: Value) {
            std::fs::write(self.d.join("receipt.json"), v.to_string()).unwrap();
        }

        fn calls(&self) -> Vec<String> {
            std::fs::read_to_string(self.d.join("calls")).unwrap_or_default().lines().map(String::from).collect()
        }

        fn call(&self, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> (String, Value) {
            let ctx = Ctx { enroll: &self.e, cfg: Some(&self.cfg), bin: Some(&self.bin), public_host: HOST, state: &self.state };
            let h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
            let (path, query) = path.split_once('?').unwrap_or((path, ""));
            let r = route(&ctx, &Req { method, path, query, headers: &h, body: body.as_bytes() }, NOW).expect("answer route");
            (r.status.to_string(), serde_json::from_slice(&r.body).unwrap_or(Value::Null))
        }

        /// As the shared card posts it: a token from `GET /answer`, then the form.
        fn post(&self, subject: &str, form: &str) -> (String, Value) {
            let csrf = self.e.as_ref().unwrap().token_for(&purpose(subject, "o/r", NOW / 86400));
            let h = [(enroll::SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
            self.call("POST", "/answer", &h, &format!("board=o%2Fr&csrf={csrf}&{form}"))
        }
    }

    // #179 A1/A5: a missing deployment gate must return a read-only reason, never dispatch.
    #[test]
    fn task_controls_off_by_default() {
        let t = T::new("controls-off");
        let ctx = Ctx { enroll: &t.e, cfg: None, bin: Some(&t.bin), public_host: HOST, state: &t.state };
        let r = route(&ctx, &Req { method: "GET", path: "/task/control", query: "board=o%2Fr", headers: &[], body: b"" }, NOW).expect("task control route");
        assert_eq!(r.status, "503 Service Unavailable");
        assert!(t.calls().is_empty());
    }

    impl T {
        fn control(&self, subject: &str, form: &str) -> (String, Value) {
            let csrf = self.e.as_ref().unwrap().token_for(&purpose_for(subject, "o/r", NOW / 86400, true));
            let h = [(enroll::SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
            self.call("POST", "/task/control", &h, &format!("board=o%2Fr&csrf={csrf}&{form}"))
        }
        fn offers(&self, state: &str, revision: i64, offers: Value) {
            self.snap(state, revision);
            let p = self.d.join("snap.json");
            let mut v: Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
            v["tasks"][0]["controls"] = offers;
            std::fs::write(p, v.to_string()).unwrap();
        }
    }

    // #179 A3/A4/A7: release argv and receipts reach the shared dispatcher/journal unchanged.
    #[test]
    fn task_controls_fixture_dispatch_and_replay() {
        control_fixture().unwrap();
        let f: Value = serde_json::from_str(include_str!("../assets/taskq/task-controls.json")).unwrap();
        for case in f["cases"].as_array().unwrap() {
            let action = case["op"]["action"].as_str().unwrap();
            let t = T::new(&format!("controls-{action}"));
            Enroll::controller(&t.d, "ws-1", "github|1", true).unwrap();
            t.offers(case["row"]["state"].as_str().unwrap(), 20, json!([action]));
            let receipt = json!({"task":12,"op_id":"dashq.control.op-1","outcome":"accepted","state":"ready","revision":21});
            t.receipt(receipt);
            let form = format!("task=12&revision=20&op_id=op-1&action={action}");
            let (s, first) = t.control("github|1", &form);
            assert_eq!((s.as_str(), first["outcome"].as_str()), ("200 OK", Some("accepted")));
            let mut argv: Vec<String> = case["argv"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
            argv[1] = "12".into();
            let at = argv.iter().position(|v| v == "--op-id").unwrap(); argv[at+1] = "dashq.control.op-1".into();
            assert_eq!(t.calls(), [argv.join(" ")]);
            t.offers("ready", 21, json!(["stop"]));
            assert_eq!(t.control("github|1", &form).1, first, "a moved revision repeats the first receipt");
            assert_eq!(t.calls().len(), 1, "retry never dispatches twice after accepted readback");
            assert_eq!(t.control("github|1", &form.replace(&format!("action={action}"), "action=stop")).0,
                if action == "stop" { "200 OK" } else { "409 Conflict" });
        }
    }

    // #179 A1/A3/A5: answer grants, CSRF, unoffered controls and incomplete reads must never write.
    #[test]
    fn task_controls_authority_and_row_gates() {
        let t = T::new("controls-guards");
        let form = "task=12&revision=700&op_id=op-1&action=stop";
        t.offers("doing", 700, json!(["stop", "later"]));
        assert_eq!(t.control("github|1", form).0, "403 Forbidden", "answer grant is not control authority");
        Enroll::controller(&t.d, "ws-2", "github|2", true).unwrap();
        assert_eq!(t.control("github|2", form).0, "403 Forbidden", "grant in another workspace");
        Enroll::controller(&t.d, "ws-1", "github|1", true).unwrap();
        let answer_csrf = t.e.as_ref().unwrap().token_for(&purpose("github|1", "o/r", NOW / 86400));
        let headers = [(enroll::SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        assert_eq!(t.call("POST", "/task/control", &headers, &format!("board=o%2Fr&csrf={answer_csrf}&{form}")).0, "403 Forbidden", "answer CSRF cannot authorize controls");
        assert_eq!(t.call("GET", "/task/control?board=o%2Fr", &[], "").0, "401 Unauthorized");
        let csrf = t.e.as_ref().unwrap().token_for(&purpose_for("github|1", "o/r", NOW / 86400, true));
        let body = format!("board=o%2Fr&csrf={csrf}&{form}");
        let mut bad = headers; bad[1] = ("origin", "https://evil.example");
        assert_eq!(t.call("POST", "/task/control", &bad, &body).0, "403 Forbidden");
        assert_eq!(t.control("github|1", &form.replace("revision=700", "revision=699")).1["outcome"], "stale");
        for action in ["close", "start", "retry"] {
            let status = t.control("github|1", &format!("task=12&revision=700&op_id={action}&action={action}")).0;
            assert_eq!(status, if action == "close" { "400 Bad Request" } else { "422 Unprocessable Content" });
        }
        for extra in ["&text=arbitrary", "&choice=1", "&action=later"] {
            assert_eq!(t.control("github|1", &(form.to_string()+extra)).0, "400 Bad Request");
        }
        t.offers("doing", 700, Value::Null);
        assert_eq!(t.control("github|1", &form.replace("op-1", "no-offer")).0, "422 Unprocessable Content");
        std::fs::write(t.d.join("snap.json"), r#"{"schema":1,"complete":false,"reason":"offline","tasks":[]}"#).unwrap();
        assert_eq!(t.control("github|1", &form.replace("op-1", "incomplete")).0, "422 Unprocessable Content");
        Enroll::controller(&t.d, "ws-1", "github|1", false).unwrap();
        assert_eq!(t.control("github|1", form).0, "403 Forbidden");
        assert!(t.calls().is_empty());
    }

    // #179 A3/A5: a failed refusal write must not turn an unoffered action into a dispatchable retry.
    #[test]
    fn task_controls_failed_precheck_never_dispatches_on_retry() {
        let t = T::new("controls-precheck-failure");
        Enroll::controller(&t.d, "ws-1", "github|1", true).unwrap();
        t.offers("doing", 700, json!([]));
        let db = rusqlite::Connection::open(t.d.join("dashq.db")).unwrap();
        db.execute_batch("CREATE TRIGGER refuse_receipt BEFORE UPDATE ON answers BEGIN SELECT RAISE(FAIL, 'fixture refuses receipt'); END;").unwrap();
        let form = "task=12&revision=700&op_id=op-1&action=stop";
        assert_eq!(t.control("github|1", form).1["outcome"], "rejected");
        assert_eq!(t.control("github|1", form).1["outcome"], "rejected");
        assert!(t.calls().is_empty(), "unoffered action must never reach the dispatcher");
    }

    // #179 A4: unknown survives restart and stale rows; only the same canonical op reconciles it.
    #[test]
    fn task_controls_unknown_and_stale_receipts() {
        let t = T::new("controls-unknown");
        Enroll::controller(&t.d, "ws-1", "github|1", true).unwrap();
        t.offers("doing", 700, json!(["stop"]));
        std::fs::write(t.d.join("receipt.json"), "lost").unwrap();
        let form = "task=12&revision=700&op_id=op-1&action=stop";
        assert_eq!(t.control("github|1", form).1["outcome"], "unknown");
        t.offers("failed", 701, json!(["retry"]));
        t.receipt(json!({"task":12,"op_id":"dashq.control.other","outcome":"accepted"}));
        assert_eq!(t.control("github|1", form).1["outcome"], "unknown", "foreign receipt is never trusted");
        t.receipt(json!({"task":12,"op_id":"dashq.control.op-1","outcome":"accepted"}));
        assert_eq!(t.control("github|1", form).1["outcome"], "unknown", "accepted without readback fields is unknown");
        let t = T { state: Answers::default(), e: Enroll::open(&t.d, HOST), ..t };
        t.receipt(json!({"task":12,"op_id":"dashq.control.op-1","outcome":"accepted","state":"failed","revision":701}));
        assert_eq!(t.control("github|1", form).1["outcome"], "accepted");
        assert_eq!(t.calls(), ["failed 12 --text Reason: Stopped by the owner. --json --op-id dashq.control.op-1 --revision 700"; 4]);
        t.offers("failed", 701, json!(["retry"]));
        t.receipt(json!({"task":12,"op_id":"dashq.control.op-2","outcome":"stale","state":"failed","revision":702,"reason":"task changed"}));
        let (s, v) = t.control("github|1", "task=12&revision=701&op_id=op-2&action=retry");
        assert_eq!((s.as_str(), v["outcome"].as_str()), ("409 Conflict", Some("stale")));
    }

    #[test]
    fn accepted_answer_goes_through_the_dispatcher_once() {
        let t = T::new("ok");
        let (s, v) = t.call("GET", "/answer?board=o%2Fr", &[(enroll::SUBJECT, "github|1")], "");
        assert_eq!(s, "200 OK");
        assert_eq!(v["csrf"], json!(t.e.as_ref().unwrap().token_for(&purpose("github|1", "o/r", NOW / 86400))));
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3");
        assert_eq!((s.as_str(), &v["outcome"], &v["state"], &v["revision"]), ("200 OK", &json!("accepted"), &json!("ready"), &json!(701)));
        assert_eq!(t.calls(), ["answer 12.3 --revision 700 --op-id dashq.op-1 --json"]);
        // double click and lost response: the same ID and payload repeat the first receipt, nothing is written again
        let (s, again) = t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3");
        assert_eq!((s.as_str(), &again), ("200 OK", &v));
        assert_eq!(t.calls().len(), 1);
        // the same ID with another payload is refused before any write
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-1&text=Files+please");
        assert_eq!((s.as_str(), &v["reason"]), ("409 Conflict", &json!("This operation ID belongs to another answer")));
        assert_eq!(t.calls().len(), 1);
        // the verified actor and sanitized receipt are recorded; the free text is not
        let c = rusqlite::Connection::open(t.d.join("dashq.db")).unwrap();
        let (who, state, receipt): (String, String, String) =
            c.query_row("SELECT subject, state, receipt FROM answers WHERE op_id = 'dashq.op-1'", [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap();
        assert_eq!((who.as_str(), state.as_str()), ("github|1", "accepted"));
        assert!(!receipt.contains(TOKEN));
    }

    #[test]
    fn free_text_follows_limits_and_reaches_taskq_as_one_argument() {
        let t = T::new("text");
        t.receipt(json!({"task": 12, "op_id": "dashq.op-2", "outcome": "accepted", "state": "ready", "revision": 702}));
        let (s, _) = t.post("github|1", "task=12&revision=700&op_id=op-2&text=Use+SQLite%3B+keep+files+as+backup");
        assert_eq!(s, "200 OK");
        assert_eq!(t.calls(), ["answer 12 --text Use SQLite; keep files as backup --revision 700 --op-id dashq.op-2 --json"]);
        for bad in ["text=", "text=-x", "text=a%0Ab%0Ac%0Ad%0Ae", &format!("text={}", "x".repeat(241)), "text=a%07b"] {
            let (s, _) = t.post("github|1", &format!("task=12&revision=700&op_id=op-3&{bad}"));
            assert_eq!(s, "400 Bad Request", "{bad}");
        }
        assert_eq!(t.calls().len(), 1);
    }

    #[test]
    fn stale_closing_and_unparsed_cards_refuse_before_mutation() {
        let t = T::new("stale");
        let (s, v) = t.post("github|1", "task=12&revision=699&op_id=op-1&choice=3");
        assert_eq!((s.as_str(), &v["outcome"], &v["revision"]), ("409 Conflict", &json!("stale"), &json!(700)));
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-2&choice=1");
        assert_eq!((s.as_str(), &v["reason"]), ("422 Unprocessable Content", &json!("Close and publication are answered on the board")));
        let (s, _) = t.post("github|1", "task=12&revision=700&op_id=op-3&choice=7");
        assert_eq!(s, "422 Unprocessable Content");
        t.snap("doing", 700);
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-4&choice=3");
        assert_eq!((s.as_str(), &v["outcome"]), ("409 Conflict", &json!("stale")));
        t.snap("review", 700);
        let (s, _) = t.post("github|1", "task=12&revision=700&op_id=op-5&text=ok");
        assert_eq!(s, "422 Unprocessable Content", "free text answers only an ask");
        // an incomplete board read disables writing
        std::fs::write(t.d.join("snap.json"), r#"{"schema":1,"complete":false,"reason":"HTTP 502","tasks":[]}"#).unwrap();
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-6&choice=3");
        assert_eq!((s.as_str(), &v["reason"]), ("422 Unprocessable Content", &json!("Board read failed (taskq incomplete: HTTP 502); nothing was written")));
        assert!(t.calls().is_empty());
        assert_eq!(unavailable(&json!("Close #12 now")), json!("Close and publication are answered on the board"));
        assert_eq!(unavailable(&json!("Files")), Value::Null);
    }

    #[test]
    fn unknown_reconciles_by_the_same_operation_never_a_fresh_one() {
        let t = T::new("unknown");
        std::fs::write(t.d.join("receipt.json"), "lost").unwrap();
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3");
        assert_eq!((s.as_str(), &v["outcome"]), ("202 Accepted", &json!("unknown")));
        // the first write landed and moved the revision; the retry reuses ID, payload and revision without a re-read
        t.snap("ready", 701);
        t.receipt(json!({"task": 12, "op_id": "dashq.op-1", "outcome": "accepted", "state": "ready", "revision": 701}));
        let (s, v) = t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3");
        assert_eq!((s.as_str(), &v["outcome"]), ("200 OK", &json!("accepted")));
        assert_eq!(t.calls(), ["answer 12.3 --revision 700 --op-id dashq.op-1 --json"; 2]);
        // a receipt naming another operation is not trusted
        t.receipt(json!({"task": 12, "op_id": "dashq.other", "outcome": "accepted"}));
        t.snap("ask", 701);
        let (_, v) = t.post("github|1", "task=12&revision=701&op_id=op-9&choice=3");
        assert_eq!(v["outcome"], "unknown");
    }

    #[test]
    fn identity_workspace_csrf_and_rate_are_checked_on_every_request() {
        let t = T::new("deny");
        let form = "task=12&revision=700&op_id=op-1&choice=3";
        // no subject, other workspace's answerer, administrator without the capability, unknown board
        assert_eq!(t.call("GET", "/answer?board=o%2Fr", &[], "").0, "401 Unauthorized");
        assert_eq!(t.post("github|2", form).0, "403 Forbidden");
        assert_eq!(t.post("github|3", form).0, "403 Forbidden");
        assert_eq!(t.call("GET", "/answer?board=x%2Fy", &[(enroll::SUBJECT, "github|1")], "").0, "404 Not Found");
        // cross-site, wrong content type, another account's token, repeated or missing fields
        let csrf = t.e.as_ref().unwrap().token_for(&purpose("github|1", "o/r", NOW / 86400));
        let body = format!("board=o%2Fr&csrf={csrf}&{form}");
        let post = |h: &[(&str, &str)], b: &str| t.call("POST", "/answer", h, b).0;
        assert_eq!(post(&[(enroll::SUBJECT, "github|1"), ("origin", "https://evil.example"), ("content-type", "application/x-www-form-urlencoded")], &body), "403 Forbidden");
        assert_eq!(post(&[(enroll::SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "text/plain")], &body), "403 Forbidden");
        assert_eq!(post(&[(enroll::SUBJECT, "github|3"), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")], &body), "403 Forbidden");
        // a machine, device or read credential is no identity here
        assert_eq!(post(&[("authorization", "Bearer device-credential"), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")], &body), "401 Unauthorized");
        assert_eq!(t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3&choice=1").0, "400 Bad Request");
        assert_eq!(t.post("github|1", "task=12&revision=700&op_id=op-1&choice=3&text=x").0, "400 Bad Request");
        assert_eq!(t.post("github|1", "task=12&revision=700&choice=3").0, "400 Bad Request");
        assert_eq!(t.post("github|1", "task=12&revision=700&op_id=--x&choice=3").0, "400 Bad Request");
        assert_eq!(t.post("github|1", &format!("{form}&pad={}", "x".repeat(MAX_BODY))).0, "413 Content Too Large");
        assert!(t.calls().is_empty());
        // revoked capability: refused at once, nothing written
        Enroll::answerer(&t.d, "ws-1", "github|1", false).unwrap();
        assert_eq!(t.post("github|1", form).0, "403 Forbidden");
        Enroll::answerer(&t.d, "ws-1", "github|1", true).unwrap();
        // ten answers a minute per actor
        for i in 0..PER_MINUTE {
            assert_eq!(t.post("github|1", &format!("task=12&revision=1&op_id=r{i}&choice=3")).0, "409 Conflict");
        }
        assert_eq!(t.post("github|1", form).0, "429 Too Many Requests");
        // a deployment without the answer config, or a world-readable credential, writes nothing
        let ctx = Ctx { enroll: &t.e, cfg: None, bin: Some(&t.bin), public_host: HOST, state: &t.state };
        let r = route(&ctx, &Req { method: "GET", path: "/answer", query: "board=o%2Fr", headers: &[], body: b"" }, NOW).unwrap();
        assert_eq!(r.status, "503 Service Unavailable");
        std::fs::set_permissions(&t.cfg.token, std::fs::Permissions::from_mode(0o644)).unwrap();
        let t2 = T { state: Answers::default(), ..t };
        let (s, v) = t2.post("github|1", form);
        assert_eq!((s.as_str(), &v["reason"]), ("503 Service Unavailable", &json!("The answer credential is unavailable; nothing was written")));
        assert!(t2.calls().is_empty());
    }
}
