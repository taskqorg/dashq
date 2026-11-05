use super::*;

pub(super) fn activity(inp: &Value, current: &mut Vec<Value>) -> (Vec<Value>, u64) {
    let now = inp["now"].as_u64().unwrap_or(0);
    let since = now.saturating_sub(inp["period"].as_u64().unwrap_or(1800));
    let f = &inp["filters"];
    let fhost = f["host"].as_str();
    let rows = |k: &str| observations(&inp["langfuse"], k);
    let in_period = |row: &Value| row_at(row).filter(|a| *a >= since && *a <= now + 60);
    let matches = |row: &Value| {
        let (h, a) = identity(row);
        fhost.is_none_or(|x| h.as_deref() == Some(x)) && f["agent"].as_str().is_none_or(|x| a == Some(x)) && f["session"].as_str().is_none_or(|x| row["sessionId"] == x)
    };
    let mut unattributed = 0;
    let mut keyed = |r: &Value| -> Option<(String, String)> {
        let (h, a) = identity(r);
        match (h, a.filter(|a| agent_name(a).is_some())) {
            (Some(h), Some(a)) => Some((h, a.to_string())),
            _ => {
                unattributed += 1;
                None
            }
        }
    };
    let mut activity: BTreeMap<(String, String), Value> = BTreeMap::new();
    for r in rows("activity").into_iter().filter(|r| matches(r) && in_period(r).is_some()) {
        let Some(k) = keyed(r) else { continue };
        let a = activity.entry(k.clone()).or_insert_with(|| json!({"host": k.0, "agent": k.1, "calls": 0, "errors": 0, "sessions": [], "last_at": 0, "trace_id": null, "models": {}}));
        add(a, "calls", 1.0);
        if r["level"] == "ERROR" {
            add(a, "errors", 1.0);
        }
        let at = in_period(r).unwrap();
        if a["last_at"].as_u64().is_some_and(|l| at >= l) {
            a["last_at"] = json!(at);
            a["trace_id"] = trace(r);
        }
        if let Some(s) = r["sessionId"].as_str()
            && !a["sessions"].as_array().unwrap().iter().any(|x| x == s)
        {
            a["sessions"].as_array_mut().unwrap().push(json!(s));
        }
        let model = r["model"].as_str().filter(|m| safe(m, 128, b"_.:/-")).unwrap_or("unknown");
        let m = a["models"].as_object_mut().unwrap().entry(model).or_insert_with(|| json!({"model": model, "calls": 0, "usage": {}, "cost": null, "cost_samples": 0, "trace_id": trace(r)}));
        add(m, "calls", 1.0);
        for k in USAGE {
            if let Some(n) = num(&r["usageDetails"][k]) {
                let u = &mut m["usage"][k];
                if u.is_null() {
                    *u = json!({"sum": 0, "samples": 0});
                }
                add(u, "sum", n);
                add(u, "samples", 1.0);
            }
        }
        if let Some(c) = num(&r["costDetails"]["total"]) {
            add(m, "cost", c);
            add(m, "cost_samples", 1.0);
        }
    }
    let activity: Vec<Value> = activity
        .into_values()
        .map(|mut a| {
            a["sessions"] = json!(a["sessions"].as_array().unwrap().len());
            a["models"] = Value::Array(a["models"].as_object().unwrap().values().cloned().collect());
            if a["errors"] != 0 {
                current.push(json!({"source": "Langfuse", "host": a["host"], "what": format!("{}: model call errors", agent_name(a["agent"].as_str().unwrap()).unwrap()), "since": null, "detail": format!("{} ERROR calls in the period.", a["errors"]), "trace_id": a["trace_id"]}));
            }
            a
        })
        .collect();
    (activity, unattributed)
}

pub(super) fn fleet(inp: &Value, machines: &Machines, hosts: &mut BTreeMap<String, Value>, gaps: &mut Vec<Value>, history: &mut Vec<Value>) -> Vec<Value> {
    let now = inp["now"].as_u64().unwrap_or(0);
    let since = now.saturating_sub(inp["period"].as_u64().unwrap_or(1800));
    let f = &inp["filters"];
    let fhost = f["host"].as_str();
    let rows = |k: &str| observations(&inp["langfuse"], k);
    let lf_src = source(&inp["langfuse"], now, SNAPSHOT_LIMIT, None);
    // agentq fleet (D4.1): one row per installation; usage from runs and retrievals, incidents into the history.
    let aq_latest = inp["releases"]["taskqorg/agentq"]["value"].clone();
    let scoped = f["agent"].is_null() && f["session"].is_null();
    let mut fleet: BTreeMap<String, Value> = BTreeMap::new();
    let mut aq_seen = HashSet::new();
    for key in AQ_KEYS {
        for r in rows(key) {
            let (Some(inst), Some(host), Some(at)) = (aq_identity(r), machines.of(r), row_at(r).filter(|a| *a <= now + 60)) else { continue };
            if fhost.is_some_and(|x| !x.eq_ignore_ascii_case(&host)) {
                continue;
            }
            let x = fleet.entry(inst.clone()).or_insert_with(|| {
                json!({"installation": inst, "host": host, "runtime": null, "version": null, "version_at": null, "last_seen": 0, "heartbeat_at": null,
                       "runs": if scoped { json!(0) } else { Value::Null }, "nonzero_exits": if scoped { json!(0) } else { Value::Null }, "exit_samples": 0,
                       "retrievals": if scoped { json!(0) } else { Value::Null }, "failed_retrievals": if scoped { json!(0) } else { Value::Null }, "trace_id": null})
            });
            // the newest event names the host, runtime and version
            if x["last_seen"].as_u64().is_some_and(|l| at >= l) {
                x["last_seen"] = json!(at);
                x["host"] = json!(host);
                x["runtime"] = aq(r, "agentq.runtime").as_str().filter(|s| safe(s, 16, b"")).map_or(Value::Null, |s| json!(s));
                x["version"] = semver(aq(r, "agentq.version"));
                x["version_at"] = if x["version"].is_null() { Value::Null } else { json!(at) };
                x["trace_id"] = trace(r);
            }
            match key {
                "agentq.installation" if x["heartbeat_at"].as_u64().is_none_or(|h| at > h) => x["heartbeat_at"] = json!(at),
                "agentq.run" if scoped && at >= since => {
                    add(x, "runs", 1.0);
                    if let Some(e) = aq(r, "agentq.exit_code").as_i64() {
                        add(x, "exit_samples", 1.0);
                        add(x, "nonzero_exits", if e != 0 { 1.0 } else { 0.0 });
                    }
                }
                "agentq.retrieval" if scoped && at >= since => {
                    add(x, "retrievals", 1.0);
                    add(x, "failed_retrievals", if aq(r, "agentq.ok") == false { 1.0 } else { 0.0 });
                }
                "agentq.incident" if at + 86400 >= now && trace(r).as_str().is_some_and(|t| aq_seen.insert(t.to_string())) => {
                    let stage = aq(r, "agentq.stage").as_str().filter(|s| safe(s, 32, b"_-")).unwrap_or("unknown stage");
                    history.push(json!({"source": "agentq", "host": host, "what": format!("agentq incident ({stage})"), "since": at, "occurrences": null, "trace_id": trace(r)}));
                }
                _ => {}
            }
        }
    }
    let mut fleet: Vec<Value> = fleet
        .into_values()
        .map(|mut x| {
            let age = x["heartbeat_at"].as_u64().map(|h| now.saturating_sub(h));
            let fresh = age.is_some_and(|a| a <= AQ_HEARTBEAT_LIMIT);
            x["state"] = json!(if fresh { "ok" } else { "stale" });
            x["age_s"] = json!(age);
            x["reason"] = if fresh { Value::Null } else { json!(age.map_or("no heartbeat in 24 h".into(), |a| format!("heartbeat {} old (limit {})", dur(a), dur(AQ_HEARTBEAT_LIMIT)))) };
            x["latest"] = aq_latest.clone();
            x["behind"] = behind(&aq_latest, &x["version"]);
            x
        })
        .collect();
    // One installation owns a row's agentq columns (H6): fresh first, then the higher version, then the newest event.
    // Another one on the row is superseded (H7) when a fresh owner saw newer events or runs a higher version.
    let better = |a: &Value, b: &Value| {
        if (a["state"] == "ok") != (b["state"] == "ok") {
            return a["state"] == "ok";
        }
        match (behind(&a["version"], &b["version"]), behind(&b["version"], &a["version"])) {
            (Value::Bool(true), _) => return true,
            (_, Value::Bool(true)) => return false,
            _ if a["version"].is_null() != b["version"].is_null() => return b["version"].is_null(),
            _ => {}
        }
        a["last_seen"].as_u64() > b["last_seen"].as_u64()
    };
    let mut owners: BTreeMap<String, usize> = BTreeMap::new();
    for (i, x) in fleet.iter().enumerate() {
        let k = x["host"].as_str().unwrap_or("").to_ascii_lowercase();
        if owners.get(&k).is_none_or(|&o| better(x, &fleet[o])) {
            owners.insert(k, i);
        }
    }
    for i in 0..fleet.len() {
        let (w, x) = (&fleet[owners[&fleet[i]["host"].as_str().unwrap_or("").to_ascii_lowercase()]], &fleet[i]);
        let superseded = x["installation"] != w["installation"]
            && w["state"] == "ok"
            && (x["last_seen"].as_u64() < w["last_seen"].as_u64() || (x["state"] == "ok" && behind(&w["version"], &x["version"]) == true));
        let by = superseded.then(|| json!(format!("superseded by {} {}", w["installation"].as_str().unwrap_or(""), w["version"].as_str().unwrap_or("(version unknown)"))));
        fleet[i]["superseded"] = by.unwrap_or(Value::Null);
    }
    for x in fleet.iter().filter(|x| x["state"] != "ok" && x["superseded"].is_null()) {
        gaps.push(json!({"source": "Langfuse", "host": x["host"], "what": format!("agentq installation {}: stale", x["installation"].as_str().unwrap()), "since": x["heartbeat_at"], "detail": x["reason"], "trace_id": x["trace_id"]}));
    }
    for &o in owners.values() {
        let x = &fleet[o];
        let h = host_entry(hosts, x["host"].as_str().unwrap_or(""));
        h["freshness"] = json!({"state": x["state"], "at": x["heartbeat_at"], "age_s": x["age_s"], "reason": x["reason"]});
        // a failed, stale or capped Langfuse read leaves the cached heartbeat unconfirmed, never green (D6)
        let rank = |v: &Value| RANK.iter().position(|k| v["state"] == *k).unwrap_or(4);
        if rank(&lf_src) > rank(&h["freshness"]) {
            h["freshness"]["state"] = lf_src["state"].clone();
            h["freshness"]["reason"] = json!(format!("Langfuse: {}", lf_src["reason"].as_str().unwrap_or("read not confirmed")));
        }
        h["agentq"]["installation"] = x["installation"].clone();
        h["agentq"]["running"] = x["version"].clone();
    }

    fleet
}

pub(super) fn clients(inp: &Value, fleet: &[Value], activity: &[Value], working: &BTreeMap<String, Vec<String>>, unconfirmed: &BTreeMap<String, String>, current: &mut Vec<Value>, gaps: &mut Vec<Value>) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let period = inp["period"].as_u64().unwrap_or(1800);
    let in_period = |row: &Value| row_at(row).filter(|a| *a >= now.saturating_sub(period) && *a <= now + 60);
    let f = &inp["filters"];
    let scoped = f["agent"].is_null() && f["session"].is_null();
    let lf = &inp["langfuse"];
    let lf_src = source(lf, now, SNAPSHOT_LIMIT, None);
    let rows = |k: &str| observations(lf, k);
    let capped = |k: &str| lf["value"]["truncated"].as_array().is_some_and(|t| t.iter().any(|x| x == k));
    // AgentQ coverage (#116): a running installation whose host's agents leave no compression or activity records.
    // ponytail: Langfuse heartbeats carry no routing or hook state; those rows come from enrolled snapshots below.
    // Absence is asserted only on a fresh, uncapped read of every record kind it rests on; otherwise it is a gap (D6).
    let coverage_keys = ["activity", "agentq.installation", "agentq.compress", "agentq.run"];
    let lf_gap = match coverage_keys.iter().filter(|k| capped(k)).copied().collect::<Vec<_>>() {
        _ if !matches!(lf_src["state"].as_str(), Some("ok" | "incomplete")) => lf_src["reason"].as_str().map(String::from),
        c if c.is_empty() => None,
        c => Some(format!("capped query ({})", c.join(", "))),
    };
    if scoped && let Some(reason) = &lf_gap {
        for x in fleet.iter().filter(|x| x["state"] == "ok") {
            gaps.push(json!({"source": "agentq", "host": x["host"], "client": null, "what": "AgentQ coverage unknown", "since": null,
                "detail": format!("Langfuse {reason}: missing compression or activity records of installation {} are not confirmed.", x["installation"].as_str().unwrap()), "trace_id": null}));
        }
    }
    if scoped && lf_gap.is_none() {
        let mut covered: HashMap<(String, &str), u64> = HashMap::new();
        for r in rows("agentq.compress").into_iter().chain(rows("agentq.run")).filter(|r| in_period(r).is_some()) {
            let Some(inst) = aq_identity(r) else { continue };
            let agent = aq(r, "agentq.agent").as_str().and_then(crate::runtimes::client).map_or("unknown", |c| c.key);
            *covered.entry((inst, agent)).or_default() += 1;
        }
        for x in fleet.iter().filter(|x| x["state"] == "ok") {
            let (inst, host) = (x["installation"].as_str().unwrap().to_string(), &x["host"]);
            let n = |a: &str| covered.get(&(inst.clone(), a)).copied().unwrap_or(0);
            let total: u64 = covered.iter().filter(|((i, _), _)| *i == inst).map(|(_, n)| n).sum();
            let calls: Vec<&Value> = activity.iter().filter(|a| a["host"] == *host).collect();
            for a in &calls {
                let agent = a["agent"].as_str().unwrap();
                if n(agent) == 0 && n("unknown") == 0 {
                    let client = agent_name(agent).unwrap();
                    current.push(json!({"source": "agentq", "host": host, "client": client, "what": format!("{client}: no compression records"), "since": null,
                        "detail": format!("{} model calls in {}, but installation {inst} sent no agentq.compress or agentq.run record for this client.", a["calls"], dur(period)), "trace_id": a["trace_id"]}));
                }
            }
            let name = host.as_str().unwrap().to_ascii_lowercase();
            if let Some(tasks) = working.get(&name).filter(|t| !t.is_empty() && calls.is_empty()) {
                let none = if total == 0 { "activity or compression" } else { "activity" };
                current.push(json!({"source": "agentq", "host": host, "client": "TaskQ workers", "what": format!("TaskQ workers: no {none} records"), "since": null,
                    "detail": format!("TaskQ attempts {} run on this host, but no {none} records arrived in {} (installation {inst}).", tasks.join(", "), dur(period)), "trace_id": null}));
            }
            if let Some(reason) = unconfirmed.get(&name).filter(|_| calls.is_empty() && working.get(&name).is_none_or(|t| t.is_empty())) {
                gaps.push(json!({"source": "agentq", "host": host, "client": "TaskQ workers", "what": "TaskQ workers: liveness unknown", "since": null,
                    "detail": format!("TaskQ board read {reason}: whether agents run here without records is not confirmed (installation {inst})."), "trace_id": null}));
            }
        }
    }
    // Enrolled AgentQ profiles: an onboarded (AgentQ-owned) client that routes direct or lost its hooks.
    for r in inp["installations"]["rows"].as_array().into_iter().flatten().filter(|r| r["product"] == "agentq" && r["connection"] != "revoked") {
        let c = &r["snapshot"]["payload"]["configuration"];
        if !matches!(c["state"].as_str(), Some("fresh" | "stale")) {
            continue;
        }
        let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
        let host = format!("{label}/{}", if r["runtime"] == "wsl" { "wsl" } else { "native" });
        let last = if c["state"] == "stale" || r["observation"]["state"] != "ok" { " Last known: the snapshot is not fresh." } else { "" };
        for p in c["profiles"].as_array().into_iter().flatten().filter(|p| p["owned"] == true) {
            let client = p["label"].as_str().filter(|l| safe(l, 48, b" ._-")).unwrap_or("unknown client");
            let item = |what: String, detail: String| json!({"source": "agentq", "host": host, "client": client, "what": format!("{client}: {what}"), "since": c["at"], "detail": detail + last, "trace_id": null});
            match p["routing"].as_str() {
                Some("direct") => current.push(item("routing direct".into(), "Onboarded client calls its provider directly: no AgentQ routing or provider compression.".into())),
                Some("bypassed") => current.push(item("routing bypassed".into(), "Another provider owns the route of this onboarded client: no AgentQ routing.".into())),
                _ => {}
            }
            if p["operations"].as_array().is_some_and(|o| o.iter().any(|x| x == "hooks")) {
                match p["hooks"].as_str() {
                    Some(h @ ("missing" | "absent" | "partial")) => current.push(item(format!("hooks {h}"), format!("AgentQ hooks of this onboarded client are {h}: hook compression and evidence stop."))),
                    Some("registered" | "disabled" | "unsupported") => {}
                    _ => gaps.push(item("hooks unknown".into(), "AgentQ could not read the hooks of this onboarded client.".into())),
                }
            }
        }
    }
    // Per-client use (#131): heartbeat routing and hooks, enrolled MCP, 24 h compress and retrieval records.
    let use_gap = |k: &str| match lf_src["state"].as_str() {
        Some("ok" | "incomplete") if capped(k) => Some(format!("capped query ({k})")),
        Some("ok" | "incomplete") => None,
        _ => Some(lf_src["reason"].as_str().unwrap_or("read not confirmed").to_string()),
    };
    let mut clients = crate::clients::build(
        &crate::clients::Ctx {
            now, heartbeats: rows("agentq.installation"), compress: rows("agentq.compress.24h"), retrieval: rows("agentq.retrieval.24h"),
            compress_gap: use_gap("agentq.compress.24h"), retrieval_gap: use_gap("agentq.retrieval.24h"),
            incomplete: json!({"agentq.compress.24h": capped("agentq.compress.24h"), "agentq.retrieval.24h": capped("agentq.retrieval.24h")}),
            scoped, agent: f["agent"].as_str(),
        },
        fleet, activity, &inp["installations"], current, gaps,
    );
    if !matches!(lf_src["state"].as_str(), Some("ok" | "incomplete")) {
        // a stale or failed Langfuse read shows the cached heartbeat as last known, never as current (D6)
        for r in clients["rows"].as_array_mut().into_iter().flatten() {
            r["heartbeat"]["state"] = lf_src["state"].clone();
        }
    }
    clients
}
