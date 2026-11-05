//! AssetQ summary (dashq.md D28, docs/design/assetq-summary.md). AssetQ owns jobs, methods, comparisons,
//! recommendations and assets. This module reads them, GET only, projects an allowlist of metadata (R1) and renders
//! one guarded read-only view. No field passes through unless it is named here, and nothing here writes to AssetQ.

use crate::page::esc;
use crate::source::{self, Fail};
use serde_json::{Map, Value, json};
use std::fmt::Write;
use std::path::PathBuf;

const FRESH_S: u64 = 180;
pub const TTL_S: u64 = 60;
/// Comparison details and recommendation scopes read per project on one refresh (views detail).
const DETAIL_CAP: usize = 10;
const REASON_CAP: usize = 240;
const ACTIVE: [&str; 3] = ["queued", "active", "waiting"];
/// Failed job rows read per project; further pages read as capped (D6).
const FAILED_CAP: usize = 50;

#[derive(Debug, Clone)]
pub struct Project {
    pub id: String,
    pub workspace: String,
    pub read_token: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base: String,
    /// The Cloudflare Access service token for this origin: client ID file, client secret file.
    pub access: Option<(PathBuf, PathBuf)>,
    pub projects: Vec<Project>,
}

/// `assetq` config (D28); errors name the bad key, never its value (D2).
pub fn config(v: &Value) -> Result<Option<Config>, String> {
    let Some(a) = v.get("assetq") else { return Ok(None) };
    let base = a["base_url"].as_str().filter(|x| x.starts_with("https://") && !x.contains(['@', '"', '?', '#']) && x.len() > 8 && !x[8..].trim_end_matches('/').contains('/')).ok_or("config: bad key assetq.base_url")?;
    let rows = a["projects"].as_object().filter(|o| !o.is_empty()).ok_or("config: bad key assetq.projects")?;
    let path = |p: &Value, key: &str| p[key].as_str().filter(|x| std::path::Path::new(x).is_absolute()).map(PathBuf::from).ok_or(format!("config: bad key assetq.projects.{key}"));
    let mut projects = vec![];
    for (id, p) in rows {
        if !crate::enroll::id(id, 128) { return Err("config: bad key assetq.projects".into()); }
        let workspace = p["workspace"].as_str().filter(|w| crate::enroll::id(w, 64)).ok_or("config: bad key assetq.projects.workspace")?.to_string();
        projects.push(Project { id: id.clone(), workspace, read_token: path(p, "read_token")? });
    }
    let file = |key: &str| a[key].as_str().filter(|x| std::path::Path::new(x).is_absolute()).map(PathBuf::from).ok_or(format!("config: bad key assetq.{key}"));
    let access = match (a.get("access_client_id"), a.get("access_client_secret")) {
        (None, None) => None,
        (Some(_), None) => return Err("config: bad key assetq.access_client_secret".into()),
        (None, Some(_)) => return Err("config: bad key assetq.access_client_id".into()),
        _ => Some((file("access_client_id")?, file("access_client_secret")?)),
    };
    Ok(Some(Config { base: base.trim_end_matches('/').to_string(), access, projects }))
}

/// One credential file: a single printable token, never logged. Its reason names the file role, never content.
fn token(path: &std::path::Path) -> Result<String, Fail> {
    std::fs::read_to_string(path).ok().map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty() && x.len() <= 512 && x.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or(("credential unreadable".into(), None))
}

/// One GET with the read credentials (R7: `source::get` passes headers on stdin). Access decides whether the request
/// reaches AssetQ; the Bearer token decides what AssetQ returns. A redirect is Access asking for a sign-in.
fn get(c: &Config, path: &str, headers: &[String]) -> Result<Value, String> {
    let r = source::get(&format!("{}{path}", c.base), headers).map_err(|e| e.0)?;
    match r.status {
        200 => Ok(r.body),
        300..=399 => Err("redirected to a sign-in".into()),
        401 | 403 => Err("credential refused".into()),
        s => Err(format!("HTTP {s}")),
    }
}

/// The source read of one project (D4): `/v1/status` decides the read; each list keeps its own gap.
pub fn read(c: &Config, p: &Project) -> Result<(Value, bool), Fail> {
    let mut headers = vec![format!("Authorization: Bearer {}", token(&p.read_token)?)];
    if let Some((id, secret)) = &c.access {
        headers.push(format!("CF-Access-Client-Id: {}", token(id)?));
        headers.push(format!("CF-Access-Client-Secret: {}", token(secret)?));
    }
    let get = |path: &str| get(c, path, &headers);
    let status = get("/v1/status").map_err(|e| (e, None))?;
    if status["project"] != p.id.as_str() { return Err(("credential names another project".into(), None)); }
    if !status["revisions"].is_object() || !status["jobs"].is_object() { return Err(("unsupported status shape".into(), None)); }
    let mut raw = json!({"status": status, "jobs": {}, "details": {}, "recommendations": []});
    for phase in ACTIVE { raw["jobs"][phase] = list(get(&format!("/v1/jobs?phase={phase}&limit=100"))); }
    raw["jobs"]["failed"] = list(get(&format!("/v1/jobs?phase=failed&limit={FAILED_CAP}")));
    raw["methods"] = list(get("/v1/methods?limit=500"));
    raw["objects"] = list(get("/v1/objects?limit=50"));
    raw["comparisons"] = list(get("/v1/comparisons?limit=50"));
    let rows = raw["comparisons"]["items"].as_array().cloned().unwrap_or_default();
    let (mut scopes, mut skipped) = (vec![], false);
    for (i, c) in rows.iter().enumerate() {
        let Some(id) = c["id"].as_str() else { continue };
        if i >= DETAIL_CAP { skipped = true; continue }
        let detail = get(&format!("/v1/comparisons/{}", source::enc(id))).unwrap_or_else(|e| json!({"read_error": e}));
        let scope = (c["use_case"].as_str().unwrap_or("").to_string(), detail["plan"]["constraints"].as_object().cloned().map_or(json!({}), Value::Object));
        if !scope.0.is_empty() && !scopes.contains(&scope) { if scopes.len() < DETAIL_CAP { scopes.push(scope) } else { skipped = true } }
        raw["details"][id] = detail;
    }
    raw["details_skipped"] = json!(skipped);
    for (use_case, constraints) in scopes {
        let q = format!("/v1/recommendations?use_case={}&constraints={}", source::enc(&use_case), source::enc(&constraints.to_string()));
        raw["recommendations"].as_array_mut().unwrap().push(get(&q).unwrap_or_else(|e| json!({"use_case": use_case, "constraints": constraints, "read_error": e})));
    }
    Ok((raw, false))
}

/// `{items, next}` or a bare array; a failed list keeps its reason.
fn list(r: Result<Value, String>) -> Value {
    match r {
        Ok(Value::Array(a)) => json!({"items": a, "next": null}),
        Ok(v) if v["items"].is_array() => json!({"items": v["items"], "next": v["next"]}),
        Ok(_) => json!({"error": "unsupported list shape"}),
        Err(e) => json!({"error": e}),
    }
}

fn pick(v: &Value, keys: &[&str]) -> Value {
    let mut o = Map::new();
    for k in keys { o.insert((*k).into(), v.get(*k).cloned().unwrap_or(Value::Null)); }
    Value::Object(o)
}

/// What a shown count means (D6): an exact total, a lower bound or no read at all.
fn coverage(l: &Value) -> &'static str {
    if !l["items"].is_array() { "unknown" } else if l["next"].is_null() { "complete" } else { "capped" }
}

/// A count in the view: never an exact number for a capped list, never 0 for an unread one (R2).
fn count(n: usize, coverage: &str) -> String {
    match coverage {
        "complete" => n.to_string(),
        "capped" => format!("≥{n} (capped)"),
        _ => "— · unknown".into(),
    }
}

/// A producer sentence: bounded, one line, never a free-form multi-line dump (R1).
fn line(v: &Value) -> Value {
    v.as_str().map_or(Value::Null, |s| json!(s.lines().next().unwrap_or("").chars().filter(|c| !c.is_control()).take(REASON_CAP).collect::<String>()))
}

/// `{state, reason}` or a fresh `{value, observed_at}` observation (AssetQ § Status values).
fn observation(v: &Value) -> Value {
    let inner = &v["value"];
    let state = v["health"].as_str().or(v["state"].as_str()).or(inner.as_str()).or(inner["health"].as_str()).or(inner["state"].as_str());
    match state {
        Some(s) => json!({"provider": v["provider"], "state": line(&json!(s)), "reason": line(&v["reason"]), "observed_at": v["observed_at"]}),
        None => json!({"provider": v["provider"], "state": "unknown", "reason": "unsupported observation shape", "observed_at": v["observed_at"]}),
    }
}

/// A page of the AssetQ site. The machine `/v1` paths take no browser session, so a link never names one (AssetQ
/// `docs/design/management-site.md`).
fn site(origin: &str, project: &str, path: &str) -> String {
    format!("{origin}{path}?project={}", source::enc(project))
}

fn job(v: &Value, origin: &str, project: &str) -> Value {
    let mut o = pick(v, &["id", "method", "phase", "step", "steps", "effect", "provider_operation", "submissions", "cancel", "reserved_credits", "attempts", "created_at", "updated_at"]);
    o["operation"] = v["request"]["operation"].clone();
    o["usage"] = json!({"credits": v["usage"]["credits"], "unknown_reason": line(&v["usage"]["unknown_reason"])});
    o["reason"] = line(&v["reason"]);
    if let Some(id) = v["id"].as_str() {
        // The job page shows its verified results, and a failed job has none.
        o["links"] = json!({"record": site(origin, project, &format!("/jobs/{}", source::enc(id)))});
    }
    o
}

fn qualification(v: &Value) -> Value {
    if !v.is_object() { return Value::Null }
    let mut o = pick(v, &["scope", "observed_at", "operation", "technical", "general_dispatch", "reason", "owner_judgment"]);
    for k in ["scope", "technical", "reason", "owner_judgment", "general_dispatch"] { o[k] = line(&o[k]); }
    o["evidence"] = json!(v["evidence"].as_array().into_iter().flatten().filter_map(Value::as_str).filter(|u| u.starts_with("https://")).collect::<Vec<_>>());
    o
}

fn method(v: &Value, origin: &str, project: &str) -> Value {
    let mut o = pick(v, &["id", "version", "status", "created_at"]);
    o["name"] = json!(format!("{}@{}", v["id"].as_str().unwrap_or(""), v["version"]));
    let spec = &v["spec"];
    for k in ["operation", "provider", "model", "api_revision", "pipeline_revision", "max_credits", "synthetic"] { o[k] = spec[k].clone(); }
    for k in ["inputs", "outputs", "capabilities", "limitations"] { o[k] = spec[k].clone(); }
    if let (Some(id), Some(version)) = (v["id"].as_str(), v["version"].as_u64()) {
        o["link"] = json!(site(origin, project, &format!("/methods/{}/versions/{version}", source::enc(id))));
    }
    o["qualification"] = qualification(&v["qualification"]);
    o
}

fn object(v: &Value, origin: &str, project: &str) -> Value {
    let mut o = pick(v, &["id", "sha256", "size", "content_type", "kind", "job_id", "synthetic", "acceptance", "retention_until", "created_at"]);
    o["provenance"] = pick(&v["provenance"], &["source", "method", "provider", "operation_id"]);
    if let Some(id) = v["id"].as_str() {
        let id = source::enc(id);
        o["links"] = json!({"record": site(origin, project, &format!("/assets/{id}")), "download": site(origin, project, &format!("/browser/v1/objects/{id}/content"))});
    }
    o
}

fn comparison(row: &Value, detail: &Value, origin: &str, project: &str) -> Value {
    let mut o = pick(row, &["id", "use_case", "dataset_version", "evaluator_version", "method_versions", "created_at"]);
    o["link"] = json!(row["id"].as_str().map(|id| site(origin, project, &format!("/benchmarks/{}", source::enc(id)))));
    if detail["plan"].is_object() {
        for k in ["state", "terminal", "failures", "failure_rate", "credits", "credits_reported", "usage_unknown", "human_judgments", "method_metrics", "winner", "winner_reason", "revision"] { o[k] = detail[k].clone(); }
        o["operation"] = detail["plan"]["operation"].clone();
        o["constraints"] = detail["plan"]["constraints"].clone();
        o["winner_reason"] = line(&detail["winner_reason"]);
        o["missing_cases"] = json!(detail["missing_cases"].as_array().into_iter().flatten().map(|m| json!({"sample": m["sample"], "reason": line(&m["reason"])})).collect::<Vec<_>>());
    } else {
        o["state"] = json!("unknown");
        o["reason"] = match detail["read_error"].as_str() {
            Some(e) => json!(format!("read failed ({e})")),
            None => json!("evidence-only record; results stay on AssetQ"),
        };
        o["missing_cases"] = json!(row["missing_cases"].as_array().map(Vec::len));
    }
    o
}

fn recommendation(v: &Value) -> Value {
    let mut o = pick(v, &["id", "use_case", "constraints", "method", "observed_at", "state", "method_status", "operation"]);
    o["evidence_keys"] = json!(v["evidence"].as_object().map(|e| e.keys().cloned().collect::<Vec<_>>()));
    o
}

fn scope(v: &Value) -> Value {
    if let Some(e) = v["read_error"].as_str() {
        return json!({"use_case": v["use_case"], "constraints": v["constraints"], "state": "unknown", "reason": format!("read failed ({e})"), "recommended": null, "records": []});
    }
    let map = |k: &str| json!(v[k].as_array().into_iter().flatten().map(recommendation).collect::<Vec<_>>());
    json!({"use_case": v["use_case"], "operation": v["operation"], "constraints": v["constraints"], "state": "ok",
        "recommended": if v["recommended"].is_object() { recommendation(&v["recommended"]) } else { Value::Null },
        "none_reason": line(&v["none_reason"]), "records": map("records"), "records_truncated": v["records_truncated"],
        "revision": v["revision"], "observed_at": v["observed_at"],
        "qualification_evidence": v["qualification_evidence"].as_array().into_iter().flatten().map(|e| json!({"method": e["method"], "qualification": qualification(&e["qualification"])})).collect::<Vec<_>>()})
}

/// One project row of `/data.assets` from its cached read `snap` (D6: the retained value keeps its time).
pub fn project(p: &Project, origin: &str, snap: &Value, now: u64) -> Value {
    let raw = &snap["value"];
    let mut source = crate::data::source(snap, now, FRESH_S, None);
    let page = |path: &str| site(origin, &p.id, path);
    let mut row = json!({"project": p.id, "workspace": p.workspace, "links": {"jobs": page("/jobs"), "methods": page("/methods"), "benchmarks": page("/benchmarks"), "operations": page("/operations"), "assets": page("/assets")}});
    if raw.is_null() { row["source"] = source; return row }
    let st = &raw["status"];
    let mut gaps = vec![];
    let mut jobs = vec![];
    for phase in ACTIVE {
        let l = &raw["jobs"][phase];
        if let Some(e) = l["error"].as_str() { gaps.push(format!("{phase} jobs: {e}")); }
        if !l["next"].is_null() && l["error"].is_null() { gaps.push(format!("{phase} jobs capped")); }
        jobs.extend(l["items"].as_array().into_iter().flatten().map(|j| job(j, origin, &p.id)));
    }
    let listed = |key: &str, what: &str, gaps: &mut Vec<String>| -> Vec<Value> {
        let l = &raw[key];
        if let Some(e) = l["error"].as_str() { gaps.push(format!("{what}: {e}")); }
        if !l["next"].is_null() && l["error"].is_null() { gaps.push(format!("{what} capped")); }
        l["items"].as_array().cloned().unwrap_or_default()
    };
    let failed: Vec<Value> = listed_failed(raw, &mut gaps).iter().map(|j| job(j, origin, &p.id)).collect();
    let methods: Vec<Value> = listed("methods", "methods", &mut gaps).iter().map(|m| method(m, origin, &p.id)).collect();
    let objects: Vec<Value> = listed("objects", "objects", &mut gaps).iter().map(|o| object(o, origin, &p.id)).collect();
    let comparisons: Vec<Value> = listed("comparisons", "comparisons", &mut gaps).iter().filter(|c| !raw["details"][c["id"].as_str().unwrap_or("")].is_null()).map(|c| comparison(c, &raw["details"][c["id"].as_str().unwrap_or("")], origin, &p.id)).collect();
    if raw["details_skipped"] == true { gaps.push(format!("only {DETAIL_CAP} comparison details read")); }
    if st["method_outcomes_truncated"] == true { gaps.push("method outcomes capped".into()); }
    if raw["recommendations"].as_array().into_iter().flatten().any(|r| r["records_truncated"] == true) { gaps.push("recommendation records capped".into()); }
    if !gaps.is_empty() && source["state"] == "ok" {
        source["state"] = json!("incomplete");
        source["incomplete"] = json!(true);
    }
    if !gaps.is_empty() {
        let joined = gaps.join("; ");
        source["reason"] = json!(match source["reason"].as_str() { Some(r) => format!("{r}; {joined}"), None => joined });
    }
    row["source"] = source;
    let active = ACTIVE.iter().map(|p| coverage(&raw["jobs"][p])).collect::<Vec<_>>();
    row["coverage"] = json!({
        "active": if active.contains(&"unknown") { "unknown" } else if active.contains(&"capped") { "capped" } else { "complete" },
        "failed": coverage(&raw["jobs"]["failed"]),
        "methods": coverage(&raw["methods"]),
        "objects": coverage(&raw["objects"]),
        "comparisons": if coverage(&raw["comparisons"]) == "complete" && raw["details_skipped"] == true { "capped" } else { coverage(&raw["comparisons"]) },
    });
    row["revisions"] = pick(&st["revisions"], &["registry", "comparisons", "recommendations", "evaluations", "qualification"]);
    row["jobs"] = json!({"by_phase": st["jobs"]["by_phase"], "uncertain_submissions": st["jobs"]["uncertain_submissions"], "leased": st["jobs"]["leased"], "rows": jobs, "failed": failed,
        "capped": ACTIVE.iter().any(|p| !raw["jobs"][p]["next"].is_null())});
    row["method_outcomes"] = json!(st["methods"].as_array().into_iter().flatten().map(|m| pick(m, &["method", "phase", "jobs", "usage_unknown", "credits_reported"])).collect::<Vec<_>>());
    row["providers"] = json!(st["providers"].as_array().into_iter().flatten().map(observation).collect::<Vec<_>>());
    row["store"] = json!({"used_bytes": st["store"]["used_bytes"], "quota_bytes": st["store"]["quota_bytes"], "full": st["store"]["full"],
        "physical_inventory": pick(&st["store"]["physical_inventory"], &["state", "object_bytes", "temp_bytes", "full", "reason"]),
        "free_space": pick(&st["store"]["free_space"], &["state", "available_bytes", "floor_bytes", "admission", "reason"]),
        "metadata": st["admission"]["metadata"].as_object().map(|m| Value::Object(m.iter().filter(|(_, v)| !v.is_object() && !v.is_array()).map(|(k, v)| (k.clone(), v.clone())).collect()))});
    row["integrity"] = observation(&st["store"]["integrity"]);
    row["backup"] = observation(&st["backup"]);
    row["restore"] = observation(&st["restore"]);
    row["methods"] = json!(methods);
    row["objects"] = json!(objects);
    row["comparisons"] = json!(comparisons);
    row["recommendations"] = json!(raw["recommendations"].as_array().into_iter().flatten().map(scope).collect::<Vec<_>>());
    row["recommendations_note"] = json!("AssetQ lists recommendations per scope; scopes shown are those its comparisons name.");
    row
}

fn listed_failed(raw: &Value, gaps: &mut Vec<String>) -> Vec<Value> {
    let l = &raw["jobs"]["failed"];
    if let Some(e) = l["error"].as_str() { gaps.push(format!("failed jobs: {e}")); }
    if coverage(l) == "capped" { gaps.push("failed jobs capped".into()); }
    l["items"].as_array().cloned().unwrap_or_default()
}

/// `/data.assets`: every project the actor may see, each with its own source state.
pub fn build(origin: &str, rows: Vec<Value>, now: u64) -> Value {
    const RANK: [&str; 5] = ["ok", "incomplete", "stale", "unconfirmed", "unknown"];
    let worst = rows.iter().max_by_key(|r| RANK.iter().position(|s| r["source"]["state"] == *s).unwrap_or(4));
    let source = match worst {
        Some(r) => {
            let mut s = r["source"].clone();
            let bad: Vec<String> = rows.iter().filter(|r| r["source"]["state"] != "ok").map(|r| format!("{}: {}", r["project"].as_str().unwrap_or("project"), r["source"]["reason"].as_str().unwrap_or("unknown"))).collect();
            if !bad.is_empty() { s["reason"] = json!(bad.join("; ")); }
            s
        }
        None => json!({"state": "unknown", "at": null, "age_s": null, "reason": "no AssetQ project in this actor's workspaces", "incomplete": false}),
    };
    json!({"origin": origin, "generated_at": now, "source": source, "projects": rows})
}

fn val(v: &Value) -> String {
    match v {
        Value::Null => "—".into(),
        Value::String(s) => esc(s),
        x => esc(&x.to_string()),
    }
}

fn bytes(v: &Value) -> String {
    v.as_u64().map_or_else(|| "—".into(), |n| {
        const G: f64 = (1u64 << 30) as f64;
        if n >= 1 << 30 { format!("{:.1} GiB", n as f64 / G) } else { format!("{:.1} MiB", n as f64 / (1u64 << 20) as f64) }
    })
}

/// AssetQ sends PostgreSQL timestamptz text; keep its offset and discard only fractional seconds.
fn timestamp(v: &Value) -> Option<u64> {
    if let Some(t) = crate::findings::time(v) { return Some(t) }
    let s = v.as_str()?;
    let head = s.get(..19)?.replace(' ', "T");
    let t = source::iso_secs(&head)?;
    if source::iso(t).get(..19)? != head { return None }
    let mut tail = s.get(19..)?;
    if let Some(fraction) = tail.strip_prefix('.') {
        let n = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 { return None }
        tail = &fraction[n..];
    }
    let offset = if tail == "Z" { 0 } else {
        if !matches!(tail.len(), 3 | 6) || !matches!(tail.get(..1)?, "+" | "-") { return None }
        if !tail[1..].bytes().all(|b| b.is_ascii_digit() || b == b':') { return None }
        let h = tail.get(1..3)?.parse::<i64>().ok()?;
        let m = if tail.len() == 6 {
            if tail.get(3..4)? != ":" { return None }
            tail.get(4..6)?.parse::<i64>().ok()?
        } else { 0 };
        if !(0..=23).contains(&h) || !(0..=59).contains(&m) { return None }
        (h * 3600 + m * 60) * if tail.starts_with('-') { -1 } else { 1 }
    };
    u64::try_from(i64::try_from(t).ok()? - offset).ok()
}

fn stamp(v: &Value, now: u64) -> String {
    match timestamp(v) {
        Some(t) => format!("<time data-at=\"{t}\">{}</time>", esc(&crate::page::at(&json!(t), now))),
        None => esc(&crate::page::at(&Value::Null, now)),
    }
}

fn state(v: &Value, now: u64) -> String {
    let s = v["state"].as_str().unwrap_or("unknown");
    let class = match s { "ok" | "healthy" | "observed" | "verified" => "ok", "unknown" | "stale" | "unconfirmed" | "incomplete" => "unknown", _ => "warn" };
    let reason = v["reason"].as_str().map_or(String::new(), |r| format!(" · {}", esc(r)));
    let at = if v["observed_at"].is_null() { String::new() } else { format!(" · observed {}", stamp(&v["observed_at"], now)) };
    format!("<span class=\"s {class}\">{}</span>{reason}{at}", esc(s))
}

fn link(href: &Value, label: &str) -> String {
    href.as_str().filter(|h| h.starts_with("https://")).map_or(String::new(), |h| format!("<a href=\"{}\" rel=\"noreferrer\">{label}</a>", esc(h)))
}

fn job_rows(w: &mut String, p: &Value, now: u64) {
    let rows = p["jobs"]["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        let text = if p["source"]["state"] == "ok" { "No queued, active or waiting jobs." } else { "Active jobs unknown · see the source state above." };
        let _ = write!(w, "<p>{text}</p>");
    }
    for j in &rows { job_row(w, j, now); }
}

fn job_row(w: &mut String, j: &Value, now: u64) {
    let steps = j["steps"].as_array().map_or(0, Vec::len);
    let step = j["step"].as_u64().map_or_else(|| "—".to_string(), |s| format!("{} of {steps}", s + 1));
    let usage = if j["usage"]["credits"].is_null() { format!("— · unknown: {}", j["usage"]["unknown_reason"].as_str().map_or("not reported".into(), esc)) } else { val(&j["usage"]["credits"]) };
    let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>{}</span><span>Phase {}</span><span>Step {step}</span><span>Effect {}</span><span>Cancellation {}</span><span>Credits {usage} · reserved {}</span><span>Updated {}</span>", val(&j["id"]), val(&j["method"]), val(&j["phase"]), val(&j["effect"]), val(&j["cancel"]), val(&j["reserved_credits"]), stamp(&j["updated_at"], now));
    if !j["reason"].is_null() { let _ = write!(w, "<span>{}</span>", val(&j["reason"])); }
    let _ = write!(w, "<span>{}</span>", link(&j["links"]["record"], "Job on AssetQ"));
    w.push_str("</div>");
}

/// Failed work with AssetQ's bounded reason; generation, validation and owner acceptance stay separate (AssetQ status `states`).
fn failed_rows(w: &mut String, p: &Value, now: u64) {
    let rows = p["jobs"]["failed"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-failed\"><summary id=\"assetq-{0}-failed-summary\">Failed jobs · {1}</summary><p class=\"note\">Phase reports generation; a verified manifest reports validation; each retained record reports owner acceptance.</p>", val(&p["project"]), count(rows.len(), p["coverage"]["failed"].as_str().unwrap_or("unknown")));
    if rows.is_empty() {
        let _ = write!(w, "<p>{}</p>", if p["source"]["state"] == "ok" { "No failed jobs." } else { "Failed jobs unknown · see the source state above." });
    }
    for j in &rows { job_row(w, j, now); }
    w.push_str("</details>");
}

fn storage(w: &mut String, p: &Value, now: u64) {
    let s = &p["store"];
    let f = &s["free_space"];
    let i = &s["physical_inventory"];
    let _ = write!(w, "<h4>Storage</h4><div class=\"asset-row\"><span>Store {} of {}{}</span><span>Inventory {}{}</span><span>Free space {} · floor {} · admission {}{}</span></div><div class=\"asset-row\"><span>Integrity {}</span><span>Backup {}</span><span>Restore {}</span></div>",
        bytes(&s["used_bytes"]), bytes(&s["quota_bytes"]), if s["full"] == true { " · full" } else { "" },
        bytes(&i["object_bytes"]), i["reason"].as_str().map_or(String::new(), |r| format!(" · unknown: {}", esc(r))),
        bytes(&f["available_bytes"]), bytes(&f["floor_bytes"]), val(&f["admission"]), f["reason"].as_str().map_or(String::new(), |r| format!(" · unknown: {}", esc(r))),
        state(&p["integrity"], now), state(&p["backup"], now), state(&p["restore"], now));
}

fn methods(w: &mut String, p: &Value, now: u64) {
    let rows = p["methods"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-methods\"><summary id=\"assetq-{0}-methods-summary\">Methods · {1}{2}</summary>", val(&p["project"]), count(rows.len(), p["coverage"]["methods"].as_str().unwrap_or("unknown")), if p["coverage"]["methods"] == "unknown" { "" } else { " versions" });
    w.push_str("<p class=\"note\">Registry status, qualification records and provider health are separate. A record names what it proves; it never enables dispatch.</p>");
    let _ = write!(w, "<p>{}</p>", link(&p["links"]["methods"], "Manage methods on AssetQ"));
    for m in &rows {
        let outcomes: Vec<String> = p["method_outcomes"].as_array().into_iter().flatten().filter(|o| o["method"] == m["name"]).map(|o| format!("{} {}", val(&o["phase"]), val(&o["jobs"]))).collect();
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Status {}</span><span>{}</span><span>Provider {}</span><span>Model {}</span><span>Maximum credits {}</span><span>Outcomes {}</span>", val(&m["name"]), val(&m["status"]), val(&m["operation"]), val(&m["provider"]), val(&m["model"]), val(&m["max_credits"]), if outcomes.is_empty() { "—".into() } else { outcomes.join(" · ") });
        if m["qualification"].is_object() {
            let q = &m["qualification"];
            let _ = write!(w, "<span>Qualification {} · general dispatch {} · {} · observed {}</span>", val(&q["scope"]), val(&q["general_dispatch"]), val(&q["reason"]), stamp(&q["observed_at"], now));
        } else {
            w.push_str("<span>Qualification — · no retained receipt for this version</span>");
        }
        let _ = write!(w, "<span>{}</span></div>", link(&m["link"], "Method on AssetQ"));
    }
    w.push_str("</details>");
}

fn comparisons(w: &mut String, p: &Value, now: u64) {
    let rows = p["comparisons"].as_array().cloned().unwrap_or_default();
    let scopes = p["recommendations"].as_array().cloned().unwrap_or_default();
    let coverage = p["coverage"]["comparisons"].as_str().unwrap_or("unknown");
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-comparisons\"><summary id=\"assetq-{0}-comparisons-summary\">Comparisons · {1} · recommendation scopes · {2}</summary>", val(&p["project"]), count(rows.len(), coverage), count(scopes.len(), coverage));
    let _ = write!(w, "<p>{}</p>", link(&p["links"]["benchmarks"], "Benchmarks on AssetQ"));
    if rows.is_empty() { let _ = write!(w, "<p>{}</p>", if p["source"]["state"] == "ok" { "No comparisons." } else { "Comparisons unknown · see the source state above." }); }
    for c in &rows {
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Use case {}</span><span>{}</span><span>Dataset {} · evaluator {}</span><span>State {}{}</span><span>Failure rate {} · credits {} · judgments {}</span><span>{}</span></div>", val(&c["id"]), val(&c["use_case"]), val(&c["operation"]), val(&c["dataset_version"]), val(&c["evaluator_version"]), val(&c["state"]), c["reason"].as_str().map_or(String::new(), |r| format!(" · {}", esc(r))), val(&c["failure_rate"]), val(&c["credits"]), val(&c["human_judgments"]), link(&c["link"], "Record"));
        for m in c["method_metrics"].as_array().into_iter().flatten() {
            let _ = write!(w, "<div class=\"asset-row asset-sub\"><strong>{}</strong><span>Samples {} · terminal {} · failures {}</span><span>Failure rate {}</span><span>Credits {} · unknown {}</span><span>Duration {}</span><span>Human mean {} · missing {}</span></div>", val(&m["method"]), val(&m["samples"]), val(&m["terminal"]), val(&m["failures"]), val(&m["failure_rate"]), val(&m["credits"]), val(&m["usage_unknown"]), m["duration_ms"].as_u64().map_or("? unavailable".into(), |ms| esc(&crate::data::dur(ms / 1000))), val(&m["human_score_mean"]), val(&m["human_judgments_missing"]));
        }
        let missing: Vec<String> = c["missing_cases"].as_array().into_iter().flatten().map(|m| format!("{}: {}", val(&m["sample"]), val(&m["reason"]))).collect();
        if !missing.is_empty() { let _ = write!(w, "<p class=\"note\">Missing · {}</p>", missing.join(" · ")); }
        if c["winner_reason"].is_string() { let _ = write!(w, "<p class=\"note\">{}</p>", val(&c["winner_reason"])); }
    }
    let _ = write!(w, "<h4>Recommendations</h4><p class=\"note\">{}</p>", val(&p["recommendations_note"]));
    for s in &scopes {
        let current = if s["recommended"].is_object() { format!("Recommended {}", val(&s["recommended"]["method"])) } else { format!("None · {}", s["none_reason"].as_str().or(s["reason"].as_str()).map_or("unknown".into(), esc)) };
        let records = s["records"].as_array().map_or(0, Vec::len);
        let shown = count(records, if s["state"] != "ok" { "unknown" } else if s["records_truncated"] == true { "capped" } else { "complete" });
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Constraints {}</span><span>{current}</span><span>Records {shown}</span><span>Observed {}</span></div>", val(&s["use_case"]), val(&s["constraints"]), stamp(&s["observed_at"], now));
        for r in s["records"].as_array().into_iter().flatten() {
            let keys: Vec<String> = r["evidence_keys"].as_array().into_iter().flatten().map(val).collect();
            let _ = write!(w, "<div class=\"asset-row asset-sub\"><strong>{}</strong><span>{} · {}</span><span>Method status {}</span><span>Evidence {} · observed {}</span>", val(&r["id"]), val(&r["method"]), val(&r["state"]), val(&r["method_status"]), if keys.is_empty() { "—".into() } else { keys.join(", ") }, stamp(&r["observed_at"], now));
            w.push_str("</div>");
        }
    }
    w.push_str("</details>");
}

fn records(w: &mut String, p: &Value) {
    let rows = p["objects"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-records\"><summary id=\"assetq-{0}-records-summary\">Retained records · {1}</summary>", val(&p["project"]), count(rows.len(), p["coverage"]["objects"].as_str().unwrap_or("unknown")));
    let _ = write!(w, "<p class=\"note\">Previews, search and asset detail are on {}.</p>", link(&p["links"]["assets"], "the AssetQ site"));
    for o in &rows {
        let synthetic = if o["synthetic"] == true { " · synthetic" } else { "" };
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>{}{synthetic}</span><span>{}</span><span>SHA-256 {}</span><span>Source {} · job {}</span><span>Owner acceptance {}</span><span>{} · {}</span></div>", val(&o["id"]), val(&o["content_type"]), bytes(&o["size"]), val(&o["sha256"]), val(&o["provenance"]["source"]), val(&o["job_id"]), val(&o["acceptance"]), link(&o["links"]["record"], "Asset and provenance on AssetQ"), link(&o["links"]["download"], "Download"));
    }
    w.push_str("</details>");
}

pub fn render(w: &mut String, assets: &Value) {
    if !assets.is_object() { return }
    let now = assets["generated_at"].as_u64().unwrap_or(0);
    let origin = &assets["origin"];
    let _ = write!(w, "<section id=\"assets\"><h2>AssetQ</h2><p>AssetQ owns this state; each value keeps its source and time. People act on <a href=\"{}\" rel=\"noreferrer\">the AssetQ site</a>; this page only reads.</p>", val(origin));
    let projects = assets["projects"].as_array().cloned().unwrap_or_default();
    if projects.is_empty() { let _ = write!(w, "<p>Unknown · {}.</p>", val(&assets["source"]["reason"])); }
    for p in projects {
        let src = &p["source"];
        let age = src["age_s"].as_u64().map_or("age unknown".into(), |a| format!("{} old", esc(&crate::data::dur(a))));
        let _ = write!(w, "<article id=\"assetq-{0}\"><h3>{0}</h3><p data-assets-state>{1} · {age} · workspace {2}</p>", val(&p["project"]), state(src, now), val(&p["workspace"]));
        if p["revisions"].is_object() {
            let phases = p["jobs"]["by_phase"].as_object().map_or("—".to_string(), |o| ["queued", "active", "waiting", "succeeded", "failed", "cancelled"].iter().map(|k| format!("{k} {}", o.get(*k).map_or("0".into(), val))).collect::<Vec<_>>().join(" · "));
            let _ = write!(w, "<h4>Jobs</h4><p>{phases}</p><p>Uncertain submissions {} · leased {} · Active rows {}. Rows list queued, active and waiting jobs.</p>", val(&p["jobs"]["uncertain_submissions"]), val(&p["jobs"]["leased"]), count(p["jobs"]["rows"].as_array().map_or(0, Vec::len), p["coverage"]["active"].as_str().unwrap_or("unknown")));
            job_rows(w, &p, now);
            failed_rows(w, &p, now);
            let _ = write!(w, "<p>{} · {}</p>", link(&p["links"]["jobs"], "All jobs and new requests on AssetQ"), link(&p["links"]["operations"], "Operations on AssetQ"));
            let providers: Vec<String> = p["providers"].as_array().into_iter().flatten().map(|x| format!("{} {}", val(&x["provider"]), state(x, now))).collect();
            let _ = write!(w, "<h4>Providers</h4><p>{}</p>", if providers.is_empty() { "Unknown · no provider observations.".to_string() } else { providers.join("<br>") });
            storage(w, &p, now);
            methods(w, &p, now);
            comparisons(w, &p, now);
            records(w, &p);
        } else {
            w.push_str("<p>No AssetQ observation yet.</p>");
        }
        w.push_str("</article>");
    }
    w.push_str("</section>");
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn config_fixture() -> Project {
        Project { id: "invented-project".into(), workspace: "invented-workspace".into(), read_token: "/invented/read".into() }
    }

    /// Invented AssetQ replies in the shapes of AssetQ `docs/service.md`, including the fields R1 must drop.
    pub fn raw() -> Value {
        let job = |id: &str, phase: &str, cancel: &str, credits: Value| json!({"id": id, "method": "synthetic.fake@1", "phase": phase, "step": 1, "steps": ["provider.submit", "provider.import", "validate"], "effect": "submitted", "provider_operation": "invented-operation", "submissions": 1, "cancel": cancel, "usage": {"credits": credits, "unknown_reason": "provider did not report usage"}, "reserved_credits": 1, "reason": null, "attempts": 1, "fence": 2, "leased": false, "request": {"operation": "synthetic.generate", "text": "SECRET-PROMPT-TEXT", "attachments": ["SECRET-ATTACHMENT"], "budget": {"max_credits": 1}}, "created_at": "2026-11-05 10:00:00+00", "updated_at": "2026-11-05 10:05:00+00"});
        json!({
            "status": {"project": "invented-project", "jobs": {"by_phase": {"waiting": 1, "active": 1, "succeeded": 3, "failed": 1}, "uncertain_submissions": 1, "leased": 1},
                "methods": [{"method": "synthetic.fake@1", "phase": "succeeded", "jobs": 3, "usage_unknown": 0, "credits_reported": 3}],
                "providers": [{"provider": "synthetic.fake", "health": "unknown", "reason": "no fresh health observation recorded"}, {"value": {"health": "healthy"}, "observed_at": "2026-11-05 10:04:00+00"}],
                "store": {"used_bytes": 2147483648u64, "quota_bytes": 53687091200u64, "full": false, "integrity": {"state": "unknown", "reason": "no fresh full integrity scan recorded"},
                    "physical_inventory": {"state": "observed", "object_bytes": 2147483648u64, "temp_bytes": 0, "full": false},
                    "free_space": {"state": "observed", "available_bytes": 107374182400u64, "floor_bytes": 1073741824u64, "write_overhead_bytes": 65536, "admission": "available"}},
                "backup": {"state": "unknown", "reason": "no fresh backup or restore observation recorded"},
                "restore": {"state": "unknown", "reason": "backup/restore qualification deferred; no fresh scoped restore observation"},
                "revisions": {"registry": "3:17", "comparisons": "1:5", "recommendations": "2:9", "evaluations": "2:3", "qualification": "abc"},
                "admission": {"metadata": {"state": "observed", "used_bytes": 1024}}},
            "jobs": {"queued": {"items": [], "next": null}, "active": {"items": [job("job-active", "active", "none", Value::Null)], "next": null}, "waiting": {"items": [job("job-waiting", "waiting", "requested", json!(1))], "next": null},
                "failed": {"items": [{"id": "job-failed", "method": "synthetic.fake@1", "phase": "failed", "step": 2, "steps": ["provider.submit", "provider.import", "validate"], "effect": "done", "cancel": "none", "usage": {"credits": 1, "unknown_reason": null}, "reserved_credits": 0, "reason": "validation refused the import\nSECRET-SECOND-LINE", "attempts": 1, "request": {"operation": "synthetic.generate", "text": "SECRET-PROMPT-TEXT"}, "created_at": "2026-11-05 09:00:00+00", "updated_at": "2026-11-05 09:01:00+00"}], "next": "job-failed"}},
            "methods": {"items": [
                {"id": "synthetic.fake", "version": 1, "status": "experimental", "created_at": "2026-11-01", "qualification": null, "spec": {"operation": "synthetic.generate", "provider": "synthetic.fake", "model": "none", "max_credits": 1, "synthetic": true, "steps": ["provider.submit"]}},
                {"id": "image.worker.v1", "version": 5, "status": "experimental", "created_at": "2026-11-04", "spec": {"operation": "image.generate", "provider": "agentq.image", "model": "inherit", "max_credits": 0},
                    "qualification": {"scope": "one bounded protected generation", "observed_at": "2026-11-04", "technical": "completed original operation", "general_dispatch": "refused", "reason": "one qualified tool/output path does not qualify general dispatch", "job": "SECRET-RECEIPT-JOB", "owner_judgment": "pending", "evidence": ["https://github.com/taskqorg/assetq/blob/invented/receipt.md", "file:///SECRET-PATH"]}},
                {"id": "image.worker.v1", "version": 1, "status": "disabled", "created_at": "2026-11-01", "qualification": null, "spec": {"operation": "image.generate", "provider": "agentq.image", "model": "unqualified", "max_credits": 0}}], "next": null},
            "objects": {"items": [{"id": "object-1", "sha256": "aa11", "size": 2048, "content_type": "image/png", "kind": "output", "job_id": "job-old", "provenance": {"source": "job", "method": "image.worker.v1@5", "prompt": "SECRET-PROMPT-TEXT"}, "synthetic": false, "acceptance": "pending", "retention_until": null, "created_at": "2026-11-04"}], "next": "object-1"},
            "comparisons": {"items": [{"id": "comparison-1", "use_case": "invented use case", "dataset_version": "dataset@1", "evaluator_version": "evaluator@1", "method_versions": ["synthetic.fake@1"], "results": {"secret": "SECRET-PROMPT-TEXT"}, "missing_cases": [], "created_at": "2026-11-05"}], "next": null},
            "details": {"comparison-1": {"id": "comparison-1", "plan": {"operation": "synthetic.generate", "constraints": {}, "dataset": {"document": {"cases": [{"text": "SECRET-PROMPT-TEXT"}]}}}, "samples": [{"request": {"text": "SECRET-PROMPT-TEXT"}}], "missing_cases": [{"sample": "0", "reason": "usage unknown"}, {"sample": "1", "reason": "human judgment missing"}], "method_metrics": [{"method": "synthetic.fake@1", "samples": 2, "terminal": 2, "failures": 1, "failure_rate": 0.5, "credits": null, "usage_unknown": 1, "duration_ms": 1200, "human_score_mean": null, "human_judgments_missing": 1}], "state": "incomplete", "terminal": 2, "failures": 1, "failure_rate": 0.5, "credits": null, "credits_reported": 1, "usage_unknown": 1, "human_judgments": 0, "winner": null, "winner_reason": "owner assessment of the frozen decision rule required; no automatic promotion", "revision": "cmp-rev"}},
            "details_skipped": false,
            "recommendations": [{"use_case": "invented use case", "operation": null, "constraints": {}, "recommended": null, "none_reason": "no promoted recommendation with an enabled method for this operation/use case/constraints", "records": [{"id": "rec-1", "use_case": "invented use case", "constraints": {}, "method": "synthetic.fake@1", "evidence": {"comparison": "comparison-1", "note": "SECRET-PROMPT-TEXT"}, "observed_at": "2026-11-05", "state": "proposed", "proposed_by": "invented-project", "method_status": "experimental", "operation": "synthetic.generate"}], "records_truncated": false, "revision": "rec-rev", "observed_at": "2026-11-05 10:05:00+00", "qualification_evidence": []}]
        })
    }

    pub const NOW: u64 = 1_800_000_000;

    pub fn doc() -> Value {
        let snap = json!({"value": raw(), "at": NOW - 20, "error": null, "incomplete": false});
        build("https://assets.example.com", vec![project(&config_fixture(), "https://assets.example.com", &snap, NOW)], NOW)
    }

    /// R1/D28: request text, dataset cases, free-form evidence and receipt fields never reach /data or the page.
    #[test]
    fn assetq_projection_drops_content() {
        let d = doc();
        let mut html = String::new();
        render(&mut html, &d);
        for leak in ["SECRET", "file:///"] {
            assert!(!d.to_string().contains(leak), "/data leaked {leak}");
            assert!(!html.contains(leak), "page leaked {leak}");
        }
        // A4: the real records stay one link away on the AssetQ site; this page copies none of their bytes. Defect
        // guarded: links to the machine `/v1` paths, which a browser session cannot open (AssetQ docs/service.md).
        for record in ["/assets/object-1", "/browser/v1/objects/object-1/content", "/jobs/job-active", "/jobs/job-failed", "/methods/synthetic.fake/versions/1", "/benchmarks/comparison-1", "/operations", "/jobs", "/methods", "/benchmarks"] {
            assert!(html.contains(&format!("https://assets.example.com{record}?project=invented-project\"")), "{record}");
        }
        assert!(!html.contains("example.com/v1/"), "a link names a machine path");
    }

    /// R2/D6: unknown usage, a capped list and deferred observations stay explicit; no value becomes ok or zero.
    #[test]
    fn assetq_unknowns_stay_explicit() {
        let d = doc();
        let p = &d["projects"][0];
        assert_eq!(p["source"]["state"], "incomplete");
        assert!(p["source"]["reason"].as_str().unwrap().contains("objects capped"));
        assert!(p["jobs"]["rows"][0]["usage"]["credits"].is_null());
        assert_eq!(p["backup"]["state"], "unknown");
        assert_eq!(p["restore"]["state"], "unknown");
        assert_eq!(p["providers"][1]["state"], "healthy");
        assert_eq!(p["comparisons"][0]["state"], "incomplete");
        assert!(p["comparisons"][0]["winner"].is_null());
        assert!(p["recommendations"][0]["recommended"].is_null());
        let mut html = String::new();
        render(&mut html, &d);
        assert!(html.contains("— · unknown: provider did not report usage"));
        // a failed read keeps the values with their time and never reads ok
        let snap = json!({"value": raw(), "at": NOW - 900, "error": "timeout", "incomplete": false});
        let r = project(&config_fixture(), "https://assets.example.com", &snap, NOW);
        assert_eq!(r["source"]["state"], "unconfirmed");
        assert_eq!(r["jobs"]["rows"].as_array().unwrap().len(), 2);
        // no read at all renders no empty panel of zeros
        let none = project(&config_fixture(), "https://assets.example.com", &json!({"value": null, "at": null, "error": "timeout"}), NOW);
        assert_eq!(none["source"]["state"], "unknown");
        assert!(none["jobs"].is_null());
    }

    #[test]
    fn assetq_config_names_keys_only() {
        assert!(config(&json!({})).unwrap().is_none());
        let abs = std::env::temp_dir().join("invented-read").display().to_string();
        let good = json!({"assetq": {"base_url": "https://assets.example.com/", "projects": {"p": {"workspace": "w", "read_token": abs}}}});
        let c = config(&good).unwrap().unwrap();
        assert_eq!(c.base, "https://assets.example.com");
        assert!(c.access.is_none());
        let both = json!({"assetq": {"base_url": "https://a.example", "access_client_id": abs, "access_client_secret": abs, "projects": {"p": {"workspace": "w", "read_token": abs}}}});
        assert!(config(&both).unwrap().unwrap().access.is_some());
        for (bad, key) in [(json!({"assetq": {"base_url": "https://a.example", "access_client_id": abs, "projects": {"p": {"workspace": "w", "read_token": abs}}}}), "access_client_secret"), (json!({"assetq": {"base_url": "https://a.example", "access_client_secret": abs, "projects": {"p": {"workspace": "w", "read_token": abs}}}}), "access_client_id"), (json!({"assetq": {"base_url": "https://a.example", "access_client_id": "relative-secret", "access_client_secret": abs, "projects": {"p": {"workspace": "w", "read_token": abs}}}}), "access_client_id"), (json!({"assetq": {"base_url": "http://a.example", "projects": {"p": {"workspace": "w", "read_token": abs}}}}), "base_url"), (json!({"assetq": {"base_url": "https://a.example", "projects": {"p": {"workspace": "w", "read_token": "relative-secret"}}}}), "projects")] {
            let e = config(&bad).unwrap_err();
            assert!(e.contains(key) && !e.contains("relative-secret"), "{e}");
        }
    }

    fn rendered(raw: Value) -> String {
        let snap = json!({"value": raw, "at": NOW - 20, "error": null, "incomplete": false});
        let d = build("https://assets.example.com", vec![project(&config_fixture(), "https://assets.example.com", &snap, NOW)], NOW);
        let mut html = String::new();
        render(&mut html, &d);
        html
    }

    /// D6: a capped list counts `≥N (capped)`, and a failed list read counts unknown, never 0 (R2). Defect guarded:
    /// a truncated or unread list shown as an exact total.
    #[test]
    fn assetq_capped_counts_read_at_least() {
        let html = rendered(raw());
        assert!(html.contains("Retained records · ≥1 (capped)"), "objects has a next page");
        assert!(html.contains("Methods · 3 versions"), "a complete list stays exact");
        assert!(html.contains("recommendation scopes · 1</summary>"));
        assert!(html.contains("Failed jobs · ≥1 (capped)"), "failed has a next page");
        let mut r = raw();
        r["methods"] = json!({"error": "HTTP 500"});
        r["comparisons"] = json!({"error": "timeout"});
        r["recommendations"] = json!([]);
        let html = rendered(r);
        assert!(html.contains("Methods · — · unknown"), "a failed read is no zero");
        assert!(html.contains("Comparisons · — · unknown"));
        assert!(html.contains("recommendation scopes · — · unknown"), "unread comparison scopes are unknown");
        // details read for only part of the list: the shown count is a lower bound
        let mut r = raw();
        r["details_skipped"] = json!(true);
        let html = rendered(r);
        assert!(html.contains("Comparisons · ≥1 (capped)"));
        assert!(html.contains("recommendation scopes · ≥1 (capped)"), "skipped comparisons can name more scopes");
        let mut r = raw();
        r["comparisons"]["next"] = json!("comparison-1");
        assert!(rendered(r).contains("recommendation scopes · ≥1 (capped)"));
        // a recommendation scope with truncated records
        let mut r = raw();
        r["recommendations"][0]["records_truncated"] = json!(true);
        assert!(rendered(r).contains("Records ≥1 (capped)"), "truncated scope records");
        // jobs: a further page on any active phase
        let mut r = raw();
        r["jobs"]["active"]["next"] = json!("job-active");
        assert!(rendered(r).contains("Active rows ≥2 (capped)"));
    }

    /// D6/specq Time: producer timestamps use the shared relative/exact format on every rendered path.
    #[test]
    fn assetq_times_use_shared_format() {
        let mut r = raw();
        let producer_time = "2026-11-05 10:05:00.123456+00";
        let epoch = source::iso_secs("2026-11-05T10:05:00Z").unwrap();
        r["status"]["providers"][1]["observed_at"] = json!(producer_time);
        r["methods"]["items"][1]["qualification"]["observed_at"] = json!(producer_time);
        r["recommendations"][0]["observed_at"] = json!(producer_time);
        r["recommendations"][0]["records"][0]["observed_at"] = json!(producer_time);
        r["jobs"]["active"]["items"][0]["updated_at"] = json!(producer_time);
        for key in ["backup", "restore"] { r["status"][key]["observed_at"] = json!(producer_time); }
        r["status"]["store"]["integrity"]["observed_at"] = json!(producer_time);
        let html = rendered(r);
        assert_eq!(html.matches(&format!("data-at=\"{epoch}\"")).count(), 9, "active/waiting jobs, provider, integrity/backup/restore, qualification, scope and record timestamps");
        assert!(html.contains("&lt;1m old"), "source age uses the shared duration");
        assert!(!html.contains(producer_time), "raw PostgreSQL time reached the page");
        assert!(html.contains("Updated <time") && html.contains("ago (2026-11-05 10:05 UTC)"));
        assert!(html.contains("Duration &lt;1m"));
        for value in [json!(epoch), json!("2026-11-05T10:05:00Z"), json!("2026-11-05 15:35:00.999+05:30"), json!("2026-11-05 06:05:00-04")] {
            assert_eq!(timestamp(&value), Some(epoch), "{value}");
        }
        for value in [Value::Null, json!("2026-11-05"), json!("invalid"), json!("2026-02-30 10:05:00+00"), json!("2026-11-05 10:05:00"), json!("2026-11-05 10:05:00.+00"), json!("2026-11-05 10:05:00+25:00")] {
            assert_eq!(stamp(&value, NOW), "? unavailable", "{value}");
        }
    }

    /// Design States, AssetQ status `states`: a failed job keeps AssetQ's bounded reason, offers no result link,
    /// and names validation and owner acceptance as separate. Defect guarded: failed work visible only as a count.
    #[test]
    fn assetq_failed_jobs_keep_their_reason() {
        let html = rendered(raw());
        assert!(html.contains("job-failed") && html.contains("validation refused the import"));
        assert!(!html.contains("SECRET"), "only the first line, no request");
        assert!(!html.contains("job-failed/result"), "a failed job has no verified manifest");
        assert!(html.contains("Phase reports generation; a verified manifest reports validation; each retained record reports owner acceptance."));
        let mut r = raw();
        r["jobs"]["failed"] = json!({"error": "HTTP 500"});
        let snap = json!({"value": r, "at": NOW - 20, "error": null, "incomplete": false});
        let row = project(&config_fixture(), "https://assets.example.com", &snap, NOW);
        assert!(row["source"]["reason"].as_str().unwrap().contains("failed jobs: HTTP 500"));
    }

    /// Writes the review page used by tests/assets-browser.cjs.
    #[test]
    fn assetq_page_evidence() {
        let mut input: Value = serde_json::from_str(include_str!("../fixture/hosts.json")).unwrap();
        let mut doc_full = crate::data::build(&input["input"].take());
        doc_full["assets"] = doc();
        let html = crate::page::render_view(&doc_full, &json!({}), "/assets");
        assert!(html.contains("id=\"view-assets\"") && html.contains("id=\"assets\""));
        let mut section = String::new();
        render(&mut section, &doc_full["assets"]);
        for write in ["<form", "<button", "<textarea", "<input", "<select"] { assert!(!section.contains(write), "D28: the view has no write control: {write}"); }
        let mut none = doc_full.clone();
        none.as_object_mut().unwrap().remove("assets");
        let unconfigured = crate::page::render_view(&none, &json!({}), "/assets");
        assert!(!unconfigured.contains("id=\"view-assets\"") && !unconfigured.contains("id=\"assets\""), "no navigation entry and no panel");
        if let Ok(path) = std::env::var("DASHQ_ASSETS_EVIDENCE") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(std::path::Path::new(&path).join("page.html"), &html).unwrap();
            std::fs::write(std::path::Path::new(&path).join("unconfigured.html"), &unconfigured).unwrap();
        }
    }
    // ---------------------------------------------------------------- reads, over real HTTP to an in-process stub

    use std::io::{BufRead, BufReader, Write as _};
    use std::sync::{Arc, Mutex};

    /// Method, path and lowercase request headers of every call the stub received.
    type Calls = Arc<Mutex<Vec<(String, String, Vec<String>)>>>;

    /// An invented AssetQ behind an invented Access guard: without both Access headers it redirects like the real guard.
    fn stub(project: &'static str, accept_bearer: bool) -> (String, Calls) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        let calls: Calls = Default::default();
        let seen = calls.clone();
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                if r.read_line(&mut first).is_err() { continue }
                let mut headers = vec![];
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() { break }
                    headers.push(h.trim().to_ascii_lowercase());
                }
                let mut parts = first.split_whitespace();
                let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let path = target.split('?').next().unwrap_or("").to_string();
                let access = headers.iter().any(|h| h == "cf-access-client-id: invented-access-id") && headers.iter().any(|h| h == "cf-access-client-secret: invented-access-secret");
                let bearer = headers.iter().any(|h| h == "authorization: bearer invented-read-token");
                seen.lock().unwrap().push((method, path.clone(), headers));
                let (status, body) = if !access { ("302 Found", String::new()) }
                    else if !bearer || !accept_bearer { ("401 Unauthorized", json!({"error": "unauthorized"}).to_string()) }
                    else if path == "/v1/status" { ("200 OK", json!({"project": project, "jobs": {"by_phase": {"active": 1}}, "revisions": {"registry": "1:1"}}).to_string()) }
                    else if path == "/v1/comparisons" { ("200 OK", json!({"items": [{"id": "comparison-1", "use_case": "invented use case"}], "next": null}).to_string()) }
                    else if path == "/v1/comparisons/comparison-1" { ("200 OK", json!({"id": "comparison-1", "plan": {"constraints": {}}}).to_string()) }
                    else if path == "/v1/recommendations" { ("200 OK", json!({"records": []}).to_string()) }
                    else { ("200 OK", json!({"items": [], "next": null}).to_string()) };
                let location = if status.starts_with("302") { "Location: https://invented-team.example/login\r\n" } else { "" };
                let mut s = s;
                let _ = s.write_all(format!("HTTP/1.1 {status}\r\n{location}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
            }
        });
        (base, calls)
    }

    fn files(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dashq-assetq-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (file, value) in [("read", "invented-read-token"), ("id", "invented-access-id"), ("secret", "invented-access-secret")] { std::fs::write(dir.join(file), value).unwrap(); }
        dir
    }

    fn reader(base: String, dir: &std::path::Path, access: bool) -> (Config, Project) {
        let p = Project { id: "invented-project".into(), workspace: "invented-workspace".into(), read_token: dir.join("read") };
        (Config { base, access: access.then(|| (dir.join("id"), dir.join("secret"))), projects: vec![p.clone()] }, p)
    }

    /// D28/R3/R7: every read carries the Bearer token and the Access service token, and only ever uses GET. Risk: a
    /// read that cannot pass the guard (found on the live origin), or a mutation from a reader.
    #[test]
    fn assetq_reads_carry_both_credentials_and_only_get() {
        let dir = files("read");
        let (base, calls) = stub("invented-project", true);
        let (c, p) = reader(base, &dir, true);
        let (raw, _) = read(&c, &p).unwrap();
        let calls = calls.lock().unwrap();
        for path in ["/v1/status", "/v1/jobs", "/v1/methods", "/v1/objects", "/v1/comparisons", "/v1/comparisons/comparison-1", "/v1/recommendations"] {
            assert!(calls.iter().any(|x| x.1 == path), "{path} was not read");
        }
        for (method, path, headers) in calls.iter() {
            assert_eq!(method, "GET", "{path}");
            assert!(headers.iter().any(|h| h == "authorization: bearer invented-read-token"), "{path}");
            assert!(headers.iter().any(|h| h == "cf-access-client-id: invented-access-id"), "{path}");
            assert!(headers.iter().any(|h| h == "cf-access-client-secret: invented-access-secret"), "{path}");
        }
        let row = project(&p, &c.base, &json!({"value": raw, "at": source::now(), "error": null, "incomplete": false}), source::now());
        assert_eq!(row["source"]["state"], "ok", "{}", row["source"]);
        for secret in ["invented-read-token", "invented-access-id", "invented-access-secret"] { assert!(!row.to_string().contains(secret), "R7"); }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// D6/R2: each failure of the guard or credential reads with its own reason, never ok, zero or a credential.
    #[test]
    fn assetq_read_failures_name_their_reason() {
        let dir = files("fail");
        let reason = |project, accept, access: bool, read_token: Option<&str>| {
            let (base, _) = stub(project, accept);
            let (c, mut p) = reader(base, &dir, access);
            if let Some(f) = read_token { p.read_token = dir.join(f); }
            read(&c, &p).unwrap_err().0
        };
        assert_eq!(reason("invented-project", true, false, None), "redirected to a sign-in", "no Access token against the guard");
        assert_eq!(reason("invented-project", false, true, None), "credential refused");
        assert_eq!(reason("another-project", true, true, None), "credential names another project");
        assert_eq!(reason("invented-project", true, true, Some("missing")), "credential unreadable");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real AssetQ on loopback, or the deployed origin through Access: reads only. Ignored by default.
    #[test]
    #[ignore = "needs ASSETQ_BASE, ASSETQ_PROJECT and ASSETQ_READ_TOKEN_FILE; optional ASSETQ_ACCESS_ID_FILE and ASSETQ_ACCESS_SECRET_FILE"]
    fn assetq_live_read() {
        let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} unset"));
        let p = Project { id: env("ASSETQ_PROJECT"), workspace: "invented-workspace".into(), read_token: env("ASSETQ_READ_TOKEN_FILE").into() };
        let access = std::env::var("ASSETQ_ACCESS_ID_FILE").ok().map(|id| (PathBuf::from(id), PathBuf::from(env("ASSETQ_ACCESS_SECRET_FILE"))));
        let c = Config { base: env("ASSETQ_BASE"), access, projects: vec![p.clone()] };
        let (raw, _) = read(&c, &p).expect("read");
        let now = source::now();
        let row = project(&p, &c.base, &json!({"value": raw, "at": now, "error": null, "incomplete": false}), now);
        println!("state {} reason {}", row["source"]["state"], row["source"]["reason"]);
        for k in ["by_phase", "uncertain_submissions", "leased"] { println!("jobs.{k} {}", row["jobs"][k]); }
        println!("methods {} objects {} comparisons {} scopes {}", row["methods"].as_array().map_or(0, Vec::len), row["objects"].as_array().map_or(0, Vec::len), row["comparisons"].as_array().map_or(0, Vec::len), row["recommendations"].as_array().map_or(0, Vec::len));
        println!("backup {} restore {} integrity {}", row["backup"]["state"], row["restore"]["state"], row["integrity"]["state"]);
        println!("coverage {}", row["coverage"]);
        assert!(!row["revisions"].is_null());
    }
}
