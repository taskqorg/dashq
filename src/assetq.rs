//! AssetQ controls (dashq.md D27, docs/design/assetq-controls.md). AssetQ owns jobs, methods, comparisons,
//! recommendations and assets. This module reads them with a GET-only credential, projects an allowlist of
//! metadata (R1) and renders one guarded view. No field passes through unless it is named here.

use crate::enroll::{Enroll, Req, Resp};
use crate::page::esc;
use crate::source::{self, Fail};
use crate::store::{self, Store};
use rusqlite::{OptionalExtension, params};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

const FRESH_S: u64 = 180;
const TTL_S: u64 = 60;
/// Comparison details and recommendation scopes read per project on one refresh (views detail).
const DETAIL_CAP: usize = 10;
const SPEC_CAP: usize = 16 << 10;
const REASON_CAP: usize = 240;
const ACTIVE: [&str; 3] = ["queued", "active", "waiting"];

#[derive(Debug, Clone)]
pub struct Project {
    pub id: String,
    pub workspace: String,
    pub read_token: PathBuf,
    pub control_token: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub base: String,
    pub projects: Vec<Project>,
}

/// `assetq` config (D27); errors name the bad key, never its value (D2).
pub fn config(v: &Value) -> Result<Option<Config>, String> {
    let Some(a) = v.get("assetq") else { return Ok(None) };
    let base = a["base_url"].as_str().filter(|x| x.starts_with("https://") && !x.contains(['@', '"', '?', '#']) && x.len() > 8 && !x[8..].trim_end_matches('/').contains('/')).ok_or("config: bad key assetq.base_url")?;
    let rows = a["projects"].as_object().filter(|o| !o.is_empty()).ok_or("config: bad key assetq.projects")?;
    let path = |p: &Value, key: &str| p[key].as_str().filter(|x| std::path::Path::new(x).is_absolute()).map(PathBuf::from).ok_or(format!("config: bad key assetq.projects.{key}"));
    let mut projects = vec![];
    for (id, p) in rows {
        if !crate::enroll::id(id, 128) { return Err("config: bad key assetq.projects".into()); }
        let workspace = p["workspace"].as_str().filter(|w| crate::enroll::id(w, 64)).ok_or("config: bad key assetq.projects.workspace")?.to_string();
        let control_token = if p.get("control_token").is_some() { Some(path(p, "control_token")?) } else { None };
        projects.push(Project { id: id.clone(), workspace, read_token: path(p, "read_token")?, control_token });
    }
    Ok(Some(Config { base: base.trim_end_matches('/').to_string(), projects }))
}

/// One credential file: a single printable token, never logged. Its reason names the file role, never content.
pub fn token(path: &std::path::Path) -> Result<String, Fail> {
    std::fs::read_to_string(path).ok().map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty() && x.len() <= 512 && x.bytes().all(|b| b.is_ascii_graphic()))
        .ok_or(("credential unreadable".into(), None))
}

fn get(base: &str, path: &str, token: &str) -> Result<Value, String> {
    let r = source::get(&format!("{base}{path}"), &[format!("Authorization: Bearer {token}")]).map_err(|e| e.0)?;
    match r.status {
        200 => Ok(r.body),
        401 | 403 => Err("credential refused".into()),
        s => Err(format!("HTTP {s}")),
    }
}

/// The source read of one project (D4): `/v1/status` decides the read; each list keeps its own gap.
pub fn read(base: &str, p: &Project) -> Result<(Value, bool), Fail> {
    let token = token(&p.read_token)?;
    let status = get(base, "/v1/status", &token).map_err(|e| (e, None))?;
    if status["project"] != p.id.as_str() { return Err(("credential names another project".into(), None)); }
    if !status["revisions"].is_object() || !status["jobs"].is_object() { return Err(("unsupported status shape".into(), None)); }
    let mut raw = json!({"status": status, "jobs": {}, "details": {}, "recommendations": []});
    for phase in ACTIVE { raw["jobs"][phase] = list(get(base, &format!("/v1/jobs?phase={phase}&limit=100"), &token)); }
    raw["methods"] = list(get(base, "/v1/methods?limit=500", &token));
    raw["objects"] = list(get(base, "/v1/objects?limit=50", &token));
    raw["comparisons"] = list(get(base, "/v1/comparisons?limit=50", &token));
    let rows = raw["comparisons"]["items"].as_array().cloned().unwrap_or_default();
    let (mut scopes, mut skipped) = (vec![], false);
    for (i, c) in rows.iter().enumerate() {
        let Some(id) = c["id"].as_str() else { continue };
        if i >= DETAIL_CAP { skipped = true; continue }
        let detail = get(base, &format!("/v1/comparisons/{}", source::enc(id)), &token).unwrap_or_else(|e| json!({"read_error": e}));
        let scope = (c["use_case"].as_str().unwrap_or("").to_string(), detail["plan"]["constraints"].as_object().cloned().map_or(json!({}), Value::Object));
        if !scope.0.is_empty() && !scopes.contains(&scope) { if scopes.len() < DETAIL_CAP { scopes.push(scope) } else { skipped = true } }
        raw["details"][id] = detail;
    }
    raw["details_skipped"] = json!(skipped);
    for (use_case, constraints) in scopes {
        let q = format!("/v1/recommendations?use_case={}&constraints={}", source::enc(&use_case), source::enc(&constraints.to_string()));
        raw["recommendations"].as_array_mut().unwrap().push(get(base, &q, &token).unwrap_or_else(|e| json!({"use_case": use_case, "constraints": constraints, "read_error": e})));
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

/// What a person reviews before cancelling. `updated_at` is not it: AssetQ touches a job on every poll, so that
/// time would read stale for any job still running.
fn job_revision(j: &Value) -> String {
    format!("{}/{}/{}/{}", j["phase"].as_str().unwrap_or(""), j["step"], j["effect"].as_str().unwrap_or(""), j["cancel"].as_str().unwrap_or(""))
}

fn job(v: &Value, origin: &str) -> Value {
    let mut o = pick(v, &["id", "method", "phase", "step", "steps", "effect", "provider_operation", "submissions", "cancel", "reserved_credits", "attempts", "created_at", "updated_at"]);
    o["operation"] = v["request"]["operation"].clone();
    o["revision"] = json!(job_revision(v));
    o["usage"] = json!({"credits": v["usage"]["credits"], "unknown_reason": line(&v["usage"]["unknown_reason"])});
    o["reason"] = line(&v["reason"]);
    if let Some(id) = v["id"].as_str() {
        let id = source::enc(id);
        o["links"] = json!({"record": format!("{origin}/v1/jobs/{id}"), "result": format!("{origin}/v1/jobs/{id}/result")});
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

fn method(v: &Value) -> Value {
    let mut o = pick(v, &["id", "version", "status", "created_at"]);
    o["name"] = json!(format!("{}@{}", v["id"].as_str().unwrap_or(""), v["version"]));
    let spec = &v["spec"];
    for k in ["operation", "provider", "model", "api_revision", "pipeline_revision", "max_credits", "synthetic"] { o[k] = spec[k].clone(); }
    for k in ["inputs", "outputs", "capabilities", "limitations"] { o[k] = spec[k].clone(); }
    // The editable registry document; AssetQ validates a new version. An oversized one is linked, not copied.
    if spec.is_object() && spec.to_string().len() <= SPEC_CAP { o["spec"] = spec.clone(); } else { o["spec"] = Value::Null; o["spec_reason"] = json!("specification too large or unreadable"); }
    o["qualification"] = qualification(&v["qualification"]);
    o
}

fn object(v: &Value, origin: &str) -> Value {
    let mut o = pick(v, &["id", "sha256", "size", "content_type", "kind", "job_id", "synthetic", "acceptance", "retention_until", "created_at"]);
    o["provenance"] = pick(&v["provenance"], &["source", "method", "provider", "operation_id"]);
    if let Some(id) = v["id"].as_str() {
        let id = source::enc(id);
        o["links"] = json!({"record": format!("{origin}/v1/objects/{id}"), "download": format!("{origin}/v1/objects/{id}/content")});
    }
    o
}

fn comparison(row: &Value, detail: &Value, origin: &str) -> Value {
    let mut o = pick(row, &["id", "use_case", "dataset_version", "evaluator_version", "method_versions", "created_at"]);
    o["link"] = json!(row["id"].as_str().map(|id| format!("{origin}/v1/comparisons/{}", source::enc(id))));
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
pub fn project(p: &Project, origin: &str, snap: &Value, now: u64, controls: Value) -> Value {
    let raw = &snap["value"];
    let mut source = crate::data::source(snap, now, FRESH_S, None);
    let mut row = json!({"project": p.id, "workspace": p.workspace, "controls": controls});
    if raw.is_null() { row["source"] = source; return row }
    let st = &raw["status"];
    let mut gaps = vec![];
    let mut jobs = vec![];
    for phase in ACTIVE {
        let l = &raw["jobs"][phase];
        if let Some(e) = l["error"].as_str() { gaps.push(format!("{phase} jobs: {e}")); }
        if !l["next"].is_null() && l["error"].is_null() { gaps.push(format!("{phase} jobs capped")); }
        jobs.extend(l["items"].as_array().into_iter().flatten().map(|j| job(j, origin)));
    }
    let listed = |key: &str, what: &str, gaps: &mut Vec<String>| -> Vec<Value> {
        let l = &raw[key];
        if let Some(e) = l["error"].as_str() { gaps.push(format!("{what}: {e}")); }
        if !l["next"].is_null() && l["error"].is_null() { gaps.push(format!("{what} capped")); }
        l["items"].as_array().cloned().unwrap_or_default()
    };
    let methods: Vec<Value> = listed("methods", "methods", &mut gaps).iter().map(method).collect();
    let objects: Vec<Value> = listed("objects", "objects", &mut gaps).iter().map(|o| object(o, origin)).collect();
    let comparisons: Vec<Value> = listed("comparisons", "comparisons", &mut gaps).iter().filter(|c| !raw["details"][c["id"].as_str().unwrap_or("")].is_null()).map(|c| comparison(c, &raw["details"][c["id"].as_str().unwrap_or("")], origin)).collect();
    if raw["details_skipped"] == true { gaps.push(format!("only {DETAIL_CAP} comparison details read")); }
    if st["method_outcomes_truncated"] == true { gaps.push("method outcomes capped".into()); }
    if !gaps.is_empty() && source["state"] == "ok" {
        source["state"] = json!("incomplete");
        source["incomplete"] = json!(true);
    }
    if !gaps.is_empty() {
        let joined = gaps.join("; ");
        source["reason"] = json!(match source["reason"].as_str() { Some(r) => format!("{r}; {joined}"), None => joined });
    }
    row["source"] = source;
    row["revisions"] = pick(&st["revisions"], &["registry", "comparisons", "recommendations", "evaluations", "qualification"]);
    row["jobs"] = json!({"by_phase": st["jobs"]["by_phase"], "uncertain_submissions": st["jobs"]["uncertain_submissions"], "leased": st["jobs"]["leased"], "rows": jobs,
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

pub fn cached(cache: &source::Cache, base: &str, p: &Project) -> Value {
    cache.get(&format!("assetq:{}", p.id), TTL_S, || read(base, p))
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

fn state(v: &Value) -> String {
    let s = v["state"].as_str().unwrap_or("unknown");
    let class = match s { "ok" | "healthy" | "observed" | "verified" => "ok", "unknown" | "stale" | "unconfirmed" | "incomplete" => "unknown", _ => "warn" };
    let reason = v["reason"].as_str().map_or(String::new(), |r| format!(" · {}", esc(r)));
    let at = v["observed_at"].as_str().map_or(String::new(), |t| format!(" · observed {}", esc(t)));
    format!("<span class=\"s {class}\">{}</span>{reason}{at}", esc(s))
}

fn link(href: &Value, label: &str) -> String {
    href.as_str().filter(|h| h.starts_with("https://")).map_or(String::new(), |h| format!("<a href=\"{}\" rel=\"noreferrer\">{label}</a>", esc(h)))
}

/// A closed or denied form never reaches the server: `data-assetq-can` is the server's answer for this actor.
fn gated(w: &mut String, p: &Value, grant: &str) {
    let can = p["controls"][grant] == true;
    let reason = p["controls"]["reason"].as_str().unwrap_or("Actions unavailable.");
    let _ = write!(w, "<fieldset class=\"asset-gate\" data-assetq-can=\"{can}\" disabled>");
    if !can { let _ = write!(w, "<p class=\"note\">{}</p>", esc(reason)); }
}

/// `key` makes the form and its fields stable ids, so a refresh restores drafts and receipts (D10).
fn action_form(w: &mut String, p: &Value, grant: &str, key: &str, action: &str, revision: &Value, body: impl FnOnce(&mut String, &str)) {
    let id = format!("assetq-{}-{}", val(&p["project"]), esc(key));
    let _ = write!(w, "<form id=\"{id}\" class=\"asset-form\" data-assetq-action=\"{}\" data-project=\"{}\" data-revision=\"{}\">", esc(action), val(&p["project"]), val(revision));
    gated(w, p, grant);
    body(w, &id);
    w.push_str("</fieldset><output class=\"asset-receipt\" role=\"status\"></output></form>");
}

fn job_rows(w: &mut String, p: &Value) {
    let rows = p["jobs"]["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        let text = if p["source"]["state"] == "ok" { "No queued, active or waiting jobs." } else { "Active jobs unknown · see the source state above." };
        let _ = write!(w, "<p>{text}</p>");
    }
    for j in &rows {
        let steps = j["steps"].as_array().map_or(0, Vec::len);
        let step = j["step"].as_u64().map_or_else(|| "—".to_string(), |s| format!("{} of {steps}", s + 1));
        let usage = if j["usage"]["credits"].is_null() { format!("— · unknown: {}", j["usage"]["unknown_reason"].as_str().map_or("not reported".into(), esc)) } else { val(&j["usage"]["credits"]) };
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>{}</span><span>Phase {}</span><span>Step {step}</span><span>Effect {}</span><span>Cancellation {}</span><span>Credits {usage} · reserved {}</span><span>Updated {}</span>", val(&j["id"]), val(&j["method"]), val(&j["phase"]), val(&j["effect"]), val(&j["cancel"]), val(&j["reserved_credits"]), val(&j["updated_at"]));
        if !j["reason"].is_null() { let _ = write!(w, "<span>{}</span>", val(&j["reason"])); }
        let _ = write!(w, "<span>{} · {}</span>", link(&j["links"]["record"], "Record"), link(&j["links"]["result"], "Result"));
        // AssetQ decides whether a repeat changes anything; a requested cancellation offers nothing more here.
        if j["cancel"].is_null() || j["cancel"] == "none" {
            action_form(w, p, "jobs", &format!("cancel-{}", j["id"].as_str().unwrap_or("")), "job.cancel", &j["revision"], |w, _| { let _ = write!(w, "<input type=\"hidden\" name=\"job\" value=\"{}\"><button type=\"submit\">Cancel job</button>", val(&j["id"])); });
        }
        w.push_str("</div>");
    }
}

fn submit_form(w: &mut String, p: &Value) {
    let _ = write!(w, "<details class=\"asset-action\" id=\"assetq-{}-submit\"><summary id=\"assetq-{}-submit-summary\">New job</summary>", val(&p["project"]), val(&p["project"]));
    action_form(w, p, "jobs", "new-job", "job.submit", &p["revisions"]["registry"], |w, id| {
        let _ = write!(w, "<label>Method <select id=\"{id}-method\" name=\"method\" required>");
        for m in p["methods"].as_array().into_iter().flatten().filter(|m| m["status"] != "disabled") {
            let _ = write!(w, "<option value=\"{}\" data-operation=\"{}\">{} · {}</option>", val(&m["name"]), val(&m["operation"]), val(&m["name"]), val(&m["status"]));
        }
        let _ = write!(w, "</select></label><label>Text <textarea id=\"{id}-text\" name=\"text\" rows=\"3\" maxlength=\"65536\"></textarea></label><label>Attachment object IDs <input id=\"{id}-attachments\" name=\"attachments\" autocomplete=\"off\" placeholder=\"Space-separated, at most 16\"></label><label>Constraints (JSON) <textarea id=\"{id}-constraints\" name=\"constraints\" rows=\"2\">{{}}</textarea></label><label>Maximum credits <input id=\"{id}-max-credits\" name=\"max_credits\" type=\"number\" min=\"0\" required></label><p class=\"note\">AssetQ applies the method's limits and may refuse this method. Native and provider qualification stays AssetQ's.</p><button type=\"submit\">Submit job</button>");
    });
    w.push_str("</details>");
}

fn storage(w: &mut String, p: &Value) {
    let s = &p["store"];
    let f = &s["free_space"];
    let i = &s["physical_inventory"];
    let _ = write!(w, "<h4>Storage</h4><div class=\"asset-row\"><span>Store {} of {}{}</span><span>Inventory {}{}</span><span>Free space {} · floor {} · admission {}{}</span></div><div class=\"asset-row\"><span>Integrity {}</span><span>Backup {}</span><span>Restore {}</span></div>",
        bytes(&s["used_bytes"]), bytes(&s["quota_bytes"]), if s["full"] == true { " · full" } else { "" },
        bytes(&i["object_bytes"]), i["reason"].as_str().map_or(String::new(), |r| format!(" · unknown: {}", esc(r))),
        bytes(&f["available_bytes"]), bytes(&f["floor_bytes"]), val(&f["admission"]), f["reason"].as_str().map_or(String::new(), |r| format!(" · unknown: {}", esc(r))),
        state(&p["integrity"]), state(&p["backup"]), state(&p["restore"]));
}

fn methods(w: &mut String, p: &Value) {
    let rows = p["methods"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-methods\"><summary id=\"assetq-{0}-methods-summary\">Methods · {1} versions</summary>", val(&p["project"]), rows.len());
    w.push_str("<p class=\"note\">Registry status, qualification records and provider health are separate. A record names what it proves; it never enables dispatch.</p>");
    for m in &rows {
        let outcomes: Vec<String> = p["method_outcomes"].as_array().into_iter().flatten().filter(|o| o["method"] == m["name"]).map(|o| format!("{} {}", val(&o["phase"]), val(&o["jobs"]))).collect();
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Status {}</span><span>{}</span><span>Provider {}</span><span>Model {}</span><span>Maximum credits {}</span><span>Outcomes {}</span>", val(&m["name"]), val(&m["status"]), val(&m["operation"]), val(&m["provider"]), val(&m["model"]), val(&m["max_credits"]), if outcomes.is_empty() { "—".into() } else { outcomes.join(" · ") });
        if m["qualification"].is_object() {
            let q = &m["qualification"];
            let _ = write!(w, "<span>Qualification {} · general dispatch {} · {} · observed {}</span>", val(&q["scope"]), val(&q["general_dispatch"]), val(&q["reason"]), val(&q["observed_at"]));
        } else {
            w.push_str("<span>Qualification — · no retained receipt for this version</span>");
        }
        if m["spec"].is_object() {
            let _ = write!(w, "<details class=\"asset-action\" id=\"assetq-{0}-edit-{1}\"><summary id=\"assetq-{0}-edit-{1}-summary\">Edit as new version</summary>", val(&p["project"]), val(&m["name"]));
            action_form(w, p, "methods", &format!("save-{}", m["name"].as_str().unwrap_or("")), "method.save", &p["revisions"]["registry"], |w, id| {
                let _ = write!(w, "<input type=\"hidden\" name=\"method_id\" value=\"{}\"><label>Status {}</label><label>Specification (JSON) <textarea id=\"{id}-spec\" name=\"spec\" rows=\"8\" spellcheck=\"false\">{}</textarea></label><p class=\"note\">Saving creates the next version. Running jobs keep their version. AssetQ validates the specification.</p><button type=\"submit\">Save new version</button>", val(&m["id"]), status_select(&format!("{id}-status"), &m["status"]), esc(&serde_json::to_string_pretty(&m["spec"]).unwrap_or_default()));
            });
            w.push_str("</details>");
        }
        w.push_str("</div>");
    }
    let _ = write!(w, "<details class=\"asset-action\" id=\"assetq-{0}-add\"><summary id=\"assetq-{0}-add-summary\">Add method</summary>", val(&p["project"]));
    action_form(w, p, "methods", "add-method", "method.save", &p["revisions"]["registry"], |w, id| {
        let _ = write!(w, "<label>Method ID <input id=\"{id}-id\" name=\"method_id\" maxlength=\"100\" required autocomplete=\"off\"></label><label>Status {}</label><label>Specification (JSON) <textarea id=\"{id}-spec\" name=\"spec\" rows=\"8\" spellcheck=\"false\" required></textarea></label><button type=\"submit\">Add method</button>", status_select(&format!("{id}-status"), &json!("experimental")));
    });
    w.push_str("</details></details>");
}

fn status_select(id: &str, current: &Value) -> String {
    let opts: String = ["experimental", "recommended", "deprecated", "disabled"].iter().map(|s| format!("<option{}>{s}</option>", if current == s { " selected" } else { "" })).collect();
    format!("<select id=\"{id}\" name=\"status\">{opts}</select>")
}

fn comparisons(w: &mut String, p: &Value) {
    let rows = p["comparisons"].as_array().cloned().unwrap_or_default();
    let scopes = p["recommendations"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-comparisons\"><summary id=\"assetq-{0}-comparisons-summary\">Comparisons · {1} · recommendation scopes · {2}</summary>", val(&p["project"]), rows.len(), scopes.len());
    if rows.is_empty() { let _ = write!(w, "<p>{}</p>", if p["source"]["state"] == "ok" { "No comparisons." } else { "Comparisons unknown · see the source state above." }); }
    for c in &rows {
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Use case {}</span><span>{}</span><span>Dataset {} · evaluator {}</span><span>State {}{}</span><span>Failure rate {} · credits {} · judgments {}</span><span>{}</span></div>", val(&c["id"]), val(&c["use_case"]), val(&c["operation"]), val(&c["dataset_version"]), val(&c["evaluator_version"]), val(&c["state"]), c["reason"].as_str().map_or(String::new(), |r| format!(" · {}", esc(r))), val(&c["failure_rate"]), val(&c["credits"]), val(&c["human_judgments"]), link(&c["link"], "Record"));
        for m in c["method_metrics"].as_array().into_iter().flatten() {
            let _ = write!(w, "<div class=\"asset-row asset-sub\"><strong>{}</strong><span>Samples {} · terminal {} · failures {}</span><span>Failure rate {}</span><span>Credits {} · unknown {}</span><span>Duration {} ms</span><span>Human mean {} · missing {}</span></div>", val(&m["method"]), val(&m["samples"]), val(&m["terminal"]), val(&m["failures"]), val(&m["failure_rate"]), val(&m["credits"]), val(&m["usage_unknown"]), val(&m["duration_ms"]), val(&m["human_score_mean"]), val(&m["human_judgments_missing"]));
        }
        let missing: Vec<String> = c["missing_cases"].as_array().into_iter().flatten().map(|m| format!("{}: {}", val(&m["sample"]), val(&m["reason"]))).collect();
        if !missing.is_empty() { let _ = write!(w, "<p class=\"note\">Missing · {}</p>", missing.join(" · ")); }
        if c["winner_reason"].is_string() { let _ = write!(w, "<p class=\"note\">{}</p>", val(&c["winner_reason"])); }
    }
    let _ = write!(w, "<h4>Recommendations</h4><p class=\"note\">{}</p>", val(&p["recommendations_note"]));
    for s in &scopes {
        let current = if s["recommended"].is_object() { format!("Recommended {}", val(&s["recommended"]["method"])) } else { format!("None · {}", s["none_reason"].as_str().or(s["reason"].as_str()).map_or("unknown".into(), esc)) };
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>Constraints {}</span><span>{current}</span><span>Observed {}</span></div>", val(&s["use_case"]), val(&s["constraints"]), val(&s["observed_at"]));
        for r in s["records"].as_array().into_iter().flatten() {
            let keys: Vec<String> = r["evidence_keys"].as_array().into_iter().flatten().map(val).collect();
            let _ = write!(w, "<div class=\"asset-row asset-sub\"><strong>{}</strong><span>{} · {}</span><span>Method status {}</span><span>Evidence {} · observed {}</span>", val(&r["id"]), val(&r["method"]), val(&r["state"]), val(&r["method_status"]), if keys.is_empty() { "—".into() } else { keys.join(", ") }, val(&r["observed_at"]));
            if r["state"] == "proposed" {
                action_form(w, p, "methods", &format!("promote-{}", r["id"].as_str().unwrap_or("")), "recommendation.promote", &p["revisions"]["recommendations"], |w, _| {
                    let _ = write!(w, "<input type=\"hidden\" name=\"recommendation\" value=\"{}\"><input type=\"hidden\" name=\"use_case\" value=\"{}\"><input type=\"hidden\" name=\"constraints\" value=\"{}\"><button type=\"submit\">Promote for this scope</button>", val(&r["id"]), val(&s["use_case"]), esc(&s["constraints"].to_string()));
                });
            }
            w.push_str("</div>");
        }
    }
    w.push_str("</details>");
}

fn records(w: &mut String, p: &Value) {
    let rows = p["objects"].as_array().cloned().unwrap_or_default();
    let _ = write!(w, "<details class=\"asset-group\" id=\"assetq-{0}-records\"><summary id=\"assetq-{0}-records-summary\">Retained records · {1}</summary>", val(&p["project"]), rows.len());
    let _ = write!(w, "<p class=\"note\">Previews, search and asset detail are on <a href=\"{}\" rel=\"noreferrer\">the AssetQ site</a>.</p>", val(&p["origin"]));
    for o in &rows {
        let synthetic = if o["synthetic"] == true { " · synthetic" } else { "" };
        let _ = write!(w, "<div class=\"asset-row\"><strong>{}</strong><span>{}{synthetic}</span><span>{}</span><span>SHA-256 {}</span><span>Source {} · job {}</span><span>Owner acceptance {}</span><span>{} · {}</span></div>", val(&o["id"]), val(&o["content_type"]), bytes(&o["size"]), val(&o["sha256"]), val(&o["provenance"]["source"]), val(&o["job_id"]), val(&o["acceptance"]), link(&o["links"]["record"], "Record and provenance"), link(&o["links"]["download"], "Download"));
    }
    w.push_str("</details>");
}

pub fn render(w: &mut String, assets: &Value) {
    if !assets.is_object() { return }
    let origin = &assets["origin"];
    let _ = write!(w, "<section id=\"assets\"><h2>AssetQ</h2><p>AssetQ owns this state; each value keeps its source and time. <a href=\"{}\" rel=\"noreferrer\">Open the AssetQ site</a>.</p>", val(origin));
    let projects = assets["projects"].as_array().cloned().unwrap_or_default();
    if projects.is_empty() { let _ = write!(w, "<p>Unknown · {}.</p>", val(&assets["source"]["reason"])); }
    for mut p in projects {
        p["origin"] = origin.clone();
        let src = &p["source"];
        let age = src["age_s"].as_u64().map_or("age unknown".into(), |a| format!("{a} s old"));
        let _ = write!(w, "<article id=\"assetq-{0}\"><h3>{0}</h3><p data-assets-state>{1} · {age} · workspace {2}</p>", val(&p["project"]), state(src), val(&p["workspace"]));
        if p["revisions"].is_object() {
            let phases = p["jobs"]["by_phase"].as_object().map_or("—".to_string(), |o| ["queued", "active", "waiting", "succeeded", "failed", "cancelled"].iter().map(|k| format!("{k} {}", o.get(*k).map_or("0".into(), val))).collect::<Vec<_>>().join(" · "));
            let _ = write!(w, "<h4>Jobs</h4><p>{phases}</p><p>Uncertain submissions {} · leased {}. Rows list queued, active and waiting jobs.</p>", val(&p["jobs"]["uncertain_submissions"]), val(&p["jobs"]["leased"]));
            job_rows(w, &p);
            submit_form(w, &p);
            let providers: Vec<String> = p["providers"].as_array().into_iter().flatten().map(|x| format!("{} {}", val(&x["provider"]), state(x))).collect();
            let _ = write!(w, "<h4>Providers</h4><p>{}</p>", if providers.is_empty() { "Unknown · no provider observations.".to_string() } else { providers.join("<br>") });
            storage(w, &p);
            methods(w, &p);
            comparisons(w, &p);
            records(w, &p);
        } else {
            w.push_str("<p>No AssetQ observation yet.</p>");
        }
        w.push_str("</article>");
    }
    w.push_str("</section>");
}

// ---------------------------------------------------------------- actions (D27)

pub struct Ctx<'a> {
    pub enroll: &'a Result<Enroll, String>,
    pub cfg: Option<&'a Config>,
    pub public_host: &'a str,
    pub state: &'a State,
    pub cache: &'a source::Cache,
}

/// Per-process guards: the write rate per actor and one action at a time per project.
#[derive(Default)]
pub struct State {
    recent: Mutex<HashMap<String, Vec<i64>>>,
    projects: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

const PER_MINUTE: usize = 10;
const MAX_TEXT: usize = 65536;
const MAX_ATTACHMENTS: usize = 16;
const STATUSES: [&str; 4] = ["experimental", "recommended", "deprecated", "disabled"];
const TERMINAL: [&str; 3] = ["succeeded", "failed", "cancelled"];

/// One journal row (`assetq_ops`). It holds a digest, never the text, specification or constraints (R1).
pub struct Op {
    pub digest: Vec<u8>,
    pub state: String,
    pub receipt: String,
    pub baseline: Option<i64>,
}

impl Store {
    pub fn assetq_grant(&self, workspace: &str, subject: &str, grant: &str, on: bool) -> Result<(), store::Error> {
        self.write(|tx| {
            if on { tx.execute("INSERT OR IGNORE INTO assetq_grants VALUES (?1,?2,?3)", [workspace, subject, grant])?; }
            else { tx.execute("DELETE FROM assetq_grants WHERE workspace=?1 AND subject=?2 AND grant_name=?3", [workspace, subject, grant])?; }
            Ok(())
        })
    }

    /// The separate grant and the workspace administrator role; checked on every request, so revocation is immediate.
    pub fn assetq_can(&self, workspace: &str, subject: &str, grant: &str) -> Result<bool, store::Error> {
        Ok(self.lock()?.query_row("SELECT EXISTS(SELECT 1 FROM assetq_grants g JOIN members m USING(workspace,subject) WHERE g.workspace=?1 AND g.subject=?2 AND g.grant_name=?3 AND m.role='admin')", [workspace, subject, grant], |r| r.get(0))?)
    }

    pub fn assetq_op(&self, op_id: &str) -> Result<Option<Op>, store::Error> {
        Ok(self.lock()?.query_row("SELECT digest,state,receipt,baseline FROM assetq_ops WHERE op_id=?1", [op_id], |r| Ok(Op { digest: r.get(0)?, state: r.get(1)?, receipt: r.get(2)?, baseline: r.get(3)? })).optional()?)
    }

    // ponytail: rows stay, one small row per rate-limited action; prune with a retention rule if the file ever matters.
    fn assetq_record(&self, op_id: &str, project: &str, digest: &[u8], state: &str, receipt: &str, baseline: Option<i64>, now: i64) -> Result<(), store::Error> {
        self.write(|tx| {
            tx.execute("INSERT INTO assetq_ops VALUES (?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(op_id) DO UPDATE SET state=?4, receipt=?5, at=?7", params![op_id, project, digest, state, receipt, baseline, now])?;
            Ok(())
        })
    }
}

fn reply(status: &'static str, v: Value) -> Resp {
    Resp { status, ctype: "application/json", body: v.to_string().into_bytes(), headers: String::new(), forms: false }
}

/// A refusal before any effect or journal row: never reflected input.
fn refuse(status: &'static str, reason: &str) -> Resp {
    reply(status, json!({"state": "refused", "reason": reason}))
}

pub fn route(c: &Ctx, r: &Req, now: i64) -> Option<Resp> {
    if r.path != "/assetq/control" { return None }
    Some(match r.method {
        "GET" => capability(c, r, now),
        "POST" => post(c, r, now),
        _ => Resp { status: "405 Method Not Allowed", ctype: "text/plain", body: b"method not allowed".to_vec(), headers: String::new(), forms: false },
    })
}

fn purpose(subject: &str, project: &str, day: i64) -> String {
    format!("{subject}\nassetq/control\n{project}\n{day}")
}

fn actor<'a>(c: &'a Ctx, r: &'a Req) -> Result<(&'a Config, &'a Enroll, &'a str), Resp> {
    let (Some(cfg), Ok(e)) = (c.cfg, c.enroll) else { return Err(refuse("503 Service Unavailable", "unavailable")) };
    let Some(subject) = r.header(crate::enroll::SUBJECT).filter(|s| crate::enroll::valid_subject(s)) else { return Err(refuse("401 Unauthorized", "unauthorized")) };
    Ok((cfg, e, subject))
}

/// A foreign or unknown project reads the same (no oracle).
fn member<'a>(cfg: &'a Config, e: &Enroll, subject: &str, project: &str) -> Result<&'a Project, Resp> {
    let p = cfg.projects.iter().find(|p| p.id == project).ok_or_else(|| refuse("403 Forbidden", "authority"))?;
    let w = e.store().workspaces(subject).map_err(|_| refuse("503 Service Unavailable", "unavailable"))?;
    if w.iter().any(|(x, role)| *x == p.workspace && role == "admin") { Ok(p) } else { Err(refuse("403 Forbidden", "authority")) }
}

/// `GET ?project=P`: a CSRF token and capabilities for this actor and project only. Reading grants nothing.
fn capability(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (cfg, e, subject) = match actor(c, r) { Ok(x) => x, Err(x) => return x };
    let q = crate::enroll::form(r.query.as_bytes());
    let p = match member(cfg, e, subject, q.get("project").map_or("", String::as_str)) { Ok(p) => p, Err(x) => return x };
    let can = |g: &str| p.control_token.is_some() && e.store().assetq_can(&p.workspace, subject, g).unwrap_or(false);
    reply("200 OK", json!({"csrf": e.token_for(&purpose(subject, &p.id, now / 86400)), "jobs": can("jobs"), "methods": can("methods")}))
}

/// What this actor may do on one project (`/data.assets[].controls`). The server repeats the check on every request.
pub fn controls(p: &Project, e: &Result<Enroll, String>, subject: Option<&str>, source: &Value) -> Value {
    let off = |why: String| json!({"jobs": false, "methods": false, "reason": why});
    if p.control_token.is_none() { return off("AssetQ actions are not configured for this project.".into()) }
    let Some(subject) = subject else { return off("Sign in through the dashboard name to act.".into()) };
    if !matches!(source["state"].as_str(), Some("ok" | "incomplete")) {
        return off(format!("AssetQ read is {}; actions wait for a fresh read.", source["state"].as_str().unwrap_or("unknown")));
    }
    let Ok(e) = e else { return off("Grant store unavailable.".into()) };
    let can = |g: &str| e.store().assetq_can(&p.workspace, subject, g).unwrap_or(false);
    let (jobs, methods) = (can("jobs"), can("methods"));
    let missing: Vec<&str> = [("jobs", jobs), ("methods", methods)].into_iter().filter(|(_, x)| !x).map(|(n, _)| n).collect();
    json!({"jobs": jobs, "methods": methods, "reason": if missing.is_empty() { Value::Null } else { json!(format!("Needs the AssetQ {} grant in {}.", missing.join(" and "), p.workspace)) }})
}

fn limited(s: &State, actor: &str, now: i64) -> bool {
    let mut m = s.recent.lock().unwrap();
    let w = m.entry(actor.to_string()).or_default();
    w.retain(|t| now - *t < 60);
    if w.len() >= PER_MINUTE { return true }
    w.push(now);
    false
}

fn plain(v: &Value, max: usize) -> bool {
    v.as_str().is_some_and(|s| !s.is_empty() && s.len() <= max && !s.chars().any(char::is_control))
}

fn exact(v: &Value, keys: &[&str]) -> bool {
    v.as_object().is_some_and(|o| o.len() == keys.len() + 4 && keys.iter().chain(["project", "action", "op_id", "revision"].iter()).all(|k| o.contains_key(*k)))
}

/// The grant an action needs, after its strict shape check; `None` refuses it. AssetQ validates the meaning.
fn check_shape(v: &Value) -> Option<&'static str> {
    let oid = |x: &Value| x.as_str().is_some_and(|s| crate::enroll::id(s, 128));
    match v["action"].as_str()? {
        "job.submit" if exact(v, &["operation", "method", "text", "attachments", "constraints", "max_credits"])
            && plain(&v["operation"], 100) && v["method"].as_str().is_some_and(|m| m.contains('@') && plain(&v["method"], 140))
            && v["text"].as_str().is_some_and(|t| t.len() <= MAX_TEXT)
            && v["attachments"].as_array().is_some_and(|a| a.len() <= MAX_ATTACHMENTS && a.iter().all(oid))
            && v["constraints"].is_object() && v["max_credits"].is_u64() => Some("jobs"),
        "job.cancel" if exact(v, &["job"]) && oid(&v["job"]) => Some("jobs"),
        "method.save" if exact(v, &["id", "status", "spec"]) && plain(&v["id"], 100) && v["status"].as_str().is_some_and(|s| STATUSES.contains(&s)) && v["spec"].is_object() => Some("methods"),
        "recommendation.promote" if exact(v, &["recommendation", "use_case", "constraints"]) && oid(&v["recommendation"]) && plain(&v["use_case"], 500) && v["constraints"].is_object() => Some("methods"),
        _ => None,
    }
}

fn post(c: &Ctx, r: &Req, now: i64) -> Resp {
    let (cfg, e, subject) = match actor(c, r) { Ok(x) => x, Err(x) => return x };
    let origin = format!("https://{}", c.public_host);
    if r.header("origin") != Some(origin.as_str()) || !r.header("content-type").is_some_and(|t| t.starts_with("application/json")) {
        return refuse("401 Unauthorized", "unauthorized");
    }
    let Some(v) = crate::policy::strict(r.body).filter(Value::is_object) else { return refuse("400 Bad Request", "invalid") };
    let project = v["project"].as_str().unwrap_or("");
    let csrf = r.header("x-dashq-csrf").unwrap_or("");
    if ![now / 86400, now / 86400 - 1].iter().any(|d| e.token_ok(&purpose(subject, project, *d), csrf)) { return refuse("401 Unauthorized", "unauthorized") }
    if limited(c.state, subject, now) { return refuse("429 Too Many Requests", "busy") }
    let p = match member(cfg, e, subject, project) { Ok(p) => p, Err(x) => return x };
    let (Some(grant), Some(op_id), true) = (check_shape(&v), v["op_id"].as_str().filter(|o| crate::enroll::id(o, 64)), plain(&v["revision"], 200)) else { return refuse("400 Bad Request", "invalid") };
    match e.store().assetq_can(&p.workspace, subject, grant) {
        Ok(true) => {}
        Ok(false) => return refuse("403 Forbidden", "authority"),
        Err(_) => return refuse("503 Service Unavailable", "unavailable"),
    }
    let Some(control) = p.control_token.as_deref().and_then(|t| token(t).ok()) else { return refuse("503 Service Unavailable", "no_control_credential") };
    let Ok(read) = token(&p.read_token) else { return refuse("503 Service Unavailable", "no_read_credential") };
    // The digest binds actor and every field; a replay must repeat all of them.
    let digest = crate::enroll::sha256(&format!("{subject}\n{v}"));
    let lock = c.state.projects.lock().unwrap().entry(p.id.clone()).or_default().clone();
    let _one = lock.lock().unwrap();
    let t = Target { base: &cfg.base, read: &read, control: &control, project: &p.id, v: &v, op_id };
    let seen = match e.store().assetq_op(op_id) { Ok(x) => x, Err(_) => return refuse("503 Service Unavailable", "unavailable") };
    let (state, receipt, baseline) = match seen {
        Some(o) if o.digest != digest => return refuse("409 Conflict", "replay_mismatch"),
        Some(o) if o.state != "pending" && o.state != "unknown" => return answer(&o.state, serde_json::from_str(&o.receipt).unwrap_or_default()),
        Some(o) => { let (s, x) = reconcile(&t, o.baseline); (s, x, o.baseline) }
        None => match gate(&t) {
            Err(why) => return refuse("503 Service Unavailable", &why),
            Ok(Gate::Stale) => ("stale", json!({"reason": "revision_changed"}), None),
            Ok(Gate::Refused(why)) => ("refused", json!({"reason": why}), None),
            Ok(Gate::Go(baseline)) => {
                // Pending commits before the effect, so a crash reconciles instead of starting a new operation.
                if e.store().assetq_record(op_id, &p.id, &digest, "pending", "{}", baseline, now).is_err() { return refuse("503 Service Unavailable", "unavailable") }
                let (s, x) = dispatch(&t, baseline);
                (s, x, baseline)
            }
        },
    };
    let mut receipt = receipt;
    for (k, val) in [("op_id", json!(op_id)), ("project", json!(p.id)), ("action", v["action"].clone()), ("state", json!(state))] { receipt[k] = val; }
    // An unrecorded outcome keeps the row pending: the next delivery check reconciles it.
    let _ = e.store().assetq_record(op_id, &p.id, &digest, state, &receipt.to_string(), baseline, now);
    // The next page read and revision check see AssetQ's new state, not this cache.
    c.cache.forget(&format!("assetq:{}", p.id));
    answer(state, receipt)
}

fn answer(state: &str, receipt: Value) -> Resp {
    reply(match state { "accepted" => "200 OK", "stale" => "409 Conflict", "refused" => "422 Unprocessable Content", _ => "202 Accepted" }, receipt)
}

struct Target<'a> {
    base: &'a str,
    read: &'a str,
    control: &'a str,
    project: &'a str,
    v: &'a Value,
    op_id: &'a str,
}

enum Gate { Go(Option<i64>), Stale, Refused(&'static str) }

/// Fresh reads with the read credential, before anything is recorded or sent. `Err`: AssetQ unreadable, no effect.
fn gate(t: &Target) -> Result<Gate, String> {
    let v = t.v;
    let revision = v["revision"].as_str().unwrap_or("");
    let status = |key: &str| -> Result<bool, String> {
        let s = get(t.base, "/v1/status", t.read).map_err(|_| "assetq_unavailable".to_string())?;
        if s["project"] != t.project { return Err("assetq_unavailable".into()) }
        Ok(s["revisions"][key] == revision)
    };
    match v["action"].as_str().unwrap_or("") {
        "job.submit" => Ok(if status("registry")? { Gate::Go(None) } else { Gate::Stale }),
        "method.save" => {
            if !status("registry")? { return Ok(Gate::Stale) }
            let rows = methods_of(t).ok_or("assetq_unavailable")?;
            Ok(Gate::Go(Some(rows.iter().filter(|m| m["id"] == v["id"]).filter_map(|m| m["version"].as_i64()).max().unwrap_or(0))))
        }
        "job.cancel" => match get(t.base, &job_path(v), t.read) {
            Err(e) if e == "HTTP 404" => Ok(Gate::Refused("not_found")),
            Err(_) => Err("assetq_unavailable".into()),
            Ok(j) if job_revision(&j) != revision => Ok(Gate::Stale),
            Ok(j) if j["phase"].as_str().is_some_and(|p| TERMINAL.contains(&p)) => Ok(Gate::Refused("job_terminal")),
            Ok(_) => Ok(Gate::Go(None)),
        },
        "recommendation.promote" => {
            if !status("recommendations")? { return Ok(Gate::Stale) }
            match record_of(t).ok_or("assetq_unavailable")? {
                Some(r) if r["state"] == "proposed" => Ok(Gate::Go(None)),
                Some(_) => Ok(Gate::Refused("not_proposed")),
                None => Ok(Gate::Refused("not_found")),
            }
        }
        _ => Err("invalid".into()),
    }
}

fn job_path(v: &Value) -> String {
    format!("/v1/jobs/{}", source::enc(v["job"].as_str().unwrap_or("")))
}

fn methods_of(t: &Target) -> Option<Vec<Value>> {
    list(get(t.base, "/v1/methods?limit=500", t.read))["items"].as_array().cloned()
}

/// The record in its exact scope; the outer `None` is a failed read.
fn record_of(t: &Target) -> Option<Option<Value>> {
    let q = format!("/v1/recommendations?use_case={}&constraints={}", source::enc(t.v["use_case"].as_str().unwrap_or("")), source::enc(&t.v["constraints"].to_string()));
    let s = get(t.base, &q, t.read).ok()?;
    Some(s["records"].as_array()?.iter().find(|r| r["id"] == t.v["recommendation"]).cloned())
}

/// The one AssetQ write of an action, with the control credential.
fn dispatch(t: &Target, baseline: Option<i64>) -> (&'static str, Value) {
    let v = t.v;
    let (path, body) = match v["action"].as_str().unwrap_or("") {
        // The idempotency key is AssetQ's own guard against a second job for this operation.
        "job.submit" => ("/v1/jobs".to_string(), json!({"operation": v["operation"], "method": v["method"], "text": v["text"], "attachments": v["attachments"], "constraints": v["constraints"], "budget": {"max_credits": v["max_credits"]}, "idempotency_key": format!("dashq.assetq.{}", t.op_id)})),
        "job.cancel" => (format!("{}/cancel", job_path(v)), json!({})),
        "method.save" => ("/v1/methods".to_string(), json!({"id": v["id"], "status": v["status"], "spec": v["spec"]})),
        _ => (format!("/v1/recommendations/{}/promote", source::enc(v["recommendation"].as_str().unwrap_or(""))), json!({})),
    };
    let sent = source::send(&format!("{}{path}", t.base), &[format!("Authorization: Bearer {}", t.control), "Content-Type: application/json".into()], Some(&body.to_string()));
    let Ok(r) = sent else { return ("unknown", json!({"reason": "no_reply"})) };
    // AssetQ's own coded refusal is definite. Anything else, a proxy page included, leaves delivery unknown.
    if !(200..300).contains(&r.status) {
        return match (r.status, r.body["error"].as_str()) {
            (400..=499 | 503 | 507, Some(code)) => ("refused", json!({"reason": line(&json!(code))})),
            (s, _) => ("unknown", json!({"reason": format!("http_{s}")})),
        };
    }
    match v["action"].as_str().unwrap_or("") {
        "job.submit" => match r.body["job"]["id"].as_str() {
            Some(id) => match get(t.base, &format!("/v1/jobs/{}", source::enc(id)), t.read) {
                Ok(j) if j["id"] == id && j["method"] == v["method"] => ("accepted", json!({"job": id, "effect": j["phase"]})),
                Ok(_) => ("unknown", json!({"job": id, "reason": "readback_mismatch"})),
                Err(_) => ("unknown", json!({"job": id, "reason": "awaiting_readback"})),
            },
            None => ("unknown", json!({"reason": "unsupported_reply"})),
        },
        _ => reconcile(t, baseline),
    }
}

/// Reads only, except a submit, whose same idempotency key AssetQ resolves to the same job or refuses.
fn reconcile(t: &Target, baseline: Option<i64>) -> (&'static str, Value) {
    let v = t.v;
    let waiting = |what: &str| ("unknown", json!({"reason": what}));
    match v["action"].as_str().unwrap_or("") {
        "job.submit" => dispatch(t, baseline),
        "job.cancel" => match get(t.base, &job_path(v), t.read) {
            Ok(j) if j["cancel"].as_str().is_some_and(|c| c != "none") => ("accepted", json!({"job": v["job"], "effect": j["cancel"]})),
            Ok(j) if j["phase"].as_str().is_some_and(|p| TERMINAL.contains(&p)) => ("refused", json!({"job": v["job"], "reason": "job_terminal"})),
            Ok(_) => waiting("no_cancellation_visible"),
            Err(_) => waiting("awaiting_readback"),
        },
        "method.save" => match methods_of(t) {
            Some(rows) => {
                let base = baseline.unwrap_or(0);
                let mut mine: Vec<&Value> = rows.iter().filter(|m| m["id"] == v["id"] && m["version"].as_i64().is_some_and(|x| x > base) && m["spec"] == v["spec"] && m["status"] == v["status"]).collect();
                mine.sort_by_key(|m| m["version"].as_i64());
                match mine.first() {
                    Some(m) => ("accepted", json!({"method": format!("{}@{}", m["id"].as_str().unwrap_or(""), m["version"]), "effect": m["status"], "reason": if m["version"].as_i64() == Some(base + 1) { Value::Null } else { json!("registry_moved") }})),
                    None => waiting("no_new_version_visible"),
                }
            }
            None => waiting("awaiting_readback"),
        },
        _ => match record_of(t) {
            Some(Some(r)) if r["state"] == "promoted" => ("accepted", json!({"recommendation": v["recommendation"], "effect": "promoted"})),
            Some(_) => waiting("no_promotion_visible"),
            None => waiting("awaiting_readback"),
        },
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn config_fixture() -> Project {
        Project { id: "invented-project".into(), workspace: "invented-workspace".into(), read_token: "/invented/read".into(), control_token: Some("/invented/control".into()) }
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
            "jobs": {"queued": {"items": [], "next": null}, "active": {"items": [job("job-active", "active", "none", Value::Null)], "next": null}, "waiting": {"items": [job("job-waiting", "waiting", "requested", json!(1))], "next": null}},
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

    pub fn all_controls() -> Value { json!({"jobs": true, "methods": true, "reason": null}) }

    pub fn doc() -> Value {
        let snap = json!({"value": raw(), "at": NOW - 20, "error": null, "incomplete": false});
        build("https://assets.example.com", vec![project(&config_fixture(), "https://assets.example.com", &snap, NOW, all_controls())], NOW)
    }

    /// R1/D27: request text, dataset cases, free-form evidence and receipt fields never reach /data or the page.
    #[test]
    fn assetq_projection_drops_content() {
        let d = doc();
        let mut html = String::new();
        render(&mut html, &d);
        for leak in ["SECRET", "file:///"] {
            assert!(!d.to_string().contains(leak), "/data leaked {leak}");
            assert!(!html.contains(leak), "page leaked {leak}");
        }
        // A4: the real records stay one link away; this page copies none of their bytes.
        for record in ["/v1/objects/object-1/content", "/v1/objects/object-1\"", "/v1/jobs/job-active/result", "/v1/comparisons/comparison-1"] {
            assert!(html.contains(&format!("https://assets.example.com{record}")), "{record}");
        }
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
        let r = project(&config_fixture(), "https://assets.example.com", &snap, NOW, all_controls());
        assert_eq!(r["source"]["state"], "unconfirmed");
        assert_eq!(r["jobs"]["rows"].as_array().unwrap().len(), 2);
        // no read at all renders no empty panel of zeros
        let none = project(&config_fixture(), "https://assets.example.com", &json!({"value": null, "at": null, "error": "timeout"}), NOW, all_controls());
        assert_eq!(none["source"]["state"], "unknown");
        assert!(none["jobs"].is_null());
    }

    /// D27: an actor without a grant gets a disabled form that names the missing authority.
    #[test]
    fn assetq_denied_forms_name_the_gap() {
        let snap = json!({"value": raw(), "at": NOW - 20, "error": null, "incomplete": false});
        let denied = json!({"jobs": false, "methods": false, "reason": "Needs AssetQ grant in invented-workspace."});
        let d = build("https://assets.example.com", vec![project(&config_fixture(), "https://assets.example.com", &snap, NOW, denied)], NOW);
        let mut html = String::new();
        render(&mut html, &d);
        assert!(html.contains("data-assetq-can=\"false\" disabled") && html.contains("Needs AssetQ grant"));
        assert!(!html.contains("data-assetq-can=\"true\""));
    }

    #[test]
    fn assetq_config_names_keys_only() {
        assert!(config(&json!({})).unwrap().is_none());
        let abs = std::env::temp_dir().join("invented-read").display().to_string();
        let good = json!({"assetq": {"base_url": "https://assets.example.com/", "projects": {"p": {"workspace": "w", "read_token": abs}}}});
        let c = config(&good).unwrap().unwrap();
        assert_eq!(c.base, "https://assets.example.com");
        assert!(c.projects[0].control_token.is_none());
        for (bad, key) in [(json!({"assetq": {"base_url": "http://a.example", "projects": {"p": {"workspace": "w", "read_token": abs}}}}), "base_url"), (json!({"assetq": {"base_url": "https://a.example", "projects": {"p": {"workspace": "w", "read_token": "relative-secret"}}}}), "projects")] {
            let e = config(&bad).unwrap_err();
            assert!(e.contains(key) && !e.contains("relative-secret"), "{e}");
        }
    }

    /// Writes the review page used by tests/assets-browser.cjs.
    #[test]
    fn assetq_page_evidence() {
        let mut input: Value = serde_json::from_str(include_str!("../fixture/hosts.json")).unwrap();
        let mut doc_full = crate::data::build(&input["input"].take());
        doc_full["assets"] = doc();
        let html = crate::page::render_view(&doc_full, &json!({}), "/assets");
        assert!(html.contains("id=\"view-assets\"") && html.contains("id=\"assets\""));
        if let Ok(path) = std::env::var("DASHQ_ASSETS_EVIDENCE") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(std::path::Path::new(&path).join("page.html"), &html).unwrap();
        }
    }
    // ---------------------------------------------------------------- actions, over real HTTP to an in-process stub

    use crate::enroll::SUBJECT as HEADER;
    use std::io::{BufRead, BufReader, Read as _, Write as _};

    const ACTOR: &str = "github|invented-admin";
    const WS: &str = "invented-workspace";
    const PUBLIC: &str = "dash.example.com";
    const ACTIVE_JOB: &str = "active/1/submitted/none";

    /// Scripted AssetQ state; `calls` records method, path and bearer token. Only the shapes D27 reads are served.
    #[derive(Default)]
    struct Stub { registry: String, job_cancel: String, job_phase: String, rec_state: String, methods: Vec<Value>, calls: Vec<(String, String, String)>, jobs: HashMap<String, Value>, lose_next_reply: bool, refuse_next: Option<(u16, String)>, html_next: bool, last_body: Value }

    fn serve_stub(stub: Arc<Mutex<Stub>>) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", l.local_addr().unwrap());
        std::thread::spawn(move || {
            for s in l.incoming().flatten() {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let mut first = String::new();
                if r.read_line(&mut first).is_err() { continue }
                let (mut len, mut auth) = (0usize, String::new());
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).unwrap_or(0) == 0 || h.trim().is_empty() { break }
                    let lower = h.to_ascii_lowercase();
                    if let Some(x) = lower.strip_prefix("content-length:") { len = x.trim().parse().unwrap_or(0); }
                    if lower.starts_with("authorization:") { auth = h["authorization:".len()..].trim().trim_start_matches("Bearer ").to_string(); }
                }
                let mut body = vec![0; len];
                let _ = r.read_exact(&mut body);
                let mut parts = first.split_whitespace();
                let (method, target) = (parts.next().unwrap_or("").to_string(), parts.next().unwrap_or("").to_string());
                let (status, out) = stub_reply(&stub, &method, &target, &auth, &body);
                let mut s = s;
                if status == 0 { continue } // the reply is lost after the effect
                let reply = if status == 1 { "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string() } else { format!("HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}", out.len()) };
                let _ = s.write_all(reply.as_bytes());
            }
        });
        base
    }

    fn stub_reply(stub: &Mutex<Stub>, method: &str, target: &str, auth: &str, body: &[u8]) -> (u16, String) {
        let mut g = stub.lock().unwrap();
        let path = target.split('?').next().unwrap_or("").to_string();
        g.calls.push((method.into(), path.clone(), auth.into()));
        let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        if method == "POST" {
            g.last_body = body.clone();
            if g.html_next { g.html_next = false; return (1, String::new()) }
            if let Some((s, code)) = g.refuse_next.take() { return (s, json!({"error": code, "reason": "SECRET-REASON-TEXT"}).to_string()) }
        }
        let job = |g: &Stub| json!({"id": "job-1", "method": "synthetic.fake@1", "phase": g.job_phase, "step": 1, "effect": "submitted", "cancel": g.job_cancel, "updated_at": format!("{}", g.calls.len())});
        let (status, out) = match (method, path.as_str()) {
            ("GET", "/v1/status") => (200, json!({"project": "invented-project", "jobs": {}, "revisions": {"registry": g.registry, "recommendations": g.rec_state}})),
            ("GET", "/v1/methods") => (200, json!({"items": g.methods, "next": null})),
            ("GET", "/v1/jobs/job-1") => (200, job(&g)),
            ("GET", "/v1/recommendations") => (200, json!({"records": [{"id": "rec-1", "state": g.rec_state}]})),
            ("POST", "/v1/jobs") => {
                let key = body["idempotency_key"].as_str().unwrap_or("").to_string();
                let created = !g.jobs.contains_key(&key);
                let id = g.jobs.entry(key).or_insert_with(|| json!({"id": "job-new", "method": body["method"], "phase": "queued"})).clone();
                g.jobs.insert("job-new".into(), id.clone());
                (if created { 201 } else { 200 }, json!({"job": id, "created": created}))
            }
            ("GET", "/v1/jobs/job-new") => (200, g.jobs["job-new"].clone()),
            ("POST", "/v1/jobs/job-1/cancel") => { g.job_cancel = "requested".into(); (200, job(&g)) }
            ("POST", "/v1/methods") => {
                let version = g.methods.iter().filter(|m| m["id"] == body["id"]).filter_map(|m| m["version"].as_i64()).max().unwrap_or(0) + 1;
                let m = json!({"id": body["id"], "version": version, "status": body["status"], "spec": body["spec"]});
                g.methods.push(m.clone());
                g.registry = format!("registry-{version}");
                (201, m)
            }
            ("POST", "/v1/recommendations/rec-1/promote") => { g.rec_state = "promoted".into(); (200, json!({"id": "rec-1", "state": "promoted"})) }
            _ => (404, json!({"error": "no_route"})),
        };
        if method == "POST" && g.lose_next_reply { g.lose_next_reply = false; return (0, String::new()) }
        (status, out.to_string())
    }

    struct Rig { dir: PathBuf, e: Result<Enroll, String>, cfg: Config, state: State, cache: source::Cache, stub: Arc<Mutex<Stub>> }

    impl Drop for Rig { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.dir); } }

    fn rig(name: &str, granted: &[&str]) -> Rig {
        let dir = std::env::temp_dir().join(format!("dashq-assetq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("read"), "invented-read-token\n").unwrap();
        std::fs::write(dir.join("control"), "invented-control-token\n").unwrap();
        Enroll::admin(&dir, WS, ACTOR).unwrap();
        let e = Enroll::open(&dir, PUBLIC);
        let s = e.as_ref().unwrap().store();
        for g in granted { s.assetq_grant(WS, ACTOR, g, true).unwrap(); }
        let stub = Arc::new(Mutex::new(Stub { registry: "registry-1".into(), job_cancel: "none".into(), job_phase: "active".into(), rec_state: "proposed".into(),
            methods: vec![json!({"id": "invented.method", "version": 1, "status": "experimental", "spec": {"operation": "synthetic.generate"}})], ..Default::default() }));
        let base = serve_stub(stub.clone());
        let cfg = Config { base, projects: vec![Project { id: "invented-project".into(), workspace: WS.into(), read_token: dir.join("read"), control_token: Some(dir.join("control")) }] };
        Rig { dir, e, cfg, state: State::default(), cache: source::Cache::default(), stub }
    }

    impl Rig {
        fn ctx(&self) -> Ctx<'_> { Ctx { enroll: &self.e, cfg: Some(&self.cfg), public_host: PUBLIC, state: &self.state, cache: &self.cache } }
        fn csrf(&self) -> String { self.e.as_ref().unwrap().token_for(&purpose(ACTOR, "invented-project", NOW as i64 / 86400)) }
        fn call(&self, subject: Option<&str>, csrf: &str, body: &Value) -> (String, Value) {
            let mut h = vec![("origin".to_string(), format!("https://{PUBLIC}")), ("content-type".to_string(), "application/json".to_string()), ("x-dashq-csrf".to_string(), csrf.to_string())];
            if let Some(s) = subject { h.push((HEADER.to_string(), s.to_string())); }
            let bytes = body.to_string();
            let r = route(&self.ctx(), &Req { method: "POST", path: "/assetq/control", query: "", headers: &h, body: bytes.as_bytes() }, NOW as i64).unwrap();
            (r.status.into(), serde_json::from_slice(&r.body).unwrap())
        }
        fn post(&self, body: &Value) -> (String, Value) { self.call(Some(ACTOR), &self.csrf(), body) }
        fn writes(&self) -> Vec<(String, String, String)> { self.stub.lock().unwrap().calls.iter().filter(|c| c.0 == "POST").cloned().collect() }
    }

    fn submit(op: &str, revision: &str) -> Value {
        json!({"project": "invented-project", "action": "job.submit", "op_id": op, "revision": revision, "operation": "synthetic.generate", "method": "synthetic.fake@1", "text": "SECRET-PROMPT-TEXT", "attachments": [], "constraints": {}, "max_credits": 1})
    }
    fn cancel(op: &str, revision: &str) -> Value {
        json!({"project": "invented-project", "action": "job.cancel", "op_id": op, "revision": revision, "job": "job-1"})
    }
    fn save(op: &str, revision: &str) -> Value {
        json!({"project": "invented-project", "action": "method.save", "op_id": op, "revision": revision, "id": "invented.method", "status": "experimental", "spec": {"operation": "synthetic.generate", "note": "SECRET-SPEC-TEXT"}})
    }
    fn promote(op: &str, revision: &str) -> Value {
        json!({"project": "invented-project", "action": "recommendation.promote", "op_id": op, "revision": revision, "recommendation": "rec-1", "use_case": "invented use case", "constraints": {}})
    }

    /// A3: each allowed action reaches AssetQ once with the control credential; reads use only the read credential;
    /// the receipt follows readback. Defect guarded: a GET-only credential used to write, or a reply trusted unread.
    #[test]
    fn assetq_actions_dispatch_once_with_verified_receipts() {
        let t = rig("actions", &["jobs", "methods"]);
        let (s, r) = t.post(&submit("op-submit", "registry-1"));
        assert_eq!((s.as_str(), r["state"].as_str(), r["job"].as_str()), ("200 OK", Some("accepted"), Some("job-new")));
        assert_eq!(t.stub.lock().unwrap().last_body["idempotency_key"], "dashq.assetq.op-submit");
        assert_eq!(t.stub.lock().unwrap().last_body["budget"]["max_credits"], 1);
        let (_, r) = t.post(&cancel("op-cancel", ACTIVE_JOB));
        assert_eq!((r["state"].as_str(), r["effect"].as_str()), (Some("accepted"), Some("requested")));
        let (_, r) = t.post(&save("op-save", "registry-1"));
        assert_eq!((r["state"].as_str(), r["method"].as_str()), (Some("accepted"), Some("invented.method@2")));
        let (_, r) = t.post(&promote("op-promote", "proposed"));
        assert_eq!((r["state"].as_str(), r["effect"].as_str()), (Some("accepted"), Some("promoted")));
        let calls = t.stub.lock().unwrap().calls.clone();
        assert_eq!(calls.iter().filter(|c| c.0 == "POST").count(), 4);
        for (method, path, token) in &calls {
            assert_eq!(token, if method == "POST" { "invented-control-token" } else { "invented-read-token" }, "{method} {path}");
        }
        // R1: neither the receipt nor the journal holds the text or the specification
        for op in ["op-submit", "op-save"] {
            let o = t.e.as_ref().unwrap().store().assetq_op(op).unwrap().unwrap();
            assert!(!o.receipt.contains("SECRET"), "{}", o.receipt);
        }
    }

    /// A3: reading never grants a mutation. Each missing authority stops the action before AssetQ sees a write.
    #[test]
    fn assetq_authority_stays_separate() {
        let t = rig("authority", &["jobs"]);
        let body = save("op-1", "registry-1");
        assert_eq!(t.post(&body).0, "403 Forbidden", "jobs grant must not edit methods");
        assert_eq!(t.call(None, &t.csrf(), &submit("op-2", "registry-1")).0, "401 Unauthorized");
        assert_eq!(t.call(Some(ACTOR), "00", &submit("op-3", "registry-1")).0, "401 Unauthorized");
        let other = "github|invented-other";
        let store = t.e.as_ref().unwrap().store();
        // a workspace member without the administrator role holds no action, whatever the grant row says
        store.add_member(WS, other, "member").unwrap();
        store.assetq_grant(WS, other, "jobs", true).unwrap();
        let token = t.e.as_ref().unwrap().token_for(&purpose(other, "invented-project", NOW as i64 / 86400));
        assert_eq!(t.call(Some(other), &token, &submit("op-4", "registry-1")).0, "403 Forbidden");
        // revocation applies on the next request
        store.assetq_grant(WS, ACTOR, "jobs", false).unwrap();
        assert_eq!(t.post(&submit("op-5", "registry-1")).0, "403 Forbidden");
        // an extra field is refused, so a request cannot smuggle authority or configuration
        store.assetq_grant(WS, ACTOR, "jobs", true).unwrap();
        let mut smuggled = submit("op-6", "registry-1");
        smuggled["permissions"] = json!("owner");
        assert_eq!(t.post(&smuggled).0, "400 Bad Request");
        assert!(t.writes().is_empty(), "a refused action reached AssetQ: {:?}", t.writes());
        // no control credential: reads may work, every action stays off
        let mut off = t.cfg.clone();
        off.projects[0].control_token = None;
        let c = Ctx { enroll: &t.e, cfg: Some(&off), public_host: PUBLIC, state: &t.state, cache: &t.cache };
        let h = vec![("origin".to_string(), format!("https://{PUBLIC}")), ("content-type".to_string(), "application/json".to_string()), ("x-dashq-csrf".to_string(), t.csrf()), (HEADER.to_string(), ACTOR.to_string())];
        let b = submit("op-7", "registry-1").to_string();
        assert_eq!(route(&c, &Req { method: "POST", path: "/assetq/control", query: "", headers: &h, body: b.as_bytes() }, NOW as i64).unwrap().status, "503 Service Unavailable");
        assert!(t.writes().is_empty());
        let shown = controls(&off.projects[0], &t.e, Some(ACTOR), &json!({"state": "ok"}));
        assert_eq!((&shown["jobs"], &shown["methods"]), (&json!(false), &json!(false)));
    }

    /// A3: an exact revision. A moved registry, job or recommendation refuses before any write, and a stale read
    /// source gates the form. Defect guarded: a change nobody reviewed applied through an old draft.
    #[test]
    fn assetq_exact_revisions_refuse_stale_actions() {
        let t = rig("revisions", &["jobs", "methods"]);
        for body in [submit("a", "old"), cancel("b", "old"), save("c", "old"), promote("d", "old")] {
            let (s, r) = t.post(&body);
            assert_eq!((s.as_str(), r["state"].as_str(), r["reason"].as_str()), ("409 Conflict", Some("stale"), Some("revision_changed")), "{body}");
        }
        t.stub.lock().unwrap().job_phase = "succeeded".into();
        assert_eq!(t.post(&cancel("e", "succeeded/1/submitted/none")).1["reason"], "job_terminal");
        t.stub.lock().unwrap().rec_state = "promoted".into();
        assert_eq!(t.post(&promote("f", "promoted")).1["reason"], "not_proposed");
        assert!(t.writes().is_empty(), "{:?}", t.writes());
        // a stored receipt repeats without another read or write
        let reads = t.stub.lock().unwrap().calls.len();
        assert_eq!(t.post(&submit("a", "old")).1["state"], "stale");
        assert_eq!(t.stub.lock().unwrap().calls.len(), reads);
        assert_eq!(controls(&t.cfg.projects[0], &t.e, Some(ACTOR), &json!({"state": "stale"}))["jobs"], false);
        assert_eq!(controls(&t.cfg.projects[0], &t.e, Some(ACTOR), &json!({"state": "incomplete"}))["jobs"], true);
    }

    /// D27 / AssetQ R4: a lost reply reads unknown and never starts a second effect. Defect guarded: a retried POST
    /// that creates a second method version or a second job.
    #[test]
    fn assetq_unknown_delivery_reconciles_without_a_second_effect() {
        let t = rig("delivery", &["jobs", "methods"]);
        t.stub.lock().unwrap().lose_next_reply = true;
        let body = save("op-save", "registry-1");
        let (s, r) = t.post(&body);
        assert_eq!((s.as_str(), r["state"].as_str()), ("202 Accepted", Some("unknown")), "the effect happened; the reply was lost");
        // the row stays deduplicated: same ID and payload reads back, the registry having moved is no refusal
        let (_, r) = t.post(&body);
        assert_eq!((r["state"].as_str(), r["method"].as_str()), (Some("accepted"), Some("invented.method@2")));
        assert_eq!(t.writes().len(), 1, "method.save posted twice: {:?}", t.writes());
        let mut changed = body.clone();
        changed["status"] = json!("disabled");
        assert_eq!(t.post(&changed).0, "409 Conflict", "same ID, different payload");

        // a submit repeats the same AssetQ idempotency key, so one job exists
        t.stub.lock().unwrap().registry = "registry-2".into();
        t.stub.lock().unwrap().lose_next_reply = true;
        let job = submit("op-job", "registry-2");
        assert_eq!(t.post(&job).1["state"], "unknown");
        let (_, r) = t.post(&job);
        assert_eq!((r["state"].as_str(), r["job"].as_str()), (Some("accepted"), Some("job-new")));
        assert_eq!(t.stub.lock().unwrap().jobs.values().filter(|j| j["id"] == "job-new").count(), 2, "one job, one key plus its lookup row");
        assert_eq!(t.stub.lock().unwrap().jobs.len(), 2);

        // a cancel with nothing visible stays unknown and does not post again
        let before = t.writes().len();
        t.stub.lock().unwrap().html_next = true;
        let stop = cancel("op-stop", ACTIVE_JOB);
        assert_eq!(t.post(&stop).1["reason"], "http_502", "a proxy page is no AssetQ refusal");
        assert_eq!(t.post(&stop).1["reason"], "no_cancellation_visible");
        assert_eq!(t.writes().len(), before + 1, "cancel posted twice");
    }

    /// A3: AssetQ's role and limits still decide; its coded refusal is final and never reflects its text (R1).
    #[test]
    fn assetq_refusal_keeps_only_its_code() {
        let t = rig("refusal", &["methods"]);
        t.stub.lock().unwrap().refuse_next = Some((403, "forbidden".into()));
        let (s, r) = t.post(&save("op-1", "registry-1"));
        assert_eq!((s.as_str(), r["state"].as_str(), r["reason"].as_str()), ("422 Unprocessable Content", Some("refused"), Some("forbidden")));
        assert!(!r.to_string().contains("SECRET"));
        assert_eq!(t.post(&save("op-1", "registry-1")).1["reason"], "forbidden");
        assert_eq!(t.writes().len(), 1);
    }

    /// D2/R5: a closed route without config; a read needs the workspace administrator and hands out no capability.
    #[test]
    fn assetq_route_gates() {
        let t = rig("route", &["jobs"]);
        let get = |c: &Ctx, subject: Option<&str>, q: &str| {
            let h: Vec<(String, String)> = subject.map(|s| (HEADER.to_string(), s.to_string())).into_iter().collect();
            let r = route(c, &Req { method: "GET", path: "/assetq/control", query: q, headers: &h, body: b"" }, NOW as i64).unwrap();
            (r.status.to_string(), serde_json::from_slice::<Value>(&r.body).unwrap_or(Value::Null))
        };
        let (s, v) = get(&t.ctx(), Some(ACTOR), "project=invented-project");
        assert_eq!((s.as_str(), &v["jobs"], &v["methods"]), ("200 OK", &json!(true), &json!(false)));
        assert!(v["csrf"].is_string());
        assert_eq!(get(&t.ctx(), Some("github|invented-other"), "project=invented-project").0, "403 Forbidden");
        assert_eq!(get(&t.ctx(), Some(ACTOR), "project=foreign").0, "403 Forbidden");
        assert_eq!(get(&t.ctx(), None, "project=invented-project").0, "401 Unauthorized");
        let off = Ctx { enroll: &t.e, cfg: None, public_host: PUBLIC, state: &t.state, cache: &t.cache };
        assert_eq!(get(&off, Some(ACTOR), "project=invented-project").0, "503 Service Unavailable");
        assert!(route(&t.ctx(), &Req { method: "GET", path: "/other", query: "", headers: &[], body: b"" }, 0).is_none());
        assert_eq!(route(&t.ctx(), &Req { method: "PUT", path: "/assetq/control", query: "", headers: &[], body: b"" }, 0).unwrap().status, "405 Method Not Allowed");
    }

    /// A5: the same read and actions against a real local AssetQ service; run by docs/design/evidence/assetq/README.md.
    #[test]
    #[ignore = "needs a running AssetQ service, see docs/design/evidence/assetq/README.md"]
    fn assetq_live_local_service() {
        let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} required"));
        let dir = std::env::temp_dir().join(format!("dashq-assetq-live-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("read"), var("ASSETQ_MEMBER_TOKEN")).unwrap();
        std::fs::write(dir.join("control"), var("ASSETQ_OWNER_TOKEN")).unwrap();
        let project = var("ASSETQ_PROJECT");
        Enroll::admin(&dir, WS, ACTOR).unwrap();
        let e = Enroll::open(&dir, PUBLIC);
        let s = e.as_ref().unwrap().store();
        for g in ["jobs", "methods"] { s.assetq_grant(WS, ACTOR, g, true).unwrap(); }
        let cfg = Config { base: var("ASSETQ_BASE"), projects: vec![Project { id: project.clone(), workspace: WS.into(), read_token: dir.join("read"), control_token: Some(dir.join("control")) }] };
        let (state, cache) = (State::default(), source::Cache::default());
        let c = Ctx { enroll: &e, cfg: Some(&cfg), public_host: PUBLIC, state: &state, cache: &cache };
        let view = || project_row(&cfg, &cache, &e);
        // A repeated run is a new set of operations; AssetQ's idempotency key would otherwise return the old job.
        let run = source::now().to_string();
        let post = |mut b: Value| {
            b["project"] = json!(project);
            b["op_id"] = json!(format!("{}-{run}", b["op_id"].as_str().unwrap()));
            let h = vec![("origin".to_string(), format!("https://{PUBLIC}")), ("content-type".to_string(), "application/json".to_string()), ("x-dashq-csrf".to_string(), e.as_ref().unwrap().token_for(&purpose(ACTOR, &project, now_day()))), (HEADER.to_string(), ACTOR.to_string())];
            let body = b.to_string();
            let r = route(&c, &Req { method: "POST", path: "/assetq/control", query: "", headers: &h, body: body.as_bytes() }, source::now() as i64).unwrap();
            serde_json::from_slice::<Value>(&r.body).unwrap()
        };
        // read: a real status, a real registry with its qualification records, explicit unknowns
        let first = view();
        println!("read state: {} / {}", first["source"]["state"], first["source"]["reason"]);
        assert!(matches!(first["source"]["state"].as_str(), Some("ok" | "incomplete")), "{}", first["source"]);
        assert_eq!(first["backup"]["state"], "unknown");
        assert!(first["methods"].as_array().unwrap().iter().any(|m| m["id"] == "synthetic.fake"));
        assert!(first["methods"].as_array().unwrap().iter().any(|m| m["qualification"].is_object()), "real qualification records");
        let registry = first["revisions"]["registry"].as_str().unwrap().to_string();
        // a stale revision reaches no write
        assert_eq!(post(submit("live-stale", "old"))["state"], "stale");
        // submit: real durable admission, same job on repeat, no prompt text in /data
        let job = json!({"action": "job.submit", "op_id": "live-submit", "revision": registry, "operation": "synthetic.generate", "method": "synthetic.fake@1", "text": "SECRET-LIVE-PROMPT", "attachments": [], "constraints": {"polls": 100000}, "max_credits": 1});
        let r = post(job.clone());
        println!("submit: {r}");
        assert_eq!(r["state"], "accepted", "{r}");
        let id = r["job"].as_str().unwrap().to_string();
        assert_eq!(post(job)["job"], id.as_str());
        let after = view();
        assert!(!after.to_string().contains("SECRET-LIVE-PROMPT"), "prompt reached /data");
        assert!(after["jobs"]["rows"].as_array().unwrap().iter().any(|j| j["id"] == id.as_str()), "submitted job is listed");
        // cancel at the row's exact revision. A fresh job moves queued, active, waiting; a move reads stale until the
        // page shows it, so each try reads the row again like a person after a stale receipt.
        let mut r = Value::Null;
        for attempt in 0..8 {
            let shown = view();
            let row = shown["jobs"]["rows"].as_array().unwrap().iter().find(|j| j["id"] == id.as_str()).cloned().expect("job still listed");
            r = post(json!({"action": "job.cancel", "op_id": format!("live-cancel-{attempt}"), "revision": row["revision"], "job": id}));
            println!("cancel: {r}");
            if r["state"] != "stale" { break }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert_eq!(r["state"], "accepted", "{r}");
        // method edit creates the next version; old revision goes stale
        let revision = view()["revisions"]["registry"].as_str().unwrap().to_string();
        assert_eq!(revision, registry, "a job does not move the registry");
        let spec = first["methods"].as_array().unwrap().iter().find(|m| m["id"] == "synthetic.fake").unwrap()["spec"].clone();
        let r = post(json!({"action": "method.save", "op_id": "live-save", "revision": revision, "id": "synthetic.fake", "status": "experimental", "spec": spec}));
        println!("save: {r}");
        assert_eq!(r["state"], "accepted", "{r}");
        assert!(r["method"].as_str().unwrap().starts_with("synthetic.fake@"));
        let fresh = view();
        assert_ne!(fresh["revisions"]["registry"].as_str().unwrap(), revision);
        assert_eq!(post(json!({"action": "method.save", "op_id": "live-save-2", "revision": revision, "id": "synthetic.fake", "status": "experimental", "spec": {"operation": "x"}}))["state"], "stale");
        // a member grant cannot edit: AssetQ's role still decides
        std::fs::write(dir.join("control"), var("ASSETQ_MEMBER_TOKEN")).unwrap();
        let denied = post(json!({"action": "method.save", "op_id": "live-denied", "revision": fresh["revisions"]["registry"], "id": "synthetic.fake", "status": "experimental", "spec": fresh["methods"][0]["spec"]}));
        println!("member edit: {denied}");
        assert_eq!(denied["state"], "refused", "{denied}");
        std::fs::write(dir.join("control"), var("ASSETQ_OWNER_TOKEN")).unwrap();
        // a real comparison: incomplete evidence, per-method metrics, its scope's recommendation read, no winner, no case text
        let owner = |path: &str, body: Value| {
            for _ in 0..8 {
                let r = source::send(&format!("{}{path}", cfg.base), &[format!("Authorization: Bearer {}", var("ASSETQ_OWNER_TOKEN")), "Content-Type: application/json".into()], Some(&body.to_string())).unwrap();
                if r.body["error"] == "admission_busy" { std::thread::sleep(std::time::Duration::from_millis(500)); continue }
                assert!((200..300).contains(&r.status), "{path}: {}", r.body);
                return r.body;
            }
            panic!("{path}: admission stayed busy");
        };
        owner("/v1/datasets", json!({"id": "live-set", "version": 1, "document": {"operation": "synthetic.generate", "cases": [{"id": "case-1", "text": "SECRET-LIVE-CASE", "attachments": []}]}}));
        owner("/v1/evaluators", json!({"id": "live-eval", "version": 1, "document": {"decision_rule": "invented rule", "quality_rubric": "invented rubric"}}));
        let cmp = owner("/v1/comparisons", json!({"use_case": "dashq live check", "dataset_version": "live-set@1", "evaluator_version": "live-eval@1", "method_versions": ["synthetic.fake@1"], "plan": {"dataset": "live-set@1", "evaluator": "live-eval@1", "constraints": {}, "repeats": 1, "max_jobs": 1, "max_credits": 1}}));
        owner(&format!("/v1/comparisons/{}/run", cmp["id"].as_str().unwrap()), json!({}));
        let seen = view();
        assert!(!seen.to_string().contains("SECRET-LIVE"), "case or prompt text reached /data");
        let row = seen["comparisons"].as_array().unwrap().iter().find(|c| c["id"] == cmp["id"]).expect("comparison listed");
        println!("comparison: {}", json!({"state": row["state"], "missing": row["missing_cases"], "metrics": row["method_metrics"], "winner": row["winner"]}));
        assert_eq!(row["method_metrics"][0]["method"], "synthetic.fake@1");
        // the page renders these real replies: rows, records and no content
        let mut html = String::new();
        render(&mut html, &build(&cfg.base, vec![seen.clone()], source::now()));
        for shown in ["synthetic.fake@1", "image.worker.v1@5", "general dispatch refused", "Backup", "Save new version", "job not terminal"] {
            assert!(html.contains(shown), "{shown}");
        }
        assert!(!html.contains("SECRET-LIVE"), "content reached the page");
        assert!(row["winner"].is_null());
        assert_eq!(seen["recommendations"][0]["use_case"], "dashq live check");
        assert!(seen["recommendations"][0]["recommended"].is_null(), "no promoted route yet");
        // promotion of a proposed record in its exact scope, with an owner credential
        let rec = owner("/v1/recommendations", json!({"use_case": "dashq live check", "method": "synthetic.fake@1", "evidence": {"source": "local synthetic check"}, "observed_at": "2026-11-05T00:00:00Z"}));
        let revision = view()["revisions"]["recommendations"].as_str().unwrap().to_string();
        // AssetQ's `admission_busy` is a definite refusal of that operation while its executor holds admission; a person
        // sends a new one. Nothing here retries for them, so the check does.
        let mut r = Value::Null;
        for attempt in 0..8 {
            r = post(json!({"action": "recommendation.promote", "op_id": format!("live-promote-{attempt}"), "revision": revision, "recommendation": rec["id"], "use_case": "dashq live check", "constraints": {}}));
            println!("promote: {r}");
            if r["reason"] != "admission_busy" { break }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        assert_eq!(r["state"], "accepted", "{r}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn now_day() -> i64 { source::now() as i64 / 86400 }

    fn project_row(cfg: &Config, cache: &source::Cache, e: &Result<Enroll, String>) -> Value {
        let p = &cfg.projects[0];
        cache.forget(&format!("assetq:{}", p.id));
        let snap = cached(cache, &cfg.base, p);
        let _ = e;
        project(p, &cfg.base, &snap, source::now(), all_controls())
    }
}
