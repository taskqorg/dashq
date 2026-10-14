//! Per-client AgentQ use (dashq.md D8.4, #131): one row per agentq installation and client with routing and hooks
//! from the newest `agentq.installation` heartbeat (agentq#134), MCP from the enrolled snapshot and 24 h use from
//! `agentq.compress` and `agentq.retrieval`. Unknown stays `null` with its reason (D6).

use crate::data::{AGENTS, aq, aq_identity, dur, row_at, safe};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const DAY: u64 = 86400;

/// Producer agent or profile kind -> client key of `AGENTS`; other clients (Claude Desktop chat) have no row.
fn client(a: &str) -> Option<&'static str> {
    let a = if a == "claude" { "claude-code" } else { a };
    AGENTS.iter().find(|(k, _)| *k == a).map(|(k, _)| *k)
}

fn name(c: &str) -> &'static str {
    AGENTS.iter().find(|(k, _)| *k == c).map_or("unknown client", |(_, n)| *n)
}

/// A producer reason: short plain words only; anything path- or content-like is withheld.
fn words(v: &Value, max: usize) -> Value {
    v.as_str().filter(|s| safe(s, max, b" ._-'`(),:")).map_or(Value::Null, |s| json!(s))
}

fn unix(v: &Value) -> Value {
    v.as_u64().map_or(Value::Null, |t| json!(t))
}

/// A heartbeat JSON-string field of the expected shape: the parsed value, or the reason it reads unknown (D6).
/// `agentq.routing` is an object of per-client objects; `agentq.hooks` is a list of objects that name their agent.
fn field(row: &Value, key: &str) -> Result<Value, String> {
    let v: Value = match aq(row, key) {
        Value::Null => return Err(format!("heartbeat has no {key} (agentq#134)")),
        Value::String(s) => serde_json::from_str(s).map_err(|_| format!("{key} unreadable (cut or invalid)"))?,
        _ => return Err(format!("{key} unreadable (not a JSON string)")),
    };
    let ok = if key == "agentq.routing" {
        v.as_object().is_some_and(|m| m.values().all(Value::is_object))
    } else {
        v.as_array().is_some_and(|l| l.iter().all(|h| h["agent"].is_string()))
    };
    if ok { Ok(v) } else { Err(format!("{key} unreadable (unexpected shape)")) }
}

pub struct Ctx<'a> {
    pub now: u64,
    pub heartbeats: Vec<&'a Value>,
    pub compress: Vec<&'a Value>,
    pub retrieval: Vec<&'a Value>,
    /// `(host, agent)` with legacy compression or native RTK records in the period: presence, as in #116.
    pub legacy: Vec<(String, String)>,
    /// Absence of 24 h compress or retrieval records is asserted only without these reasons (D6).
    pub compress_gap: Option<String>,
    pub retrieval_gap: Option<String>,
    pub incomplete: Value,
    /// No agent or session filter: use counts and Attention apply.
    pub scoped: bool,
    pub agent: Option<&'a str>,
}

/// `{window_s, incomplete, rows}`; Attention items go to `current` and `gaps`.
pub fn build(cx: &Ctx, fleet: &[Value], activity: &[Value], enrolled: &Value, current: &mut Vec<Value>, gaps: &mut Vec<Value>) -> Value {
    let since = cx.now.saturating_sub(DAY);
    let at = |r: &Value| row_at(r).filter(|a| *a >= since && *a <= cx.now + 60);
    let mut newest: BTreeMap<String, (u64, &Value)> = BTreeMap::new();
    for r in &cx.heartbeats {
        if let (Some((inst, _)), Some(t)) = (aq_identity(r), row_at(r).filter(|a| *a <= cx.now + 60))
            && newest.get(&inst).is_none_or(|(n, _)| t >= *n)
        {
            newest.insert(inst, (t, r));
        }
    }
    // (installation, client or "unknown") -> [records, hook records, hook compressed, bytes saved, bytes samples]
    let mut used: BTreeMap<(String, &str), [u64; 5]> = BTreeMap::new();
    for r in cx.compress.iter().filter(|r| at(r).is_some()) {
        let Some((inst, _)) = aq_identity(r) else { continue };
        let c = aq(r, "agentq.agent").as_str().and_then(client).unwrap_or("unknown");
        let u = used.entry((inst, c)).or_default();
        u[0] += 1;
        if aq(r, "agentq.adapter") == "hook" {
            u[1] += 1;
            if aq(r, "agentq.outcome") == "compressed" {
                u[2] += 1;
                // OTLP `intValue` may arrive as a string; `"null"` is an unmeasured passthrough
                let bytes = |k: &str| aq(r, k).as_u64().or_else(|| aq(r, k).as_str()?.parse().ok());
                if let (Some(i), Some(o)) = (bytes("agentq.bytes_in"), bytes("agentq.bytes_out")) {
                    u[3] += i.saturating_sub(o);
                    u[4] += 1;
                }
            }
        }
    }
    let mut retrievals: BTreeMap<String, u64> = BTreeMap::new();
    for r in cx.retrieval.iter().filter(|r| at(r).is_some()) {
        if let Some((inst, _)) = aq_identity(r) {
            *retrievals.entry(inst).or_default() += 1;
        }
    }
    let mut rows = Vec::new();
    for x in fleet {
        let (inst, host) = (x["installation"].as_str().unwrap_or(""), &x["host"]);
        let hb = newest.get(inst).map(|(_, r)| *r);
        let routing = hb.ok_or_else(|| "no heartbeat in 24 h".to_string()).and_then(|r| field(r, "agentq.routing"));
        let hooks = hb.ok_or_else(|| "no heartbeat in 24 h".to_string()).and_then(|r| field(r, "agentq.hooks"));
        let profiles: Vec<&Value> = enrolled["rows"].as_array().into_iter().flatten()
            .filter(|r| r["product"] == "agentq" && r["connection"] != "revoked" && r["installation"] == inst)
            .flat_map(|r| r["snapshot"]["payload"]["configuration"]["profiles"].as_array().into_iter().flatten())
            .collect();
        let mut clients: Vec<&'static str> = Vec::new();
        let mut note = |c: Option<&'static str>| {
            if let Some(c) = c.filter(|c| !clients.contains(c)) {
                clients.push(c);
            }
        };
        routing.iter().flat_map(|r| r.as_object().into_iter().flatten()).for_each(|(a, _)| note(client(a)));
        hooks.iter().flat_map(|h| h.as_array().into_iter().flatten()).for_each(|h| note(h["agent"].as_str().and_then(client)));
        profiles.iter().for_each(|p| note(p["kind"].as_str().and_then(client)));
        used.keys().filter(|(i, _)| i == inst).for_each(|(_, c)| note(client(c)));
        clients.sort_by_key(|c| AGENTS.iter().position(|(k, _)| k == c));
        // an installation that names no client still has a row, so an old heartbeat reads unknown, not absent
        if clients.is_empty() && cx.agent.is_none() {
            let why = |r: &Result<Value, String>| r.as_ref().err().map_or(json!("no client reported"), |e| json!(e));
            rows.push(json!({"installation": inst, "host": host, "runtime": x["runtime"], "client": null, "heartbeat": {"state": x["state"], "at": x["heartbeat_at"]},
                "routing": null, "routing_reason": why(&routing), "hooks": null, "hooks_reason": why(&hooks), "hooks_configured": null,
                "mcp": null, "mcp_reason": "no client reported", "use": null, "use_reason": "no client reported"}));
        }
        let retrieved = retrievals.get(inst).copied().unwrap_or(0);
        for c in clients.into_iter().filter(|c| cx.agent.is_none_or(|a| a == *c)) {
            let route = routing.as_ref().map(|r| r.as_object().into_iter().flatten().find(|(a, _)| client(a) == Some(c)).map(|(_, v)| v));
            let routing_v = match &route {
                Ok(Some(v)) => json!({"state": v["routing"].as_str().filter(|s| ["routed", "direct", "bypassed", "unknown"].contains(s)).unwrap_or("unknown"),
                    "reason": words(&v["reason"], 120), "intended": v["intended"].as_str().filter(|s| ["routed", "direct", "unknown"].contains(s)), "intended_reason": words(&v["intended_reason"], 64)}),
                _ => Value::Null,
            };
            let routing_reason = match &route {
                Ok(Some(_)) => Value::Null,
                Ok(None) => json!("no config root for this client in the heartbeat"),
                Err(e) => json!(e),
            };
            let hook_rows: Result<Vec<Value>, String> = hooks.as_ref().map_err(Clone::clone).map(|h| {
                h.as_array().into_iter().flatten().filter(|h| h["agent"].as_str().and_then(client) == Some(c)).map(|h| {
                    let f = &h["last_failure"];
                    json!({"event": h["event"].as_str().filter(|e| safe(e, 64, b"_-")).unwrap_or("unknown"),
                        "state": h["state"].as_str().filter(|s| ["ok", "failed", "unknown"].contains(s)).unwrap_or("unknown"),
                        "last_ok_at": unix(&h["last_ok_at"]),
                        "last_failure": if f.is_object() { json!({"at": unix(&f["at"]), "error": words(&f["error"], 160)}) } else { Value::Null }})
                }).collect()
            });
            let profile = profiles.iter().find(|p| p["kind"].as_str().and_then(client) == Some(c));
            let mcp = profile.and_then(|p| p["mcp"].as_str()).filter(|s| safe(s, 32, b"_"));
            let enrolled_hooks = profile.and_then(|p| p["hooks"].as_str()).filter(|s| safe(s, 32, b"_"));
            let n = |k: usize| used.get(&(inst.to_string(), c)).map_or(0, |u| u[k]);
            let (compressed, samples) = (n(2), n(4));
            let use_v = cx.scoped.then(|| json!({
                "compress": n(0), "unattributed": used.get(&(inst.to_string(), "unknown")).map_or(0, |u| u[0]),
                "hook_compress": n(1), "hook_compressed": compressed,
                "bytes_saved": if samples == compressed { json!(n(3)) } else { Value::Null }, "bytes_samples": samples,
                "retrievals": retrieved, "retrieval_scope": "installation",
            }));
            let row = json!({
                "installation": inst, "host": host, "runtime": x["runtime"], "client": c, "heartbeat": {"state": x["state"], "at": x["heartbeat_at"]},
                "routing": routing_v, "routing_reason": routing_reason,
                "hooks": hook_rows.as_ref().ok(), "hooks_reason": hook_rows.as_ref().err(), "hooks_configured": enrolled_hooks,
                "mcp": mcp, "mcp_reason": if mcp.is_some() { Value::Null } else if profiles.is_empty() { json!("not enrolled: the heartbeat carries no MCP state") } else { json!("no enrolled profile for this client") },
                "use": use_v.unwrap_or(Value::Null), "use_reason": if cx.scoped { Value::Null } else { json!("not available under an agent or session filter") },
            });
            // A2: configured hooks or MCP, client activity in 24 h, and no compress or retrieval record in 24 h
            if cx.scoped && x["state"] == "ok" {
                let hook_list = row["hooks"].as_array().cloned().unwrap_or_default();
                let hooks_on = enrolled_hooks == Some("registered") || !hook_list.is_empty();
                let mcp_on = mcp == Some("registered");
                let ran = |h: &Value| [&h["last_ok_at"], &h["last_failure"]["at"]].iter().any(|t| t.as_u64().is_some_and(|t| t >= since));
                let calls = activity.iter().find(|a| a["host"] == *host && a["agent"] == c).map_or(0, |a| a["calls"].as_u64().unwrap_or(0));
                if (hooks_on || mcp_on) && (calls > 0 || hook_list.iter().any(ran)) {
                    let evidence = if calls > 0 { format!("{calls} model calls in the period") } else { "a hook ran in 24 h".into() };
                    let item = |what: String, detail: String| json!({"source": "agentq", "host": host, "client": name(c), "what": format!("{}: {what}", name(c)), "since": null, "detail": detail, "trace_id": null});
                    // records without an agent count for every client (#116); the #116 item already names a silent client
                    let silent = n(0) == 0 && used.get(&(inst.to_string(), "unknown")).is_none_or(|u| u[0] == 0)
                        && !cx.legacy.iter().any(|(h, a)| host == h && a == c)
                        && !current.iter().any(|i| i["host"] == *host && i["what"] == format!("{}: no compression records", name(c)));
                    let mut unknown = Vec::new();
                    match (&cx.compress_gap, hooks_on && silent) {
                        (_, false) => {}
                        (Some(g), _) => unknown.push(format!("hook compression ({g})")),
                        (None, _) => current.push(item("hooks configured, no compress records in 24 h".into(), format!("{evidence}, but installation {inst} sent no agentq.compress record for this client in {}.", dur(DAY)))),
                    }
                    match (&cx.retrieval_gap, mcp_on && retrieved == 0) {
                        (_, false) => {}
                        (Some(g), _) => unknown.push(format!("MCP retrieval ({g})")),
                        (None, _) => current.push(item("MCP configured, no retrieval calls in 24 h".into(), format!("{evidence}, but installation {inst} sent no agentq.retrieval record in {}.", dur(DAY)))),
                    }
                    if !unknown.is_empty() {
                        gaps.push(item("AgentQ use unknown".into(), format!("Langfuse read not confirmed for {} of installation {inst} in 24 h.", unknown.join(", "))));
                    }
                }
            }
            rows.push(row);
        }
    }
    json!({"window_s": DAY, "incomplete": cx.incomplete, "rows": rows})
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    /// Chain #131 A1-A2: heartbeat routing and hooks, enrolled MCP and 24 h compress/retrieval records become one row per
    /// installation and client, and a client with configured hooks or MCP that is active but sent no record in 24 h
    /// reaches Attention; a capped read is a gap, and the item clears once records arrive. Risk: a client whose hooks or
    /// MCP are configured but silently unused reads healthy.
    #[test]
    fn client_use_rows_and_silent_hooks_or_mcp_reach_attention() {
        let now = 1_791_875_139;
        let at = |ago: u64| crate::source::iso(now - ago);
        let span = |name: &str, inst: &str, host: &str, ago: u64, extra: Value| {
            let mut m = json!({"attributes.agentq.installation": inst, "attributes.host.name": host, "attributes.agentq.runtime": "macos"});
            for (k, v) in extra.as_object().unwrap() {
                m[format!("attributes.{k}")] = v.clone();
            }
            json!({"id": format!("{name}-{inst}-{ago}"), "name": name, "startTime": at(ago), "traceId": "t1", "metadata": m})
        };
        let routing = json!({"codex": {"routing": "routed", "reason": "", "intended": "routed", "intended_reason": "onboarded"},
            "claude": {"routing": "direct", "reason": "routing not shipped", "intended": "direct", "intended_reason": "routing not shipped"}});
        let hooks = json!([{"agent": "codex", "event": "Stop", "state": "failed", "last_ok_at": now - 3000, "last_failure": {"at": now - 1000, "error": "stdin: stream did not contain valid UTF-8"}},
            {"agent": "claude", "event": "PreToolUse", "state": "ok", "last_ok_at": now - 500, "last_failure": null}]);
        let hb = span("agentq.installation", "inst-mac", "mac", 600, json!({"agentq.routing": routing.to_string(), "agentq.hooks": hooks.to_string()}));
        let old = span("agentq.installation", "inst-win", "win", 600, json!({}));
        let hook_compress = |inst: &str, agent: &str| span("agentq.compress", inst, "mac", 40_000, json!({"agentq.agent": agent, "agentq.adapter": "hook", "agentq.outcome": "compressed", "agentq.bytes_in": 1000, "agentq.bytes_out": "200"}));
        let cases: Value = serde_json::from_str(include_str!("../fixture/agentq/configuration.json")).unwrap();
        let mut payload = cases["cases"]["profile_state_and_revision"].clone();
        payload["configuration"]["profiles"][1]["mcp"] = json!("registered");
        let mut inp = json!({
            "now": now, "period": 1800, "filters": {},
            "langfuse": {"at": now, "value": {"agentq.installation": [hb, old], "agentq.compress.24h": [hook_compress("inst-mac", "claude")], "agentq.retrieval.24h": []}},
            "installations": {"source": {"state": "ok"}, "rows": [{"installation": "inst-mac", "product": "agentq", "connection": "active", "host_label": "mac", "runtime": "macos",
                "observation": {"state": "ok"}, "snapshot": {"payload": payload}}]},
        });

        let doc = crate::data::build(&inp);
        let rows = doc["agents"]["clients"]["rows"].as_array().unwrap();
        let by = |inst: &str, c: Value| rows.iter().find(|r| r["installation"] == inst && r["client"] == c).unwrap().clone();
        let codex = by("inst-mac", json!("codex"));
        assert_eq!((codex["routing"]["state"].as_str(), codex["mcp"].as_str(), codex["hooks_configured"].as_str()), (Some("routed"), Some("registered"), Some("registered")));
        assert_eq!(codex["hooks"], json!([{"event": "Stop", "state": "failed", "last_ok_at": now - 3000, "last_failure": {"at": now - 1000, "error": "stdin: stream did not contain valid UTF-8"}}]));
        let claude = by("inst-mac", json!("claude-code"));
        assert_eq!((claude["routing"]["state"].as_str(), claude["routing"]["reason"].as_str()), (Some("direct"), Some("routing not shipped")));
        assert_eq!(claude["use"], json!({"compress": 1, "unattributed": 0, "hook_compress": 1, "hook_compressed": 1, "bytes_saved": 800, "bytes_samples": 1, "retrievals": 0, "retrieval_scope": "installation"}));
        // Hermes appears only in the enrolled snapshot: MCP from enrollment, routing and hooks absent from the heartbeat
        let hermes = by("inst-mac", json!("hermes"));
        assert_eq!((hermes["mcp"].as_str(), hermes["routing"].is_null(), hermes["routing_reason"].as_str()), (Some("registered"), true, Some("no config root for this client in the heartbeat")));
        // a pre-#134 heartbeat reads unknown with its reason, never absent or healthy
        let win = by("inst-win", Value::Null);
        assert_eq!((win["routing_reason"].as_str(), win["hooks_reason"].as_str()), (Some("heartbeat has no agentq.routing (agentq#134)"), Some("heartbeat has no agentq.hooks (agentq#134)")));
        assert_eq!(rows.len(), 4, "codex, claude-code and hermes of inst-mac, one placeholder of inst-win");

        let items = |inp: &Value, part: &str| -> Vec<(String, String)> {
            let doc = crate::data::build(inp);
            let mut v: Vec<(String, String)> = doc["attention"][part].as_array().unwrap().iter()
                .filter(|i| i["source"] == "agentq" && i["host"] == "mac/native")
                .filter(|i| i["what"].as_str().is_some_and(|w| w.ends_with(" 24 h") || w.ends_with("AgentQ use unknown")))
                .map(|i| (i["client"].as_str().unwrap().into(), i["what"].as_str().unwrap().into())).collect();
            v.sort();
            v
        };
        let pairs = |v: &[(&str, &str)]| v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>();
        // Codex and Claude Code are active through their hook runs; Hermes has no activity, so no item
        assert_eq!(items(&inp, "current"), pairs(&[
            ("Claude Code", "Claude Code: MCP configured, no retrieval calls in 24 h"),
            ("Codex", "Codex: MCP configured, no retrieval calls in 24 h"),
            ("Codex", "Codex: hooks configured, no compress records in 24 h"),
        ]));
        let codex_item = doc["attention"]["current"].as_array().unwrap().iter().find(|i| i["what"] == "Codex: hooks configured, no compress records in 24 h").unwrap().clone();
        assert_eq!(codex_item["detail"], "a hook ran in 24 h, but installation inst-mac sent no agentq.compress record for this client in 24h.");

        // D6: a capped 24 h read never asserts absence; it is one gap per affected client
        let mut capped = inp.clone();
        capped["langfuse"]["value"]["truncated"] = json!(["agentq.retrieval.24h"]);
        capped["langfuse"]["incomplete"] = json!(true);
        assert_eq!(items(&capped, "current"), pairs(&[("Codex", "Codex: hooks configured, no compress records in 24 h")]));
        assert_eq!(items(&capped, "gaps"), pairs(&[("Claude Code", "Claude Code: AgentQ use unknown"), ("Codex", "Codex: AgentQ use unknown")]));
        // a stale heartbeat keeps its rows as last known but asserts nothing
        let mut stale = inp.clone();
        stale["now"] = json!(now + 8000);
        let stale_doc = crate::data::build(&stale);
        assert!(stale_doc["attention"]["current"].as_array().unwrap().iter().all(|i| !i["what"].as_str().unwrap().ends_with(" 24 h")));
        // ... and renders its routing and hook badges as last known, never as current ok
        let html = crate::page::render(&stale_doc, &json!({}));
        let table = &html[html.find("<h3>agentq clients").unwrap()..];
        let table = &table[..table.find("</table>").unwrap()];
        assert!(table.contains("Last known routed") && table.contains("Last known ok") && !table.contains("s ok\""), "{table}");
        // a stale Langfuse read with a heartbeat that was fresh when read shows last known badges, never green
        let mut stale_src = inp.clone();
        stale_src["now"] = json!(now + 1000);
        let stale_src_doc = crate::data::build(&stale_src);
        assert_eq!(stale_src_doc["sources"]["langfuse"]["state"], "stale");
        let codex = stale_src_doc["agents"]["clients"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-mac" && r["client"] == "codex").unwrap().clone();
        assert_eq!(codex["heartbeat"]["state"], "stale");
        let html = crate::page::render(&stale_src_doc, &json!({}));
        let table = &html[html.find("<h3>agentq clients").unwrap()..];
        let table = &table[..table.find("</table>").unwrap()];
        assert!(table.contains("Last known routed") && table.contains("Last known ok") && !table.contains("s ok\""), "{table}");
        // D6: a heartbeat field of the wrong JSON shape reads unknown with its reason, never as no hook runs or no routing
        let mut odd = inp.clone();
        let m = &mut odd["langfuse"]["value"]["agentq.installation"][0]["metadata"];
        m["attributes.agentq.routing"] = json!("[]");
        m["attributes.agentq.hooks"] = json!("{}");
        let odd_doc = crate::data::build(&odd);
        let odd_codex = odd_doc["agents"]["clients"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-mac" && r["client"] == "codex").unwrap().clone();
        assert_eq!((odd_codex["routing_reason"].as_str(), odd_codex["hooks"].is_null(), odd_codex["hooks_reason"].as_str()),
            (Some("agentq.routing unreadable (unexpected shape)"), true, Some("agentq.hooks unreadable (unexpected shape)")));
        let html = crate::page::render(&odd_doc, &json!({}));
        assert!(!html.contains("no hook runs") && html.contains("agentq.hooks unreadable (unexpected shape)"));
        // agent or session filters keep the rows but not the unfiltered 24 h use
        let mut filtered = inp.clone();
        filtered["filters"] = json!({"session": "s1"});
        assert!(crate::data::build(&filtered)["agents"]["clients"]["rows"].as_array().unwrap().iter().all(|r| r["use"].is_null()));

        // recovery: a Codex compress and one retrieval clear every item
        let v = &mut inp["langfuse"]["value"];
        v["agentq.compress.24h"].as_array_mut().unwrap().push(hook_compress("inst-mac", "codex"));
        v["agentq.retrieval.24h"].as_array_mut().unwrap().push(span("agentq.retrieval", "inst-mac", "mac", 70_000, json!({"agentq.tool": "get_original", "agentq.ok": true})));
        assert_eq!(items(&inp, "current"), vec![]);
        assert_eq!(items(&inp, "gaps"), vec![]);
    }
}
