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

fn badge(state: &str, label: &str) -> String {
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

/// Questions read only (S3, S7): text, then the source choices with `k`; ★ marks the recommendation. No answer control.
fn questions(b: &Value) -> String {
    let v: Vec<String> = b["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| {
            let mut out = issue(q);
            if q["state"] == "review" {
                out += " <span class=\"s unknown\">review</span>";
            }
            if let Some(t) = q["text"].as_str() {
                let _ = write!(out, "<br><span class=\"detail\">{}</span>", esc(t));
            }
            let choices: Vec<String> = q["choices"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| format!("<li value=\"{}\">{}{}</li>", text(&c["k"]), text(&c["text"]), if q["recommended"] == c["k"] { " <span aria-label=\"recommended\">★</span>" } else { "" }))
                .collect();
            if !choices.is_empty() {
                let _ = write!(out, "<ol class=\"choices\">{}</ol>", choices.join(""));
            }
            out
        })
        .collect();
    if v.is_empty() { "—".into() } else { v.join("<br>") }
}

fn failures(b: &Value) -> String {
    let v: Vec<String> = b["failed"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|i| format!("{}{}", issue(i), i["reason"].as_str().filter(|r| !r.is_empty()).map(|r| format!("<br><span class=\"detail\">{}</span>", esc(r))).unwrap_or_default()))
        .collect();
    if v.is_empty() { "—".into() } else { v.join("<br>") }
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
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>dashq</title>\n<link rel=\"stylesheet\" href=\"/style.css\">\n<script src=\"/app.js\" defer></script>\n</head>\n<body>\n<header>\n<h1>dashq</h1>\n<nav><a href=\"{}\">Langfuse traces</a> · <a href=\"{}\">Langfuse dashboards</a></nav>\n</header>\n<main id=\"main\">\n",
        esc(links["traces"].as_str().unwrap_or("")),
        esc(links["dashboards"].as_str().unwrap_or(""))
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
    let _ = writeln!(w, "<p class=\"bar\">Updated <time>{}</time> · Period {} <button id=\"refresh\" type=\"button\">Refresh</button></p>", clock(now, now), periods.join(" · "));
    let _ = writeln!(w, "<p id=\"banner\" class=\"banner\" role=\"alert\" hidden></p>");
    let not_ok: Vec<String> = doc["sources"]
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, s)| s["state"] != "ok")
        .map(|(n, s)| {
            let name = match n.as_str() {
                "langfuse" => "Langfuse",
                "github" => "GitHub",
                "enrollment" => "Connections",
                "taskq" => "taskq",
                _ => "serverq",
            };
            format!("<li>{} {name}: {}</li>", badge(s["state"].as_str().unwrap_or("unknown"), s["state"].as_str().unwrap_or("unknown")), esc(s["reason"].as_str().unwrap_or("")))
        })
        .collect();
    if !not_ok.is_empty() {
        let _ = writeln!(w, "<ul class=\"summary\">{}</ul>", not_ok.join(""));
    }

    // 1. Attention
    let att = &doc["attention"];
    let _ = writeln!(w, "<section id=\"attention\">\n<h2>Attention</h2>");
    for (key, title) in [("current", "Current problems"), ("gaps", "Observation gaps"), ("incidents", "Incident history (24 h, deduplicated)")] {
        let items = att[key].as_array().cloned().unwrap_or_default();
        let _ = writeln!(w, "<h3>{title}</h3>");
        if items.is_empty() {
            let none = if att["source"]["state"] == "ok" || key == "incidents" { "None observed." } else { "None observed; some sources are not ok (above)." };
            let _ = writeln!(w, "<p class=\"note\">{none}</p>");
            continue;
        }
        let _ = writeln!(w, "<ul class=\"items\">");
        for i in items {
            let link = i["url"].as_str().map(|u| format!(" <a href=\"{}\">issue</a>", esc(u))).unwrap_or_default();
            let since = if i["since"].is_null() { " · in this period".into() } else { format!(" · since {}", at(&i["since"], now)) };
            let mut line = format!("<li><b>{}</b>{} · {}{since}", text(&i["source"]), i["host"].as_str().map(|x| format!(" · {}", esc(x))).unwrap_or_default(), text(&i["what"]));
            if !i["occurrences"].is_null() {
                let _ = write!(line, " · {} occurrences", text(&i["occurrences"]));
            }
            if let Some(d) = i["detail"].as_str().filter(|d| !d.is_empty()) {
                let _ = write!(line, "<br><span class=\"detail\">{}</span>", esc(d));
            }
            if let Some(nf) = i["native_failure"].as_object() {
                let fields: Vec<String> = nf.iter().map(|(k, v)| format!("{k} {}", if v.is_null() { "unknown".into() } else { text(v) })).collect();
                let _ = write!(line, "<br><span class=\"detail\">Last native failure receipt (at snapshot time, not a countdown): {}</span>", fields.join(" · "));
            }
            let _ = writeln!(w, "{line}{}{link}</li>", trace(&i["trace_id"], base));
        }
        let _ = writeln!(w, "</ul>");
    }
    let _ = writeln!(w, "</section>");

    installations(w, &doc["installations"], now);
    resources(w, &doc["resources"], now);

    // 2. Hosts
    let _ = writeln!(w, "<section id=\"hosts\">\n<h2>Hosts</h2>");
    let hosts = doc["hosts"].as_array().cloned().unwrap_or_default();
    if hosts.is_empty() {
        let _ = writeln!(w, "<p class=\"note\">No host seen in any source.</p>");
    } else {
        let _ = writeln!(w, "<table>\n<tr><th>Host</th><th>Snapshot</th><th>taskq</th><th>Monitor</th><th>Proxy</th><th>Selected</th><th>agentq latest</th><th>Updater</th><th>serverq</th><th>Services</th></tr>");
        for x in hosts {
            let (s, t, a, q) = (&x["snapshot"], &x["taskq"], &x["agentq"], &x["serverq"]);
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
            let snap = state(s, s["age_s"].as_u64().map_or("no snapshot".into(), |a| format!("{} old", dur(a))));
            let taskq = format!("{} {}", ver(&t["version"], &t["behind"]), state(t, t["age_s"].as_u64().map_or("unknown".into(), |a| format!("{} old", dur(a)))));
            // green only for a fresh report whose checks are all ok
            let q_state = if q["state"] == "ok" { json!({"state": match q["status"].as_str() { Some("ok") => "ok", Some("warn") => "warn", _ => "unknown" }}) } else { q.clone() };
            let serverq = state(&q_state, format!("{}{}", q["status"].as_str().unwrap_or("unknown"), q["age_s"].as_u64().map(|a| format!(", {} old", dur(a))).unwrap_or_default()));
            let updater = format!("{} · subscription {}", unk(&a["updater"]), unk(&a["subscription"]));
            let services: Vec<String> = x["services"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|v| {
                    let name = format!("{}{}", text(&v["service"]), v["agent"].as_str().map(|a| format!(" · {a}")).unwrap_or_default());
                    format!("<span title=\"{}\">{}</span>", text(&v["reason"]), badge(v["state"].as_str().unwrap_or("unknown"), &name))
                })
                .collect();
            let _ = writeln!(
                w,
                "<tr><td>{}</td><td>{snap}</td><td>{taskq}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{updater}</td><td>{serverq}</td><td>{}</td></tr>",
                text(&x["id"]),
                ver(&a["monitor"], &a["behind"]),
                ver(&a["proxy"], &a["behind"]),
                unk(&a["selected"]),
                unk(&a["latest"]),
                if services.is_empty() { "—".into() } else { services.join(" ") }
            );
        }
        let _ = writeln!(w, "</table>");
    }
    let _ = writeln!(w, "</section>");

    // 3. Boards
    let _ = writeln!(w, "<section id=\"boards\">\n<h2>Boards</h2>");
    let _ = writeln!(w, "<table>\n<tr><th>Project</th><th>Tasks</th><th>Questions</th><th>Failed</th><th>Live attempts</th></tr>");
    for b in doc["boards"].as_array().into_iter().flatten() {
        let src = &b["source"];
        let attempts: Vec<String> = b["attempts"].as_array().into_iter().flatten().map(|a| format!("{}: #{} {}", text(&a["host"]), text(&a["task"]), text(&a["attempt"]))).collect();
        let _ = writeln!(
            w,
            "<tr><td><a href=\"{}\">{}</a>{}</td><td>In work {} · Waiting for answer {} · Ready {}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
            esc(b["url"].as_str().unwrap_or("")),
            text(&b["project"]),
            unknown_line(src),
            num(&b["in_work"], src),
            num(&b["waiting"], src),
            num(&b["ready"], src),
            questions(b),
            failures(b),
            if attempts.is_empty() { "—".into() } else { attempts.join("<br>") }
        );
    }
    let _ = writeln!(w, "</table>\n</section>");

    // 4. Agents
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
    let _ = writeln!(w, "</table>\n<h3>Compression (successes, normal skips and failures are disjoint)</h3>\n<table>\n<tr><th>Host</th><th>Agent</th><th>Compressed</th><th>Normal skips</th><th>Failed</th><th>Unknown</th><th>Characters before → after</th></tr>");
    let cc = capped("compression");
    for c in ag["compression"].as_array().into_iter().flatten() {
        let chars = if c["chars_before"].is_null() { "<span class=\"unknown\" title=\"unknown: not every call has before/after\">—</span>".into() } else { format!("{} → {}", text(&c["chars_before"]), text(&c["chars_after"])) };
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{chars}{}</td></tr>", text(&c["host"]), name(&c["agent"]), num(&c["compressed"], &cc), num(&c["normal_skips"], &cc), num(&c["failed"], &cc), num(&c["unknown"], &cc), trace(&c["trace_id"], base));
    }
    let _ = writeln!(w, "</table>\n<h3>Native RTK (estimates, not model tokens or savings)</h3>\n<table>\n<tr><th>Host</th><th>Agent</th><th>Events</th><th>Estimated tokens before → after</th><th>Reduced · unchanged · expanded · no estimate</th><th>Origin production · qualification · unknown</th><th>Non-zero exits</th></tr>");
    let cr = capped("native_tools");
    for x in ag["native_rtk"].as_array().into_iter().flatten() {
        let est = if x["input_estimate"].is_null() { "<span class=\"unknown\" title=\"unknown: no estimate\">—</span>".into() } else { format!("{} → {}", text(&x["input_estimate"]), text(&x["output_estimate"])) };
        let o = &x["origins"];
        let _ = writeln!(
            w,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{est}</td><td>{} · {} · {} · {}</td><td>{} · {} · {}</td><td>{} of {}</td></tr>",
            text(&x["host"]), name(&x["agent"]), num(&x["calls"], &cr), text(&x["reduced"]), text(&x["unchanged"]), text(&x["expanded"]), text(&x["unknown"]), text(&o["production"]), text(&o["qualification"]), text(&o["unknown"]), text(&x["nonzero_exits"]), text(&x["exit_samples"])
        );
    }
    let _ = writeln!(w, "</table>\n<details>\n<summary>Process counters</summary>\n<p class=\"note\">Counters reset when their process restarts; a reset is not a drop. Values are from the snapshot time.</p>\n<table>\n<tr><th>Host</th><th>Service</th><th>Snapshot</th><th>Counters</th></tr>");
    for c in ag["counters"].as_array().into_iter().flatten() {
        let m: Vec<String> = c["metrics"].as_object().into_iter().flatten().map(|(k, v)| format!("{k} {}", text(v))).collect();
        let _ = writeln!(w, "<tr><td>{}</td><td>{}{}</td><td>{}</td><td>{}</td></tr>", text(&c["host"]), text(&c["service"]), c["agent"].as_str().map(|a| format!(" · {a}")).unwrap_or_default(), at(&c["at"], now), m.join(" · "));
    }
    let _ = writeln!(w, "</table>\n</details>\n</section>");

    // 5. Server checks
    let ch = &doc["checks"];
    let _ = writeln!(w, "<section id=\"checks\">\n<h2>Server checks</h2>\n{}<p class=\"note\">Report at {}</p>", unknown_line(&ch["source"]), at(&ch["at"], now));
    let _ = writeln!(w, "<table>\n<tr><th>Check</th><th>Status</th><th>Detail</th></tr>");
    for r in ch["results"].as_array().into_iter().flatten() {
        let st = r["status"].as_str().unwrap_or("unknown");
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td></tr>", text(&r["check"]), badge(st, st), text(&r["detail"]));
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
    let _ = writeln!(w, "</section>\n</main>\n</body>\n</html>");
    h
}

/// Connected installations (D11, enrollment C1): connection, data age, history warmup and export gaps are separate
/// states; a revoked installation keeps its history and is not an active failure. Absent until enrollment is set up.
fn installations(w: &mut String, inst: &Value, now: u64) {
    if inst.is_null() {
        return;
    }
    let _ = writeln!(w, "<section id=\"installations\">\n<h2>Installations</h2>\n{}", unknown_line(&inst["source"]));
    let rows = inst["rows"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        let _ = writeln!(w, "<p class=\"note\">No installation connected yet. Run the product's enroll command on its host.</p>\n</section>");
        return;
    }
    let _ = writeln!(w, "<table>\n<tr><th>Installation</th><th>Connection</th><th>Data</th><th>History</th><th>Export</th><th>Evidence</th><th></th></tr>");
    for r in rows {
        let (inst_id, ws) = (r["installation"].as_str().unwrap_or(""), r["workspace"].as_str().unwrap_or(""));
        let id = crate::enroll::anchor(inst_id, ws);
        let revoked = r["connection"] == "revoked";
        let connection = match r["connection"].as_str() {
            Some("active") => badge("ok", "active"),
            Some("pending") => format!("{} <span class=\"detail\">approved, waiting for the host</span>", badge("unknown", "pending")),
            _ => format!("{} <span class=\"detail\">history kept</span>", badge("unknown", "revoked")),
        };
        let o = &r["observation"];
        let data = match (o["state"].as_str(), o["age_s"].as_u64()) {
            (Some(st), Some(age)) if !revoked => format!("{}{}", badge(st, &format!("{} old", dur(age))), o["reason"].as_str().filter(|x| !x.is_empty()).map(|x| format!(" <span class=\"detail\">{}</span>", esc(x))).unwrap_or_default()),
            // a revoked installation's last data is history, not a stale failure
            (_, Some(_)) => format!("<span class=\"detail\">last {}</span>", at(&o["at"], now)),
            _ => badge("unknown", o["reason"].as_str().unwrap_or("unknown")),
        };
        let h = &r["history"];
        // a revoked installation sends no more samples: no warmup cue
        let history = if h["state"] == "warmup" && !revoked { badge("unknown", h["reason"].as_str().unwrap_or("")) } else { format!("{} sample{}", text(&h["samples"]), if h["samples"] == 1 { "" } else { "s" }) };
        let x = &r["export"];
        let export = if x["state"] == "gap" { badge("warn", &format!("gap · {}", x["reason"].as_str().unwrap_or(""))) } else { "<span class=\"detail\">no gap recorded</span>".into() };
        let evidence = match r["snapshot"]["event_id"].as_str() {
            Some(ev) => format!("<code>{}</code><br><span class=\"detail\">{}</span>", esc(ev), text(&r["snapshot"]["schema"])),
            None => "—".into(),
        };
        let action = if revoked { String::new() } else { format!("<a id=\"revoke-{id}\" href=\"/connect?revoke={}&amp;workspace={}\">Revoke…</a>", enc(inst_id), enc(ws)) };
        let _ = writeln!(
            w,
            "<tr id=\"{id}\"><td><b>{} ({})</b> · {}<br><span class=\"detail\">{} · workspace {} · generation {}</span></td><td>{connection}</td><td>{data}</td><td>{history}</td><td>{export}</td><td>{evidence}</td><td>{action}</td></tr>",
            text(&r["host_label"]), text(&r["runtime"]), text(&r["product"]), esc(inst_id), esc(ws), text(&r["generation"])
        );
    }
    let _ = writeln!(w, "</table>\n</section>");
}

/// Bytes in GB with one decimal (10^9, the producers' own figures).
fn gb(v: &Value) -> String {
    v.as_u64().map_or("—".into(), |b| format!("{:.1}", b as f64 / 1e9))
}

/// `resources` (D4.4) as plain current values with provenance and history state; charts are D10 (#76).
fn resources(w: &mut String, res: &Value, now: u64) {
    let rows = res["rows"].as_array().cloned().unwrap_or_default();
    let _ = writeln!(w, "<section id=\"resources\">\n<h2>Resources</h2>");
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
