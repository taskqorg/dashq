//! Service versions (dashq.md D18, service-versions.md V1–V3): one observation-only row per installation and
//! workspace from the existing producer contracts. A field no contract reports stays `unknown` and names its
//! producer gap; nothing is read from status files, logs or paths, and nothing here can update or restart.

use crate::data::{dur, source};
use serde_json::{Value, json};

/// Release repository per product; available is the configured repo's `releases/latest`, never a producer claim.
const REPOS: [(&str, &str); 2] = [("agentq", "taskqorg/agentq"), ("taskq", "taskqorg/taskq")];
const RELEASE_LIMIT: u64 = 900;

/// Fields each producer contract cannot report today; follow-up dependencies, never scraped elsewhere.
fn gaps(provenance: &str) -> &'static [&'static str] {
    match provenance {
        // agentq#101 snapshot: version, status_at, service.state, update.{available,error}, engines, queue
        "enrollment" => &["selected", "health", "lifecycle", "coreq"],
        // agentq.md § 6 events: agentq.version and time only
        "langfuse" => &[
            "selected",
            "health",
            "lifecycle",
            "coreq",
            "engines",
            "queue",
        ],
        // legacy Node updater snapshot: installed, available, status (free text), subscription
        "langfuse-legacy" => &["health", "lifecycle", "coreq", "engines", "queue"],
        // taskq.md § 4 heartbeat: version, service id, attempts
        "taskq-heartbeat" => &[
            "selected",
            "health",
            "lifecycle",
            "coreq",
            "engines",
            "queue",
        ],
        _ => &[
            "running",
            "selected",
            "health",
            "lifecycle",
            "coreq",
            "engines",
            "queue",
        ],
    }
}

fn known(v: &Value, src: &str) -> Value {
    if v.is_null() {
        json!({"version": null, "source": null, "reason": "not reported"})
    } else {
        json!({"version": v, "source": src, "reason": null})
    }
}

fn gap(field: &str, provenance: &str) -> Value {
    json!({"state": "unknown", "reason": format!("{field} not reported by {provenance}")})
}

/// One row from the producer's own fields; `obs` is the host observation (producer time), separate from release time.
#[allow(clippy::too_many_arguments)]
fn row(
    product: &str,
    provenance: &str,
    installation: Value,
    workspace: Value,
    host: Value,
    obs: Value,
    running: Value,
    selected: Value,
    extra: Value,
    inp: &Value,
) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let repo = REPOS.iter().find(|(p, _)| *p == product).map(|(_, r)| *r);
    let configured = repo.is_some_and(|r| {
        inp["projects"]
            .as_array()
            .is_some_and(|p| p.iter().any(|x| x == r))
    });
    let rel = repo.filter(|_| configured).map(|r| &inp["releases"][r]);
    let release = match rel {
        Some(r) => {
            let s = source(r, now, RELEASE_LIMIT, None);
            json!({"version": crate::data::semver(&r["value"]), "at": s["at"], "state": s["state"], "reason": s["reason"]})
        }
        None => {
            json!({"version": null, "at": null, "state": "unknown", "reason": "release source not configured"})
        }
    };
    let fresh = obs["state"] == "ok";
    let mut running = known(&running, provenance);
    let mut selected = known(&selected, provenance);
    // stale: historical values stay, visibly last known; current health is unknown (V2)
    for v in [&mut running, &mut selected] {
        v["last_known"] = json!(!fresh && !v["version"].is_null());
    }
    let missing = gaps(provenance);
    // a verified comparison alone permits the update-available label (V1)
    let update = match (release["version"].as_str(), running["version"].as_str()) {
        _ if extra["update_error"] == true => {
            json!({"state": "error", "reason": "the producer's last pass reported an error; cause not reported"})
        }
        (Some(l), Some(r)) if fresh && release["state"] == "ok" => {
            json!({"state": if coreq::update::newer(l, r) { "available" } else { "current" }, "reason": null})
        }
        (Some(_), Some(_)) => {
            json!({"state": "unknown", "reason": if fresh { "release check not current" } else { "observation not current" }})
        }
        _ => json!({"state": "unknown", "reason": "no verified version comparison"}),
    };
    let health = match obs["state"].as_str() {
        Some("ok") => gap("health receipt", provenance),
        _ => {
            json!({"state": "unknown", "reason": format!("observation {}: {}", obs["state"].as_str().unwrap_or("unknown"), obs["reason"].as_str().unwrap_or("no reason"))})
        }
    };
    let reported = |f: &str| !missing.contains(&f);
    json!({
        "product": product, "provenance": provenance, "installation": installation, "workspace": workspace, "host": host,
        "observation": obs, "running": running, "selected": selected, "release": release,
        "reported_available": extra["reported_available"], "update": update, "health": health, "lifecycle": gap("lifecycle stage", provenance),
        // libraries are metadata, never rows (V1): engines when reported, coreq not reported by any contract yet
        "libraries": {"coreq": null, "engines": extra["engines"]},
        "queue_age_s": extra["queue_age_s"], "connection": extra["connection"],
        "coverage": (["running", "selected", "engines", "queue"].into_iter().filter(|f| reported(f)).collect::<Vec<_>>()),
        "gaps": missing,
    })
}

/// Rows from enrolled snapshots, then Langfuse installations not enrolled, legacy updaters and taskq heartbeats.
/// `legacy`: `{device, host, at, monitor, installed, available}`; `heartbeats`: `{service, host, age_s, ttl, version}`.
pub fn build(inp: &Value, fleet: &[Value], legacy: &[Value], heartbeats: &[Value]) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let mut rows = Vec::new();
    let mut enrolled = Vec::new();
    for r in inp["installations"]["rows"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let p = &r["snapshot"]["payload"];
        let obs = r["observation"].clone();
        let product = r["product"].as_str().unwrap_or("unknown");
        // no snapshot yet (startup) reads like a snapshot that reports nothing; another schema is unsupported
        let ok = product == "agentq"
            && (r["snapshot"].is_null()
                || (r["snapshot"]["schema"] == "agentq-snapshot/1" && p["schema"] == 1));
        let prov = if ok { "enrollment" } else { "unsupported" };
        enrolled.push(r["installation"].clone());
        let engines = p["engines"].as_object().map(|e| {
            e.iter()
                .filter(|(k, _)| crate::data::safe_id(k))
                .map(|(k, v)| (k.clone(), crate::data::semver(&v["version"])))
                .collect::<serde_json::Map<_, _>>()
        });
        let extra = json!({"engines": if ok { json!(engines) } else { Value::Null },
            "reported_available": if ok { crate::data::semver(&p["update"]["available"]) } else { Value::Null },
            "update_error": ok && p["update"]["error"] == true,
            "queue_age_s": if ok { p["queue"]["oldest_age"].as_u64().map_or(Value::Null, |a| json!(a)) } else { Value::Null },
            "connection": r["connection"]});
        let mut x = row(
            product,
            prov,
            r["installation"].clone(),
            r["workspace"].clone(),
            json!(format!(
                "{}/{}",
                r["host_label"].as_str().unwrap_or(""),
                r["runtime"].as_str().unwrap_or("")
            )),
            obs,
            if ok {
                crate::data::semver(&p["version"])
            } else {
                Value::Null
            },
            Value::Null,
            extra,
            inp,
        );
        if !ok {
            x["reason"] = json!(format!(
                "unsupported snapshot schema {}",
                r["snapshot"]["schema"].as_str().unwrap_or("unknown")
            ));
        }
        rows.push(x);
    }
    for f in fleet
        .iter()
        .filter(|f| !enrolled.contains(&f["installation"]))
    {
        let obs = json!({"state": f["state"], "at": f["heartbeat_at"], "age_s": f["heartbeat_at"].as_u64().map(|h| now.saturating_sub(h)), "reason": f["reason"]});
        rows.push(row(
            "agentq",
            "langfuse",
            f["installation"].clone(),
            Value::Null,
            f["host"].clone(),
            obs,
            f["version"].clone(),
            Value::Null,
            json!({}),
            inp,
        ));
    }
    for l in legacy {
        let obs = source(&json!({"value": true, "at": l["at"]}), now, 600, None);
        let extra = json!({"reported_available": l["available"]});
        rows.push(row(
            "agentq",
            "langfuse-legacy",
            json!(format!("device:{}", l["device"].as_str().unwrap_or(""))),
            Value::Null,
            l["host"].clone(),
            obs,
            l["monitor"].clone(),
            l["installed"].clone(),
            extra,
            inp,
        ));
    }
    for h in heartbeats {
        let (age, ttl) = (h["age_s"].as_u64(), h["ttl"].as_u64().unwrap_or(300));
        let state = match age {
            None => "unknown",
            Some(a) if a > ttl => "stale",
            _ => "ok",
        };
        let reason = match age {
            None => Some("heartbeat age unknown".to_string()),
            Some(a) if a > ttl => Some(format!("heartbeat {} old (ttl {})", dur(a), dur(ttl))),
            _ => None,
        };
        let obs = json!({"state": state, "at": age.map(|a| now.saturating_sub(a)), "age_s": age, "reason": reason});
        let mut x = row(
            "taskq",
            "taskq-heartbeat",
            h["service"].clone(),
            Value::Null,
            h["host"].clone(),
            obs,
            h["version"].clone(),
            Value::Null,
            json!({}),
            inp,
        );
        if h["service"].is_null() {
            x["reason"] = json!("older heartbeat: no service id");
        }
        rows.push(x);
    }
    // mixed versions: a product whose rows run different known versions (V1); each row keeps its own values
    let mut mixed = Vec::new();
    for (p, _) in REPOS {
        let mut v: Vec<&str> = rows
            .iter()
            .filter(|r| r["product"] == p)
            .filter_map(|r| r["running"]["version"].as_str())
            .collect();
        v.sort();
        v.dedup();
        if v.len() > 1 {
            mixed.push(json!({"product": p, "running": v}));
        }
    }
    json!({"rows": rows, "mixed": mixed})
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    fn doc(langfuse_failed: bool) -> Value {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        let v: Value = serde_json::from_str(include_str!("../fixture/versions.json")).unwrap();
        inp["installations"] = v["installations"].clone();
        if langfuse_failed {
            inp["langfuse"] =
                json!({"value": null, "at": null, "error": "timeout", "incomplete": false});
        }
        crate::data::build(&inp)["versions"].clone()
    }

    /// Chain: producer snapshot, heartbeat and release reads to `/data` `versions` rows (V1–V3, A1–A5): every
    /// fixture case holds; unreported fields stay unknown and a Langfuse failure leaves enrolled and taskq rows.
    #[test]
    fn version_cases_project_only_reported_evidence() {
        let v: Value = serde_json::from_str(include_str!("../fixture/versions.json")).unwrap();
        let out = doc(false);
        for (name, case) in v["cases"].as_object().unwrap() {
            let target = match case["installation"].as_str() {
                Some(i) => out["rows"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["installation"] == i)
                    .unwrap_or_else(|| panic!("{name}: no row"))
                    .clone(),
                None => out.clone(),
            };
            for (ptr, want) in case["expect"].as_object().unwrap() {
                assert_eq!(target.pointer(ptr), Some(want), "{name} {ptr}");
            }
        }
        let rows = out["rows"].as_array().unwrap();
        // coreq is a library: never a row; every unreported field names its producer gap
        assert!(rows.iter().all(|r| r["product"] != "coreq"
            && r["health"]["state"] == "unknown"
            && r["gaps"].as_array().is_some_and(|g| !g.is_empty())));
        let legacy = rows
            .iter()
            .find(|r| r["provenance"] == "langfuse-legacy")
            .unwrap();
        assert_eq!(
            (
                legacy["installation"].as_str(),
                legacy["selected"]["version"].as_str()
            ),
            (Some("device:dev-fra"), Some("0.4.16"))
        );
        assert!(rows.iter().any(|r| r["provenance"] == "taskq-heartbeat"
            && r["reason"] == "older heartbeat: no service id"));
        let text = out.to_string();
        assert!(
            !text.contains("/Users")
                && !text.contains("healthy")
                && !text.contains("waiting_for_connections"),
            "no raw producer claims"
        );
        // a failed source drops only its own rows
        let failed = doc(true);
        let prov: Vec<&str> = failed["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["provenance"].as_str())
            .collect();
        assert!(
            prov.contains(&"enrollment")
                && prov.contains(&"taskq-heartbeat")
                && !prov.contains(&"langfuse"),
            "{prov:?}"
        );
    }
}
