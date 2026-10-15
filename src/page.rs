//! The page (dashq.md D8): one server-side renderer of the `/data` document.

use crate::data::{AGENTS, PERIODS, dur};
use crate::source::{clock, enc};
use serde_json::{Value, json};
use std::fmt::Write;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => esc(s),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 { format!("{}", f as i64) } else { format!("{f:.4}").trim_end_matches('0').to_string() }
        }
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

/// A number or `—` with its unknown reason, never `0`; a capped count says so.
fn num(v: &Value, src: &Value) -> String {
    if v.is_null() {
        let why = src["reason"].as_str().unwrap_or("not measured");
        return format!("<span class=\"unknown\" title=\"unknown: {}\">—</span>", esc(why));
    }
    if src["incomplete"] == true { format!("≥{} (capped)", text(v)) } else { text(v) }
}

pub(crate) fn badge(state: &str, label: &str) -> String {
    let class = match state {
        "ok" => "ok",
        "warn" | "warning" | "stale" | "unconfirmed" | "incomplete" => "warn",
        "error" => "bad",
        _ => "unknown",
    };
    format!("<span class=\"s {class}\">{}</span>", esc(label))
}

fn at(v: &Value, now: u64) -> String {
    v.as_u64().map_or("—".into(), |a| format!("{} ({} ago)", clock(a, now), dur(now.saturating_sub(a))))
}

fn trace(id: &Value, base: &str) -> String {
    match id.as_str() {
        Some(t) if !base.is_empty() => format!(" <a href=\"{}/{}\">trace</a>", esc(base), esc(t)),
        _ => String::new(),
    }
}

fn unknown_line(src: &Value) -> String {
    match src["state"].as_str() {
        Some("ok") | None => String::new(),
        Some(s) => format!("<p class=\"note\">{} {}</p>", badge(s, s), esc(src["reason"].as_str().unwrap_or(""))),
    }
}

fn issue(i: &Value) -> String {
    format!("<a href=\"{}\">#{}</a> {}", esc(i["url"].as_str().unwrap_or("")), text(&i["number"]), text(&i["title"]))
}

/// `links`: `{traces, dashboards}` Langfuse project URLs.
pub fn render(doc: &Value, links: &Value) -> String {
    let now = doc["generated_at"].as_u64().unwrap_or(0);
    let base = links["traces"].as_str().unwrap_or("");
    let filters: String = ["host", "agent", "session"].iter().filter_map(|k| doc["filters"][*k].as_str().map(|v| format!("&{k}={}", enc(v)))).collect();
    let mut h = String::new();
    let w = &mut h;
    let _ = write!(
        w,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>dashq</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n<script src=\"/app.js\" defer></script>\n</head>\n<body>\n<header>\n<h1>dashq</h1>\n{}\n</header>\n<main id=\"main\">\n",
        if links["traces"].as_str().is_some_and(|s| !s.is_empty()) {
            format!("<nav><a href=\"{}\">Langfuse traces</a> · <a href=\"{}\">Langfuse dashboards</a></nav>", text(&links["traces"]), text(&links["dashboards"]))
        } else { String::new() }
    );
    let period = doc["period"].as_u64().unwrap_or(1800);
    let periods: Vec<String> = PERIODS
        .iter()
        .map(|p| {
            let name = match p {
                1800 => "30 min",
                7200 => "2 h",
                _ => "24 h",
            };
            if *p == period { format!("<strong>{name}</strong>") } else { format!("<a href=\"/?period={p}{}\">{name}</a>", esc(&filters)) }
        })
        .collect();
    let _ = writeln!(w, "<p class=\"bar\">Updated <time data-at=\"{now}\">{}</time> · Period {} <button id=\"refresh\" type=\"button\">Refresh</button></p>", clock(now, now), periods.join(" · "));
    let _ = writeln!(w, "<p id=\"banner\" class=\"banner\" role=\"alert\" hidden></p>");
    let not_ok: Vec<String> = doc["sources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, s)| s["state"] != "ok" && s["reason"] != "not configured")
        .map(|(n, s)| {
            let name = match n.as_str() {
                "langfuse" => "Langfuse",
                "github" => "GitHub",
                "enrollment" => "Connections",
                "taskq" => "taskq",
                "export" => "Export",
                "agent_work" => "Agent work",
                _ => "serverq",
            };
            format!("<li>{} {name}: {} · {}</li>", badge(s["state"].as_str().unwrap_or("unknown"), s["state"].as_str().unwrap_or("unknown")), esc(s["reason"].as_str().unwrap_or("")), s["age_s"].as_u64().map_or("age unknown".into(), |a| format!("<span data-age-s=\"{a}\">{} old</span>", dur(a))))
        })
        .collect();
    if !not_ok.is_empty() {
        let _ = writeln!(w, "<ul class=\"summary\">{}</ul>", not_ok.join(""));
    }

    let configured = |name: &str| doc["sources"][name].is_object() && doc["sources"][name]["reason"] != "not configured";
    let has_rows = |v: &Value| v.as_array().is_some_and(|a| !a.is_empty());
    let unconfigured: Vec<&str> = [("langfuse", "Langfuse"), ("github", "GitHub boards"), ("serverq", "ServerQ report")]
        .into_iter().filter(|(key, _)| !configured(key)).map(|(_, name)| name).collect();
    if !unconfigured.is_empty() {
        let _ = writeln!(w, "<p class=\"note\" id=\"source-context\">Not configured: {}. Independent installations can connect without these sources.</p>", unconfigured.join(" · "));
    }
    let first_launch = unconfigured.len() == 3 && !has_rows(&doc["installations"]["rows"]);
    if first_launch {
        let _ = writeln!(w, "<section id=\"startup\"><h2>Connect an existing installation</h2><p>Connect TaskQ, AgentQ or ServerQ using its existing service and identity.</p><a id=\"add-service\" href=\"/connect\">＋ Add service</a><p class=\"note\">Approval waits for credential activation, a durable first snapshot and dashboard readback. No observations received yet.</p></section></main></body></html>");
        return h;
    }

    // 1. Attention
    let att = &doc["attention"];
    let _ = writeln!(w, "<section id=\"attention\">\n<h2>Attention</h2>");
    for (key, title) in [("current", "Current problems"), ("gaps", "Observation gaps"), ("incidents", "Incident history (24 h, deduplicated)")] {
        let items = att[key].as_array().cloned().unwrap_or_default();
        // incident history is history, not a current problem: on demand (D10 last)
        // compact (D10): current problems stay open; gaps and history are counted and open on demand
        let n = if items.is_empty() { String::new() } else { format!(" ({})", items.len()) };
        let (open, close, end) = if key == "current" { ("<h3>".to_string(), "</h3>", "") } else { (format!("<details id=\"attention-{key}\"><summary>"), "</summary>", "</details>") };
        let _ = writeln!(w, "{open}{title}{n}{close}");
        if items.is_empty() {
            let none = if att["source"]["state"] == "ok" || key == "incidents" { "None observed." } else { "None observed; some sources are not ok (above)." };
            let _ = writeln!(w, "<p class=\"note\">{none}</p>{end}");
            continue;
        }
        let _ = writeln!(w, "<ul class=\"items\">");
        for i in items {
            let link = i["url"].as_str().map(|u| format!(" <a href=\"{}\">issue</a>", esc(u))).unwrap_or_default();
            let since = if i["since"].is_null() { " · in this period".into() } else { format!(" · since {}", at(&i["since"], now)) };
            let mut line = format!("<li><b>{}</b>{} · {}{since}", text(&i["source"]), i["host"].as_str().map(|x| format!(" · {}", esc(x))).unwrap_or_default(), text(&i["what"]));
            let source = match i["source"].as_str() { Some("serverq") => "serverq", Some("taskq") => if doc["sources"]["taskq"].is_object() { "taskq" } else { "github" }, _ => "langfuse" };
                let observation = if i["source"] == "taskq" {
                    doc["boards"].as_array().into_iter().flatten().find(|b| b["url"].as_str().is_some_and(|url| i["url"].as_str().is_some_and(|u| u.starts_with(&format!("{url}/")))))
                        .map_or(&doc["sources"][source], |b| &b["source"])
                } else { &doc["sources"][source] };
                if key == "current" && observation["state"] != "ok" {
                let _ = write!(line, " <span class=\"detail\">Last-known evidence · current condition unverified</span>");
            }
            if !i["occurrences"].is_null() {
                let _ = write!(line, " · {} occurrences", text(&i["occurrences"]));
            }
            if let Some(d) = i["detail"].as_str().filter(|d| !d.is_empty()) {
                let _ = write!(line, " <span class=\"detail\">— {}</span>", esc(d));
            }
            if let Some(nf) = i["native_failure"].as_object() {
                let fields: Vec<String> = nf.iter().map(|(k, v)| format!("{k} {}", if v.is_null() { "unknown".into() } else { text(v) })).collect();
                let _ = write!(line, "<details><summary>Last native failure receipt</summary><span class=\"detail\">At snapshot time, not a countdown: {}</span></details>", fields.join(" · "));
            }
            let _ = writeln!(w, "{line}{}{link}</li>", trace(&i["trace_id"], base));
        }
        let _ = writeln!(w, "</ul>{end}");
    }
    for b in doc["boards"].as_array().into_iter().flatten() {
        for q in b["questions"].as_array().into_iter().flatten() {
            let _ = writeln!(w, "<p><a data-board-question=\"{}\" data-task=\"{}\" href=\"#boards\">{} · #{} {}</a></p>", text(&b["project"]), text(&q["number"]), text(&b["project"]), text(&q["number"]), text(&q["title"]));
        }
    }
    let _ = writeln!(w, "</section>");

    first(w, doc, now);
    installations(w, &doc["installations"], &doc["resources"], now);
    integrations(w, &doc["installations"], doc["controls"]["agentq_configure"] == true);
    if configured("langfuse") || has_rows(&doc["resources"]["rows"]) {
        resources(w, &doc["resources"], now);
    }
    if configured("langfuse") { crate::optimization::render(w, doc); }

    // 2. Hosts
    let enrolled_checks = has_rows(&doc["checks"]["installations"]);
    if has_rows(&doc["hosts"]) && (configured("langfuse") || configured("serverq") || enrolled_checks) {
        let _ = writeln!(w, "<section id=\"hosts\">\n<h2>Hosts</h2>");
        let hosts = doc["hosts"].as_array().cloned().unwrap_or_default();
        if hosts.is_empty() {
            let _ = writeln!(w, "<p class=\"note\">No host seen in any source.</p>");
        } else {
            let _ = writeln!(w, "<table>\n<tr><th>Host</th><th>Freshness</th><th>taskq</th><th>agentq</th><th>agentq latest</th><th>serverq</th></tr>");
            for x in hosts {
                let (t, a, q) = (&x["taskq"], &x["agentq"], &x["serverq"]);
                let state = |o: &Value, label: String| -> String {
                    let st = o["state"].as_str().unwrap_or("unknown");
                    let mut out = badge(st, &label);
                    if let Some(r) = o["reason"].as_str() {
                        let _ = write!(out, " <span class=\"detail\">{}</span>", esc(r));
                    }
                    out
                };
                let unk = |v: &Value| if v.is_null() { "<span class=\"unknown\" title=\"unknown: not reported\">—</span>".to_string() } else { text(v) };
                let ver = |v: &Value, behind: &Value| format!("{}{}", unk(v), if behind == true { " <span class=\"s warn\">behind</span>" } else { "" });
                let age = |o: &Value, none: &str| o["age_s"].as_u64().map_or(none.into(), |a| format!("{} old", dur(a)));
                let fresh = state(&x["freshness"], age(&x["freshness"], "unknown"));
                let host = format!("{}{}", text(&x["id"]), x["reason"].as_str().map(|r| format!(" <span class=\"detail\">{}</span>", esc(r))).unwrap_or_default());
                let agentq = format!("{}{}", ver(&a["running"], &a["behind"]), a["installation"].as_str().map(|i| format!(" <span class=\"detail\">{}</span>", esc(i))).unwrap_or_default());
                let taskq = format!("{} {}", ver(&t["version"], &t["behind"]), state(t, t["age_s"].as_u64().map_or("unknown".into(), |a| format!("{} old", dur(a)))));
                // green only for a fresh report whose checks are all ok
                let q_state = if q["state"] == "ok" { json!({"state": match q["status"].as_str() { Some("ok") => "ok", Some("warn") => "warn", _ => "unknown" }}) } else { q.clone() };
                let serverq = state(&q_state, format!("{}{}", q["status"].as_str().unwrap_or("unknown"), q["age_s"].as_u64().map(|a| format!(", {} old", dur(a))).unwrap_or_default()));
                let _ = writeln!(w, "<tr><td>{host}</td><td>{fresh}</td><td>{taskq}</td><td>{agentq}</td><td>{}</td><td>{serverq}</td></tr>", unk(&a["latest"]));
            }
            let _ = writeln!(w, "</table>");
        }
        let _ = writeln!(w, "</section>");
    }

    versions(w, &doc["versions"], now);

    // 4. Agents
    if configured("langfuse") {
        let ag = &doc["agents"];
        let src = &ag["source"];
        let name = |a: &Value| AGENTS.iter().find(|(k, _)| a == *k).map_or(text(a), |(_, n)| n.to_string());
        let _ = writeln!(w, "<section id=\"agents\">\n<h2>Agents</h2>\n{}", unknown_line(src));
        let capped = |k: &str| -> Value { json!({"incomplete": ag["incomplete"][k], "reason": src["reason"]}) };
        let _ = writeln!(w, "<h3>agentq fleet (one row per installation)</h3>\n<table>\n<tr><th>Host</th><th>Installation</th><th>Version</th><th>Last seen</th><th>Heartbeat</th><th>Runs (non-zero exits)</th><th>Retrievals (failed)</th></tr>");
        let (cn, cv) = (capped("agentq.run"), capped("agentq.retrieval"));
        for x in ag["fleet"].as_array().into_iter().flatten() {
            let hb = format!("{}{}", badge(x["state"].as_str().unwrap_or("unknown"), x["state"].as_str().unwrap_or("unknown")), x["reason"].as_str().map(|r| format!(" <span class=\"detail\">{}</span>", esc(r))).unwrap_or_default());
            let ver = format!("{}{}", if x["version"].is_null() { "—".into() } else { text(&x["version"]) }, if x["behind"] == true { " <span class=\"s warn\">behind</span>" } else { "" });
            let _ = writeln!(
                w,
                "<tr><td>{}</td><td>{}</td><td>{ver}</td><td>{}{}</td><td>{hb}</td><td>{} ({} of {})</td><td>{} ({})</td></tr>",
                text(&x["host"]), text(&x["installation"]), at(&x["last_seen"], now), trace(&x["trace_id"], base),
                num(&x["runs"], &cn), num(&x["nonzero_exits"], &cn), text(&x["exit_samples"]), num(&x["retrievals"], &cv), num(&x["failed_retrievals"], &cv)
            );
        }
        let _ = writeln!(w, "</table>");
        clients(w, &ag["clients"], &json!({"incomplete": ag["clients"]["incomplete"]["agentq.compress.24h"], "reason": src["reason"]}),
            &json!({"incomplete": ag["clients"]["incomplete"]["agentq.retrieval.24h"], "reason": src["reason"]}), now);
        let _ = writeln!(w, "<h3>Model activity</h3>\n<table>\n<tr><th>Host</th><th>Agent</th><th>Calls</th><th>Errors</th><th>Sessions</th><th>Last call</th></tr>");
        let ca = capped("activity");
        for a in ag["activity"].as_array().into_iter().flatten() {
            let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}{}</td></tr>", text(&a["host"]), name(&a["agent"]), num(&a["calls"], &ca), num(&a["errors"], &ca), num(&a["sessions"], &ca), at(&a["last_at"], now), trace(&a["trace_id"], base));
        }
        let _ = writeln!(w, "</table>\n<h3>Model usage (sum and samples per field; fields are not added together)</h3>\n<table>\n<tr><th>Host</th><th>Agent</th><th>Model</th><th>Calls</th><th>Usage</th><th>Reported cost</th></tr>");
        for a in ag["activity"].as_array().into_iter().flatten() {
            for m in a["models"].as_array().into_iter().flatten() {
                let usage: Vec<String> = m["usage"].as_object().into_iter().flatten().map(|(k, u)| format!("{k} {} ({} samples)", text(&u["sum"]), text(&u["samples"]))).collect();
                let cost = if m["cost"].is_null() { "<span class=\"unknown\" title=\"unknown: not reported\">—</span>".into() } else { format!("{} ({} samples)", text(&m["cost"]), text(&m["cost_samples"])) };
                let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{cost}</td></tr>", text(&a["host"]), name(&a["agent"]), text(&m["model"]), num(&m["calls"], &ca), if usage.is_empty() { "—".into() } else { usage.join("<br>") });
            }
        }
        let _ = writeln!(w, "</table>\n</section>");

    // 5. Server checks
    }
    crate::agent_work::render(w, doc);
    if configured("serverq") || enrolled_checks {
        let _ = writeln!(w, "<section id=\"checks\">\n<h2>Server checks</h2>");
        if configured("serverq") {
            checks(w, &doc["checks"], "Report", now);
        }
        // one block per enrolled installation (serverq.md § 3 projection); equal host labels stay apart
        for c in doc["checks"]["installations"].as_array().into_iter().flatten() {
            let version = c["version"].as_str().map(|v| format!(" {}", esc(v))).unwrap_or_default();
            let _ = writeln!(w, "<h3>{} · ServerQ{version} · <code>{}</code></h3>", text(&c["host"]), text(&c["installation"]));
            checks(w, c, "Observed", now);
        }
        let _ = writeln!(w, "</section>");
    }
    let _ = writeln!(w, "</main>\n</body>\n</html>");
    h
}

/// D18 V1–V4: render only the safe version projection; unavailable producer receipts stay unknown.
/// D8.4 per-client rows (#131): routing and hooks from the heartbeat, MCP from enrollment, use over 24 h.
fn clients(w: &mut String, c: &Value, cc: &Value, cr: &Value, now: u64) {
    let none = |why: &Value| format!("<span class=\"unknown\" title=\"unknown: {}\">—</span>", esc(why.as_str().unwrap_or("not reported")));
    let _ = writeln!(w, "<h3>agentq clients (use over 24 h)</h3>\n<table>\n<tr><th>Host</th><th>Installation</th><th>Client</th><th>Routing</th><th>Hooks (last failure)</th><th>MCP</th><th>Hook compress (compressed · bytes saved)</th><th>Compress records</th><th>Retrievals (installation)</th></tr>");
    for r in c["rows"].as_array().into_iter().flatten() {
        let client = AGENTS.iter().find(|(k, _)| r["client"] == *k).map_or_else(|| none(&r["use_reason"]), |(_, n)| n.to_string());
        let fresh = r["heartbeat"]["state"] == "ok";
        let stale = if fresh { String::new() } else { " <span class=\"detail\">last known</span>".into() };
        // a stale or unconfirmed heartbeat never shows a current badge (D6)
        let last = |s: &str| if fresh { s.to_string() } else { format!("Last known {s}") };
        let rt = &r["routing"];
        let routing = if rt.is_null() {
            none(&r["routing_reason"])
        } else {
            let s = rt["state"].as_str().unwrap_or("unknown");
            let class = match s { _ if !fresh => "unconfirmed", "routed" => "ok", "direct" | "bypassed" => "warn", _ => "unknown" };
            format!("{}{}", badge(class, &last(s)), rt["reason"].as_str().map(|x| format!(" <span class=\"detail\">{}</span>", esc(x))).unwrap_or_default())
        };
        let hooks = match r["hooks"].as_array() {
            None => none(&r["hooks_reason"]),
            Some(h) if h.is_empty() => format!("no hook runs{}", r["hooks_configured"].as_str().map(|x| format!(" · {}", esc(x))).unwrap_or_default()),
            Some(h) => h.iter().map(|h| {
                let s = h["state"].as_str().unwrap_or("unknown");
                let f = &h["last_failure"];
                let fail = if f.is_null() { String::new() } else { format!(" <span class=\"detail\">failed {}: {}</span>", at(&f["at"], now), f["error"].as_str().map_or("reason withheld".into(), esc)) };
                format!("{} {}{fail}", text(&h["event"]), badge(if !fresh { "unconfirmed" } else if s == "failed" { "error" } else { s }, &last(s)))
            }).collect::<Vec<_>>().join("<br>"),
        };
        let mcp = if r["mcp"].is_null() { none(&r["mcp_reason"]) } else { text(&r["mcp"]) };
        let u = &r["use"];
        let (hook, all, ret) = if u.is_null() {
            (none(&r["use_reason"]), none(&r["use_reason"]), none(&r["use_reason"]))
        } else {
            let saved = if u["bytes_saved"].is_null() { format!("<span class=\"unknown\" title=\"unknown: {} of {} compressed calls measured\">—</span>", text(&u["bytes_samples"]), text(&u["hook_compressed"])) } else { format!("{} B", text(&u["bytes_saved"])) };
            (format!("{} of {} · {saved}", num(&u["hook_compressed"], cc), num(&u["hook_compress"], cc)), format!("{}{}", num(&u["compress"], cc), if u["unattributed"] != 0 { format!(" (+{} without agent)", text(&u["unattributed"])) } else { String::new() }), num(&u["retrievals"], cr))
        };
        let _ = writeln!(w, "<tr><td>{}</td><td>{}{stale}</td><td>{client}</td><td>{routing}</td><td>{hooks}</td><td>{mcp}</td><td>{hook}</td><td>{all}</td><td>{ret}</td></tr>", text(&r["host"]), text(&r["installation"]));
    }
    let _ = writeln!(w, "</table>");
}

fn versions(w: &mut String, versions: &Value, now: u64) {
    let rows = versions["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() { return; }
    // Known installations keep identity across host renames; legacy rows without an ID use host within source/workspace.
    let id = |r: &Value| format!("version-{}", enc(&json!([r["product"], r["provenance"], r["installation"], r["workspace"], if r["installation"].is_null() { &r["host"] } else { &Value::Null }]).to_string()));
    let value = |v: &Value| if v.is_null() { "Not reported".into() } else { text(v) };
    let version = |v: &Value| format!("{}{}", v["version"].as_str().map_or_else(|| format!("<span class=\"muted\" title=\"Unknown: {}\">—</span>", esc(v["reason"].as_str().unwrap_or("not reported"))), esc), if v["last_known"] == true { "<small>Last known</small>" } else { "" });
    let _ = writeln!(w, "<section id=\"qversions\"><div class=\"app\"><div class=\"line spread\"><h2>Services</h2><small>Versions · updates · diagnostics</small></div><small>Host-managed updates · observation only</small><div class=\"inventory\"><div class=\"row rowhead\"><span>Service / installation</span><span>Running</span><span>Selected</span><span>Available</span><span>State / observation</span></div>");
    for (i, r) in rows.iter().enumerate() {
        let key = id(r);
        let obs = &r["observation"];
        let update = match r["update"]["state"].as_str() { Some("available") => "Update available", Some("current") => "Version current", Some("error") => "Update error reported", _ => "Update unknown" };
        let _ = writeln!(w, "<button type=\"button\" class=\"row version-select\" id=\"select-{key}\" aria-controls=\"{key}\" aria-pressed=\"{}\" aria-expanded=\"{}\"><span class=\"identity\"><strong>{}</strong><small>{}</small><small>Workspace: {}</small>{}</span><span><span class=\"label\">Running</span>{}</span><span><span class=\"label\">Selected</span>{}</span><span><span class=\"label\">Available</span>{}<small>Checked {}</small><small>{}</small></span><span class=\"health muted\">{update}<small>{} · {}</small><small>Current health unknown</small></span></button>", i == 0, i == 0, text(&r["product"]), text(&r["host"]), value(&r["workspace"]), r["reason"].as_str().map(|s| format!("<small>{}</small>", esc(s))).unwrap_or_default(), version(&r["running"]), version(&r["selected"]), version(&r["release"]), at(&r["release"]["at"], now), text(&r["release"]["state"]), text(&obs["state"]), at(&obs["at"], now));
    }
    let _ = writeln!(w, "</div>");
    for (i, r) in rows.iter().enumerate() {
        let key = id(r);
        let _ = writeln!(w, "<section class=\"detail version-body\" id=\"{key}\" aria-labelledby=\"select-{key}\"{}><h3>{} · {}</h3><p class=\"notice\">Current health unknown: {}. Service health does not verify every integration or workload.</p><div class=\"columns\"><section><h3>Update</h3><ol class=\"steps\">", if i == 0 { "" } else { " hidden" }, text(&r["product"]), text(&r["host"]), text(&r["health"]["reason"]));
        for (label, v) in [("Release check", &r["release"]), ("Version comparison", &r["update"]), ("Lifecycle", &r["lifecycle"])] {
            let _ = writeln!(w, "<li><span aria-hidden=\"true\">{}</span><span>{label}: {}{}</span></li>", if v["state"] == "error" { "!" } else { "—" }, text(&v["state"]), v["reason"].as_str().map(|s| format!(" · {}", esc(s))).unwrap_or_default());
        }
        let _ = writeln!(w, "</ol></section><section><h3>Diagnostics</h3>");
        let engines = r["libraries"]["engines"].as_object().map(|e| e.iter().map(|(k,v)| format!("{} {}", esc(k), value(v))).collect::<Vec<_>>().join(" · ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "Not reported".into());
        for (label, v) in [("Health receipt", "Not reported".into()), ("Safe reason code", "Not reported".into()), ("Queue oldest age at observation", r["queue_age_s"].as_u64().map_or("Not reported".into(), dur)), ("Engine / component", engines), ("coreq version", value(&r["libraries"]["coreq"]))] {
            let _ = writeln!(w, "<div class=\"fact\"><small>{label}</small><span>{v}</span></div>");
        }
        let _ = writeln!(w, "</section></div><details id=\"events-{key}\"><summary id=\"summary-events-{key}\">Evidence &amp; timeline</summary>");
        for (label, v) in [("Installation", value(&r["installation"])), ("Workspace", value(&r["workspace"])), ("Source", text(&r["provenance"])), ("Host observation", at(&r["observation"]["at"], now)), ("Host state", text(&r["observation"]["state"])), ("Observation reason", value(&r["observation"]["reason"])), ("Release check", at(&r["release"]["at"], now)), ("Producer-reported candidate (unverified)", value(&r["reported_available"])), ("Update history", "Not reported".into())] {
            let _ = writeln!(w, "<div class=\"fact\"><small>{label}</small><span>{v}</span></div>");
        }
        let _ = writeln!(w, "</details></section>");
    }
    let _ = writeln!(w, "<details id=\"version-meanings\"><summary id=\"summary-version-meanings\">Version meanings</summary><p><strong>Running</strong> — last observed service process.</p><p><strong>Selected</strong> — version chosen by the host launcher; may not be running yet.</p><p><strong>Available</strong> — release found by the last check; not an installation receipt. Check state and time remain separate from host observation.</p><small>coreq is a shared library, not another service. Missing or stale evidence never means healthy. Last known versions do not establish current process state.</small></details></div></section>");
}

/// Connected installations (D11, enrollment C1): connection, data age, history warmup and export gaps are separate
/// states; a revoked installation keeps its history and is not an active failure. Absent until enrollment is set up.
fn installations(w: &mut String, inst: &Value, resources: &Value, now: u64) {
    if inst.is_null() { return; }
    let _ = writeln!(w, "<section id=\"installations\" class=\"connections\"><div class=\"connect-title\"><h2>Connections</h2><a id=\"add-service\" href=\"/connect\">＋ Add service</a></div>{}", unknown_line(&inst["source"]));
    let rows = inst["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        let _ = writeln!(w, "<p class=\"detail\">{}</p></section>", if inst["source"]["state"] == "ok" { "No installation connected yet." } else { "Installation list unavailable." });
        return;
    }
    let _ = writeln!(w, "<div class=\"connect-head\"><span>Installation</span><span>Connection</span><span>Data</span></div><div class=\"connect-list\">");
    let mut panels = String::new();
    for (index, r) in rows.iter().enumerate() {
        let (inst_id, ws) = (r["installation"].as_str().unwrap_or(""), r["workspace"].as_str().unwrap_or(""));
        let id = crate::enroll::anchor(inst_id, ws);
        let revoked = r["connection"] == "revoked";
        let verified = r["projected"] == true;
        let recent = r["received_at"].as_u64().is_some_and(|t| t <= now && now - t <= crate::enroll::stale_secs(r["snapshot"]["schema"].as_str().unwrap_or("")));
        let (class, connection) = match r["connection"].as_str() {
            Some("revoked") => ("detail", "Revoked · history kept"),
            Some("pending") => ("detail", "Pending · credential activation"),
            Some("active") if !verified => ("detail", "Pending · first snapshot readback"),
            Some("active") if recent => ("connect-good", "✓ Connected"),
            Some("active") if r["received_at"].as_u64().is_some_and(|t| t <= now) => ("connect-ask", "No recent receipt"),
            _ => ("detail", "Connection unknown"),
        };
        let o = &r["observation"];
        let data = match o["age_s"].as_u64() {
            Some(age) => format!("{} {} ago{}", if revoked { "Last observation" } else { "Observed" }, dur(age), if !revoked && o["state"] != "ok" { format!(" · {}", text(&o["state"])) } else { String::new() }),
            None => format!("Unknown · {}", o["reason"].as_str().map(esc).unwrap_or_else(|| "no observation".into())),
        };
        let selected = index == 0;
        let product = match r["product"].as_str() { Some("agentq") => "AgentQ", Some("serverq") => "ServerQ", Some("taskq") => "TaskQ", _ => "Unknown product" };
        let duplicate = rows.iter().filter(|other| other["host_label"] == r["host_label"] && other["runtime"] == r["runtime"] && other["product"] == r["product"]).count() > 1;
        let identity = if duplicate { format!("<small>{}</small>", esc(inst_id)) } else { String::new() };
        let _ = writeln!(w, "<button type=\"button\" class=\"connect-select\" id=\"select-{id}\" aria-controls=\"{id}\" aria-expanded=\"{selected}\" aria-pressed=\"{selected}\"><span><strong>{product}</strong><small>{} · {}</small>{identity}</span><span class=\"{class}\">{connection}</span><span class=\"connect-data\">{data}</span></button>", text(&r["host_label"]), text(&r["runtime"]));
        let w = &mut panels;
        let _ = writeln!(w, "<div class=\"connect-body\" id=\"{id}\" {}><strong>{product} · {} ({})</strong>", if selected { "" } else { "hidden" }, text(&r["host_label"]), text(&r["runtime"]));
        let received = if r["received_at"].as_u64().is_some_and(|t| t > now) { "Unknown · server clock uncertain".into() } else if r["received_at"].is_null() { "Unknown · no durable receipt yet".into() } else { at(&r["received_at"], now) };
        let _ = writeln!(w, "<p class=\"detail\">Host activity unknown. Last durable receipt: {}. {}</p>", received, if revoked { "Local work continues independently." } else { "The host retries with its existing grant; silence does not prove it stopped or disconnected locally." });
        if let Some(reason) = o["reason"].as_str() {
            let _ = writeln!(w, "<p class=\"detail\">Producer: {}</p>", esc(reason));
        }
        let resource = resources["rows"].as_array().and_then(|rows| rows.iter().find(|v| v["installation"] == inst_id && v["workspace"] == ws));
        if r["product"] == "agentq" && !revoked {
            if let Some(res) = resource.filter(|r| r["source"]["state"] != "unknown") {
                let h = &res["history"];
                let n = num(&h["samples"], &res["source"]);
                let _ = writeln!(w, "<p>Resource history: {} · {n} real samples in this window</p>", if h["state"] == "warmup" { "collecting · trends need 3 samples" } else { "available" });
                if res["current"]["cpu"]["percent"].is_null() {
                    let _ = writeln!(w, "<p class=\"detail\">CPU unknown · {}</p>", res["current"]["cpu"]["reason"].as_str().map(esc).unwrap_or_else(|| "no sample".into()));
                }
            } else {
                let _ = writeln!(w, "<p class=\"detail\">Resource history unavailable · trends need 3 real samples. CPU unknown until a real baseline arrives.</p>");
            }
        }
        let p = &r["snapshot"]["payload"];
        if r["product"] == "taskq" && !revoked && crate::enroll::taskq_payload(p) {
            let b = &p["board"];
            let totals = match p["totals"].as_object() {
                Some(t) if p["complete"] == true => t.iter().map(|(k, v)| format!("{} {}", esc(k), num(v, &json!({"reason": "not reported"})))).collect::<Vec<_>>().join(" · "),
                _ => format!("Totals unknown · {}", p["reason"].as_str().map(esc).unwrap_or_else(|| "incomplete".into())),
            };
            let _ = writeln!(w, "<p>Board {}:{}/{} · {totals} · {} tasks observed</p>", text(&b["kind"]), text(&b["host"]), text(&b["repo"]), p["tasks"].as_array().map_or(0, Vec::len));
        }
        let x = &r["export"];
        let export = match x["state"].as_str() {
            Some("gap") => format!("gap · {}", text(&x["reason"])),
            Some("not_applicable") => "Not applicable · no Langfuse export required".into(),
            Some("unknown") => format!("Unknown · {}", text(&x["reason"])),
            _ => "Unknown · no qualified export evidence".into(),
        };
        let _ = writeln!(w, "<p class=\"detail\">Export: {export}</p><details id=\"identity-{id}\"><summary id=\"scope-{id}\">Identity &amp; scope</summary><dl><dt>Installation</dt><dd><code>{}</code></dd><dt>Workspace</dt><dd>{}</dd><dt>Generation</dt><dd>{}</dd><dt>Version</dt><dd>{}</dd><dt>Authorized data</dt><dd>{}</dd><dt>Observation authority</dt><dd>Observation only · no task actions or host commands</dd>", esc(inst_id), esc(ws), text(&r["generation"]), r["snapshot"]["payload"]["version"].as_str().or_else(|| r["snapshot"]["payload"]["agentq.version"].as_str()).map(esc).unwrap_or_else(|| "Unknown · not reported".into()), r["scopes"].as_array().map(|v| v.iter().map(text).collect::<Vec<_>>().join(", ")).filter(|s| !s.is_empty()).unwrap_or_else(|| "Unknown · not reported".into()));
        if let Some(caps) = r["capabilities"].as_array().filter(|c| !c.is_empty()) {
            let _ = writeln!(w, "<dt>Separately approved operations</dt><dd>{}</dd>", caps.iter().map(text).collect::<Vec<_>>().join(", "));
        }
        if !r["scope"].is_null() { let _ = writeln!(w, "<dt>Board scope</dt><dd>{}: {}</dd>", text(&r["scope"]["name"]), text(&r["scope"]["value"])); }
        let evidence = r["snapshot"]["event_id"].as_str().map(|s| format!("<code>{}</code> · {}", esc(s), text(&r["snapshot"]["schema"]))).unwrap_or_else(|| "No snapshot yet".into());
        let _ = writeln!(w, "<dt>Evidence</dt><dd>{evidence}</dd><dt>Snapshot history</dt><dd>{} retained snapshots; resource trends use their own samples</dd></dl></details>", num(&r["history"]["samples"], &json!({"reason": "history unavailable"})));
        if revoked {
            let _ = writeln!(w, "<p>New browser approval is required. Keep the existing identity to preserve history; a lost identity stays separate.</p><a id=\"reconnect-{id}\" href=\"/connect\">Reconnect…</a>");
        } else {
            if !verified {
                let _ = writeln!(w, "<p><a id=\"readback-{id}\" href=\"/connect?wait={}&amp;workspace={}&amp;generation={}&amp;since={now}\">Check pending readback</a></p>", enc(inst_id), enc(ws), text(&r["generation"]));
            }
            let _ = writeln!(w, "<p><a id=\"revoke-{id}\" href=\"/connect?revoke={}&amp;workspace={}\">Revoke…</a></p>", enc(inst_id), enc(ws));
        }
        let _ = writeln!(w, "</div>");
    }
    let _ = writeln!(w, "</div>{panels}</section>");
}

/// One serverq report or enrolled projection: per-check status (last known unless fresh), reason, observation time
/// and evidence; manual rules never verified; open incidents.
fn checks(w: &mut String, ch: &Value, label: &str, now: u64) {
    let _ = writeln!(w, "{}<p class=\"note\">{label} at {}</p>", unknown_line(&ch["source"]), at(&ch["at"], now));
    let _ = writeln!(w, "<table>\n<tr><th>Check</th><th>Status</th><th>Detail</th></tr>");
    for r in ch["results"].as_array().into_iter().flatten() {
        let st = r["status"].as_str().unwrap_or("unknown");
        let status = if ch["source"]["state"] == "ok" { badge(st, st) } else { badge("unconfirmed", &format!("Last known {st}")) };
        // a local report carries its own detail; a projection only fixed reasons, times and counts
        let detail = if r.get("detail").is_some() {
            text(&r["detail"])
        } else {
            let evidence: Vec<String> = r["evidence"].as_object().into_iter().flatten().map(|(k, v)| format!("{} {}", esc(k), text(v))).collect();
            let mut parts = vec![r["reason"].as_str().map(esc).unwrap_or_else(|| if st == "ok" { "no reason needed".into() } else { "no reason reported".into() })];
            parts.push(format!("observed {}", at(&r["observed_at"], now)));
            if !evidence.is_empty() {
                parts.push(evidence.join(", "));
            }
            parts.join(" · ")
        };
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{detail}</td></tr>", text(&r["check"]), status);
    }
    for m in ch["manual_rules"].as_array().into_iter().flatten() {
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>Manual rule: never verified by serverq.</td></tr>", text(&m["rule"]), badge("unknown", "not verified"));
    }
    let _ = writeln!(w, "</table>\n<h3>Open incidents</h3>");
    let open = ch["incidents"].as_array().cloned().unwrap_or_default();
    if open.is_empty() {
        let _ = writeln!(w, "<p class=\"note\">None in the report.</p>");
    }
    for i in open {
        let _ = writeln!(w, "<p>{} · {} · first {} · last {} · {} occurrences</p>", text(&i["check"]), text(&i["state"]), at(&i["first_seen"], now), at(&i["last_seen"], now), text(&i["occurrences"]));
    }
}

/// One projected value as glyph and word: configured/registered, partial, missing, bypassed, disabled, unknown and
/// unsupported stay distinct (agentq-settings.md G1).
fn aq_state(v: &Value) -> (&'static str, &'static str, &'static str) {
    match v.as_str() {
        Some("registered") => ("good", "✓", "registered"),
        Some("enabled") => ("good", "✓", "enabled"),
        Some("configured") => ("good", "✓", "configured"),
        Some("routed") => ("good", "✓", "AgentQ"),
        Some("direct") => ("muted", "→", "Direct"),
        Some("partial") => ("ask", "◐", "partial"),
        Some("absent") => ("ask", "✕", "missing"),
        Some("missing") => ("ask", "✕", "missing"),
        Some("bypassed") => ("ask", "↷", "bypassed"),
        Some("disabled") => ("muted", "○", "off"),
        Some("unsupported") => ("muted", "—", "not supported"),
        _ => ("ask", "?", "unknown"),
    }
}

fn aq_word(v: &Value) -> String {
    let (class, glyph, word) = aq_state(v);
    format!("<span class=\"aq-{class}\">{glyph} {word}</span>")
}

fn aq_glyph(name: &str, v: &Value) -> String {
    let (class, glyph, word) = aq_state(v);
    format!("<span class=\"aq-{class}\" title=\"{name} {word}\" role=\"img\" aria-label=\"{name} {word}\">{glyph}</span>")
}

/// AgentQ's fixed codes in words; an unlisted code is shown as sent (it is a short identifier).
fn aq_code(c: &Value) -> String {
    match c.as_str().unwrap_or("other") {
        "foreign_provider" => "another provider owns the routing".into(),
        "unparseable_config" => "client config cannot be parsed".into(),
        "unreadable_config" => "client config cannot be read".into(),
        "not_released" => "configuration adapter not released".into(),
        "stale" => "stale snapshot".into(),
        "truncated" => "profile list truncated".into(),
        "incomplete" => "incomplete readback".into(),
        "conflict" => "conflicting config".into(),
        "project_override" => "project overrides present".into(),
        "other" => "unlisted problem".into(),
        other => esc(other),
    }
}

fn aq_codes(v: &Value) -> Vec<String> {
    v.as_array().into_iter().flatten().map(aq_code).collect()
}

/// AgentQ client integrations (agentq-settings.md G1, G4): profiles from each connected AgentQ snapshot's
/// `configuration` projection. Configured is never verified; nothing here is a TaskQ task or a saving. With
/// `controls` (config `agentq_configure`, #63) a fresh installation granted `agentq.configure` gets draft selects
/// per writable profile field and one review/apply editor; `app.js` drives the guarded `/agentq/configure` route.
fn integrations(w: &mut String, inst: &Value, controls: bool) {
    let rows: Vec<&Value> = inst["rows"].as_array().into_iter().flatten().filter(|r| r["product"] == "agentq" && r["connection"] != "revoked").collect();
    if rows.is_empty() {
        return;
    }
    let count: usize = rows.iter().filter_map(|r| r["snapshot"]["payload"]["configuration"]["profiles"].as_array()).map(Vec::len).sum();
    let _ = writeln!(
        w,
        "<section id=\"integrations\" class=\"aq\">\n<h2>Agent integrations</h2>\n<p class=\"note\">{count} profile{} · {}. ✓ configured · ◐ partial · ✕ missing · ↷ bypassed · ○ off · ? unknown · — not supported. Client activity is verified separately.</p>\n<div class=\"aq-head\" aria-hidden=\"true\"><span>Client / profile</span><span>Routing</span><span>MCP / hooks</span><span>Optimization</span></div>",
        if count == 1 { "" } else { "s" },
        if controls { "review shows the host's exact plan before any change" } else { "read-only" }
    );
    for r in rows {
        let (inst_id, ws) = (r["installation"].as_str().unwrap_or(""), r["workspace"].as_str().unwrap_or(""));
        let host = format!("{} · {}", text(&r["host_label"]), text(&r["runtime"]));
        let payload = &r["snapshot"]["payload"];
        let c = &payload["configuration"];
        let o = &r["observation"];
        let age = o["age_s"].as_u64().map_or_else(|| esc(o["reason"].as_str().unwrap_or("age unknown")), |a| format!("observed {} ago", dur(a)));
        if !matches!(c["state"].as_str(), Some("fresh" | "stale")) {
            let why = match (r["snapshot"].is_null(), c["reason"].as_str()) {
                (true, _) => "no snapshot yet".into(),
                (_, Some(why)) => esc(why),
                _ => "not reported by this AgentQ version".into(),
            };
            let _ = writeln!(w, "<p class=\"aq-gap\"><b>{host}</b> · {} <span class=\"detail\">{why}</span></p>", aq_word(&Value::Null));
            continue;
        }
        let mut notes = vec![];
        // D6: a stale profile is never green
        let stale = c["state"] == "stale" || o["state"] != "ok";
        if stale {
            notes.push(format!("{} snapshot · {age}", badge("stale", "stale")));
        }
        if c["truncated"] == true {
            notes.push("Only the first 16 profiles arrived.".into());
        }
        if c["incomplete"] == true {
            notes.push("Some profiles failed validation and are not shown.".into());
        }
        if !notes.is_empty() {
            let _ = writeln!(w, "<p class=\"aq-gap\"><b>{host}</b> · {}</p>", notes.join(" "));
        }
        // G1: writes need the route, the agentq.configure grant and a fresh complete snapshot; else the precise reason
        let granted = r["capabilities"].as_array().is_some_and(|c| c.iter().any(|x| x == "agentq.configure"));
        let locked = match (controls, granted, stale || c["truncated"] == true || c["incomplete"] == true) {
            (false, _, _) => None,
            (_, false, _) => Some("this connection has no agentq.configure grant"),
            (_, _, true) => Some("the snapshot is stale or incomplete"),
            _ => Some(""),
        };
        let anchor = crate::enroll::anchor(inst_id, ws);
        for p in c["profiles"].as_array().into_iter().flatten() {
            let id = format!("aq-{anchor}-{}", esc(p["id"].as_str().unwrap_or("")));
            let ops: Vec<&str> = p["operations"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let plugin = ops.contains(&"plugin");
            let second = if plugin { aq_glyph("plugin", &p["plugin"]) } else { aq_glyph("hooks", &p["hooks"]) };
            let eff = &p["effective"];
            let _ = writeln!(
                w,
                "<details id=\"{id}\" class=\"aq-row{}\"><summary><span class=\"aq-client\"><b>{}</b><small>{host} · {}</small></span><span><i class=\"aq-label\">Routing</i>{}</span><span><i class=\"aq-label\">MCP / {}</i>{} / {second}</span><span><i class=\"aq-label\">Optimization</i>{}</span></summary>",
                if stale { " aq-stale" } else { "" },
                text(&p["label"]),
                text(&p["id"]),
                aq_word(&p["routing"]),
                if plugin { "plugin" } else { "hooks" },
                aq_glyph("MCP", &p["mcp"]),
                aq_word(&eff["state"])
            );
            let routing_note = match (p["kind"].as_str(), p["routing"].as_str()) {
                (_, Some("unsupported")) => "This client has no provider route.",
                (Some("codex"), Some("direct")) => "Direct keeps MCP and telemetry; it bypasses the provider optimization.",
                (Some("claude-code"), Some("direct")) => "Direct removes the AgentQ route; the command hook can still use RTK.",
                (_, Some("routed")) => "The client's provider runs through the local AgentQ proxy.",
                (_, Some("bypassed")) => "Another provider owns the route; AgentQ does not overwrite it.",
                _ => "Routing could not be read.",
            };
            let adapter = match eff["adapter"].as_str() {
                Some("provider") => "provider proxy",
                Some("hook") => "tool hook",
                Some("plugin") => "tool-result plugin",
                _ => "no adapter",
            };
            let pref = if p["optimization"] == "unsupported" { aq_word(&p["optimization"]) } else { format!("preference {} · {adapter}", aq_word(&p["optimization"])) };
            let mut chips = vec![format!("<span class=\"aq-chip\">agent_context {}</span>", aq_word(&p["mcp"]))];
            if plugin {
                chips.push(format!("<span class=\"aq-chip\">plugin {}</span>", aq_word(&p["plugin"])));
            } else if p["hooks"] != "unsupported" {
                chips.push(format!("<span class=\"aq-chip\">hooks {}</span>", aq_word(&p["hooks"])));
            }
            let aliases: Vec<String> = p["aliases"].as_array().into_iter().flatten().map(|a| format!("<span class=\"aq-chip\">{}</span>", text(a))).collect();
            let overrides = match p["overrides"].as_u64() {
                Some(0) => "none".into(),
                Some(n) => format!("<span class=\"aq-ask\">{n}</span>"),
                None => "<span class=\"unknown\" title=\"unknown: not reported\">—</span>".into(),
            };
            let owned = match p["owned"].as_bool() {
                Some(true) => "yes",
                Some(false) => "no",
                None => "unknown",
            };
            let problems = [aq_codes(&p["conflicts"]), aq_codes(&p["incomplete"])].concat();
            let problems = if problems.is_empty() { String::new() } else { format!("<p class=\"aq-ask\">{}</p>", problems.join(" · ")) };
            let verification = if p["verification"]["state"] == "verified" {
                "<p class=\"aq-good\">✓ Verified client activity</p>".to_string()
            } else {
                format!(
                    "<p class=\"aq-ask\">Client activity unverified</p><small>{}A written config or healthy service does not verify the client path.</small>",
                    if p["verification"]["reason"] == "no_evidence" { "No client observation for this profile yet. " } else { "" }
                )
            };
            let engines: Vec<String> = payload["engines"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(e, v)| format!("<div class=\"aq-fact\"><span>{}</span><span>{}</span></div>", esc(e), if v["version"].is_null() { "unknown".into() } else { format!("{} · {}", text(&v["version"]), text(&v["state"])) }))
                .collect();
            let d = &c["defaults"];
            let writes = format!("Read-only: {}.", aq_codes(&p["writes"]["reasons"]).join(", "));
            let writes = match locked {
                Some("") if p["writes"]["enabled"] == true => {
                    let fields: Vec<String> = crate::enroll::CHANGES
                        .iter()
                        .filter(|(f, _)| ops.contains(f))
                        .filter_map(|(f, values)| {
                            let opts: String = values.iter().filter(|v| p[*f] != **v).map(|v| format!("<option value=\"{v}\">{}</option>", aq_state(&json!(v)).2)).collect();
                            // a field already at its only value has nothing to change
                            (!opts.is_empty()).then(|| format!(
                                "<label class=\"aq-fact\"><span>{f}</span><select id=\"{id}-{f}\" data-installation=\"{}\" data-profile=\"{}\" data-field=\"{f}\"><option value=\"\">keep {}</option>{opts}</select></label>",
                                esc(inst_id),
                                text(&p["id"]),
                                aq_state(&p[*f]).2
                            ))
                        })
                        .collect();
                    format!("<div class=\"aq-draft\"><b>Draft</b>{}<small>Shared config: the change applies to every alias above.</small></div>", fields.join(""))
                }
                Some(why) if !why.is_empty() => format!("{writes} Changes disabled: {why}."),
                _ => writes,
            };
            let _ = writeln!(
                w,
                "<div class=\"aq-body\"><div><div class=\"aq-fact\"><span>Routing</span><span>{}</span></div><small>{routing_note}</small>\n<div class=\"aq-fact\"><span>Context optimization</span><span>{}</span></div><small>{pref}</small>\n<div class=\"aq-fact\"><span>MCP &amp; {}</span><span class=\"aq-chips\">{}</span></div>\n<div class=\"aq-fact\"><span>Shared config</span><span class=\"aq-chips\">{}</span></div>\n<div class=\"aq-fact\"><span>Project overrides</span><span>{overrides}</span></div>\n<div class=\"aq-fact\"><span>AgentQ-owned entries</span><span>{owned}</span></div>{problems}\n<p class=\"detail\">{writes}</p></div>\n<div><b>Verification</b>{verification}\n<div class=\"aq-fact\"><span>Revision</span><span><code>{}</code></span></div>\n<details><summary>Managed engines</summary>{}<small>Managed by the AgentQ release, not per client. Host minimum {} bytes; host optimization {}.</small></details>\n<details><summary>Activity</summary><small>Active task and progress are not supplied by this AgentQ projection.</small></details></div></div></details>",
                aq_word(&p["routing"]),
                aq_word(&eff["state"]),
                if plugin { "plugin" } else { "hooks" },
                chips.join(""),
                aliases.join(""),
                text(&p["revision"]),
                engines.join(""),
                if d["min_bytes"].is_null() { "unknown".into() } else { text(&d["min_bytes"]) },
                match d["enabled"].as_bool() {
                    Some(true) => "on",
                    Some(false) => "off",
                    None => "unknown",
                }
            );
        }
        // one editor per installation (G1): review asks the host for the plan, apply confirms exactly that plan
        if locked == Some("") {
            let _ = writeln!(
                w,
                "<div class=\"aq-edit\" id=\"aqe-{anchor}\" data-installation=\"{}\" data-revision=\"{}\"><button type=\"button\" data-aq=\"review\">Review changes on {host}</button><div class=\"aq-out\" aria-live=\"polite\"></div></div>",
                esc(inst_id),
                text(&c["revision"])
            );
        }
    }
    let _ = writeln!(w, "<p class=\"detail\">AgentQ owns integrations. Client login, model and approval policy stay with the client.</p>\n</section>");
}

/// Trend window of the server grid (overview.md: 30 min on a shared 0–100 % scale), whatever the period.
const TREND: u64 = 1800;

fn pct(used: &Value, total: &Value) -> Option<f64> {
    used.as_f64().zip(total.as_f64().filter(|t| *t > 0.0)).map(|(u, t)| u / t * 100.0)
}

/// A current value with an `id` and `data-n` so a refresh can tint what changed, plus its percentage-point change
/// from the previous sample.
fn value(id: &str, shown: &str, now: Option<f64>, prev: Option<f64>) -> String {
    let d = match now.zip(prev).map(|(a, b)| a - b) {
        Some(d) if d.abs() >= 0.05 => format!(" <small class=\"d\" title=\"change since the previous sample\">{}{:.1} pp</small>", if d > 0.0 { "+" } else { "−" }, d.abs()),
        _ => String::new(),
    };
    format!("<span class=\"v\" id=\"{id}\" data-n=\"{}\">{shown}</span>{d}", now.map_or("".into(), |n| format!("{n:.1}")))
}

/// Capacity bar on 0–100 %; SVG geometry, not inline style (CSP).
fn bar(p: Option<f64>) -> String {
    p.map_or(String::new(), |p| format!("<svg class=\"cap\" viewBox=\"0 0 100 6\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><rect width=\"100\" height=\"6\" rx=\"2\"/><rect class=\"fill\" width=\"{:.1}\" height=\"6\" rx=\"2\"/></svg>", p.clamp(0.0, 100.0)))
}

/// 30 min line on 0–100 %; a gap of more than two buckets, an unknown value or a reset breaks the line. Fewer than
/// two connected samples draw nothing: no chart from a single reading.
fn trend(points: &[Value], to: u64, f: &dyn Fn(&Value) -> Option<f64>, label: &str) -> String {
    if points.len() < 3 { return "<span class=\"trend none\">no trend yet · warmup, 3 real samples needed</span>".into(); }
    let from = to.saturating_sub(TREND);
    let mut lines: Vec<Vec<(u64, f64)>> = vec![vec![]];
    for p in points.iter().filter(|p| p["at"].as_u64().is_some_and(|a| a >= from)) {
        let at = p["at"].as_u64().unwrap();
        let last = lines.last_mut().unwrap();
        if last.last().is_some_and(|(a, _)| at - a > 2 * crate::resource::BUCKET) {
            lines.push(vec![]);
        }
        match f(p) {
            Some(v) => lines.last_mut().unwrap().push((at, v.clamp(0.0, 100.0))),
            None => lines.push(vec![]),
        }
    }
    let lines: Vec<String> = lines
        .iter()
        .filter(|l| l.len() > 1)
        .map(|l| format!("<polyline points=\"{}\"/>", l.iter().map(|(a, v)| format!("{:.1},{:.1}", (a - from) as f64 / TREND as f64 * 120.0, 32.0 - v / 100.0 * 32.0)).collect::<Vec<_>>().join(" ")))
        .collect();
    if lines.is_empty() {
        return "<span class=\"trend none\">no trend yet</span>".into();
    }
    format!("<svg class=\"trend\" viewBox=\"0 0 120 32\" preserveAspectRatio=\"none\" role=\"img\" aria-label=\"{label}, last 30 min, 0–100 %\">{}</svg>", lines.join(""))
}

/// The combined first screen (first-screen.md, D10): servers and Boards side by side, reflowing to one column.
fn first(w: &mut String, doc: &Value, now: u64) {
    let _ = writeln!(w, "<div class=\"first\">");
    if doc["sources"]["langfuse"]["reason"] != "not configured" || doc["resources"]["rows"].as_array().is_some_and(|a| !a.is_empty()) {
        servers(w, &doc["resources"], now);
    }
    if let Some(rows) = doc["boards"].as_array().filter(|a| !a.is_empty()) { boards(w, rows); }
    let _ = writeln!(w, "</div>");
}

/// The shared host grid: identity and sample age, policy entry, CPU, RAM and every filesystem of one installation.
fn servers(w: &mut String, res: &Value, now: u64) {
    let rows = res["rows"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(w, "<section id=\"servers\">\n<h2>Servers</h2>");
    if rows.is_empty() {
        let _ = writeln!(w, "<p class=\"note\">No resource sample in the last {}.</p>\n</section>", dur(res["window_s"].as_u64().unwrap_or(0)));
        return;
    }
    let to = res["to"].as_u64().unwrap_or(now);
    let _ = writeln!(w, "<p class=\"note\">Trends: last 30 min, 0–100 %. Disks: current use. — unknown (hover for the reason).</p>");
    let unk = |why: &Value| format!("<span class=\"unknown\" title=\"unknown: {}\">—</span>", esc(why.as_str().unwrap_or("not reported")));
    for r in rows {
        let inst = esc(r["installation"].as_str().unwrap_or(""));
        let (src, c, h) = (&r["source"], &r["current"], &r["history"]);
        let points = h["points"].as_array().map_or(&[][..], Vec::as_slice);
        // the sample before the current one, for pp changes
        let prev = points.len().checked_sub(2).map(|i| &points[i]).unwrap_or(&Value::Null);
        let state = src["state"].as_str().unwrap_or("unknown");
        let age = src["age_s"].as_u64().map_or("no sample".into(), |a| format!("{} old", dur(a)));
        let warm = h["reason"].as_str().map(|x| format!(" <span class=\"detail\">{}</span>", esc(x))).unwrap_or_default();
        let _ = writeln!(
            w,
            "<article class=\"node\" id=\"node-{inst}\"><div class=\"who\"><b>{}</b> <small>{} · {}</small> {}{}{warm}<br><span class=\"policy detail\">Work mode: no TaskQ policy adapter</span></div>",
            text(&r["host"]),
            text(&r["runtime"]),
            text(&r["provenance"]),
            src["age_s"].as_u64().map_or_else(|| badge(state, &age), |a| format!("<span data-age-s=\"{a}\">{}</span>", badge(state, &age))),
            src["reason"].as_str().map(|x| format!(" <span class=\"detail\">{}</span>", esc(x))).unwrap_or_default()
        );
        let cpu = |s: &Value| s["cpu"]["percent"].as_f64();
        let cpu_now = cpu(c);
        let shown = cpu_now.map_or_else(|| unk(&c["cpu"]["reason"]), |p| format!("{p:.1} %"));
        let _ = writeln!(w, "<div class=\"m\"><span class=\"k\">CPU</span> {}{}</div>", value(&format!("cpu-{inst}"), &shown, cpu_now, cpu(prev)), trend(points, to, &cpu, "CPU"));
        let ram = |s: &Value| pct(&s["memory"]["used_bytes"], &s["memory"]["total_bytes"]);
        let m = &c["memory"];
        let shown = if m["used_bytes"].is_null() { unk(&m["reason"]) } else { format!("{} / {} GB · {:.0} %", gb(&m["used_bytes"]), gb(&m["total_bytes"]), ram(c).unwrap_or(0.0)) };
        let _ = writeln!(w, "<div class=\"m\"><span class=\"k\">RAM</span> {}{}{}</div>", value(&format!("ram-{inst}"), &shown, ram(c), ram(prev)), bar(ram(c)), trend(points, to, &ram, "RAM"));
        let _ = write!(w, "<div class=\"m disks\">");
        for d in c["disks"].as_array().into_iter().flatten() {
            let id = d["id"].as_str().unwrap_or("");
            let p = pct(&d["used_bytes"], &d["total_bytes"]);
            let before = prev["disks"].as_array().into_iter().flatten().find(|x| x["id"] == id).and_then(|x| pct(&x["used_bytes"], &x["total_bytes"]));
            let name = esc(&format!("{}{}", d["role"].as_str().unwrap_or("disk"), d["fstype"].as_str().map(|f| format!(" {f}")).unwrap_or_default()));
            let shown = if p.is_none() { unk(&d["reason"]) } else { format!("{} / {} GB · {:.0} %", gb(&d["used_bytes"]), gb(&d["total_bytes"]), p.unwrap_or(0.0)) };
            let _ = write!(w, "<div class=\"disk\"><span class=\"k\" title=\"filesystem {}\">{name}</span> {}{}</div>", esc(id), value(&format!("disk-{inst}-{}", esc(id)), &shown, p, before), bar(p));
        }
        if !c["disk_reason"].is_null() {
            let _ = write!(w, "<div class=\"disk\"><span class=\"k\">Disks</span> {}</div>", unk(&c["disk_reason"]));
        }
        let _ = write!(w, "</div>");
        if let Some(groups) = r["taskq_groups"].as_array() {
            // UI view model only: observation transport and verified cross-product joins are a follow-up.
            let _ = writeln!(w, "<div class=\"node-policy\" data-taskq-groups=\"{}\"></div>", esc(&serde_json::to_string(groups).unwrap()));
        }
        let _ = writeln!(w, "</article>");
    }
    let _ = writeln!(w, "</section>");
}

/// D13: the TaskQ-owned browser components render these rows; static output keeps source/issue links.
fn boards(w: &mut String, boards: &[Value]) {
    let _ = writeln!(w, "<section id=\"boards\" data-boards=\"{}\"><h2>Boards</h2><div class=\"board-blocks\"></div><div class=\"board-fallback\">", esc(&serde_json::to_string(boards).unwrap()));
    for b in boards {
        let _ = writeln!(w, "<p><a href=\"{}\">{}</a> · Shared TaskQ blocks require JavaScript.</p>{}", text(&b["url"]), text(&b["project"]), unknown_line(&b["source"]));
        for q in b["questions"].as_array().into_iter().flatten() {
            let _ = writeln!(w, "<p>{} · {}</p>", issue(q), text(&q["text"]));
        }
    }
    let _ = writeln!(w, "</div></section>");
}

/// Bytes in GB with one decimal (10^9, the producers' own figures).
fn gb(v: &Value) -> String {
    v.as_u64().map_or("—".into(), |b| format!("{:.1}", b as f64 / 1e9))
}

/// `resources` (D4.4) as plain current values with provenance and history state; charts are D10 (#76).
fn resources(w: &mut String, res: &Value, now: u64) {
    let rows = res["rows"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(w, "<section id=\"resources\">\n<h2>Resource samples</h2>");
    if rows.is_empty() {
        let _ = writeln!(w, "<p class=\"note\">No resource sample in the last {}.</p>\n</section>", dur(res["window_s"].as_u64().unwrap_or(0)));
        return;
    }
    let _ = writeln!(w, "<table>\n<tr><th>Host</th><th>Source</th><th>Sample</th><th>CPU</th><th>RAM used / total</th><th>Disks used / free / total</th><th>History</th></tr>");
    let unk = |why: &Value| format!("<span class=\"unknown\" title=\"unknown: {}\">—</span>", esc(why.as_str().unwrap_or("not reported")));
    for r in rows {
        let (src, c, h) = (&r["source"], &r["current"], &r["history"]);
        let cpu = c["cpu"]["percent"].as_f64().map_or_else(|| unk(&c["cpu"]["reason"]), |p| format!("{p:.1} %"));
        let m = &c["memory"];
        let ram = if m["used_bytes"].is_null() { unk(&m["reason"]) } else { format!("{} / {} GB", gb(&m["used_bytes"]), gb(&m["total_bytes"])) };
        let mut disks: Vec<String> = c["disks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|d| {
                let v = if d["used_bytes"].is_null() { unk(&d["reason"]) } else { format!("{} / {} / {} GB", gb(&d["used_bytes"]), gb(&d["free_bytes"]), gb(&d["total_bytes"])) };
                format!("{} {v}", esc(&format!("{}{}", d["role"].as_str().unwrap_or("disk"), d["fstype"].as_str().map(|f| format!(" {f}")).unwrap_or_default())))
            })
            .collect();
        if !c["disk_reason"].is_null() {
            disks.push(unk(&c["disk_reason"]));
        }
        let state = src["state"].as_str().unwrap_or("unknown");
        let sample = format!("{} {}", badge(state, &at(&src["at"], now)), esc(src["reason"].as_str().unwrap_or("")));
        let gaps = h["gaps"].as_array().map_or(0, Vec::len);
        let history = format!(
            "{} samples{}{}{}",
            text(&h["samples"]),
            h["reason"].as_str().map(|x| format!(", {}", esc(x))).unwrap_or_default(),
            if gaps > 0 { format!(", {gaps} gaps") } else { String::new() },
            if h["incomplete"] == true { ", capped" } else { "" }
        );
        let _ = writeln!(
            w,
            "<tr id=\"res-{}\"><td>{}</td><td>{}</td><td>{sample}</td><td>{cpu}</td><td>{ram}</td><td>{}</td><td>{history}</td></tr>",
            esc(r["installation"].as_str().unwrap_or("")),
            text(&r["host"]),
            text(&r["provenance"]),
            if disks.is_empty() { "—".into() } else { disks.join("<br>") }
        );
    }
    let _ = writeln!(w, "</table>\n</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// D19 U1/U2: no configured product -> one real connection entry; a failed board is not first launch.
    #[test]
    fn startup_availability() {
        let empty = crate::data::build(&json!({"now": 10000, "period": 1800, "projects": []}));
        let html = render(&empty, &json!({}));
        assert!(html.contains("Connect an existing installation") && html.contains("href=\"/connect\""));
        for id in ["servers", "boards", "hosts", "agents", "checks", "resources"] {
            assert!(!html.contains(&format!("id=\"{id}\"")), "unconfigured {id}");
        }
        let failed = crate::data::build(&json!({"now": 10000, "period": 1800, "projects": ["taskqorg/dashq"],
            "boards": {"taskqorg/dashq": {"value": null, "at": null, "error": "timeout"}}}));
        let html = render(&failed, &json!({}));
        assert!(html.contains("id=\"boards\"") && html.contains("read failed (timeout)"));
        assert!(!html.contains("id=\"servers\"") && !html.contains("id=\"agents\""));
        assert!(crate::data::all_failed(&failed), "unconfigured sources do not mask first-ever failure");
        let mut board_only = empty.clone();
        let base = first_screen_doc();
        board_only["boards"] = json!([base["boards"][0].clone()]);
        board_only["sources"]["github"] = json!({"state": "ok", "at": 10000, "age_s": 0});
        let html = render(&board_only, &json!({}));
        assert!(html.contains("id=\"boards\"") && !html.contains("id=\"startup\""));
        assert!(!html.contains("id=\"servers\"") && !html.contains("id=\"checks\""));
        let mut fresh_empty = board_only.clone();
        fresh_empty["boards"][0]["tasks"] = json!([]);
        fresh_empty["boards"][0]["questions"] = json!([]);
        fresh_empty["boards"][0]["counts"] = json!({"doing": 0, "review": 0, "ask": 0, "failed": 0, "ready": 0, "waiting": 0, "later": 0, "closed": null});
        let mut pending = empty.clone();
        pending["installations"] = json!({"source": {"state": "ok"}, "rows": [{"installation": "new", "workspace": "w", "product": "taskq",
            "connection": "active", "projected": false, "host_label": "test", "runtime": "native", "observation": {"state": "unknown", "reason": "no snapshot yet"}}]});
        assert!(render(&pending, &json!({})).contains("Pending · first snapshot readback"));
        let mut warmup = base.clone();
        for r in warmup["resources"]["rows"].as_array_mut().unwrap() {
            r["history"]["points"].as_array_mut().unwrap().truncate(2);
        }
        assert!(!render(&warmup, &json!({})).contains("<polyline"), "two samples do not draw a trend");
        let mut partial = base.clone();
        partial["boards"][0]["source"]["state"] = json!("unconfirmed");
        partial["boards"][0]["source"]["reason"] = json!("read failed (timeout)");
        partial["sources"]["github"]["state"] = json!("unconfirmed");
        partial["sources"]["github"]["reason"] = json!("read failed (timeout)");
        let mut all = partial.clone();
        for source in all["sources"].as_object_mut().unwrap().values_mut() {
            source["state"] = json!("unconfirmed");
            source["reason"] = json!("read failed (timeout)");
        }
        for b in all["boards"].as_array_mut().unwrap() { b["source"]["state"] = json!("unconfirmed"); }
        for r in all["resources"]["rows"].as_array_mut().unwrap() { r["source"]["state"] = json!("unconfirmed"); }
        if let Some(out) = std::env::var_os("DASHQ_STARTUP_EVIDENCE") {
            let dir = std::path::Path::new(&out);
            std::fs::create_dir_all(dir).unwrap();
            for (name, doc) in [("first-launch", empty), ("first-failure", failed), ("taskq-only", board_only), ("empty-board", fresh_empty),
                ("pending", pending), ("warmup", warmup), ("partial", partial), ("all-failed", all), ("recovery", base)] {
                std::fs::write(dir.join(format!("{name}.html")), render(&doc, &json!({}))).unwrap();
                std::fs::write(dir.join(format!("{name}.json")), doc.to_string()).unwrap();
            }
        }
    }

    /// D18 V1–V4 chain: producer fixtures -> safe projection -> selectable contextual page.
    /// Stages without producer receipts remain unknown; stale versions never imply current health.
    #[test]
    fn service_versions_render_projected_evidence() {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        let cases: Value = serde_json::from_str(include_str!("../fixture/versions.json")).unwrap();
        inp["installations"] = cases["installations"].clone();
        let doc = crate::data::build(&inp);
        let mut html = String::new();
        versions(&mut html, &doc["versions"], doc["generated_at"].as_u64().unwrap());
        let rows = doc["versions"]["rows"].as_array().unwrap();
        assert_eq!(html.matches("class=\"row version-select\"").count(), rows.len());
        // Legacy TaskQ hosts in the same workspace have no installation ID.
        let keys = |page: &str| page.split(" aria-controls=\"").skip(1)
            .map(|s| s.split('"').next().unwrap().to_owned()).collect::<Vec<_>>();
        let ids = keys(&html);
        assert_eq!(ids.iter().collect::<std::collections::BTreeSet<_>>().len(), rows.len(), "each service controls its own detail");
        let legacy: Vec<_> = rows.iter().enumerate().filter(|(_, r)| r["product"] == "taskq" && r["installation"].is_null()).collect();
        assert_eq!(legacy.len(), 2);
        assert_eq!(legacy[0].1["workspace"], legacy[1].1["workspace"]);
        for (i, r) in &legacy {
            let detail = html.split(&format!("id=\"{}\"", ids[*i])).nth(1).unwrap().split("</h3>").next().unwrap();
            assert!(detail.contains(&format!("taskq · {}", r["host"].as_str().unwrap())));
        }
        let mut reordered = doc["versions"].clone();
        reordered["rows"].as_array_mut().unwrap().reverse();
        // Fresh values and renamed known hosts must not replace installation identity.
        for row in reordered["rows"].as_array_mut().unwrap() {
            row["running"]["version"] = json!("9.9.9");
            if !row["installation"].is_null() { row["host"] = json!("renamed"); }
        }
        let mut refreshed = String::new();
        versions(&mut refreshed, &reordered, 0);
        assert_eq!(keys(&refreshed), ids.into_iter().rev().collect::<Vec<_>>());
        assert_eq!(html.matches("aria-pressed=\"true\"").count(), 1);
        assert_eq!(html.matches(" hidden>").count(), rows.len() - 1);
        for case in cases["cases"].as_object().unwrap().values() {
            if let Some(id) = case["installation"].as_str() { assert!(html.contains(id), "{id}"); }
        }
        for expected in ["Last known", "Current health unknown", "health receipt not reported", "Lifecycle: unknown", "Update error reported", "Update history", "Host observation", "Release check", "Not reported", "0.51.0", "12s", "win-1/windows", "win-1/wsl"] {
            assert!(html.contains(expected), "{expected}");
        }
        for absent in ["data-phase", "data-font", "<select", "<form", "recovery confirmed", "checksum-mismatch", "Running · healthy"] { assert!(!html.contains(absent), "{absent}"); }
        if let Some(out) = std::env::var_os("DASHQ_VERSION_EVIDENCE") {
            std::fs::write(out, format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><link rel=\"stylesheet\" href=\"/style.css\"><script src=\"/app.js\" defer></script></head><body><main id=\"main\"><p id=\"banner\" hidden></p>{html}</main></body></html>")).unwrap();
        }
        // Only explicit projected fields are rendered, not new/raw diagnostic payloads.
        let mut unsafe_doc = doc["versions"].clone();
        unsafe_doc["rows"][0]["raw_status"] = json!("secret-marker");
        unsafe_doc["rows"][0]["host"] = json!("<img src=x onerror=alert(1)>");
        let mut safe = String::new();
        versions(&mut safe, &unsafe_doc, 0);
        assert!(!safe.contains("secret-marker") && !safe.contains("<img"));
        assert!(safe.contains("&lt;img"));
        let mut empty = String::new();
        versions(&mut empty, &Value::Null, 0);
        assert!(empty.is_empty());
    }

    /// One connected AgentQ installation per fixture case (fixture/agentq/configuration.json).
    pub fn agentq_doc() -> Value {
        let cases: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        let rows: Vec<Value> = ["profile_state_and_revision", "profile_projection", "profile_projection_overrides", "profile_projection_stale", "old_producer_configuration_unknown", "unsupported_schema", "no_status"]
            .iter()
            .map(|k| {
                let stale = *k == "profile_projection_stale";
                json!({"installation": format!("inst-{}", k.len()), "product": "agentq", "runtime": "native", "host_label": k.replace('_', "-"), "workspace": "ws-1", "generation": 1, "connection": "active",
                    "observation": {"state": if stale { "stale" } else { "ok" }, "at": 1000, "age_s": if stale { 1000 } else { 30 }, "reason": if stale { json!("older than 180 s") } else { Value::Null }},
                    "history": {"samples": 3, "state": "ok"}, "export": {"state": "ok"}, "snapshot": {"event_id": k, "schema": "agentq-snapshot/1", "payload": cases["cases"][k]}})
            })
            .chain([json!({"installation": "inst-new", "product": "agentq", "runtime": "wsl", "host_label": "pc", "workspace": "ws-1", "connection": "active", "observation": {"state": "unknown", "reason": "no snapshot yet"}, "snapshot": null})])
            .collect();
        json!({"generated_at": 1030, "installations": {"source": {"state": "ok"}, "rows": rows}})
    }

    /// agentq-settings.md G1/G4, #62 A3–A5: actual capabilities, aliases, routing, MCP/hooks/plugin, preference and
    /// effective state from AgentQ's own projection; missing, unsupported, stale, bypassed, configured and unverified
    /// stay distinct; no write control, task or saving appears.
    #[test]
    fn agentq_profiles_render_read_only_from_snapshots() {
        let mut w = String::new();
        integrations(&mut w, &agentq_doc()["installations"], false);
        if let Some(out) = std::env::var_os("DASHQ_EVIDENCE") {
            let css = include_str!("../assets/style.css");
            std::fs::write(out, format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\"><style>{css}</style></head><body><main id=\"main\">{w}</main></body></html>")).unwrap();
        }
        let section = |host: &str| w.split("<b>").find(|s| s.contains(host)).unwrap_or_else(|| panic!("{host} in {w}")).to_string();
        // the onboarded Codex profile: Direct bypasses provider optimization, MCP and hooks registered, shared aliases
        let codex = w.split("<details id=").find(|s| s.contains("<b>Codex</b><small>profile-state-and-revision")).unwrap();
        assert!(codex.contains("→ Direct") && codex.contains("↷ bypassed") && codex.contains("aria-label=\"MCP registered\"") && codex.contains("aria-label=\"hooks registered\""), "{codex}");
        assert!(codex.contains(">codex-cli<") && codex.contains(">codex-desktop<") && codex.contains("preference <span class=\"aq-good\">✓ enabled</span> · provider proxy"), "{codex}");
        let claude = w.split("<details id=").find(|s| s.contains("<b>Claude Code</b><small>profile-state-and-revision")).unwrap();
        assert!(claude.contains("aria-label=\"MCP unknown\"") && claude.contains("client config cannot be parsed · client config cannot be read") && claude.contains("Read-only: configuration adapter not released, incomplete readback, conflicting config."), "{claude}");
        let hermes = w.split("<details id=").find(|s| s.contains("<b>Hermes</b>")).unwrap();
        assert!(hermes.contains("MCP / plugin") && hermes.contains("aria-label=\"plugin enabled\"") && hermes.contains("✓ configured") && hermes.contains("— not supported"), "{hermes}");
        let chat = w.split("<details id=").find(|s| s.contains("<b>Claude Desktop Chat</b><small>profile-state-and-revision")).unwrap();
        assert!(chat.contains("aria-label=\"MCP missing\"") && chat.contains("This client has no provider route.") && !chat.contains("preference"), "{chat}");
        // configured is never verified
        assert!(hermes.contains("Client activity unverified") && !w.contains("Verified client activity"));
        // the projection's own drops and states
        assert!(section("profile-projection").contains("Some profiles failed validation"));
        assert!(w.contains("project overrides present") && w.contains("<span class=\"aq-ask\">3</span>"));
        assert!(section("profile-projection-stale").contains("stale</span> snapshot · observed 16m ago"), "{w}");
        assert!(w.contains("class=\"aq-row aq-stale\"><summary><span class=\"aq-client\"><b>Codex</b><small>profile-projection-stale"));
        assert!(section("old-producer").contains("older producer") && section("unsupported-schema").contains("unsupported schema") && section("no-status").contains("no status"));
        assert!(section("pc").contains("no snapshot yet"));
        // read-only: no form control, no task or saving claim
        for absent in ["<button", "<input", "<select", "<form", "saving", "saved"] {
            assert!(!w.contains(absent), "{absent}");
        }
        assert!(w.contains("Active task and progress are not supplied by this AgentQ projection."));
        assert!(w.contains("0.51.0 · ready") && w.contains("Host minimum 2048 bytes"));
    }

    /// G1 (#63): with the route on, only a fresh, complete, agentq.configure-granted installation gets one editor
    /// and draft selects for AgentQ's writable profile fields (no value equal to the current one); everything else
    /// keeps its precise read-only reason.
    #[test]
    fn agentq_editor_only_where_writes_are_possible() {
        let mut doc = agentq_doc();
        let rows = doc["installations"]["rows"].as_array_mut().unwrap();
        // profile_projection: grant the capability, mark it complete and AgentQ's writes released for Codex
        let r = rows.iter_mut().find(|r| r["host_label"] == "profile-projection").unwrap();
        r["capabilities"] = json!(["agentq.configure"]);
        let c = &mut r["snapshot"]["payload"]["configuration"];
        c["incomplete"] = json!(false);
        c["profiles"][0]["writes"] = json!({"enabled": true, "reasons": []});
        rows.iter_mut().find(|r| r["host_label"] == "profile-projection-stale").unwrap()["capabilities"] = json!(["agentq.configure"]);
        let mut w = String::new();
        integrations(&mut w, &doc["installations"], true);
        assert_eq!(w.matches("class=\"aq-edit\"").count(), 1, "{w}");
        assert!(w.contains("data-installation=\"inst-18\" data-revision=\"00112233445566ff\"><button type=\"button\" data-aq=\"review\">"), "{w}");
        let codex = w.split("<details id=").find(|s| s.contains("<b>Codex</b><small>profile-projection · ")).unwrap();
        assert!(codex.contains("data-profile=\"0123456789abcdef\" data-field=\"routing\"><option value=\"\">keep AgentQ</option><option value=\"direct\">Direct</option></select>"), "{codex}");
        assert!(codex.contains("data-field=\"optimization\"><option value=\"\">keep enabled</option><option value=\"disabled\">off</option>") && !codex.contains("data-field=\"plugin\""), "{codex}");
        // the same installation's unreleased profile, an ungranted installation and a stale one say why
        assert!(w.contains("Read-only: configuration adapter not released.</p>"));
        assert!(w.contains("Changes disabled: this connection has no agentq.configure grant.") && w.contains("Changes disabled: the snapshot is stale or incomplete."));
        assert_eq!(w.matches("<select").count(), 2);
    }

    /// first-screen.md F-A1 fixture: the pinned fixture plus two synthetic nodes (one long name with three filesystems,
    /// one warming up) and three boards (known zero, labels only, unknown).
    pub fn first_screen_doc() -> Value {
        let mut doc = crate::fixture_doc();
        let now = doc["generated_at"].as_u64().unwrap();
        let cpu = [12, 35, 61, 58, 72, 40, 44];
        let lf: Vec<Value> = (0..7u64)
            .map(|i| {
                let at = now - 1800 + 300 * i;
                let iso = crate::source::iso(at);
                json!({"id": format!("b{i}"), "traceId": format!("b{i}"), "startTime": iso, "metadata": {
                    "attributes.agentq.installation": "inst-build", "attributes.host.name": "build-runner-with-a-rather-long-hostname-eu-central-1", "attributes.agentq.runtime": "linux",
                    "attributes.agentq.resource.cpu_percent": cpu[i as usize], "attributes.agentq.resource.memory_used_bytes": 10_000_000_000u64 + i * 900_000_000,
                    "attributes.agentq.resource.memory_total_bytes": 16_000_000_000u64,
                    "attributes.agentq.resource.disks": format!("[{{\"id\":\"r00t\",\"role\":\"root\",\"fstype\":\"ext4\",\"used_bytes\":{},\"free_bytes\":10000000000,\"total_bytes\":100000000000}},{{\"id\":\"d474\",\"role\":\"other\",\"fstype\":\"xfs\",\"used_bytes\":400000000000,\"free_bytes\":600000000000,\"total_bytes\":1000000000000}},{{\"id\":\"b4ck\",\"role\":\"other\",\"fstype\":\"zfs\",\"used_bytes\":1900000000000,\"free_bytes\":100000000000,\"total_bytes\":2000000000000}}]", 88_000_000_000u64 + i * 100_000_000)}})
            })
            .chain([json!({"id": "m1", "traceId": "m1", "startTime": crate::source::iso(now - 120), "metadata": {"attributes.agentq.installation": "inst-mac", "attributes.host.name": "mac", "attributes.agentq.runtime": "macos", "attributes.agentq.resource.cpu_percent": 9.5, "attributes.agentq.resource.memory_used_bytes": 12_000_000_000u64, "attributes.agentq.resource.memory_total_bytes": 24_000_000_000u64, "attributes.agentq.resource.disks": "[{\"id\":\"m0\",\"role\":\"root\",\"fstype\":\"apfs\",\"used_bytes\":300000000000,\"free_bytes\":200000000000,\"total_bytes\":500000000000}]"}})])
            .collect();
        let refs: Vec<&Value> = lf.iter().collect();
        let extra = crate::resource::build(&refs, false, &json!([]), now, 1800, None);
        doc["resources"]["rows"].as_array_mut().unwrap().extend(extra["rows"].as_array().unwrap().iter().cloned());
        let ok = json!({"state": "ok", "at": now - 20, "age_s": 20, "reason": null, "incomplete": false});
        let zero = json!({"doing": 0, "review": 0, "ask": 0, "failed": 0, "ready": 0, "waiting": 0, "later": 0, "closed": null});
        let mut labels = zero.clone();
        labels["doing"] = json!(1);
        labels["waiting"] = json!(3);
        let none = json!({"doing": null, "review": null, "ask": null, "failed": null, "ready": null, "waiting": null, "later": null, "closed": null});
        for (p, counts, src) in [("taskqorg/coreq", labels, ok.clone()), ("taskqorg/specq", zero, ok), ("taskqorg/serverq", none, json!({"state": "unknown", "at": null, "age_s": null, "reason": "read failed (timeout)", "incomplete": false}))] {
            doc["boards"].as_array_mut().unwrap().push(json!({"project": p, "url": format!("https://github.com/{p}/issues"), "source": src, "counts": counts, "questions": [], "failed": [], "attempts": []}));
        }
        doc
    }

    /// first-screen.md F-A1/F-A5, overview.md, boards.md: four nodes beside six boards; trends only from two or more
    /// connected samples on 0–100 %; unknown stays `—` with its reason; zero counters hidden, unknown `?`, Closed
    /// unknown; no demo control, simulated value or threshold mark.
    #[test]
    fn first_screen_renders_servers_beside_boards() {
        let doc = first_screen_doc();
        let html = render(&doc, &json!({}));
        if let Some(out) = std::env::var_os("DASHQ_FIRST_EVIDENCE") {
            let css = include_str!("../assets/style.css");
            std::fs::write(out, html.replace("<link rel=\"stylesheet\" href=\"/style.css\">", &format!("<style>{css}</style>")).replace("<script src=\"/app.js\" defer></script>\n", "")).unwrap();
        }
        let first = &html[html.find("<div class=\"first\">").unwrap()..html.find("<section id=\"resources\">").unwrap()];
        // Attention above both columns, servers before Boards, deeper views after
        assert!(html.find("id=\"attention\"").unwrap() < html.find("class=\"first\"").unwrap() && first.find("id=\"servers\"").unwrap() < first.find("id=\"boards\"").unwrap());
        for id in ["agents", "hosts", "checks", "resources"] {
            assert!(html.contains(&format!("<section id=\"{id}\">")), "{id}");
        }
        assert_eq!(first.matches("<article class=\"node\"").count(), 4);
        assert_eq!(first.matches("Shared TaskQ blocks require JavaScript.").count(), 6);
        let node = |id: &str| first.split("<article").find(|a| a.contains(&format!("id=\"node-{id}\""))).unwrap().to_string();
        // 7 samples: CPU and RAM lines on 0–100 %, three filesystems with bars, pp change from the previous sample
        let build = node("inst-build");
        assert_eq!(build.matches("<polyline").count(), 2, "{build}");
        assert!(build.contains("aria-label=\"CPU, last 30 min, 0–100 %\"") && build.contains("id=\"cpu-inst-build\" data-n=\"44.0\">44.0 %</span> <small class=\"d\" title=\"change since the previous sample\">+4.0 pp</small>"), "{build}");
        assert_eq!(build.matches("class=\"disk\"").count(), 3);
        assert!(build.contains("1900.0 / 2000.0 GB · 95 %") && build.contains("<rect class=\"fill\" width=\"95.0\""));
        // one sample: values, no chart; a stat failure stays unknown with its reason
        let mac = node("inst-mac");
        assert!(mac.contains("no trend yet") && !mac.contains("<polyline") && mac.contains("9.5 %"), "{mac}");
        assert!(node("inst-fra").contains("title=\"unknown: stat failed\">—</span>"));
        assert_eq!(first.matches("Work mode: no TaskQ policy adapter</span>").count(), 4);
        assert!(first.contains("data-boards="));
        assert!(first.contains("Shared TaskQ blocks require JavaScript."));
        assert!(first.contains("https://github.com/taskqorg/taskq/issues"));
        assert!(!first.contains("class=\"counts\""), "no independent counter renderer");
    }

    /// D14/#46: shared fixture groups enter the existing server row, not another node list.
    /// This qualifies composition only; enrollment observation delivery remains a follow-up.
    #[test]
    fn shared_resources_fixture_in_server_row() {
        let mut doc = first_screen_doc();
        let fx: Value = serde_json::from_str(include_str!("../fixture/taskq/resource-controls.json")).unwrap();
        let groups = fx["work"][0]["groups"].clone();
        let node = doc["resources"]["rows"].as_array_mut().unwrap().last_mut().unwrap();
        node["installation"] = groups[0]["targets"][0]["target"]["installation_id"].clone();
        node["taskq_groups"] = groups;
        let html = render(&doc, &json!({}));
        assert_eq!(html.matches("data-taskq-groups=").count(), 1);
        assert_eq!(html.matches("<article class=\"node\"").count(), 4);
        assert!(html.contains("&quot;installation_id&quot;") && html.contains("&quot;project_id&quot;"));
        assert!(html.contains("<polyline") && html.contains("id=\"boards\""));
        if let Some(out) = std::env::var_os("DASHQ_RESOURCE_EVIDENCE") {
            std::fs::write(out, html).unwrap();
        }
    }

    #[test]
    fn integrations_absent_without_agentq_rows() {
        let mut w = String::new();
        integrations(&mut w, &Value::Null, true);
        integrations(&mut w, &json!({"rows": [{"product": "serverq"}, {"product": "agentq", "connection": "revoked"}]}), true);
        assert!(w.is_empty());
    }
}
