//! Service versions (dashq.md D18, service-versions.md V1–V3): one observation-only row per installation and
//! workspace from the existing producer contracts. A field no contract reports stays `unknown` and names its
//! producer gap; nothing is read from status files, logs or paths, and nothing here can update or restart.

use crate::data::{dur, source};
use serde_json::{Value, json};

/// Release repository per product; available is the configured repo's `releases/latest`, never a producer claim.
const REPOS: [(&str, &str); 3] = [("agentq", "taskqorg/agentq"), ("serverq", "taskqorg/serverq"), ("taskq", "taskqorg/taskq")];
const RELEASE_LIMIT: u64 = 900;

/// Fields each producer contract cannot report today; follow-up dependencies, never scraped elsewhere.
fn gaps(product: &str, provenance: &str) -> &'static [&'static str] {
    match provenance {
        // serverq.md § 3 projection: version and report time only
        "enrollment" if product == "serverq" => &["selected", "health", "lifecycle", "coreq", "engines", "queue"],
        // taskq enrollment.md Observation: board projection only, no version or service state
        "enrollment" if product == "taskq" => &["running", "selected", "health", "lifecycle", "coreq", "engines", "queue"],
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

// Keep producer time/age while carrying failed or incomplete collection evidence (D6).
fn observed(mut obs: Value, collection: &Value) -> Value {
    if collection["state"] != "ok" {
        obs["state"] = collection["state"].clone();
        obs["reason"] = collection["reason"].clone();
    }
    obs
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
    let missing = gaps(product, provenance);
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

/// Rows from enrolled snapshots, then Langfuse installations not enrolled and taskq heartbeats.
/// `heartbeats`: `{service, workspace, host, at, age_s, ttl, version, source}`.
pub fn build(inp: &Value, fleet: &[Value], heartbeats: &[Value]) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let lf = source(&inp["langfuse"], now, 600, None);
    let mut rows = Vec::new();
    let mut enrolled = Vec::new();
    for r in inp["installations"]["rows"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let p = &r["snapshot"]["payload"];
        let mut obs = r["observation"].clone();
        let product = r["product"].as_str().unwrap_or("unknown");
        // no snapshot yet (startup) reads like a snapshot that reports nothing; another schema is unsupported
        let schema = match product {
            "agentq" => "agentq-snapshot/1",
            "serverq" => "serverq-report/1",
            "taskq" => "taskq-board/1",
            _ => "",
        };
        let ok = !schema.is_empty() && (r["snapshot"].is_null() || (r["snapshot"]["schema"] == schema && p["schema"] == 1));
        let prov = if ok { "enrollment" } else { "unsupported" };
        // a fresh snapshot may carry an old status: status_at dates the versions, and a service that is not
        // running leaves its version last known (V2)
        if ok && product == "agentq" && !r["snapshot"].is_null() && obs["state"] == "ok" {
            obs = match p["status_at"].as_u64() {
                Some(t) => source(
                    &json!({"value": true, "at": t.min(obs["at"].as_u64().unwrap_or(t))}),
                    now,
                    crate::enroll::STALE_SECS,
                    None,
                ),
                None => {
                    json!({"state": "unknown", "at": obs["at"], "age_s": obs["age_s"], "reason": "status time not reported"})
                }
            };
            let service = p["service"]["state"].as_str();
            if obs["state"] == "ok" && service != Some("running") {
                obs["state"] = json!("unknown");
                obs["reason"] = json!(format!(
                    "service {}",
                    service
                        .filter(|s| crate::data::safe_id(s))
                        .unwrap_or("state not reported")
                ));
            }
        }
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
        // the event that reported the version dates it, not a later heartbeat without one
        let obs = match f["version_at"].as_u64() {
            Some(at) => source(
                &json!({"value": true, "at": at}),
                now,
                crate::data::AQ_HEARTBEAT_LIMIT,
                None,
            ),
            None => {
                json!({"state": "unknown", "at": null, "age_s": null, "reason": "no version event"})
            }
        };
        let mut x = row(
            "agentq",
            "langfuse",
            f["installation"].clone(),
            Value::Null,
            f["host"].clone(),
            observed(obs, &lf),
            f["version"].clone(),
            Value::Null,
            json!({}),
            inp,
        );
        // hosts.md H7: it left the Hosts row, never silently
        if let Some(s) = f["superseded"].as_str() {
            x["reason"] = json!(s);
        }
        rows.push(x);
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
        let obs = json!({"state": state, "at": h["at"], "age_s": age, "reason": reason});
        let mut x = row(
            "taskq",
            "taskq-heartbeat",
            h["service"].clone(),
            h["workspace"].clone(),
            h["host"].clone(),
            observed(obs, &h["source"]),
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
    // the host filter scopes every row before the mixed-version summary (D7)
    if let Some(h) = inp["filters"]["host"].as_str() {
        rows.retain(|r| r["host"] == h);
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

    /// Chain: cached source outage to version evidence; unrelated sources retain their state (A1/A5).
    #[test]
    fn cached_outages_preserve_last_known_versions() {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        inp["langfuse"]["incomplete"] = json!(false);
        let before = crate::data::build(&inp)["versions"].clone();
        inp["langfuse"]["error"] = json!("timeout");
        let failed = crate::data::build(&inp)["versions"].clone();
        for (row, old) in failed["rows"]
            .as_array()
            .unwrap()
            .iter()
            .zip(before["rows"].as_array().unwrap())
        {
            if row["product"] == "agentq" {
                assert_eq!(row["observation"]["state"], "unconfirmed");
                assert_eq!(row["observation"]["at"], old["observation"]["at"]);
                assert_eq!(row["update"]["state"], "unknown");
                for field in ["running", "selected"] {
                    if !row[field]["version"].is_null() {
                        assert_eq!(row[field]["last_known"], true);
                    }
                }
            } else {
                assert_eq!(row, old);
            }
        }
        inp["boards"]["taskqorg/taskq"]["error"] = json!("timeout");
        let failed = crate::data::build(&inp);
        assert_eq!(failed["versions"]["rows"].as_array().unwrap().iter().find(|r|r["provenance"]=="enrollment").unwrap()["observation"]["state"],"ok","independent enrolled evidence survives a GitHub outage");
        for row in failed["versions"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["product"] == "taskq" && r["provenance"] == "taskq-heartbeat")
        {
            assert_eq!(row["observation"]["state"], "unconfirmed");
            assert_eq!(row["running"]["last_known"], true);
            assert_eq!(row["update"]["state"], "unknown");
        }
    }

    /// Chain: cached board heartbeat retains producer time and its pre-release version, ages, and remains separate by project (V1).
    #[test]
    fn cached_heartbeat_time_and_project_identity() {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        inp["boards"]["taskqorg/taskq"]["value"]["hosts"][0]["body"] =
            json!("version 4.2.0-dev.1791961334\nservice abcdef123456");
        inp["boards"]["taskqorg/agentq"] = inp["boards"]["taskqorg/taskq"].clone();
        let before = crate::data::build(&inp);
        let rows: Vec<_> = before["versions"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["installation"] == "abcdef123456")
            .collect();
        assert_eq!(rows.len(), 2);
        assert_ne!(rows[0]["workspace"], rows[1]["workspace"]);
        assert_eq!(rows[0]["observation"]["at"], json!(1791875040));
        assert_eq!(rows[0]["running"]["version"], "4.2.0-dev.1791961334");
        inp["now"] = json!(inp["now"].as_u64().unwrap() + 400);
        let after = crate::data::build(&inp);
        for old in rows {
            let row = after["versions"]["rows"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| {
                    r["installation"] == old["installation"] && r["workspace"] == old["workspace"]
                })
                .unwrap();
            assert_eq!(row["observation"]["at"], old["observation"]["at"]);
            assert_eq!(
                row["observation"]["age_s"].as_u64().unwrap(),
                old["observation"]["age_s"].as_u64().unwrap() + 400
            );
            assert_eq!(row["observation"]["state"], "stale");
            assert_eq!(row["running"]["last_known"], true);
            assert_eq!(row["update"]["state"], "unknown");
        }
    }

    /// Chain: Langfuse version events date their versions, and the host filter scopes rows and mixed (V1, D7).
    #[test]
    fn langfuse_version_time_and_host_filter() {
        let mut inp: Value = serde_json::from_str(include_str!("../fixture/input.json")).unwrap();
        let doc = crate::data::build(&inp);
        let fleet = doc["agents"]["fleet"].as_array().unwrap();
        for r in doc["versions"]["rows"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["provenance"] == "langfuse")
        {
            let f = fleet
                .iter()
                .find(|f| f["installation"] == r["installation"])
                .unwrap();
            assert_eq!(r["observation"]["at"], f["version_at"]);
        }
        assert!(fleet.iter().any(|f| f["version_at"] != f["heartbeat_at"]));
        inp["filters"] = json!({"host": "fra/native"});
        let v = crate::data::build(&inp)["versions"].clone();
        let rows = v["rows"].as_array().unwrap();
        assert!(!rows.is_empty() && rows.iter().all(|r| r["host"] == "fra/native"));
        assert_eq!(v["mixed"], json!([]));
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
