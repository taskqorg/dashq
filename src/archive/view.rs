use super::*;
use crate::{page::esc, source::enc};
use std::collections::HashMap;
use std::fmt::Write;

pub(super) fn page(e: &Enroll, subject: &str, body: &str) -> String {
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Session archive · DashQ</title><link rel=\"stylesheet\" href=\"/style.css\"></head><body class=\"archive-page\"><header><a href=\"/\">DashQ</a> / <a href=\"/archive\">Session archive</a></header><main><h1>Session archive</h1><p>Full session content · owner and explicit readers only. Best-effort scrubbing can miss secrets. Last shipped bytes, not a live relay.</p>{}<p class=\"note\">Signed in as {} · Content retention: 365 days; index has no expiry. Every content read is audited.</p>{body}</main></body></html>",
        search_form(e, subject),
        esc(subject)
    )
}
fn csrf(e: &Enroll, subject: &str) -> String {
    format!(
        "<input type=\"hidden\" name=\"csrf\" value=\"{}\">",
        e.token_for(&format!("archive\n{subject}"))
    )
}
fn search_form(e: &Enroll, subject: &str) -> String {
    format!(
        "<form method=\"post\" action=\"/archive/search\" class=\"archive-search\">{}<label>Search all sessions <input type=\"search\" name=\"q\" required placeholder=\"Names, conditions or transcript text\"></label><button>Search</button></form>",
        csrf(e, subject)
    )
}
fn step_data(step: &agentq_runtime::agent::history::Step) -> String {
    use agentq_runtime::agent::history::StepKind;
    if step.kind != StepKind::Shell {
        return format!(
            "<h3>Full step data</h3><pre>{}</pre>",
            esc(&serde_json::to_string_pretty(&step.data).unwrap_or_default())
        );
    }
    let mut data = step.data.clone();
    let mut console = String::new();
    for (field, label) in [
        ("stdout", "Standard output"),
        ("stderr", "Standard error"),
        ("output", "Combined output"),
    ] {
        let content = data
            .as_object_mut()
            .and_then(|o| o.remove(field))
            .unwrap_or(Value::Null);
        let text = content.as_str().map(String::from).unwrap_or_else(|| {
            if content.is_null() {
                format!(
                    "unknown: {}",
                    step.data["unknown"][field]
                        .as_str()
                        .unwrap_or("not reported")
                )
            } else {
                content.to_string()
            }
        });
        write!(
            console,
            "<details open><summary>{label}</summary><pre>{}</pre></details>",
            esc(&text)
        )
        .unwrap();
    }
    format!(
        "<h3>Command and result</h3><pre>{}</pre>{console}",
        esc(&serde_json::to_string_pretty(&data).unwrap_or_default())
    )
}

fn value(v: &Value) -> String {
    match v {
        Value::Null => "unknown".into(),
        Value::String(s) => esc(s),
        _ => esc(&v.to_string()),
    }
}
fn arg<'a>(b: &'a HashMap<String, String>, k: &str) -> &'a str {
    b.get(k).map_or("", String::as_str)
}

pub(super) fn route(
    e: &Enroll,
    o: &objects::Objects,
    path: &str,
    b: &HashMap<String, String>,
    subject: &str,
    now: i64,
) -> Res<Resp> {
    if matches!(path, "/archive/raw" | "/archive/step") {
        let (s, src, bytes) = read_source(
            e,
            o,
            arg(b, "id"),
            arg(b, "source"),
            subject,
            if path == "/archive/raw" {
                "raw"
            } else {
                "step"
            },
            now,
        )?;
        if path == "/archive/raw" {
            let mut r = reply("200 OK", "application/octet-stream", bytes, false);
            r.headers =
                "Content-Disposition: attachment; filename=\"session-source.bin\"\r\n".into();
            return Ok(r);
        }
        if src.media {
            return Err("not_found".into());
        }
        let (steps, _) = steps(o, &s, &src, &bytes)?;
        let step = steps
            .into_iter()
            .find(|s| s.id == arg(b, "step"))
            .ok_or(Failure("not_found"))?;
        let output = format!(
            "<p><a href=\"/archive?id={}\">Back to run</a></p><h2>{} · {}</h2><p>{} · {} · revision {}</p>{}<details><summary>Exact raw record and source range</summary><pre>{}</pre></details>",
            enc(&s.id),
            esc(&format!("{:?}", step.kind)),
            esc(&step.author),
            esc(&step.state),
            esc(step
                .unknown
                .as_deref()
                .unwrap_or("mapped by AgentQ runtime")),
            step.revision,
            step_data(&step),
            esc(&serde_json::to_string_pretty(&step.source).unwrap_or_default())
        );
        return Ok(reply(
            "200 OK",
            "text/html; charset=utf-8",
            page(e, subject, &output).into_bytes(),
            true,
        ));
    }
    let id = arg(b, "id");
    let body = if !id.is_empty() {
        detail(e, o, id, subject, now)?
    } else {
        list(e, o, b, subject, now, path == "/archive/search")?
    };
    Ok(reply(
        "200 OK",
        "text/html; charset=utf-8",
        page(e, subject, &body).into_bytes(),
        true,
    ))
}

fn contains(v: &Value, term: &str) -> bool {
    match v {
        Value::String(s) => s.to_lowercase().contains(term),
        Value::Array(a) => a.iter().any(|v| contains(v, term)),
        Value::Object(o) => o.values().any(|v| contains(v, term)),
        _ => v.to_string().to_lowercase().contains(term),
    }
}

fn list(
    e: &Enroll,
    o: &objects::Objects,
    b: &HashMap<String, String>,
    subject: &str,
    now: i64,
    search: bool,
) -> Res<String> {
    let mut sessions = {
        let c = e.store().lock()?;
        let mut q=c.prepare("SELECT id,workspace,installation,manifest,name,received,deleted FROM archive.archive_sessions WHERE workspace IN (SELECT workspace FROM archive_owners WHERE subject=?1 UNION SELECT workspace FROM archive_readers WHERE subject=?1) ORDER BY received DESC,id")?;
        q.query_map([subject], session_row)?
            .collect::<Result<Vec<_>, _>>()?
    };
    sessions = {
        let c = e.store().lock()?;
        let mut rows = Vec::new();
        for s in sessions {
            for v in versions(&c, &s.id)? {
                let mut row = s.clone();
                row.manifest = v.manifest;
                row.version = Some(v.hash);
                rows.push(row);
            }
        }
        rows
    };
    let mut out = String::new();
    write!(out,"<h2>Find and compare runs</h2><form action=\"/archive\" method=\"get\" class=\"archive-filters\">").unwrap();
    for (field, label) in [
        ("project", "Project"),
        ("task", "Task"),
        ("attempt", "Attempt"),
        ("runtime", "Runtime"),
        ("model", "Model"),
        ("host", "Host"),
    ] {
        write!(
            out,
            "<label>{label}<input name=\"{field}\" value=\"{}\"></label>",
            esc(arg(b, field))
        )
        .unwrap();
        let filter = arg(b, field);
        if !filter.is_empty() {
            sessions.retain(|s| match &s.manifest[field] {
                Value::String(v) => v == filter,
                Value::Null => false,
                v => v.to_string() == filter,
            });
        }
    }
    write!(
        out,
        "<button>Filter</button> <a href=\"/archive\">Reset</a></form>"
    )
    .unwrap();
    if search {
        let term = arg(b, "q").to_lowercase();
        if term.is_empty() {
            return Err("invalid_request".into());
        }
        let mut found = Vec::new();
        let mut content_hits = HashMap::new();
        let mut unavailable = 0usize;
        for s in sessions {
            let metadata = s.name.to_lowercase().contains(&term) || contains(&s.manifest, &term);
            if metadata {
                found.push(s);
                continue;
            }
            if s.deleted {
                continue;
            }
            if let Some(hit) = content_hits.get(&s.id) {
                if *hit {
                    found.push(s);
                }
                continue;
            }
            let srcs = {
                let c = e.store().lock()?;
                sources(&c, &s, now)?
            };
            let mut hit = false;
            for src in srcs.into_iter().filter(|s| !s.expired && !s.media) {
                let bytes = match read_source(e, o, &s.id, &src.id, subject, "search", now) {
                    Ok((_, _, bytes)) => bytes,
                    Err(_) => {
                        unavailable += 1;
                        continue;
                    }
                };
                if String::from_utf8_lossy(&bytes)
                    .to_lowercase()
                    .contains(&term)
                {
                    hit = true;
                } else if let Ok((steps, _)) = steps(o, &s, &src, &bytes) {
                    hit |= steps.iter().any(|s| contains(&s.data, &term));
                }
            }
            content_hits.insert(s.id.clone(), hit);
            if hit {
                found.push(s);
            }
        }
        sessions = found;
        out.push_str("<p>Search results across all authorized retained sessions; expired or deleted content cannot be searched.</p>");
        if unavailable > 0 {
            write!(out,"<p>Search incomplete: {unavailable} sources unavailable. Other sources were searched; missing matches are unknown.</p>").unwrap();
        }
    }
    write!(out,"<p>{} session condition versions in scope. Metrics are reported agent.turn quantities, never inferred from transcript text.</p><div class=\"archive-table\"><table><thead><tr><th>Run</th><th>Task / attempt</th><th>Runtime / model</th><th>Conditions</th><th>Usage</th><th>Duration (µs) / observed outcomes</th><th>Archive position</th></tr></thead><tbody>",sessions.len()).unwrap();
    let c = e.store().lock()?;
    for s in &sessions {
        let srcs = sources(&c, s, now)?;
        let bytes: Option<i64> = srcs.iter().try_fold(0i64, |n, s| n.checked_add(s.end));
        let state = if s.deleted {
            "deleted"
        } else if srcs.iter().any(|s| s.expired) {
            "expired content"
        } else {
            "shipped"
        };
        let label = if s.name.is_empty() {
            s.manifest["session"].as_str().unwrap_or("Unnamed run")
        } else {
            &s.name
        };
        let metrics = metrics(&c, s)?;
        write!(out,"<tr><td><a href=\"/archive?id={}\">{}</a><small>{} · {}<br>{}</small></td><td>{} / {}</td><td>{}<br>{}</td><td><details><summary>Conditions</summary><dl class=\"archive-conditions\">",enc(&s.id),esc(label),value(&s.manifest["project"]),value(&s.manifest["host"]),esc(&s.ws),value(&s.manifest["task"]),value(&s.manifest["attempt"]),value(&s.manifest["runtime"]),value(&s.manifest["model"])).unwrap();
        for (k, label) in [
            ("effort", "Effort"),
            ("policy", "Policy"),
            ("account_label", "Account"),
            ("cli_version", "CLI"),
            ("instructions_hash", "Instructions"),
            ("actor", "Actor"),
            ("entry", "Entry"),
        ] {
            write!(out, "<dt>{label}</dt><dd>{}</dd>", value(&s.manifest[k])).unwrap();
        }
        write!(
            out,
            "<dt>Manifest SHA-256</dt><dd>{}</dd>",
            esc(s.version.as_deref().unwrap_or("unknown"))
        )
        .unwrap();
        write!(out,"</dl></details></td><td>{}</td><td>{}<br>{}</td><td>{state}<br>{} bytes · {} sources<small>received {} · age {}s</small></td></tr>",usage(&metrics),value(&metrics["duration_us"]),value(&metrics["outcome"]),bytes.map_or("unknown".into(),|n|n.to_string()),srcs.len(),esc(&crate::source::clock(s.received.max(0) as u64,now.max(0) as u64)),now.saturating_sub(s.received).max(0)).unwrap();
    }
    out.push_str("</tbody></table></div>");
    if sessions.is_empty() {
        out.push_str("<p>No sessions match, or no archive owner/reader role has been assigned to this account.</p>");
    }
    // Owner-only project deletion remains explicit and requires an acknowledgement in the POST form.
    let mut q =
        c.prepare("SELECT workspace FROM archive_owners WHERE subject=?1 ORDER BY workspace")?;
    let workspaces = q
        .query_map([subject], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    if !workspaces.is_empty() {
        write!(out,"<details><summary>Delete a project's archived content</summary><form method=\"post\" action=\"/archive/delete\">{}<label>Workspace<select name=\"workspace\">",csrf(e,subject)).unwrap();
        for ws in workspaces {
            write!(out, "<option>{}</option>", esc(&ws)).unwrap();
        }
        out.push_str("</select></label><label>Exact project<input name=\"project\" required></label><label><input type=\"checkbox\" name=\"confirm\" value=\"delete\" required> Permanently delete content in every matching session; keep index and refuse replay.</label><button>Delete project content</button></form></details>");
    }
    Ok(out)
}

fn detail(e: &Enroll, o: &objects::Objects, id: &str, subject: &str, now: i64) -> Res<String> {
    let (s, srcs, own) = {
        let c = e.store().lock()?;
        let s = session(&c, id, subject, false)?;
        let srcs = sources(&c, &s, now)?;
        let own = owner(&c, &s.ws, subject)?;
        (s, srcs, own)
    };
    let mut out = format!(
        "<p><a href=\"/archive\">All sessions</a></p><h2>{}</h2><h3>First recorded conditions</h3><pre class=\"archive-manifest\">{}</pre>",
        esc(if s.name.is_empty() {
            s.manifest["session"].as_str().unwrap_or("Unnamed run")
        } else {
            &s.name
        }),
        esc(&serde_json::to_string_pretty(&s.manifest).unwrap_or_default())
    );
    {
        let c = e.store().lock()?;
        out.push_str("<details><summary>All manifest versions and first byte positions</summary>");
        for v in versions(&c, id)? {
            write!(out, "<h3>{}</h3><ul>", esc(&v.hash)).unwrap();
            for (source, start) in v.positions {
                write!(out, "<li>{} · byte {start}</li>", esc(&source)).unwrap();
            }
            write!(
                out,
                "</ul><pre>{}</pre>",
                esc(&serde_json::to_string_pretty(&v.manifest).unwrap_or_default())
            )
            .unwrap();
        }
        out.push_str("</details>");
    }
    if s.deleted {
        out.push_str("<p>Deleted permanently. Index retained; upload replay refused.</p>");
        return Ok(out);
    }
    if own {
        write!(out,"<form method=\"post\" action=\"/archive/name\">{}<input type=\"hidden\" name=\"id\" value=\"{}\"><label>Session name<input name=\"name\" value=\"{}\"></label><button>Save name</button></form><details><summary>Delete session content</summary><form method=\"post\" action=\"/archive/delete\">{}<input type=\"hidden\" name=\"id\" value=\"{}\"><label><input type=\"checkbox\" name=\"confirm\" value=\"delete\" required> Permanently delete all session content; keep index and refuse replay.</label><button>Delete content</button></form></details>",csrf(e,subject),esc(id),esc(&s.name),csrf(e,subject),esc(id)).unwrap();
    }
    for src in srcs {
        write!(out,"<section><h3>Source {}</h3><p>{} · shipped through byte {} · producer time {} · {} replacements reported</p>",esc(&src.id),esc(&src.format),src.end,esc(&crate::source::clock(src.observed.max(0) as u64,now.max(0) as u64)),src.scrubbed).unwrap();
        write!(
            out,
            "<p>Last received {} · age {}s</p>",
            esc(&crate::source::clock(
                src.received.max(0) as u64,
                now.max(0) as u64
            )),
            now.saturating_sub(src.received).max(0)
        )
        .unwrap();
        if src.expired {
            out.push_str("<p>Content expired after 365 days. Index retained.</p></section>");
            continue;
        }
        if src.media && !src.complete {
            write!(out,"<p>Media pending: {} of {} bytes; unavailable until complete and whole-object checksum verified.</p></section>",src.end,src.total.map_or("unknown".into(),|n|n.to_string())).unwrap();
            continue;
        }
        write!(
            out,
            "<p><a href=\"/archive/raw?id={}&amp;source={}\">Download exact raw bytes{}</a></p>",
            enc(id),
            enc(&src.id),
            if src.media {
                " (media; never rendered)"
            } else {
                ""
            }
        )
        .unwrap();
        if src.media {
            out.push_str("</section>");
            continue;
        }
        let (s, src, bytes) = match read_source(e, o, id, &src.id, subject, "normalize", now) {
            Ok(read) => read,
            Err(_) => {
                out.push_str(
                    "<p>Source unavailable. Other sources remain available.</p></section>",
                );
                continue;
            }
        };
        match steps(o,&s,&src,&bytes) {
            Ok((steps,pending))=>{
                if pending>0 {write!(out,"<p>Incomplete final native record: {pending} pending bytes. Raw download includes them.</p>").unwrap();}
                out.push_str("<ol class=\"archive-steps\">");
                for step in steps {
                    write!(out,"<li><a href=\"/archive/step?id={}&amp;source={}&amp;step={}\">{:?} · {} · {}</a><small>{}</small></li>",enc(id),enc(&src.id),enc(&step.id),step.kind,esc(&step.author),esc(&step.state),esc(step.unknown.as_deref().unwrap_or("Open full output and exact raw record"))).unwrap();
                }
                out.push_str("</ol>");
            }
            Err(Failure("mapping_unknown"))=>out.push_str("<p>Steps unknown: unsupported format or invalid native source. Exact raw bytes remain available; no record is guessed or discarded.</p>"),
            Err(_)=>out.push_str("<p>Source normalization unavailable. Other sources remain available.</p>"),
        }
        out.push_str("</section>");
    }
    Ok(out)
}

/// Byte bindings select the manifest in force; ambiguous/missing multi-version attribution stays unknown.
pub(super) fn metrics(c: &Connection, s: &Session) -> Res<Value> {
    let desired = s
        .version
        .clone()
        .unwrap_or_else(|| coreq::enroll::digest(&s.manifest));
    let versions = versions(c, &s.id)?;
    let mut q=c.prepare("SELECT event_id,payload FROM observations WHERE workspace=?1 AND installation=?2 AND kind='agent.turn' AND json_extract(payload,'$.observation.session_id')=?3")?;
    let stored = q
        .query_map(
            params![s.ws, s.installation, s.manifest["session"].as_str()],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let identity = |v: &Value| {
        v.as_str()
            .map(String::from)
            .or_else(|| v.as_u64().map(|n| n.to_string()))
    };
    let mut rows = Vec::new();
    let mut unattributed = 0usize;
    for (event, payload) in stored {
        let parsed: Value = serde_json::from_str(&payload).map_err(|_| Failure("unavailable"))?;
        let r = &parsed["observation"];
        let conditions = r["project"] == s.manifest["project"]
            && ["task", "attempt"].iter().all(|k| {
                (r[*k].is_null() && s.manifest[*k].is_null())
                    || identity(&r[*k])
                        .zip(identity(&s.manifest[*k]))
                        .is_some_and(|(a, b)| a == b)
            });
        if !conditions {
            continue;
        }
        let mut q = c.prepare(
            "SELECT source,start FROM archive.archive_turns WHERE session=?1 AND event_id=?2",
        )?;
        let bindings = q
            .query_map([&s.id, &event], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut hashes = std::collections::BTreeSet::new();
        let mut missing = false;
        if bindings.is_empty() && versions.len() == 1 {
            hashes.insert(versions[0].hash.clone());
        } else if bindings.is_empty() {
            missing = true;
        }
        for (source, start) in bindings {
            match version_at(c, &s.id, &source, start)? {
                Some(h) => {
                    hashes.insert(h);
                }
                None => missing = true,
            }
        }
        if missing || hashes.len() != 1 {
            unattributed += 1;
        } else if hashes.contains(&desired) {
            rows.push(r.clone());
        }
    }
    let refs: Vec<_> = rows.iter().collect();
    let duration = if rows.is_empty() || unattributed > 0 {
        None
    } else {
        rows.iter().try_fold(0u64, |sum, r| {
            r["duration_us"].as_u64().and_then(|n| sum.checked_add(n))
        })
    };
    let mut tokens = crate::fleet::tokens(&refs);
    if unattributed > 0 {
        for (_, v) in tokens.as_object_mut().into_iter().flatten() {
            v["state"] = json!("unknown");
            v["of"] = json!(rows.len() + unattributed);
        }
    }
    let outcomes: std::collections::BTreeSet<_> =
        rows.iter().filter_map(|r| r["outcome"].as_str()).collect();
    Ok(
        json!({"records":rows.len(),"unattributed":unattributed,"duration_us":duration,"outcome":if rows.is_empty() || unattributed>0 || rows.iter().any(|r|r["outcome"].is_null()){Value::Null}else{json!(outcomes)},"tokens":tokens}),
    )
}

fn usage(metrics: &Value) -> String {
    let mut out = String::new();
    for (k, label) in [
        ("input_tokens", "Input"),
        ("cache_read_tokens", "Cache read"),
        ("cache_write_tokens", "Cache write"),
        ("output_tokens", "Output"),
    ] {
        let t = &metrics["tokens"][k];
        write!(
            out,
            "<div>{label}: {}</div>",
            if t["state"] == "ok" {
                value(&t["observed"])
            } else {
                "unknown".into()
            }
        )
        .unwrap();
        if t["state"] != "ok" && t["known"].as_u64().unwrap_or(0) > 0 {
            write!(
                out,
                "<small>{}/{} reported</small>",
                value(&t["known"]),
                value(&t["of"])
            )
            .unwrap();
        }
    }
    write!(
        out,
        "<small>{} agent.turn records</small>",
        value(&metrics["records"])
    )
    .unwrap();
    if metrics["unattributed"].as_u64().unwrap_or(0) > 0 {
        write!(
            out,
            "<small>Manifest attribution unknown for {} records</small>",
            value(&metrics["unattributed"])
        )
        .unwrap();
    }
    out
}
