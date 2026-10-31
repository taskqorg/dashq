//! TaskQ vocabularies (dashq.md D4.2, D13, D21) from TaskQ's own status snapshot contract, never a DashQ table.
//! A live `taskq status --json` carries `vocabulary`; without one (labels adapter, telemetry, older taskq) the
//! pinned release fixture `assets/taskq/status-snapshot.json` supplies it. A TaskQ format change is a repin.

use serde_json::Value;
use std::sync::OnceLock;

const FIXTURE: &str = include_str!("../assets/taskq/status-snapshot.json");

/// The pinned TaskQ snapshot fixture.
pub fn pinned() -> &'static Value {
    static V: OnceLock<Value> = OnceLock::new();
    V.get_or_init(|| serde_json::from_str(FIXTURE).expect("pinned TaskQ snapshot fixture"))
}

/// A TaskQ state: id, display name and canonical board label, in TaskQ counter order.
#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub id: String,
    pub name: String,
    pub label: String,
}

fn parse_states(v: &Value) -> Option<Vec<State>> {
    let s: Vec<State> = v["vocabulary"]["states"].as_array()?.iter().map(|s| Some(State {
        id: s["id"].as_str()?.into(), name: s["name"].as_str()?.into(), label: s["label"].as_str()?.into(),
    })).collect::<Option<_>>()?;
    (!s.is_empty()).then_some(s)
}

/// States of a live snapshot; one without a complete state list (older taskq) reads the pinned fixture's.
pub fn states(snapshot: &Value) -> Vec<State> {
    parse_states(snapshot).unwrap_or_else(|| pinned_states().to_vec())
}

pub fn pinned_states() -> &'static [State] {
    static V: OnceLock<Vec<State>> = OnceLock::new();
    V.get_or_init(|| parse_states(pinned()).expect("pinned TaskQ snapshot states"))
}

fn strings(v: &'static Value) -> Vec<&'static str> {
    v.as_array().into_iter().flatten().filter_map(Value::as_str).collect()
}

/// Telemetry error codes TaskQ writes on stage records: `errors` plus `stage_errors`.
pub fn telemetry_errors() -> &'static [&'static str] {
    static V: OnceLock<Vec<&'static str>> = OnceLock::new();
    V.get_or_init(|| {
        let t = &pinned()["vocabulary"]["telemetry"];
        strings(&t["errors"]).into_iter().chain(strings(&t["stage_errors"])).collect()
    })
}

/// Telemetry runtime codes.
pub fn telemetry_runtimes() -> &'static [&'static str] {
    static V: OnceLock<Vec<&'static str>> = OnceLock::new();
    V.get_or_init(|| strings(&pinned()["vocabulary"]["telemetry"]["runtimes"]))
}

/// The pin record: a changed fixture without a matching checksum fails here, and `selftest` runs it.
pub fn check() -> Result<(), String> {
    let pin: Value = serde_json::from_str(include_str!("../assets/taskq/snapshot-provenance.json")).map_err(|_| "taskq snapshot: invalid provenance")?;
    for (name, bytes) in [("status-snapshot.json", FIXTURE), ("status-snapshot.schema.json", include_str!("../assets/taskq/status-snapshot.schema.json"))] {
        let want = pin["files"][name]["sha256"].as_str().ok_or("taskq snapshot: missing checksum")?;
        let got: String = ring::digest::digest(&ring::digest::SHA256, bytes.as_bytes()).as_ref().iter().map(|b| format!("{b:02x}")).collect();
        if got != want {
            return Err(format!("taskq snapshot: {name} differs from its pinned checksum"));
        }
    }
    if pinned()["schema"] != 1 || parse_states(pinned()).is_none() || telemetry_errors().is_empty() || telemetry_runtimes().is_empty() {
        return Err("taskq snapshot: unsupported fixture".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pin_matches_its_checksums() {
        assert_eq!(check(), Ok(()));
    }

    /// A4: every snapshot field DashQ reads, at the type it reads, in the pinned fixture. A repin to a fixture that
    /// renames, drops or retypes one of them fails here before any view or route reads it.
    #[test]
    fn pinned_fixture_carries_every_field_dashq_reads() {
        let f = pinned();
        let kind = |v: &Value| match v { Value::Null => "null", Value::Bool(_) => "bool", Value::Number(n) if n.is_u64() => "u64", Value::Number(_) => "number", Value::String(_) => "string", Value::Array(_) => "array", Value::Object(_) => "object" };
        let has = |v: &Value, path: &str, want: &[&str]| {
            assert!(want.contains(&kind(v)), "snapshot field {path} is {} ({v}), DashQ reads {want:?}", kind(v));
        };
        has(&f["schema"], "schema", &["u64"]);
        has(&f["complete"], "complete", &["bool"]);
        for s in pinned_states() {
            has(&f["totals"][&s.id], &format!("totals.{}", s.id), &["u64"]);
        }
        for k in ["errors", "stage_errors", "runtimes"] {
            has(&f["vocabulary"]["telemetry"][k], &format!("vocabulary.telemetry.{k}"), &["array"]);
        }
        let tasks = f["tasks"].as_array().expect("tasks");
        let ids: Vec<&str> = pinned_states().iter().map(|s| s.id.as_str()).collect();
        for t in tasks {
            for (k, want) in [("n", &["u64"][..]), ("title", &["string"]), ("url", &["string"]), ("revision", &["u64"]), ("controls", &["array"]),
                ("state", &["string"]), ("question", &["object", "null"]), ("card", &["string", "null"]), ("admission", &["object", "null"]), ("failed", &["string", "null"])] {
                has(&t[k], &format!("tasks[].{k}"), want);
            }
            assert!(ids.contains(&t["state"].as_str().unwrap()), "task state {} outside the state vocabulary", t["state"]);
        }
        let q = tasks.iter().map(|t| &t["question"]).find(|q| q["parsed"] == true).expect("a parsed question");
        for (k, want) in [("parsed", &["bool"][..]), ("done", &["string", "null"]), ("problem", &["string", "null"]), ("text", &["string"]), ("choices", &["array"])] {
            has(&q[k], &format!("question.{k}"), want);
        }
        for c in q["choices"].as_array().unwrap() {
            for (k, want) in [("id", &["u64"][..]), ("text", &["string"]), ("recommended", &["bool"]), ("consequential", &["bool"])] {
                has(&c[k], &format!("question.choices[].{k}"), want);
            }
        }
    }

    #[test]
    fn states_follow_the_snapshot_then_the_pin() {
        let pin: Vec<&str> = pinned_states().iter().map(|s| s.id.as_str()).collect();
        assert!(pin.contains(&"held"), "the pinned vocabulary counts Held");
        assert_eq!(pinned_states().iter().find(|s| s.id == "waiting").map(|s| s.label.as_str()), Some("q-blocked"));
        let live = json!({"vocabulary": {"states": [{"id": "doing", "name": "Working", "label": "q-doing"}, {"id": "parked", "name": "Parked", "label": "q-parked"}]}});
        assert_eq!(states(&live).iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["doing", "parked"], "a new TaskQ state needs no DashQ edit");
        assert_eq!(states(&json!({"totals": {}})), pinned_states(), "an older snapshot without vocabulary reads the pin");
        assert_eq!(states(&json!({"vocabulary": {"states": [{"id": "doing"}]}})), pinned_states(), "an incomplete list is not trusted");
    }

    #[test]
    fn telemetry_codes_come_from_the_pin() {
        assert!(telemetry_errors().contains(&"argument-too-long"));
        assert!(telemetry_errors().contains(&"left-board"), "stage errors join the list");
        assert!(telemetry_runtimes().contains(&"codex-app"));
    }
}
