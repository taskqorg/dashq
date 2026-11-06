//! D30: a scoped, read-only inventory projection and deterministic observed trends. No AI or host access.
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use crate::page::{at, badge, esc};
use crate::source::enc;

const WINDOW: i64 = 7 * 86400;
const FRESH: u64 = 7200;

/// Keep seven days plus the last older sample per entity, so stale current values never disappear.
#[derive(Default)]
pub struct Samples(BTreeMap<String, (Value, BTreeMap<i64, Value>)>);
impl Samples {
    pub fn push(&mut self, row: crate::store::Observation, now: i64) {
        let Ok(v) = serde_json::from_str::<Value>(&row.payload) else { return };
        let o = &v["observation"];
        let (Some(host), Some(at)) = (o["host"].as_str(), o["started_ms"].as_i64().map(|x| x / 1000)) else { return };
        let inv = &o["inventory"];
        let key = json!([row.installation, row.workspace, row.generation, host.to_lowercase(), inv["entity"], inv["identity"]]).to_string();
        let (identity, history) = self.0.entry(key).or_insert_with(|| (json!({"installation": row.installation, "workspace": row.workspace, "generation": row.generation, "host": host}), BTreeMap::new()));
        let sample = json!({"at": at, "inventory": inv, "provenance": o["source"], "operation":o["operation"], "conflict": false});
        if let Some(old) = history.get_mut(&at) {
            if old["inventory"] != *inv { old["conflict"] = json!(true); }
        } else { history.insert(at, sample); }
        // Keep the newest spelling; equal timestamps cannot select a different identity.
        if history.last_key_value().is_some_and(|(t, _)| *t == at) { identity["host"] = json!(host); }
        let older: Vec<_> = history.range(..now - WINDOW).map(|(at, _)| *at).collect();
        for at in older.iter().take(older.len().saturating_sub(1)) { history.remove(at); }
    }
    pub fn finish(self) -> Value {
        json!(self.0.into_values().map(|(mut id, h)| { id["history"] = json!(h.into_values().collect::<Vec<_>>()); id }).collect::<Vec<_>>())
    }
}

fn unknown(why: &str) -> Value { json!({"state": "unknown", "at": null, "age_s": null, "reason": why, "incomplete": false}) }

fn entity_source(s: &Value, snap: &Value, now: u64) -> Value {
    let at = s["at"].as_u64();
    if at.is_none_or(|t| t > now) { return unknown("producer time missing or in the future") }
    let fresh=match s["operation"].as_str() {Some("resource.self")=>90,Some("resource.storage")=>900,_=>FRESH};
    let mut src = crate::data::source(&json!({"value": true, "at": at, "error": snap["error"], "incomplete": false}), now, fresh, at);
    src["fresh_for_s"]=fresh.into();
    if s["conflict"] == true { src["state"] = json!("unknown"); src["reason"] = json!("conflicting samples at one producer time"); }
    else if src["state"] == "ok" && s["inventory"]["status"] == "partial" { src["state"] = json!("incomplete"); src["reason"] = json!("producer reports partial inventory"); src["incomplete"] = json!(true); }
    src
}

fn number(inv: &Value, field: &str) -> Option<f64> {
    let n = inv[field].as_u64()? as f64;
    if field == "cpu_used_milli" && inv["cpu_count"].as_u64().is_some_and(|c| n > c as f64 * 1000.0) { return None }
    if field.starts_with("disk_") && inv["disk_total_bytes"].as_u64().is_some_and(|total| inv["disk_used_bytes"].as_u64().zip(inv["disk_free_bytes"].as_u64()).is_some_and(|(u,f)| u.checked_add(f).is_none_or(|sum| sum > total))) { return None }
    let total = match field { "ram_used_bytes" => "ram_total_bytes", "disk_used_bytes" | "disk_free_bytes" => "disk_total_bytes", _ => return Some(n) };
    if inv[total].as_u64().is_some_and(|t| n > t as f64) { None } else { Some(n) }
}

fn trend(history: &[Value], field: &str, capacity: Option<&str>, now: u64, src: &Value) -> Value {
    let fail = |why: &str| json!({"state": "unknown", "reason": why, "per_day": null, "exhausts_at": null});
    if src["state"] != "ok" { return fail(src["reason"].as_str().unwrap_or("current sample is not fresh and complete")) }
    let history: Vec<_> = history.iter().filter(|s| s["at"].as_i64().is_some_and(|t| t >= now as i64 - WINDOW && t <= now as i64)).collect();
    if history.len() < 3 { return fail("short history: need 3 samples spanning 24 h") }
    let first = history[0]["at"].as_u64().unwrap();
    let last = *history.last().unwrap();
    let last_at = last["at"].as_u64().unwrap();
    if last_at - first < 86400 { return fail("short history: need 3 samples spanning 24 h") }
    if history.windows(2).any(|w| w[1]["at"].as_u64().unwrap() - w[0]["at"].as_u64().unwrap() > FRESH) { return fail("gap over 7200 s in history") }
    if history.iter().any(|s| s["conflict"] == true || s["inventory"]["status"].as_str() != Some("ok")) { return fail("partial or conflicting history") }
    if let Some(c) = capacity {
        if last["inventory"][c].as_u64().is_none_or(|v| v == 0) || history.iter().any(|s| s["inventory"][c] != last["inventory"][c]) { return fail("capacity missing or changed") }
    }
    let points: Option<Vec<_>> = history.iter().map(|s| Some(((s["at"].as_u64()? - first) as f64 / 86400.0, number(&s["inventory"], field)?))).collect();
    let Some(points) = points else { return fail("missing or inconsistent history field") };
    let n = points.len() as f64;
    let (mx, my) = (points.iter().map(|p| p.0).sum::<f64>() / n, points.iter().map(|p| p.1).sum::<f64>() / n);
    let slope = points.iter().map(|(x,y)| (x-mx)*(y-my)).sum::<f64>() / points.iter().map(|(x,_)| (x-mx).powi(2)).sum::<f64>();
    if !slope.is_finite() { return fail("trend is not finite") }
    let mut out = json!({"state": "ok", "reason": null, "per_day": slope, "samples": n as u64, "from": first, "to": last_at, "exhausts_at": null});
    if field == "disk_used_bytes" {
        let inv = &last["inventory"];
        let (Some(free), Some(used), Some(total)) = (number(inv, "disk_free_bytes"), number(inv, field), inv["disk_total_bytes"].as_u64()) else { return fail("free space or capacity missing or inconsistent") };
        if used + free > total as f64 { return fail("inconsistent disk space") }
        if slope <= 0.0 { out["reason"] = json!("no observed positive disk growth; exhaustion date unknown"); }
        else {
            let t = last_at as f64 + free / slope * 86400.0;
            if t.is_finite() && t <= i64::MAX as f64 { out["exhausts_at"] = json!(t.ceil() as u64); }
            else { return fail("exhaustion date out of range") }
        }
    }
    out
}

fn telegram_entry(base: &str) -> bool {
    base.strip_prefix("https://t.me/").is_some_and(|bot| !bot.is_empty() && bot.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
}

/// D30: the only protocol PilotQ's chat reads is the digest in Telegram's `start` parameter.
fn pilotq_link(base: &str, selector: &str) -> String {
    if !telegram_entry(base) { return format!("{base}?server={}",enc(selector)); }
    let digest = selector.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h,b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    format!("{base}?start=server-{digest:016x}")
}

/// `snap` contains only validated inventory from currently authorized installation generations.
pub fn build(snap: &Value, installations: &Value, legacy: &Value, host_filter: Option<&str>, pilotq: Option<&str>, now: u64) -> Value {
    let active: Vec<_> = installations.as_array().into_iter().flatten().filter(|r| r["connection"] != "revoked" && r["activity"]["state"] != "retired").collect();
    let mut rows: BTreeMap<String, Value> = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for series in snap["value"].as_array().into_iter().flatten() {
        let Some(owner) = active.iter().find(|r| r["installation"] == series["installation"] && r["workspace"] == series["workspace"] && r["generation"] == series["generation"]) else { continue };
        let Some(host) = series["host"].as_str() else { continue };
        let kind = if owner["runtime"] == "wsl" { "wsl" } else { "native" };
        let host = format!("{host}/{kind}");
        let key = json!([series["workspace"], host.to_lowercase()]).to_string();
        let row = rows.entry(key.clone()).or_insert_with(|| json!({"id": key, "host": host, "workspace": series["workspace"], "installations": [], "entities": []}));
        if !row["installations"].as_array().unwrap().contains(&series["installation"]) { row["installations"].as_array_mut().unwrap().push(series["installation"].clone()); }
        seen.insert(json!([series["installation"], series["workspace"]]).to_string());
        let history = series["history"].as_array().map(Vec::as_slice).unwrap_or_default();
        let Some(current) = history.last() else { continue };
        let src = entity_source(current, snap, now);
        let mut entity = json!({"installation": series["installation"], "inventory": current["inventory"], "provenance":current["provenance"], "source": src});
        let inv = &current["inventory"];
        entity["trends"] = match inv["entity"].as_str() {
            Some("host") => json!({"cpu": trend(history,"cpu_used_milli",Some("cpu_count"),now,&src), "ram": trend(history,"ram_used_bytes",Some("ram_total_bytes"),now,&src)}),
            Some("mount") => json!({"disk": trend(history,"disk_used_bytes",Some("disk_total_bytes"),now,&src)}),
            _ => Value::Null,
        };
        row["entities"].as_array_mut().unwrap().push(entity);
    }
    for owner in &active {
        if owner["product"] != "serverq" || seen.contains(&json!([owner["installation"],owner["workspace"]]).to_string()) { continue }
        let host = format!("{}/{}",owner["host_label"].as_str().unwrap_or("unknown"),if owner["runtime"] == "wsl" { "wsl" } else { "native" });
        let key = json!([owner["workspace"],host.to_lowercase()]).to_string();
        let row = rows.entry(key.clone()).or_insert_with(|| json!({"id":key,"host":host,"workspace":owner["workspace"],"installations":[],"entities":[]}));
        row["installations"].as_array_mut().unwrap().push(owner["installation"].clone());
    }
    for h in legacy.as_array().into_iter().flatten() {
        if h["serverq"]["installation"].as_str().is_some_and(|i| rows.values().any(|r| r["installations"].as_array().unwrap().contains(&json!(i)))) { continue }
        let Some(host) = h["id"].as_str() else { continue };
        let key = json!([null,host.to_lowercase()]).to_string();
        rows.entry(key.clone()).or_insert_with(|| json!({"id":key,"host":host,"workspace":null,"installations":[],"entities":[]}));
    }
    let rows: Vec<_> = rows.into_values().filter(|r| host_filter.is_none_or(|h| r["host"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(h)))).map(|mut row| {
        // Newest sample per entity across reinstalled producers; equal-time disagreement stays unknown.
        let mut entities: BTreeMap<String, Value> = BTreeMap::new();
        for e in row["entities"].as_array().unwrap() {
            let key = if e["inventory"]["entity"] == "host" { "host".to_string() } else { json!([e["inventory"]["entity"], e["inventory"]["identity"]]).to_string() };
            if let Some(old) = entities.get_mut(&key) {
                if old["source"]["at"] == e["source"]["at"] && old["inventory"] != e["inventory"] {
                    old["source"]["state"] = json!("unknown"); old["source"]["reason"] = json!("conflicting producers at one time"); old["trends"] = Value::Null;
                } else if old["source"]["at"].as_u64() < e["source"]["at"].as_u64() { *old = e.clone(); }
            } else { entities.insert(key,e.clone()); }
        }
        row["entities"] = json!(entities.into_values().collect::<Vec<_>>());
        row["source"] = row["entities"].as_array().unwrap().iter().filter(|e| e["inventory"]["entity"] == "host").max_by_key(|e| e["source"]["at"].as_u64()).map_or_else(|| unknown("no host inventory received"), |e| e["source"].clone());
        row["pilotq_selector"] = json!(json!([row["workspace"],row["host"]]).to_string());
        row["pilotq_url"] = json!(pilotq.map(|base| pilotq_link(base,row["pilotq_selector"].as_str().unwrap())));
        row
    }).collect();
    let mut src = crate::data::source(snap,now,FRESH,None);
    if rows.iter().all(|r| r["entities"].as_array().unwrap().is_empty()) { src = unknown("no server inventory received"); }
    else if rows.iter().any(|r| r["source"]["state"] != "ok") && src["state"] == "ok" { src["state"] = json!("incomplete"); src["reason"] = json!("some server inventory is missing or not fresh; see rows"); src["incomplete"] = json!(true); }
    json!({"source":src,"rows":rows})
}

fn value(inv: &Value, field: &str, divisor: f64, unit: &str) -> String {
    number(inv,field).map_or_else(|| format!("<span class=\"unknown\">— · unknown: {}</span>",esc(inv["unknown"][field].as_str().unwrap_or("not measured or inconsistent"))), |n| format!("{:.2} {unit}",n/divisor))
}
fn status(e: &Value, now: u64) -> String {
    let src = &e["source"];
    let color = if src["state"] == "ok" && e["inventory"]["status"] != "ok" { "unknown" } else { src["state"].as_str().unwrap_or("unknown") };
    let observed = src["at"].as_u64().map_or_else(|| "age unknown".to_string(),|t| format!("<time data-at=\"{t}\">{}</time>",at(&src["at"],now)));
    format!("<span class=\"server-source\" data-server-valid-for=\"{}\" data-server-state=\"{}\">{} · {observed}<small>{}</small></span>",src["at"].as_u64().map_or(0,|t| src["fresh_for_s"].as_u64().unwrap_or(FRESH).saturating_sub(now.saturating_sub(t))),esc(src["state"].as_str().unwrap_or("unknown")),badge(color,if src["state"]=="ok" {"fresh"} else {src["state"].as_str().unwrap_or("unknown")}),esc(src["reason"].as_str().unwrap_or("")))
}
fn trend_text(t: &Value, divisor: f64, unit: &str, now: u64) -> String {
    if let Some(n) = t["per_day"].as_f64() {
        let date = t["exhausts_at"].as_u64().map(|a| format!(" · estimated disk full {}",crate::source::clock(a,now))).unwrap_or_default();
        format!("<span class=\"server-trend\" data-server-valid-for=\"{}\" data-server-state=\"ok\">{:+.2} {unit}/day{date} {}</span>",t["to"].as_u64().map_or(0,|t| FRESH.saturating_sub(now.saturating_sub(t))), n/divisor,esc(t["reason"].as_str().unwrap_or("")))
    } else { format!("unknown: {}",esc(t["reason"].as_str().unwrap_or("no comparable history"))) }
}

pub fn render(w: &mut String, doc: &Value, now: u64) {
    let Some(rows) = doc["rows"].as_array().filter(|r| !r.is_empty()) else { return };
    w.push_str("<section id=\"server-table\"><h2>Servers</h2><p>Read-only inventory · no AI. Disk dates are estimates from observed growth.</p><table><caption>All observed and expected servers</caption><thead><tr><th>Server · details</th><th>Status · data age</th><th>CPU</th><th>RAM</th><th>Disk · forecast</th><th>Largest observed consumer</th><th>Discussion</th></tr></thead><tbody>");
    let mut detail_html = String::new();
    for row in rows {
        let id = format!("server-{}",enc(row["id"].as_str().unwrap()));
        let entities = row["entities"].as_array().unwrap();
        let host = entities.iter().find(|e| e["inventory"]["entity"] == "host").cloned().unwrap_or_else(|| json!({"inventory":{},"source":row["source"]}));
        let inv = &host["inventory"];
        let _ = write!(w,"<tr id=\"{id}\"><th scope=\"row\"><a href=\"#{}\">{}</a></th>",enc(&format!("{id}-content")),esc(row["host"].as_str().unwrap()));
        let _ = write!(detail_html,"<details id=\"{id}-detail\"><summary id=\"{id}-open\">{}</summary><div id=\"{id}-content\"><p>Workspace: {}</p><a href=\"#{}\">Back to server row</a>",esc(row["host"].as_str().unwrap()),esc(row["workspace"].as_str().unwrap_or("legacy source")),enc(&id));
        for (kind,title) in [("site","Sites"),("finding","Security checks"),("service","Services"),("container","Containers"),("process","Processes"),("image","Images"),("project_root","Configured roots"),("folder","Largest folders")] {
            let mut entries: Vec<_> = entities.iter().filter(|e| e["inventory"]["entity"] == kind).collect();
            entries.sort_by_key(|e| std::cmp::Reverse(e["inventory"][if kind=="process" {"ram_used_bytes"}else{"disk_used_bytes"}].as_u64()));
            let total=entries.len();entries.truncate(100);
            let _ = write!(detail_html,"<h3>{title}</h3>");
            if entries.is_empty() { detail_html.push_str("<p>unknown: no inventory received</p>"); }
            if total>100 {let _=write!(detail_html,"<p>Showing 100 of {total}; the full inventory is available in the report/export.</p>");}
            for e in entries {
                let i = &e["inventory"];
                if matches!(kind,"site"|"image"|"finding") {
                    let _=write!(detail_html,"<p><code>{}</code> · {} · {}<br>{}</p>",esc(i["identity"].as_str().unwrap_or("unknown")),esc(i["status"].as_str().unwrap_or("unknown")),esc(i["reason"].as_str().unwrap_or("")),status(e,now));
                } else {
                if kind=="folder" {
                    let _=write!(detail_html,"<p><code>{}</code> · {} · {}<br>Disk {}</p>",esc(i["identity"].as_str().unwrap_or("unknown")),esc(i["status"].as_str().unwrap_or("unknown")),status(e,now),value(i,"disk_used_bytes",1073741824.0,"GiB"));
                } else {
                let _ = write!(detail_html,"<p><code>{}</code> · {} · {}<br>Disk {} · CPU {} · RAM {}<br>{}</p>",esc(i["identity"].as_str().unwrap_or("unknown")),esc(i["status"].as_str().unwrap_or("unknown")),status(e,now),value(i,"disk_used_bytes",1073741824.0,"GiB"),value(i,"cpu_used_milli",1000.0,"cores"),value(i,"ram_used_bytes",1073741824.0,"GiB"),esc(e["installation"].as_str().unwrap_or("")));
                }
                }
                for connection in entities.iter().filter(|x|x["inventory"]["entity"]=="link"&&x["inventory"]["from_entity"]==kind&&x["inventory"]["from_identity"]==i["identity"]).take(8) {
                    let relation=&connection["inventory"];
                    let _=write!(detail_html,"<p><small>{} → {}: <code>{}</code> · {}</small></p>",esc(relation["relation"].as_str().unwrap_or("unknown")),esc(relation["to_entity"].as_str().unwrap_or("unknown")),esc(relation["to_identity"].as_str().unwrap_or("unknown")),esc(e["provenance"].as_str().unwrap_or("source not reported")));
                }
            }
        }
        let _ = write!(detail_html,"<a data-view-link href=\"/checks?host={}\">Server checks</a></div></details>",enc(row["host"].as_str().unwrap()));
        let _ = write!(w,"<td>{}<br>{}</td><td>{}<small>Trend: {}</small></td><td>{} / {}<small>Trend: {}</small></td><td>",esc(inv["status"].as_str().unwrap_or("unknown: no host status")),status(&host,now),value(inv,"cpu_used_milli",1000.0,"cores"),trend_text(&host["trends"]["cpu"],1000.0,"cores",now),value(inv,"ram_used_bytes",1073741824.0,"GiB"),value(inv,"ram_total_bytes",1073741824.0,"GiB"),trend_text(&host["trends"]["ram"],1073741824.0,"GiB",now));
        let mounts: Vec<_> = entities.iter().filter(|e| e["inventory"]["entity"] == "mount").collect();
        if mounts.is_empty() { w.push_str("unknown: no mount inventory received"); }
        for e in mounts {
            let i = &e["inventory"];
            let _ = write!(w,"<p><code>{}</code><br>{} / {}<small>{}<br>Trend: {}</small></p>",esc(i["identity"].as_str().unwrap_or("unknown")),value(i,"disk_used_bytes",1073741824.0,"GiB"),value(i,"disk_total_bytes",1073741824.0,"GiB"),status(e,now),trend_text(&e["trends"]["disk"],1073741824.0,"GiB",now));
        }
        w.push_str("</td><td>");
        if let Some(e) = entities.iter().filter(|e| matches!(e["inventory"]["entity"].as_str(),Some("folder"|"project_root"|"service"|"container")) && e["inventory"]["disk_used_bytes"].is_u64()).max_by_key(|e|e["inventory"]["disk_used_bytes"].as_u64()) {
            let i = &e["inventory"];
            let _ = write!(w,"<code>{}</code><br>{}<small>{} · largest measured; coverage may be partial</small>",esc(i["identity"].as_str().unwrap()),value(i,"disk_used_bytes",1073741824.0,"GiB"),status(e,now));
        } else { w.push_str("unknown: no measured consumer"); }
        w.push_str("</td><td>");
        if let Some(url) = row["pilotq_url"].as_str() {
            let supported = url.split_once("?start=server-").is_some_and(|(base,_)| telegram_entry(base));
            let _ = write!(w,"<a href=\"{}\" rel=\"noreferrer\">Discuss in PilotQ{}</a>",esc(url),if supported {""} else {" · chat selection pending"});
        }
        else { w.push_str("PilotQ unavailable: not configured"); }
        w.push_str("</td></tr>");
    }
    w.push_str("</tbody></table>");
    w.push_str(&detail_html);
    w.push_str("</section>");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Value { serde_json::from_str(include_str!("../fixture/servers.json")).unwrap() }
    fn fixture_snap(f: &Value) -> Value {
        let mut samples = Samples::default();
        for series in f["series"].as_array().unwrap() {
            for s in series["history"].as_array().unwrap() {
                samples.push(crate::store::Observation {installation:series["installation"].as_str().unwrap().into(),workspace:series["workspace"].as_str().unwrap().into(),generation:series["generation"].as_i64().unwrap(),product:"serverq".into(),observed:s["at"].as_i64().unwrap(),received:f["now"].as_i64().unwrap(),payload:json!({"observation":{"host":series["host"],"started_ms":s["at"].as_i64().unwrap()*1000,"source":s["source"],"operation":s["operation"],"inventory":s["inventory"]}}).to_string()},f["now"].as_i64().unwrap());
            }
        }
        json!({"value":samples.finish(),"at":f["now"],"error":null})
    }
    fn project(f: &Value) -> Value { build(&fixture_snap(f),&f["installations"],&json!([]),None,Some("https://t.me/invented_pilot_bot"),f["now"].as_u64().unwrap()) }
    fn host<'a>(d: &'a Value, name: &str) -> &'a Value { d["rows"].as_array().unwrap().iter().find(|r|r["host"]==name).unwrap() }
    fn entity<'a>(r: &'a Value, kind: &str) -> &'a Value { r["entities"].as_array().unwrap().iter().find(|e|e["inventory"]["entity"]==kind).unwrap() }

    // A2-A6 / R2: short/stale/partial history must never invent zero values, healthy state or an exhaustion date.
    #[test]
    fn server_table_fresh_stale_partial_and_forecasts() {
        let f = fixture(); let d = project(&f); let now = f["now"].as_u64().unwrap();
        assert_eq!(d["rows"].as_array().unwrap().len(),3);
        let fresh = host(&d,"invented-fresh/native");
        assert_eq!(fresh["source"]["state"],"ok");
        let h = entity(fresh,"host"); let m = entity(fresh,"mount");
        assert!((h["trends"]["cpu"]["per_day"].as_f64().unwrap()-240.0).abs()<0.001);
        assert_eq!(m["trends"]["disk"]["exhausts_at"],now+176*3600);
        assert_eq!(host(&d,"invented-stale/native")["source"]["state"],"stale");
        assert_eq!(host(&d,"invented-partial/native")["source"]["state"],"incomplete");
        assert!(entity(host(&d,"invented-stale/native"),"host")["trends"]["ram"]["per_day"].is_null());
        let mut html = String::new(); render(&mut html,&d,now);
        for s in ["Servers","All observed and expected servers","Configured roots","Services","Sites","Security checks","container-not-running","example.test","Largest folders","stale","collection-failed","unknown","estimated disk full","/srv/invented-&lt;cache&gt;"] { assert!(html.contains(s),"{s}"); }
        assert!(!html.contains("<cache>") && !html.contains("<button") && !html.contains("<form"));
        for row in d["rows"].as_array().unwrap() {
            assert_eq!(serde_json::from_str::<Value>(row["pilotq_selector"].as_str().unwrap()).unwrap(),json!(["example",row["host"]]));
        }
        let evidence = std::env::var("DASHQ_SERVERS_EVIDENCE").ok().or_else(|| std::env::var("DASHQ_HOST_FIRST_EVIDENCE").ok().map(|p| format!("{p}/servers")));
        if let Some(dir) = evidence {
            std::fs::create_dir_all(&dir).unwrap();
            let mut doc = crate::fixture_doc(); doc["servers"] = d; doc["generated_at"] = json!(now);
            std::fs::write(format!("{dir}/page.html"),crate::page::render_view(&doc,&json!({}),"/hosts")).unwrap();
        }
    }

    // #272 A2/A3: the ?server= form cannot open the Telegram chat, so the selection is lost; only the supported payload may drop the pending label.
    #[test]
    fn pilotq_link_protocol_and_label() {
        let f = fixture(); let now = f["now"].as_u64().unwrap();
        let link = |base: Option<&str>| {
            let d = build(&fixture_snap(&f),&f["installations"],&json!([]),None,base,now);
            let mut html = String::new(); render(&mut html,&d,now);
            (d,html)
        };
        let (d,html) = link(Some("https://t.me/invented_pilot_bot"));
        let fresh = host(&d,"invented-fresh/native");
        assert_eq!(fresh["pilotq_selector"],r#"["example","invented-fresh/native"]"#);
        assert_eq!(fresh["pilotq_url"],"https://t.me/invented_pilot_bot?start=server-0ad79b58009edbd1");
        assert!(html.contains(r#"<a href="https://t.me/invented_pilot_bot?start=server-0ad79b58009edbd1" rel="noreferrer">Discuss in PilotQ</a>"#),"{html}");
        assert!(!html.contains("chat selection pending"));
        for base in ["https://pilotq.example.com/discuss","https://t.me/joinchat/invented","https://t.me/+invented","https://t.me/"] {
            let (d,html) = link(Some(base));
            let fresh = host(&d,"invented-fresh/native");
            assert_eq!(fresh["pilotq_url"],format!("{base}?server={}",enc(fresh["pilotq_selector"].as_str().unwrap())));
            assert_eq!(html.matches("chat selection pending").count(),3,"{base}");
        }
        let (d,html) = link(None);
        let fresh = host(&d,"invented-fresh/native");
        assert!(fresh["pilotq_url"].is_null());
        assert_eq!(fresh["pilotq_selector"],r#"["example","invented-fresh/native"]"#);
        assert_eq!(html.matches("PilotQ unavailable: not configured").count(),3);
        assert!(!html.contains("Discuss in PilotQ"));
    }

    // A6 live-input check: no writes or deployment; the actual receiver's document goes through this candidate renderer.
    #[test]
    #[ignore = "requires a freshly captured, authorized pds18 /data document"]
    fn live_server_table_readback() {
        let path = std::env::var("DASHQ_SERVERS_LIVE_DATA").unwrap();
        let mut doc: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let now = crate::source::now();
        assert!(doc["generated_at"].as_u64().is_some_and(|t| t <= now && now-t < 600),"live capture too old");
        doc["servers"] = build(&Value::Null,&doc["installations"]["rows"],&doc["hosts"],None,None,now);
        let pds = host(&doc["servers"],"pds18.ufobe.com/native");
        assert_eq!(pds["source"]["state"],"unknown","no inventory was supplied to this live-data check");
        let page = crate::page::render_view(&doc,&json!({}),"/hosts");
        assert!(page.contains("<section id=\"server-table\">") && page.contains("pds18.ufobe.com/native"));
        if let Ok(dir) = std::env::var("DASHQ_SERVERS_EVIDENCE") { std::fs::write(format!("{dir}/live.html"),page).unwrap(); }
        println!("PASS live pds18 document -> candidate server table; inventory unknown, not deployed");
    }

    // D11/R4: another workspace/generation, revoked producer or retired installation cannot leak inventory.
    #[test]
    fn server_inventory_scope_identity_and_replay() {
        let mut f = fixture();
        let duplicate = f["series"][0].clone(); f["series"].as_array_mut().unwrap().push(duplicate);
        let d = project(&f);
        assert_eq!(d["rows"].as_array().unwrap().len(),3,"replay does not multiply servers");
        assert_eq!(entity(host(&d,"invented-fresh/native"),"host")["trends"]["cpu"]["samples"],25);
        f["installations"][0]["workspace"] = json!("other");
        f["installations"][1]["connection"] = json!("revoked");
        f["installations"][2]["activity"]["state"] = json!("retired");
        let d = project(&f); assert_eq!(d["rows"].as_array().unwrap().len(),1);
        assert!(d["rows"][0]["entities"].as_array().unwrap().is_empty());
        let mut f = fixture(); f["installations"][0]["generation"] = json!(2);
        let d = project(&f);
        assert!(!d.to_string().contains("/srv/invented-app"));
        // A legacy source with the same hostname carries no installation proof; it must not inherit scoped inventory.
        let f = fixture(); let d = build(&fixture_snap(&f),&f["installations"],&json!([{"id":"invented-fresh/native"}]),None,None,f["now"].as_u64().unwrap());
        assert_eq!(d["rows"].as_array().unwrap().len(),4);
        assert!(d["rows"].as_array().unwrap().iter().find(|r|r["workspace"].is_null()).unwrap()["entities"].as_array().unwrap().is_empty());
    }

    // A4: changes in capacity, missing fields, gaps, conflicts and disk cleanup invalidate extrapolation.
    #[test]
    fn forecast_requires_comparable_history() {
        for mode in ["short","gap","capacity","null","partial","conflict","cleanup","future","disk-invalid"] {
            let mut f = fixture(); let now = f["now"].as_u64().unwrap(); let series = &mut f["series"][1]["history"];
            match mode {
                "short" => {series.as_array_mut().unwrap().drain(..24);},
                "gap" => {series.as_array_mut().unwrap().drain(1..4);},
                "capacity" => series[0]["inventory"]["disk_total_bytes"] = json!(999),
                "null" => series[0]["inventory"]["disk_used_bytes"] = Value::Null,
                "partial" => series[0]["inventory"]["status"] = json!("partial"),
                "conflict" => {let mut s=series[0].clone();s["inventory"]["disk_used_bytes"]=json!(1);series.as_array_mut().unwrap().push(s);},
                "cleanup" => {for (j,s) in series.as_array_mut().unwrap().iter_mut().enumerate(){s["inventory"]["disk_used_bytes"]=json!((200-j as u64)*1073741824);s["inventory"]["disk_free_bytes"]=json!((100+j as u64)*1073741824);}},
                "future" => series[24]["at"] = json!(now+1),
                _ => series[24]["inventory"]["disk_free_bytes"] = json!(u64::MAX),
            }
            let d=project(&f); let m=entity(host(&d,"invented-fresh/native"),"mount");
            assert!(m["trends"]["disk"]["exhausts_at"].is_null(),"{mode}: {m}");
        }
    }
}
