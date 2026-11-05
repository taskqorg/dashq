//! Fleet (D26): observed metadata only; no stage/turn mixing or host control authority.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

const TOKENS: [&str; 4] = ["input_tokens", "cache_read_tokens", "cache_write_tokens", "output_tokens"];

pub fn cli_lines(body: &str) -> Value {
    let mut out = json!({});
    for line in body.lines().filter_map(|l| l.strip_prefix("cli ")) {
        let mut parts = line.split_whitespace();
        if let (Some(cli), Some(v), None) = (parts.next(), parts.next(), parts.next()) && crate::runtimes::clis().any(|(k, _)| k == cli) {
            out[cli] = crate::data::semver(&json!(v));
        }
    }
    out
}

/// A3: a capped read keeps the newest received rows. `dropped_newest_ms` is the newest producer time of a dropped
/// turn (unknown time: max), so a limit no newer than it is never shown as current.
pub fn keep_newest(lines: impl IntoIterator<Item = Value>, cap: usize) -> Value {
    let mut records = std::collections::VecDeque::with_capacity(cap.min(1024));
    let mut capped = false;
    let mut dropped: Option<u64> = None;
    for l in lines {
        if records.len() >= cap.max(1) {
            capped = true;
            if let Some(old) = records.pop_front().filter(|o: &Value| o["observation"]["kind"] == "agent.turn") {
                let at = old["observation"]["started_ms"].as_u64().unwrap_or(u64::MAX);
                dropped = Some(dropped.map_or(at, |d| d.max(at)));
            }
        }
        records.push_back(l);
    }
    json!({"records": records, "capped": capped, "dropped_newest_ms": dropped})
}

fn unknown(why: &str) -> Value {
    json!({"state": "unknown", "at": null, "age_s": null, "reason": why, "incomplete": false})
}

/// One version row per host CLI of the runtime table.
fn clis(host: &Value, inp: &Value, now: u64) -> Value {
    Value::Array(crate::runtimes::clis().map(|(cli, _)| version(host, cli, inp, now)).collect())
}

fn version(host: &Value, cli: &str, inp: &Value, now: u64) -> Value {
    let row = host["taskq"]["cli_heartbeats"].as_array().into_iter().flatten()
        .min_by_key(|r| r["age_s"].as_u64().unwrap_or(u64::MAX));
    let latest = &inp["cli_releases"][cli];
    let release_source = crate::data::source(latest, now, 180, None);
    let (running, mut source) = row.map_or((Value::Null, unknown("no TaskQ heartbeat")), |r| {
        let mut s = r["source"].clone();
        if s["state"] == "ok" && r["age_s"].as_u64().is_none_or(|a| a > r["ttl"].as_u64().unwrap_or(300)) {
            s = unknown("heartbeat stale or age unknown");
            s["state"] = json!(if r["age_s"].is_null() { "unknown" } else { "stale" });
        }
        s["at"] = r["at"].clone(); s["age_s"] = r["age_s"].clone();
        (r["clis"][cli].clone(), s)
    });
    if running.is_null() { source["state"] = json!("unknown"); source["reason"] = json!("CLI version missing or unsupported in heartbeat"); }
    let behind = if source["state"] == "ok" && release_source["state"] == "ok" {
        running.as_str().zip(latest["value"].as_str()).map(|(r,l)| coreq::update::newer(l,r))
    } else { None };
    json!({"runtime": cli, "running": running, "source": source, "latest": latest["value"], "release_source": release_source, "behind": behind})
}

fn code(v: &Value) -> Option<&str> {
    v.as_str().filter(|s| crate::data::safe(s, 128, b"_.-/@"))
}

/// Each token split has independent coverage; a missing split never becomes zero.
pub(crate) fn tokens(rows: &[&Value]) -> Value {
    let mut out = json!({});
    for k in TOKENS {
        let mut sum = Some(0u64);
        let mut known = 0;
        let mut reasons = BTreeSet::new();
        for r in rows {
            if let Some(n) = r[k].as_u64() {
                known += 1;
                sum = sum.and_then(|s| s.checked_add(n));
            } else {
                reasons.insert(code(&r["unknown"][k]).unwrap_or("not-reported").to_string());
            }
        }
        if sum.is_none() { reasons.insert("counter-overflow".into()); }
        out[k] = json!({"observed": if known == 0 { None } else { sum }, "known": known, "of": rows.len(), "state": if known == rows.len() && known > 0 && sum.is_some() { "ok" } else { "unknown" }, "reasons": reasons});
    }
    out
}

fn groups(rows: &[&Value], dimension: &str, now: u64, dropped_ms: Option<u64>) -> Vec<Value> {
    let mut grouped: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for r in rows {
        let label = match dimension {
            "attempt" => r["task"].as_u64().zip(r["attempt"].as_u64()).map(|(t,a)| format!("{} #{t} / {a}", code(&r["project"]).unwrap_or("unknown project"))),
            "account" => code(&r["account"]).map(|a| format!("{} / {a}", code(&r["runtime"]).unwrap_or("unknown runtime"))),
            _ => code(&r["project"]).map(String::from),
        }.unwrap_or_else(|| format!("unknown {dimension}"));
        grouped.entry(label).or_default().push(r);
    }
    grouped.into_iter().map(|(label, rs)| {
        let latest = rs.iter().max_by_key(|r| r["started_ms"].as_u64().unwrap_or(0)).unwrap();
        let cut = dropped_ms.is_some_and(|d| latest["started_ms"].as_u64().is_none_or(|s| s <= d));
        let limit = |k: &str| {
            if cut {
                return json!({"state": "unknown", "at": latest["started_ms"].as_u64().map(|t|t/1000), "age_s": latest["started_ms"].as_u64().map(|t|now.saturating_sub(t/1000)), "value": null, "reason": "Fleet row cap reached; a newer limit may be unread"});
            }
            let valid = match k {
                "limit_used_percent" => latest[k].as_f64().is_some_and(|n| n.is_finite() && (0.0..=100.0).contains(&n)),
                "limit_resets_at" => latest[k].as_u64().is_some(),
                _ => latest[k].as_u64().is_some() || code(&latest[k]).is_some(),
            };
            let stale=latest["started_ms"].as_u64().is_none_or(|at|now.saturating_sub(at/1000)>300) || latest["limit_resets_at"].as_u64().is_some_and(|t|t<=now);
            json!({"state": if !valid {"unknown"} else if stale {"stale"} else {"ok"}, "at": latest["started_ms"].as_u64().map(|t|t/1000), "age_s": latest["started_ms"].as_u64().map(|t|now.saturating_sub(t/1000)), "value": if valid { latest[k].clone() } else { Value::Null }, "reason": if valid && stale { json!("snapshot stale or reset elapsed") } else if valid { Value::Null } else if latest[k].is_null() { json!(code(&latest["unknown"][k]).unwrap_or("not-reported")) } else { json!("invalid-producer-value") }})
        };
        json!({"label": label, "turns": rs.len(), "tokens": tokens(&rs), "at": latest["started_ms"].as_u64().map(|t|t/1000), "limit_capped": cut,
            "limit_used_percent": limit("limit_used_percent"), "limit_resets_at": limit("limit_resets_at"), "limit_window": limit("limit_window")})
    }).collect()
}

fn window(stream: &Value, now: u64, seconds: u64) -> Value {
    let mut seen = BTreeSet::new();
    let mut turns = vec![];
    let mut completed = BTreeSet::new();
    let mut workers = BTreeSet::new();
    let mut untimed = 0;
    let mut unidentified = 0;
    let mut newest: Option<u64> = None;
    for l in stream["records"].as_array().into_iter().flatten() {
        let o = &l["observation"];
        let Some(id) = l["id"].as_str() else { unidentified += 1; continue };
        if !seen.insert(id) { continue }
        let Some(ms) = o["started_ms"].as_u64() else { untimed += 1; continue };
        let at = ms / 1000;
        if at > now { continue }
        newest=Some(newest.map_or(at,|old|old.max(at)));
        if at < now.saturating_sub(seconds) { continue }
        match o["kind"].as_str() {
            Some("agent.turn") => turns.push(o),
            Some("transition") if o["product"] == "taskq" && o["to"] == "closed" => {
                if let (Some(p), Some(t)) = (code(&o["project"]), o["task"].as_u64()) { let _identity = (p,t); completed.insert(id); }
            }
            Some("worker") if o["product"] == "taskq" && o["operation"] == "spawn" && o["outcome"] == "ok" => { workers.insert(id); }
            _ => {}
        }
    }
    // The transport certifies retained rows, not a whole-window producer baseline. Never claim a complete census.
    let t = &stream["telemetry"];
    let dropped = if stream["capped"] == true { Some(stream["dropped_newest_ms"].as_u64().unwrap_or(u64::MAX)) } else { None };
    let loss_reason = stream["known_loss"].as_array().into_iter().flatten().filter_map(|b| b["reason"].as_str()).collect::<Vec<_>>().join("; ");
    let reason = if stream["error"].is_string() { stream["error"].as_str().unwrap() }
        else if !loss_reason.is_empty() { &loss_reason }
        else if stream["capped"] == true { "Fleet row cap reached" }
        else if unidentified > 0 { "observations have no stable identity" }
        else if untimed > 0 { "observations have unknown time" }
        else if t["hole_rows"].as_u64().unwrap_or(0) > 0 || t["gap_rows"].as_u64().unwrap_or(0) > 0 || t["pruned"].as_array().is_some_and(|a|!a.is_empty()) { "telemetry has loss or retention gaps" }
        else if stream["records"].as_array().is_none_or(|a|a.is_empty()) { "no Fleet observations received" }
        else { "observed only; full-window producer coverage not established" };
    let mut source = unknown(reason);
    let any = !turns.is_empty() || !completed.is_empty() || !workers.is_empty();
    source["state"] = json!(if any && stream["error"].is_string() { "unconfirmed" } else if any && loss_reason.is_empty() { "incomplete" } else { "unknown" });
    source["incomplete"] = json!(any);
    source["at"] = json!(newest);
    source["age_s"] = json!(newest.map(|at| now.saturating_sub(at)));
    if any && !stream["error"].is_string() && loss_reason.is_empty() && newest.is_some_and(|at|now.saturating_sub(at)>300) {
        source["state"]=json!("stale"); source["reason"]=json!(format!("latest producer observation older than 300 s; {reason}"));
    }
    let mut out = json!({"seconds": seconds, "from": now.saturating_sub(seconds), "to": now, "source": source, "untimed": untimed, "capped": stream["capped"] == true, "last_received": t["last_synced"],
        "projects": groups(&turns,"project",now,dropped), "attempts": groups(&turns,"attempt",now,dropped), "accounts": groups(&turns,"account",now,dropped),
        "completed_observed": (!completed.is_empty()).then_some(completed.len()),
        "completions_per_hour_observed": (!completed.is_empty()).then_some(completed.len() as f64 / (seconds as f64 / 3600.0)),
        "workers_started_observed": (!workers.is_empty()).then_some(workers.len())});
    if !loss_reason.is_empty() { out["known_loss"] = stream["known_loss"].clone(); }
    out
}

pub fn build(inp: &Value, hosts: &Value) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let filter = inp["filters"]["host"].as_str();
    let mut rows: Vec<Value> = hosts.as_array().into_iter().flatten().map(|h| json!({"host": h["id"], "clis": clis(h,inp,now), "streams": []})).collect();
    for stream in inp["fleet_records"].as_array().into_iter().flatten() {
        // Worker/transition host codes are the producer's execution host, which can differ from the board host.
        let mut by_host: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for l in stream["records"].as_array().into_iter().flatten() {
            let host = code(&l["observation"]["host"]).or_else(|| code(&stream["host"])).unwrap_or("unknown");
            by_host.entry(host.into()).or_default().push(l.clone());
        }
        if by_host.is_empty() { by_host.insert(code(&stream["host"]).unwrap_or("unknown").into(), vec![]); }
        for (label, records) in by_host {
            if filter.is_some_and(|f| !f.eq_ignore_ascii_case(&label) && !f.split('/').next().is_some_and(|h|h.eq_ignore_ascii_case(&label))) { continue }
            let mut scoped = stream.clone(); scoped["records"] = json!(records);
            // Visual grouping never merges installation/workspace identities or sums their streams.
            let candidates: Vec<usize> = rows.iter().enumerate().filter(|(_,r)| r["host"].as_str().is_some_and(|h| h.eq_ignore_ascii_case(&label) || h.split('/').next().is_some_and(|h|h.eq_ignore_ascii_case(&label)))).map(|(i,_)|i).collect();
            let index = if candidates.len()==1 { candidates[0] } else {
                // A5/R2: no heartbeat joins this host, so both CLI versions stay visible as unknown.
                let none=json!({});
                rows.push(json!({"host":label,"clis":clis(&none,inp,now),"streams":[]})); rows.len()-1
            };
            for seconds in [86400,7*86400] {
                let mut win=window(&scoped,now,seconds);
                win["installation"]=stream["installation"].clone(); win["workspace"]=stream["workspace"].clone();
                win["host_provenance"]=json!("producer host code; installation label when absent");
                rows[index]["streams"].as_array_mut().unwrap().push(win);
            }
        }
    }
    rows.sort_by_key(|r| format!("{}|{}|{}",r["host"],r["workspace"],r["installation"]));
    let mut reasons = BTreeSet::new();
    for r in &rows {
        if r["streams"].as_array().is_some_and(|a|a.is_empty()) { reasons.insert(format!("{}: no associated D11 Fleet stream",r["host"].as_str().unwrap_or("unknown"))); }
        for w in r["streams"].as_array().into_iter().flatten() { reasons.insert(format!("{}: {}",r["host"].as_str().unwrap_or("unknown"),w["source"]["reason"].as_str().unwrap_or("unknown"))); }
        for c in r["clis"].as_array().into_iter().flatten() { for key in ["source","release_source"] { if c[key]["state"] != "ok" { reasons.insert(format!("{} {} {}: {}",r["host"].as_str().unwrap_or("unknown"),c["runtime"].as_str().unwrap_or("unknown"),key,c[key]["reason"].as_str().unwrap_or("unknown"))); } } }
    }
    json!({"hosts": rows, "source": unknown(&if reasons.is_empty() {"no host observations".into()} else {reasons.into_iter().collect::<Vec<_>>().join("; ")})})
}

fn value(v: &Value) -> String {
    if v.is_null() { "—".into() } else if let Some(s) = v.as_str() { crate::page::esc(s) } else { crate::page::esc(&v.to_string()) }
}

fn observed(v: &Value, capped: bool) -> String {
    if capped && !v.is_null() { format!("\u{2265}{} (capped)",value(v)) } else {value(v)}
}

pub fn render(w: &mut String, fleet: &Value) {
    use crate::page::esc;
    let _ = write!(w, "<section id=\"fleet\"><h2>Fleet</h2><p>Observed tokens · fixed 24 h and 7 d UTC windows. Host scope; agent/session filters do not apply. Partial counts are lower bounds, not complete totals.</p>");
    let rows = fleet["hosts"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() { w.push_str("<p>Unknown · no host observations received.</p>"); }
    for h in rows {
        let _ = write!(w,"<article><h3>{}</h3>",value(&h["host"]));
        for cli in h["clis"].as_array().into_iter().flatten() {
            let status = match cli["behind"].as_bool() { Some(true)=>"behind upstream latest", Some(false)=>"not behind upstream latest", None=>"comparison unknown" };
            let _ = write!(w,"<p>{}: {} · upstream latest {} · <span data-fleet-state>{}</span><br><small>Heartbeat: {} · {} · age {} s; release: {} · {} · age {} s</small></p>",value(&cli["runtime"]),value(&cli["running"]),value(&cli["latest"]),status,value(&cli["source"]["state"]),value(&cli["source"]["reason"]),value(&cli["source"]["age_s"]),value(&cli["release_source"]["state"]),value(&cli["release_source"]["reason"]),value(&cli["release_source"]["age_s"]));
        }
        if h["streams"].as_array().is_none_or(|a|a.is_empty()) { w.push_str("<p>Usage, account limits and cadence unknown · no associated D11 Fleet stream.</p>"); }
        for win in h["streams"].as_array().into_iter().flatten() {
            // D10: the summary keeps a stable id so refresh restores its focus.
            let id=format!("fleet-{}-{}-{}-{}",esc(win["installation"].as_str().unwrap_or("none")),esc(win["workspace"].as_str().unwrap_or("none")),esc(h["host"].as_str().unwrap_or("unknown")),win["seconds"]);
            let _ = write!(w,"<details id=\"{id}\" {}><summary id=\"{id}-summary\">{} · {} · {}</summary><p>{} to {} UTC · source age {} s</p><p>Observed completions: {} · completions/hour: {} · workers started: {}</p>",if win["seconds"]==86400 {" open"} else {""},if win["seconds"]==86400 {"24 h"} else {"7 d"},value(&win["source"]["state"]),value(&win["source"]["reason"]),crate::source::iso(win["from"].as_u64().unwrap_or(0)),crate::source::iso(win["to"].as_u64().unwrap_or(0)),value(&win["source"]["age_s"]),observed(&win["completed_observed"],win["capped"]==true),win["completions_per_hour_observed"].as_f64().map_or_else(||value(&Value::Null),|n|if win["capped"]==true {format!("\u{2265}{n:.3} (capped)")} else {format!("{n:.3}")}),observed(&win["workers_started_observed"],win["capped"]==true));
            let _=write!(w,"<p>Installation {} · workspace {} · {}</p>",value(&win["installation"]),value(&win["workspace"]),value(&win["host_provenance"]));
            for (key,title) in [("projects","Tokens per project"),("attempts","Tokens per task attempt"),("accounts","Tokens and limits per account")] {
                let _ = write!(w,"<h4>{title}</h4>");
                if win[key].as_array().is_none_or(|a|a.is_empty()) { w.push_str("<p>Unknown · no matching turn observations.</p>"); }
                for r in win[key].as_array().into_iter().flatten() {
                    let _ = write!(w,"<div class=\"fleet-row\"><strong>{}</strong>",value(&r["label"]));
                    for (k,label) in TOKENS.into_iter().zip(["Input","Cache read","Cache write","Output"]) {
                        let t=&r["tokens"][k];
                        let _ = write!(w,"<span>{label}: {} · {}/{} known {}</span>",observed(&t["observed"],win["capped"]==true),t["known"],t["of"],esc(&t["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
                    }
                    if key=="accounts" {
                        for (k,label) in [("limit_used_percent","Limit used %"),("limit_resets_at","Reset"),("limit_window","Limit window")] {
                            let display = if k=="limit_resets_at" { r[k]["value"].as_u64().filter(|t|*t<=253402300799).map_or_else(||value(&r[k]["value"]),|t|format!("{} UTC",crate::source::iso(t))) } else {value(&r[k]["value"])};
                            let _ = write!(w,"<span>{label}: {} {}</span>",display,value(&r[k]["reason"]));
                        }
                        let at=r["at"].as_u64().unwrap_or(0);
                        let _ = write!(w,"<span data-fleet-state>Limit observation {} UTC · {}</span>",crate::source::iso(at),if r["limit_capped"]==true {"row cap reached; current limit unknown"} else if win["to"].as_u64().unwrap_or(0).saturating_sub(at)>300 || r["limit_resets_at"]["value"].as_u64().is_some_and(|t|t<=win["to"].as_u64().unwrap_or(0)) {"stale; current limit unknown"} else {"turn observation"});
                    }
                    w.push_str("</div>");
                }
            }
            w.push_str("</details>");
        }
        w.push_str("</article>");
    }
    w.push_str("</section>");
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture() -> Value {
        let now=1_800_000_000u64;
        let turn=json!({"product":"taskq","kind":"agent.turn","started_ms":(now-60)*1000,"runtime":"codex","project":"dashq","task":244,"attempt":7,"account":"invented-account","input_tokens":100,"cache_read_tokens":20,"cache_write_tokens":null,"output_tokens":10,"unknown":{"cache_write_tokens":"not-reported","limit_resets_at":"not-reported"},"limit_used_percent":42,"limit_resets_at":null,"limit_window":300});
        let l=|id:&str,o:Value|json!({"id":id,"observation":o});
        json!({"now":now,"filters":{},"projects":["taskqorg/dashq"],"boards":{"taskqorg/dashq":{"at":now,"value":{"date":now,"issues":[],"hosts":[{"title":"taskq host pds18.ufobe.com","updated_at":crate::source::iso(now-10),"body":"machine pds18.ufobe.com\ncli claude unknown\ncli codex 1.1.0\nttl 300\n"}]}}},"fleet_records":[{"host":"pds18.ufobe.com","installation":"invented-installation","workspace":"invented-workspace","telemetry":{"last_synced":now-10},"records":[
            l("turn-1",turn.clone()),l("turn-1",turn),
            l("worker-1",json!({"product":"taskq","kind":"worker","operation":"spawn","outcome":"ok","started_ms":(now-30)*1000})),
            l("close-1",json!({"product":"taskq","kind":"transition","project":"dashq","task":244,"to":"closed","started_ms":(now-20)*1000})),
            l("old-turn",json!({"kind":"agent.turn","project":"coreq","started_ms":(now-2*86400)*1000,"input_tokens":200})),
            l("future-turn",json!({"kind":"agent.turn","started_ms":(now+1)*1000,"input_tokens":900})),
            l("stage",json!({"kind":"stage","started_ms":now*1000,"input_tokens":900}))
        ]}],"cli_releases":{"claude":{"at":now,"value":"2.1.0"},"codex":{"at":now,"value":"1.2.0"}}})
    }
    /// D26 A2-A4 / R2: duplicate rows, stage mixing, windows and missing splits must not inflate usage or imply completeness.
    #[test]
    fn fleet_usage_windows_and_cadence() {
        let f=fixture(); let d=build(&f,&json!([])); let w=&d["hosts"][0]["streams"];
        assert_eq!(w[0]["projects"].as_array().unwrap().len(),1);
        assert_eq!(w[1]["projects"].as_array().unwrap().len(),2);
        let t=&w[0]["accounts"][0]["tokens"];
        assert_eq!(t["input_tokens"]["observed"],100);
        assert!(t["cache_write_tokens"]["observed"].is_null());
        assert_eq!(w[0]["workers_started_observed"],1);
        assert_eq!(w[0]["completed_observed"],1);
        assert_eq!(w[0]["completions_per_hour_observed"],1.0/24.0);
        assert_eq!(w[0]["source"]["state"],"incomplete");
        assert_eq!(w[0]["accounts"][0]["limit_used_percent"]["value"],42);
        assert_eq!(w[0]["accounts"][0]["limit_resets_at"]["reason"],"not-reported");
        let mut reset=f.clone(); reset["fleet_records"][0]["records"][0]["observation"]["limit_resets_at"]=json!(1_800_000_300u64);
        assert_eq!(build(&reset,&json!([]))["hosts"][0]["streams"][0]["accounts"][0]["limit_resets_at"]["value"],1_800_000_300u64);
        let mut bad=f.clone(); bad["fleet_records"][0]["records"][0]["observation"]["limit_used_percent"]=json!(150);
        assert!(build(&bad,&json!([]))["hosts"][0]["streams"][0]["accounts"][0]["limit_used_percent"]["value"].is_null());
        bad["fleet_records"][0]["error"]=json!("store unavailable");
        assert_eq!(build(&bad,&json!([]))["hosts"][0]["streams"][0]["source"]["state"],"unconfirmed");
        let mut remote=f.clone(); remote["fleet_records"][0]["records"][2]["observation"]["host"]=json!("win");
        let remote=build(&remote,&json!([]));
        let win=remote["hosts"].as_array().unwrap().iter().find(|h|h["host"]=="win").unwrap();
        assert_eq!(win["streams"][0]["workers_started_observed"],1);
        assert!(win["streams"][0]["projects"].as_array().unwrap().is_empty());
        let mut empty=f.clone(); empty["fleet_records"][0]["records"]=json!([]);
        assert!(build(&empty,&json!([]))["hosts"][0]["streams"][0]["workers_started_observed"].is_null());
        empty["filters"]["host"]=json!("other/native");
        assert!(build(&empty,&json!([]))["hosts"].as_array().unwrap().is_empty());
    }
    /// D26 A5 / R2: a telemetry-only host keeps both CLI rows as unknown instead of hiding them.
    #[test]
    fn fleet_telemetry_only_host_shows_cli_unknowns() {
        let d=build(&fixture(),&json!([]));
        let clis=d["hosts"][0]["clis"].as_array().unwrap();
        assert_eq!(clis.iter().map(|c|c["runtime"].as_str().unwrap()).collect::<Vec<_>>(),["claude","codex"]);
        assert!(clis.iter().all(|c|c["running"].is_null() && c["source"]["state"]=="unknown" && c["behind"].is_null()));
        let mut html=String::new(); render(&mut html,&d);
        assert!(html.contains("claude: — ") && html.contains("codex: — "));
    }
    /// D26 A3 / R2: a capped read whose dropped turns may be newer never shows an older limit as current.
    #[test]
    fn fleet_capped_limits_are_not_current() {
        let mut f=fixture(); let now=f["now"].as_u64().unwrap();
        f["fleet_records"][0]["capped"]=json!(true);
        f["fleet_records"][0]["dropped_newest_ms"]=json!((now-30)*1000);
        let a=&build(&f,&json!([]))["hosts"][0]["streams"][0]["accounts"][0];
        assert!(a["limit_used_percent"]["value"].is_null(),"older limit shown as current");
        assert_eq!(a["limit_used_percent"]["state"],"unknown");
        f["fleet_records"][0]["dropped_newest_ms"]=json!((now-120)*1000);
        assert_eq!(build(&f,&json!([]))["hosts"][0]["streams"][0]["accounts"][0]["limit_used_percent"]["value"],42);
    }
    /// D26 A3: the read cap keeps the newest rows and records the newest dropped turn time.
    #[test]
    fn fleet_cap_keeps_newest_rows() {
        let t=|id:&str,s:u64|json!({"id":id,"observation":{"kind":"agent.turn","started_ms":s}});
        let w=json!({"id":"w","observation":{"kind":"worker","started_ms":9}});
        let r=keep_newest([t("a",5),t("b",3),w,t("c",7)],2);
        assert_eq!(r["records"].as_array().unwrap().iter().map(|l|l["id"].as_str().unwrap()).collect::<Vec<_>>(),["w","c"]);
        assert_eq!(r["capped"],true);
        assert_eq!(r["dropped_newest_ms"],5);
        assert_eq!(keep_newest([t("a",5)],2)["capped"],false);
    }
    /// D18/D26: stale and failed release/heartbeat reads never qualify drift; missing CLI stays unknown.
    #[test]
    fn fleet_versions_keep_both_source_states() {
        let mut f=fixture(); let now=f["now"].as_u64().unwrap();
        let h=json!({"id":"pds18.ufobe.com/native","taskq":{"cli_heartbeats":[{"at":now-10,"age_s":10,"ttl":300,"source":{"state":"ok"},"clis":cli_lines("cli codex 1.1.0\ncli claude unknown\n")}]}});
        assert_eq!(version(&h,"codex",&f,now)["behind"],true);
        assert!(version(&h,"claude",&f,now)["behind"].is_null());
        f["cli_releases"]["codex"]["error"]=json!("timeout");
        assert!(version(&h,"codex",&f,now)["behind"].is_null());
        f["cli_releases"]["codex"]["error"]=Value::Null;
        assert!(version(&h,"codex",&f,now+301)["behind"].is_null());
    }
}
