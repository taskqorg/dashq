//! AssetQ controls (dashq.md D27, docs/design/assetq-controls.md). AssetQ owns jobs, methods, comparisons,
//! recommendations and assets. This module reads them with a GET-only credential, projects an allowlist of
//! metadata (R1) and renders one guarded view. No field passes through unless it is named here.

use crate::page::esc;
use crate::source::{self, Fail};
use serde_json::{Map, Value, json};
use std::fmt::Write;
use std::path::PathBuf;

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

fn job(v: &Value, origin: &str) -> Value {
    let mut o = pick(v, &["id", "method", "phase", "step", "steps", "effect", "provider_operation", "submissions", "cancel", "reserved_credits", "attempts", "created_at", "updated_at"]);
    o["operation"] = v["request"]["operation"].clone();
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
fn gate(w: &mut String, p: &Value, grant: &str) {
    let can = p["controls"][grant] == true;
    let reason = p["controls"]["reason"].as_str().unwrap_or("Actions unavailable.");
    let _ = write!(w, "<fieldset class=\"asset-gate\" data-assetq-can=\"{can}\" disabled>");
    if !can { let _ = write!(w, "<p class=\"note\">{}</p>", esc(reason)); }
}

/// `key` makes the form and its fields stable ids, so a refresh restores drafts and receipts (D10).
fn action_form(w: &mut String, p: &Value, grant: &str, key: &str, action: &str, revision: &Value, body: impl FnOnce(&mut String, &str)) {
    let id = format!("assetq-{}-{}", val(&p["project"]), esc(key));
    let _ = write!(w, "<form id=\"{id}\" class=\"asset-form\" data-assetq-action=\"{}\" data-project=\"{}\" data-revision=\"{}\">", esc(action), val(&p["project"]), val(revision));
    gate(w, p, grant);
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
            action_form(w, p, "jobs", &format!("cancel-{}", j["id"].as_str().unwrap_or("")), "job.cancel", &j["updated_at"], |w, _| { let _ = write!(w, "<input type=\"hidden\" name=\"job\" value=\"{}\"><button type=\"submit\">Cancel job</button>", val(&j["id"])); });
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
                let _ = write!(w, "<input type=\"hidden\" name=\"id\" value=\"{}\"><label>Status {}</label><label>Specification (JSON) <textarea id=\"{id}-spec\" name=\"spec\" rows=\"8\" spellcheck=\"false\">{}</textarea></label><p class=\"note\">Saving creates the next version. Running jobs keep their version. AssetQ validates the specification.</p><button type=\"submit\">Save new version</button>", val(&m["id"]), status_select(&format!("{id}-status"), &m["status"]), esc(&serde_json::to_string_pretty(&m["spec"]).unwrap_or_default()));
            });
            w.push_str("</details>");
        }
        w.push_str("</div>");
    }
    let _ = write!(w, "<details class=\"asset-action\" id=\"assetq-{0}-add\"><summary id=\"assetq-{0}-add-summary\">Add method</summary>", val(&p["project"]));
    action_form(w, p, "methods", "add-method", "method.save", &p["revisions"]["registry"], |w, id| {
        let _ = write!(w, "<label>Method ID <input id=\"{id}-id\" name=\"id\" maxlength=\"100\" required autocomplete=\"off\"></label><label>Status {}</label><label>Specification (JSON) <textarea id=\"{id}-spec\" name=\"spec\" rows=\"8\" spellcheck=\"false\" required></textarea></label><button type=\"submit\">Add method</button>", status_select(&format!("{id}-status"), &json!("experimental")));
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

    pub fn controls() -> Value { json!({"jobs": true, "methods": true, "reason": null}) }

    pub fn doc() -> Value {
        let snap = json!({"value": raw(), "at": NOW - 20, "error": null, "incomplete": false});
        build("https://assets.example.com", vec![project(&config_fixture(), "https://assets.example.com", &snap, NOW, controls())], NOW)
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
        let r = project(&config_fixture(), "https://assets.example.com", &snap, NOW, controls());
        assert_eq!(r["source"]["state"], "unconfirmed");
        assert_eq!(r["jobs"]["rows"].as_array().unwrap().len(), 2);
        // no read at all renders no empty panel of zeros
        let none = project(&config_fixture(), "https://assets.example.com", &json!({"value": null, "at": null, "error": "timeout"}), NOW, controls());
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
}
