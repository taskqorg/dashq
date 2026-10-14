//! D17 / P1-P3: bounded Langfuse metadata -> AgentQ accounting -> Results.
use crate::page::esc;
use crate::source::{enc, iso};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt::Write;

pub fn build(inp: &Value) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let period = inp["period"].as_u64().unwrap_or(1800);
    let lf = &inp["langfuse"];
    let f = &inp["filters"];
    let mut sources = BTreeMap::<String, Value>::new();
    let mut latest = None;
    for name in ["agentq.compress", "agentq.run", "agentq.installation"] {
        for row in lf["value"][name].as_array().into_iter().flatten() {
            if row["id"].as_str().is_some_and(|s| s.starts_with("t-")) {
                continue;
            }
            let attr = |k: &str| row["metadata"][format!("attributes.{k}")].clone();
            let host = attr("host.name");
            let runtime = attr("agentq.runtime");
            let host_id = format!("{}/{}", host.as_str().unwrap_or(""), if runtime == "wsl" { "wsl" } else { "native" });
            if f["host"].as_str().is_some_and(|s| s != host.as_str().unwrap_or("") && s != host_id) {
                continue;
            }
            let mut r = json!({});
            // Only schema fields cross the adapter; raw content and arbitrary metadata never reach /data.
            for k in [
                "schema",
                "event_id",
                "time",
                "installation",
                "runtime",
                "version",
                "profile",
                "agent",
                "adapter",
                "tool",
                "runner",
                "engine",
                "engine_version",
                "outcome",
                "reason",
                "bytes_in",
                "bytes_out",
                "est_tokens_in",
                "est_tokens_out",
                "session",
            ] {
                let v = attr(&format!("agentq.{k}"));
                r[k] = if v == "null" { Value::Null } else { v };
            }
            for k in ["installation", "runtime", "version", "profile", "engine_version"] {
                if !r[k].as_str().is_some_and(crate::data::safe_id) {
                    r[k] = Value::Null;
                }
            }
            // Tool and runner names are producer enums, never user commands.
            if !["shell", "Bash", "exec_command", "bash"].contains(&r["tool"].as_str().unwrap_or("")) {
                r["tool"] = json!("unknown");
            }
            if ![
                "cargo test",
                "cargo build",
                "cargo check",
                "git status",
                "git diff",
                "git log",
                "pytest",
                "npm test",
            ]
            .contains(&r["runner"].as_str().unwrap_or(""))
            {
                r["runner"] = Value::Null;
            }
            if !["claude", "codex", "hermes"].contains(&r["agent"].as_str().unwrap_or("")) {
                r["agent"] = json!("unknown");
            }
            if r["time"].is_null() {
                r["time"] = row["startTime"].as_str().and_then(crate::source::iso_secs).map_or(Value::Null, |t| json!(t));
            }
            if name != "agentq.installation"
                && (f["agent"].as_str().is_some_and(|s| r["agent"] != if s == "claude-code" { "claude" } else { s })
                    || f["session"].as_str().is_some_and(|s| r["session"] != s && row["sessionId"] != s))
            {
                continue;
            }
            if let Some(t) = r["time"].as_u64().filter(|t| *t <= now) {
                latest = Some(latest.unwrap_or(0).max(t));
            }
            let key = json!([r["installation"], r["runtime"]]).to_string();
            let source = sources.entry(key).or_insert_with(|| json!({"installation": r["installation"], "runtime": r["runtime"], "observed": [], "unknown": "exported events carry no coverage", "records": [], "capped": lf["incomplete"] == true}));
            if name == "agentq.installation" {
                continue;
            }
            if name == "agentq.run" {
                // Keep legacy overlap separate unless its producer supplied the same core event ID.
                let attributes: Vec<Value> = r.as_object().unwrap().iter().map(|(k,v)| json!({"key": format!("agentq.{k}"), "value": if v.is_number() { json!({"intValue": v}) } else { json!({"stringValue": if v.is_null() { json!("null") } else { v.clone() }}) }})).collect();
                r = json!({"name": name, "attributes": attributes});
            }
            source["records"].as_array_mut().unwrap().push(r);
        }
    }
    let input = json!({"now": now, "window": {"start": now.saturating_sub(period), "end": now}, "bucket": period / 12, "cap": crate::source::CAP * 2, "sources": sources.into_values().collect::<Vec<_>>()});
    let mut out = crate::accounting::aggregate(&input).unwrap_or_else(|_| json!({"state": "unknown", "reason": "invalid accounting window"}));
    out["source"] = crate::data::source(lf, now, 600, latest);
    // A successful HTTP read does not supply a producer observation time.
    if latest.is_none() {
        out["source"] = json!({"state": "unknown", "at": null, "age_s": null, "reason": lf["error"].as_str().unwrap_or("no source observation"), "incomplete": lf["incomplete"] == true});
    }
    out
}

fn val(v: &Value) -> String {
    v.as_u64().map_or("—".into(), |n| n.to_string())
}
fn label(v: &Value) -> String {
    esc(v.as_str().unwrap_or("unknown"))
}
fn counts(v: &Value) -> String {
    format!(
        "{} compressed · {} passed · {} failed",
        val(&v["compressed"]),
        val(&v["passed"]),
        val(&v["failed"])
    )
}
fn stack(v: &Value, max: f64, height: f64) -> String {
    let mut svg = String::new();
    let mut y = height;
    for k in ["compressed", "passed", "failed"] {
        let h = v[k].as_u64().unwrap_or(0) as f64 / max.max(1.) * height;
        y -= h;
        let _ = write!(svg, "<rect class=\"{k}\" x=\"0\" y=\"{y:.3}\" width=\"100\" height=\"{h:.3}\"/>");
    }
    svg
}

pub fn render(w: &mut String, doc: &Value) {
    let a = &doc["optimization"];
    let period = doc["period"].as_u64().unwrap_or(1800);
    let f = &doc["filters"];
    let observation = label(&a["source"]["state"]);
    let _ = write!(
        w,
        "<section id=\"aqresults\" data-observation=\"{observation}\"><div class=\"app\"><div class=\"line spread\"><h2>AgentQ › Results</h2><div class=\"line\"><label>Period <select id=\"results-period\" data-results-filter=\"period\">"
    );
    for (n, t) in [(1800, "30 min"), (7200, "2 hours"), (86400, "24 hours")] {
        let _ = write!(w, "<option value=\"{n}\"{}>{t}</option>", if n == period { " selected" } else { "" });
    }
    let _ = write!(w, "</select></label><label>Client <select id=\"results-client\" data-results-filter=\"agent\">");
    for (k, t) in [("", "All"), ("codex", "Codex"), ("claude-code", "Claude Code"), ("hermes", "Hermes")] {
        let _ = write!(
            w,
            "<option value=\"{k}\"{}>{t}</option>",
            if f["agent"].as_str().unwrap_or("") == k { " selected" } else { "" }
        );
    }
    let _ = write!(
        w,
        "</select></label></div></div><div class=\"line spread\"><small>{} – {} · UTC · [start, end)</small><small>Source: {} · {}</small></div>",
        a["window"]["start"].as_u64().map(iso).unwrap_or_else(|| "unknown".into()),
        a["window"]["end"].as_u64().map(iso).unwrap_or_else(|| "unknown".into()),
        label(&a["source"]["state"]),
        a["source"]["age_s"]
            .as_u64()
            .map(|n| format!("{} old", crate::data::dur(n)))
            .unwrap_or_else(|| "observation time unknown".into())
    );
    let _ = write!(
        w,
        "<p class=\"coverage\">{} · {}{}</p>",
        label(&a["state"]),
        if a["state"] == "complete" {
            "Complete observed window"
        } else if a["state"] == "partial" {
            "Observed counts only; incomplete coverage. No complete-period rate."
        } else {
            "Missing coverage; no calls is not proven."
        },
        a["source"]["reason"].as_str().map(|s| format!(" · {}", esc(s))).unwrap_or_default()
    );
    let _ = write!(w, "<div class=\"stats\"><strong>{} Calls</strong>", val(&a["calls"]));
    for (k, t) in [("compressed", "✓ Compressed"), ("passed", "↪ Passed"), ("failed", "× Failed")] {
        let _ = write!(
            w,
            "<button type=\"button\" id=\"results-{k}\" data-result-outcome=\"{k}\" aria-controls=\"reasons-{k}\" aria-expanded=\"false\"><strong>{}</strong> {t}</button>",
            val(&a["outcomes"][k])
        );
    }
    let _ = write!(
        w,
        "</div><svg class=\"composition\" viewBox=\"0 0 100 7\" preserveAspectRatio=\"none\" role=\"img\" aria-label=\"{}\">",
        counts(&a["outcomes"])
    );
    let total = a["calls"].as_u64().unwrap_or(0).max(1) as f64;
    let mut x = 0.;
    for k in ["compressed", "passed", "failed"] {
        let width = a["outcomes"][k].as_u64().unwrap_or(0) as f64 / total * 100.;
        let _ = write!(w, "<rect class=\"{k}\" x=\"{x:.3}\" width=\"{width:.3}\" height=\"7\"/>");
        x += width;
    }
    let _ = write!(w, "</svg>");
    for k in ["compressed", "passed", "failed"] {
        let _ = write!(w, "<div id=\"reasons-{k}\" class=\"detail\" hidden><strong>{k} · reasons</strong>");
        let mut any = false;
        for (reason, n) in a["reasons"]
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(r, _)| crate::accounting::outcome_of(r) == Some(k))
        {
            any = true;
            let _ = write!(w, "<div class=\"reason\"><span>{}</span><strong>{}</strong></div>", esc(reason), val(n));
        }
        if !any {
            let _ = write!(
                w,
                "<p>{}</p>",
                if a["state"] == "unknown" {
                    "Unknown: no source coverage"
                } else {
                    "No observed reasons"
                }
            );
        }
        if k == "failed" {
            let _ = write!(
                w,
                "<small>Optimization failure does not establish task failure. Evidence below; original-output fallback is unverified.</small>"
            );
        }
        let _ = write!(w, "</div>");
    }
    let buckets: Vec<&Value> = a["buckets"].as_array().into_iter().flatten().collect();
    let max = buckets.iter().filter_map(|b| b["calls"].as_u64()).max().unwrap_or(0).max(1) as f64;
    let scale_label = if a["state"] == "unknown" {
        "Scale unknown".into()
    } else {
        format!("0–{max} calls / bucket")
    };
    let _ = write!(
        w,
        "<div class=\"columns\"><section><div class=\"line spread\"><h3>Calls over time</h3><small>{scale_label}</small></div><div class=\"history\">"
    );
    let bucket_s = buckets.first().and_then(|b| b["end"].as_u64().zip(b["start"].as_u64())).map(|(b, a)| b - a);
    for b in buckets {
        let title = format!(
            "{} – {} UTC · {} · {}",
            iso(b["start"].as_u64().unwrap_or(0)),
            iso(b["end"].as_u64().unwrap_or(0)),
            label(&b["state"]),
            counts(&b["outcomes"])
        );
        let _ = write!(
            w,
            "<svg class=\"bucket {}\" viewBox=\"0 0 100 100\" preserveAspectRatio=\"none\" role=\"img\" aria-label=\"{title}\"><title>{title}</title>{}</svg>",
            label(&b["state"]),
            stack(&b["outcomes"], max, 100.)
        );
    }
    let _ = write!(
        w,
        "</div><small>Event time · {} s buckets · gaps are unknown, not zero</small></section><section><h3>Tool-result volume</h3>",
        bucket_s.map_or("unknown".into(), |n| n.to_string())
    );
    let volumes: Vec<&Value> = a["volume"].as_array().into_iter().flatten().collect();
    let scale = volumes
        .iter()
        .flat_map(|v| [v["bytes_in"].as_u64().unwrap_or(0), v["bytes_out"].as_u64().unwrap_or(0)])
        .max()
        .unwrap_or(0)
        .max(1) as f64;
    if volumes.is_empty() {
        let _ = write!(w, "<p>— No paired compressed calls observed</p>");
    }
    for v in volumes {
        let _ = write!(
            w,
            "<p><strong>{} · {}</strong> · {} reduction</p>",
            label(&v["adapter"]),
            label(&v["engine"]),
            v["reduction"].as_f64().map(|r| format!("{:.1}%", r * 100.)).unwrap_or("—".into())
        );
        for (k, t) in [("bytes_in", "Before"), ("bytes_out", "After")] {
            let n = v[k].as_u64().unwrap_or(0);
            if v["paired"] == 0 {
                let _ = write!(w, "<p>{t} — unknown: no valid byte pairs</p>");
                continue;
            }
            let width = n as f64 / scale * 100.;
            let _ = write!(
                w,
                "<div class=\"volume\"><span>{t}</span><svg viewBox=\"0 0 100 12\" preserveAspectRatio=\"none\" aria-hidden=\"true\"><rect class=\"{}\" width=\"{width:.3}\" height=\"12\"/></svg><strong title=\"{n} bytes\">{}</strong></div>",
                if k == "bytes_out" { "compressed" } else { "passed" },
                if n < 1048576 {
                    format!("{n} B")
                } else {
                    format!("{:.2} MiB", n as f64 / 1048576.)
                }
            );
        }
        let _ = write!(
            w,
            "<small>{}/{} paired / observed compressed · excluded: {}</small>",
            val(&v["paired"]),
            val(&v["compressed"]),
            esc(&v["excluded"].to_string())
        );
    }
    let _ = write!(
        w,
        "<small>Common zero-based byte scale · 1 MiB = 2²⁰ bytes. Byte reduction is not token or cost savings.</small></section></div><section class=\"section\"><h3>Clients</h3><div class=\"profile rowhead\"><span>Profile · recipe</span><span>✓</span><span>↪</span><span>×</span></div>"
    );
    let cohorts: Vec<&Value> = a["cohorts"].as_array().into_iter().flatten().collect();
    if cohorts.is_empty() {
        let _ = write!(
            w,
            "<p>{} · — · unknown profile and recipe; no observed calls</p>",
            f["agent"].as_str().map(esc).unwrap_or("All clients".into())
        );
    }
    for c in cohorts {
        let query = format!(
            "/?period={period}&agent={}{}#aqresults",
            enc(if c["agent"] == "claude" {
                "claude-code"
            } else {
                c["agent"].as_str().unwrap_or("")
            }),
            ["host", "session"]
                .iter()
                .filter_map(|k| f[k].as_str().map(|v| format!("&{k}={}", enc(v))))
                .collect::<String>()
        );
        let _ = write!(
            w,
            "<div class=\"profile\"><span>{} · {}<small>{} · {} · {} {}</small><small>AgentQ {} · installation {} · {}</small></span><span>{}</span><span>{}</span><span>{}</span></div>",
            if c["agent"] == "unknown" {
                label(&c["agent"])
            } else {
                format!("<a href=\"{}\">{}</a>", esc(&query), label(&c["agent"]))
            },
            label(&c["profile"]),
            label(&c["runtime"]),
            label(&c["adapter"]),
            label(&c["engine"]),
            label(&c["engine_version"]),
            label(&c["version"]),
            label(&c["installation"]),
            label(&c["runner"]),
            val(&c["outcomes"]["compressed"]),
            val(&c["outcomes"]["passed"]),
            val(&c["outcomes"]["failed"])
        );
    }
    let _ = write!(
        w,
        "<small>✓ Compressed · ↪ Passed unchanged · × Optimization failed</small></section><details id=\"results-effect\" class=\"section\"><summary id=\"results-effect-summary\">Whole-task effect · not measured</summary><p>Unknown: no identified comparable paired experiment cohort is available from this source. The selected live period is not an experiment.</p><small>A qualified comparison must identify runs, versions, sample size, quality, retries/rework, overhead, latency, separate observed usage and uncertainty. Byte reduction cannot establish task benefit.</small></details><details id=\"results-evidence\" class=\"section\"><summary id=\"results-evidence-summary\">Coverage &amp; evidence</summary><p>Schema {} · duplicates {} · unmatched legacy runs {} · excluded {}</p>",
        label(&a["schema"]),
        val(&a["coverage"]["duplicates"]),
        val(&a["coverage"]["legacy_run"]),
        esc(&a["coverage"]["excluded"].to_string())
    );
    for s in a["coverage"]["sources"].as_array().into_iter().flatten() {
        let _ = write!(
            w,
            "<p>{} · {} · {} · {} records<br><small>Observed {} · gaps {} · dropped {} · resets {} · reasons {}</small></p>",
            label(&s["installation"]),
            label(&s["runtime"]),
            label(&s["state"]),
            val(&s["records"]),
            esc(&s["observed"].to_string()),
            esc(&s["gaps"].to_string()),
            val(&s["dropped"]),
            val(&s["resets"]),
            esc(&s["reasons"].to_string())
        );
    }
    for c in a["cohorts"].as_array().into_iter().flatten() {
        let _ = write!(
            w,
            "<p>{} · {} · {}<br><small>Core event IDs: {}</small></p>",
            label(&c["agent"]),
            label(&c["adapter"]),
            label(&c["engine"]),
            esc(&c["evidence"].to_string())
        );
    }
    let _ = write!(w, "</details></div></section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(r: &Value, name: &str) -> Value {
        let mut meta = serde_json::Map::new();
        for (k, v) in r.as_object().unwrap() {
            meta.insert(format!("attributes.agentq.{k}"), v.clone());
        }
        meta.insert("attributes.host.name".into(), json!("fixture"));
        json!({"id": "span", "name": name, "startTime": iso(r["time"].as_u64().unwrap()), "metadata": meta})
    }

    // P1-P3 chain: producer-shaped metadata -> filtered projection -> safe Results, revised original bucket.
    #[test]
    fn observations_to_results() {
        let f: Value = serde_json::from_str(include_str!("../fixture/agentq/accounting-fixtures.json")).unwrap();
        let mut r = f["base_record"].clone();
        r["event_id"] = json!("000000000000000000000000000000a1");
        r["installation"] = json!("fixture-installation");
        r["runtime"] = json!("linux");
        let row = observation(&r, "agentq.compress");
        let mut input = json!({"now":2000,"period":1800,"filters":{},"langfuse":{"at":2000,"value":{"agentq.compress":[row.clone(),row]},"incomplete":false}});
        let a = build(&input);
        assert_eq!(a["calls"], 1);
        assert_eq!(a["coverage"]["duplicates"], 1);
        assert_eq!(a["state"], "partial");
        assert_eq!(a["source"]["at"], 1500);
        assert_eq!(a["source"]["age_s"], 500);
        assert_eq!(a["volume"][0]["bytes_in"], 4096);
        assert_eq!(a["volume"][0]["bytes_out"], 1024);
        assert_eq!(a["buckets"][8]["calls"], 1);
        assert!(a["buckets"][0]["calls"].is_null());
        input["filters"] = json!({"agent":"hermes"});
        assert!(build(&input)["calls"].is_null());
        input["filters"] = json!({"host":"elsewhere"});
        assert!(build(&input)["calls"].is_null());
        input["filters"] = json!({"session":"missing"});
        assert!(build(&input)["calls"].is_null());
        input["filters"] = json!({});
        input["langfuse"]["error"] = json!("timeout");
        assert_eq!(build(&input)["source"]["state"], "unconfirmed");
        input["langfuse"]["value"] = Value::Null;
        assert!(build(&input)["calls"].is_null());
        assert_eq!(build(&input)["source"]["reason"], "timeout");
        r["profile"] = json!("/private/raw-path");
        r["tool"] = json!("secret command args");
        r["runner"] = json!("secret command args");
        r["prompt"] = json!("secret prompt");
        input["langfuse"] = json!({"at":2000,"value":{"agentq.compress":[observation(&r,"agentq.compress")]}});
        let safe = build(&input).to_string();
        assert!(!safe.contains("private") && !safe.contains("secret"));

        {
            let output = std::env::var_os("DASHQ_RESULTS_PROOF").map(std::path::PathBuf::from);
            if let Some(dir) = &output {
                std::fs::create_dir_all(dir).unwrap();
            }
            // Same renderer for every published population; no fixture period multiplication.
            for name in [
                "mixed_versions_and_recipes",
                "valid_zero",
                "missing_source_is_unknown",
                "partial_source_is_lower_bound",
                "late_and_duplicate_delivery",
                "pass_through_and_failure_are_distinct",
            ] {
                let c = f["cases"].as_array().unwrap().iter().find(|c| c["name"] == name).unwrap();
                let mut i = c["input"].clone();
                for s in i["sources"].as_array_mut().unwrap() {
                    for r in s["records"].as_array_mut().into_iter().flatten() {
                        if let Some(o) = r.as_object() {
                            let mut m = f["base_record"].as_object().unwrap().clone();
                            m.extend(o.clone());
                            *r = Value::Object(m.into_iter().filter(|(_, v)| *v != json!("$absent")).collect());
                        }
                    }
                }
                if name == "mixed_versions_and_recipes" {
                    let rows: Vec<Value> = i["sources"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .flat_map(|s| s["records"].as_array().unwrap().iter().map(|r| observation(r, "agentq.compress")))
                        .collect();
                    for period in crate::data::PERIODS {
                        for agent in ["", "codex", "claude-code", "hermes"] {
                            if !agent.is_empty() {
                                assert!(crate::data::query(&format!("period={period}&agent={agent}")).is_ok());
                            }
                            let filters = if agent.is_empty() { json!({}) } else { json!({"agent":agent}) };
                            let input = json!({"now":2000,"period":period,"filters":filters,"langfuse":{"at":2000,"value":{"agentq.compress":rows}}});
                            let a = build(&input);
                            assert_eq!(
                                a["calls"],
                                match agent {
                                    "hermes" => Value::Null,
                                    "codex" => json!(1),
                                    "claude-code" => json!(2),
                                    _ => json!(3),
                                }
                            );
                            let mut html = String::new();
                            render(&mut html, &json!({"optimization":a,"period":period,"filters":filters}));
                            if let Some(dir) = &output {
                                std::fs::write(dir.join(format!("production-{period}-{agent}.html")),format!("<!doctype html><html><head><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><link rel=\"stylesheet\" href=\"/style.css\"><script src=\"/app.js\" defer></script></head><body><main id=\"main\"><p id=\"banner\" hidden></p>{html}</main></body></html>")).unwrap();
                            }
                        }
                    }
                }
                i["bucket"] = json!(100);
                let mut a = crate::accounting::aggregate(&i).unwrap();
                a["source"] = if a["state"] == "unknown" {
                    json!({"state":"unknown","at":null,"age_s":null,"reason":"no source observation"})
                } else {
                    json!({"state":"ok","at":1500,"age_s":500})
                };
                let doc = json!({"optimization":a,"filters":{},"period":1800});
                let mut html = String::new();
                render(&mut html, &doc);
                assert!(html.contains("Whole-task effect · not measured"));
                assert!(html.contains("UTC · [start, end)"));
                if let Some(dir) = &output {
                    std::fs::write(dir.join(format!("{name}.html")), format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><link rel=\"stylesheet\" href=\"/style.css\"><script src=\"/app.js\" defer></script></head><body><main id=\"main\"><p id=\"banner\" hidden></p>{html}</main></body></html>")).unwrap();
                }
            }
        }
    }
}
