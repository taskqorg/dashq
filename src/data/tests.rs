    /// D8/H1: WSL resources use the same installation identity as the canonical host, including filtered entry.
    #[test]
    fn review_resource_runtime_identity() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!("../../fixture/hosts.json")).unwrap();
        let mut inp = fixture["input"].clone();
        let now = inp["now"].as_u64().unwrap();
        inp["langfuse"]["value"]["agentq.resource"] = serde_json::json!([{"id":"r-wsl","startTime":crate::source::iso(now-10),"metadata":{
            "attributes.agentq.installation":"inst-wsl","attributes.host.name":"desktop","attributes.agentq.runtime":"linux","attributes.agentq.resource.cpu_percent":33}}]);
        for filter in [serde_json::json!({}),serde_json::json!({"host":"DESKTOP/wsl"})] {
            inp["filters"] = filter;
            let d = super::build(&inp);
            assert_eq!(d["resources"]["rows"][0]["host"],"DESKTOP/wsl");
            assert_eq!(d["resources"]["rows"].as_array().unwrap().len(),1);
        }
    }
    use super::*;

    /// H2: producer verification time must survive the enrolled read-to-page path.
    #[test]
    fn serverq_verified_episode_preserves_producer_time() {
        let mut input: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let p: Value = serde_json::from_str(include_str!("../../fixture/serverq/linux.json")).unwrap();
        input["now"] = json!(1792000000u64);
        input["installations"] = json!({"rows": [{"product": "serverq", "installation": "sq-1", "workspace": "ws-1", "host_label": "same", "runtime": "native", "connection": "active", "observation": {"state": "ok", "at": 1792000000u64}, "snapshot": {"schema": "serverq-report/1", "payload": p}}]});
        let doc = build(&input);
        assert_eq!(doc["checks"]["installations"][0]["findings"][0]["verified_at"], 1791907200u64);
    }

    /// D23 A3/A5: an enrolled ServerQ warn rule is one person request that opens its exact open episode, the
    /// verified episode inside the window reads resolved, and Attention lists nothing a person need not act on.
    #[test]
    fn requests_open_the_exact_finding_and_changes_resolve_episodes() {
        let mut input: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let p: Value = serde_json::from_str(include_str!("../../fixture/serverq/linux.json")).unwrap();
        input["now"] = json!(1791993000u64);
        input["period"] = json!(86400);
        input["installations"] = json!({"rows": [{"product": "serverq", "installation": "sq-1", "workspace": "ws-1", "host_label": "same", "runtime": "native", "connection": "active", "observation": {"state": "ok", "at": 1791993000u64}, "snapshot": {"schema": "serverq-report/1", "payload": p}}]});
        let doc = build(&input);
        let finding = doc["checks"]["installations"][0]["findings"].as_array().unwrap().iter().find(|f| f["rule"] == "failed-services" && f["state"] == "open").unwrap().clone();
        let r: Vec<&Value> = doc["attention"]["requests"].as_array().unwrap().iter().filter(|r| r["installation"] == "sq-1").collect();
        assert_eq!(r.len(), 1, "one request per warn rule");
        assert_eq!((&r[0]["kind"], &r[0]["rule"], &r[0]["since"]), (&json!("finding"), &json!("failed-services"), &json!(1791992800u64)));
        assert_eq!(r[0]["target"], format!("/checks?period=86400#finding-{}", crate::explain::finding_id(&finding)));
        let kinds: HashSet<&str> = doc["attention"]["requests"].as_array().unwrap().iter().filter_map(|r| r["kind"].as_str()).collect();
        assert!(kinds.iter().all(|k| ["question", "failed", "finding"].contains(k)), "{kinds:?}");
        let resolved: Vec<&Value> = doc["changes"]["rows"].as_array().unwrap().iter().filter(|r| r["state"] == "resolved").collect();
        assert_eq!((resolved.len(), &resolved[0]["until"]), (1, &json!(1791907200u64)), "disk-headroom verified inside the window");
        assert_eq!(doc["changes"]["coverage"].as_array().unwrap().iter().find(|c| c["source"] == "serverq").unwrap()["baseline"], true, "enrolled episodes keep the window's history");
    }

    /// D19 U2: cached resource collection failure is unconfirmed, with producer ages and other boards retained.
    #[test]
    fn startup_partial_collection_failure() {
        let mut input: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let good = build(&input);
        input["langfuse"]["error"] = json!("timeout");
        let failed = build(&input);
        assert_eq!(failed["boards"][0], good["boards"][0], "independently fresh board is unaffected");
        for (last, current) in good["resources"]["rows"].as_array().unwrap().iter().zip(failed["resources"]["rows"].as_array().unwrap()) {
            assert_eq!(current["current"], last["current"]);
            assert_eq!(current["source"]["at"], last["source"]["at"]);
            assert_eq!(current["source"]["age_s"], last["source"]["age_s"]);
            assert_eq!(current["source"]["state"], "unconfirmed");
        }
    }

    #[test]
    fn query_validates_every_parameter() {
        assert_eq!(query("").unwrap().0, 1800);
        let (p, f) = query("period=7200&host=fra%2Fnative&agent=codex&session=s-1").unwrap();
        assert_eq!((p, f["host"].as_str()), (7200, Some("fra/native")));
        for bad in ["period=60", "host=a%2Fb", "agent=x", "session=a%20b", "x=1", "host=a&host=b", "period"] {
            assert!(query(bad).is_err(), "{bad}");
        }
    }

    /// #178 A2–A5 / D6: released producer cases must reach the shared JSON/report projection without false health.
    #[test]
    fn taskq_host_observations_released_cases() {
        let bytes = include_bytes!("../../fixture/taskq/host-observations.json");
        let hash: String = ring::digest::digest(&ring::digest::SHA256, bytes).as_ref().iter().map(|b|format!("{b:02x}")).collect();
        assert_eq!(hash,"c0fba64e81d7845b12ef3c329b05cbe72e7e5cddbf36be8a66e4d1ade9808c56","TaskQ v4.6.30 exact bytes");
        let cases: Value = serde_json::from_str(include_str!("../../fixture/taskq/host-observations.json")).unwrap();
        for (name, case) in cases.as_object().unwrap() {
            let obs = case.as_array().cloned().unwrap_or_else(|| vec![case.clone()]);
            let now = if name == "read expired" { 2000 } else { 1000 };
            let inp = json!({"now":now,"projects":["taskqorg/dashq"],"installations":{"source":{"state":"ok"},"rows":[{
                "product":"taskq","connection":"active","workspace":"ws","installation":"tq-one","observation":{"state":"ok"},
                "snapshot":{"schema":"taskq-board/1","payload":{"schema":1,"board":{"kind":"github","host":"github.com","repo":"taskqorg/dashq"},"observations":obs}}
            }]}});
            let doc = build(&inp);
            assert_eq!(doc["schema"], 2);
            for o in obs {
                let h = doc["hosts"].as_array().unwrap().iter().find(|h| h["id"] == o["host"]).expect("observation host must exist");
                let got = h["taskq"]["observations"].as_array().unwrap().iter().find(|v| v["kind"] == o["kind"]).unwrap();
                for k in ["producer","observed","valid_until"] { assert_eq!(got[k],o[k],"{name}: {k}"); }
                assert_eq!(got["board"]["repo"], "taskqorg/dashq");
                if o["complete"] == true { assert_eq!(got["state"],o["state"],"{name}"); }
                else {
                    assert_eq!(got["state"],"unknown","{name}");
                    assert_eq!(got["reason"],o["reason"],"{name}");
                    assert!(got["running"].is_null() && got["queued"].is_null(),"partial counts are not current");
                    assert_ne!(got["claude"],"logged in");
                }
            }
        }
    }

    /// #178 / H2 / D11: identity, expiry, conflict and revocation cannot become positive host evidence.
    #[test]
    fn taskq_host_observations_identity_and_freshness() {
        let cases: Value = serde_json::from_str(include_str!("../../fixture/taskq/host-observations.json")).unwrap();
        let online = cases["read fresh"].as_array().unwrap().iter().find(|o|o["host"]=="mac" && o["kind"]=="online").unwrap().clone();
        let record = |o: Value| json!({"product":"taskq","connection":"active","workspace":"ws","installation":"one","projected":true,
            "snapshot":{"schema":"taskq-board/1","payload":{"schema":1,"board":{"kind":"github","host":"github.com","repo":"taskqorg/dashq"},"observations":[o]}}});
        let mut inp = json!({"now":1000,"projects":["taskqorg/dashq"],"boards":{"taskqorg/dashq":{"at":1000,"value":{"date":1000,"hosts":[{
            "title":"taskq host mac","body":"machine DESKTOP wsl","updated_at":"1970-01-01T00:16:40Z"}]}}},"installations":{"rows":[record(online.clone())]}});
        let get = |d: Value| d["hosts"].as_array().unwrap().iter().find(|h|h["id"]=="DESKTOP/wsl").unwrap()["taskq"]["observations"].as_array().unwrap().iter().find(|o|o["kind"]=="online").unwrap().clone();
        assert_eq!(get(build(&inp))["state"],"online");
        let missing = build(&inp);
        assert!(missing["hosts"][0]["taskq"]["observations"].as_array().unwrap().iter().any(|o|o["kind"]=="login" && o["reason"]=="no observation"));
        inp["now"] = json!(1300); assert_eq!(get(build(&inp))["state"],"online","expiry boundary inclusive");
        inp["now"] = json!(1301); assert_eq!(get(build(&inp))["reason"],"expired");
        inp["now"] = json!(999); assert_eq!(get(build(&inp))["reason"],"clock uncertain");
        inp["now"] = json!(1000);
        let mut other = record(online); other["installation"] = json!("two");
        inp["installations"]["rows"].as_array_mut().unwrap().push(other);
        assert_eq!(get(build(&inp))["state"],"online","duplicates do not sum or conflict");
        inp["installations"]["rows"][1]["snapshot"]["payload"]["observations"][0]["state"] = json!("offline");
        assert_eq!(get(build(&inp))["reason"],"conflicting observations");
        for r in inp["installations"]["rows"].as_array_mut().unwrap() { r["connection"] = json!("revoked"); }
        assert!(build(&inp)["hosts"][0]["taskq"]["observations"].as_array().unwrap().is_empty());
        inp["installations"]["rows"][0]["connection"] = json!("active");
        inp["installations"]["rows"][0]["projected"] = json!(false);
        assert!(build(&inp)["hosts"][0]["taskq"]["observations"].as_array().unwrap().is_empty(),"previous grant snapshot is not current");
    }

    #[test]
    fn taskq_snapshot_feeds_boards_questions_and_failures() {
        let doc = build(&serde_json::from_str(include_str!("../../fixture/input.json")).unwrap());
        let b = &doc["boards"][0];
        assert_eq!((b["adapter"].as_str(), b["in_work"].as_u64(), b["waiting"].as_u64(), b["ready"].as_u64()), (Some("taskq-snapshot"), Some(2), Some(1), Some(1)));
        let source: Value = serde_json::from_str(crate::FIXTURE).unwrap();
        let original = &source["taskq"]["taskqorg/taskq"]["value"]["tasks"];
        for t in b["tasks"].as_array().unwrap() {
            let row = original.as_array().unwrap().iter().find(|r| r["n"] == t["n"]).unwrap();
            assert_eq!(t["revision"], row["revision"]);
            assert_eq!(t["controls"], row["controls"], "#179 offers must be canonical, never inferred");
            assert_eq!(t["question"], row["question"], "D13 canonical choices/evidence/limits unchanged");
        }
        let q = &b["questions"];
        // parsed card: source k, order and text kept, ★ as recommended; unparsed review card: verbatim, no choices
        assert_eq!(q[1], json!({"id": "https://github.com/taskqorg/taskq/issues/12", "number": 12, "title": "Which option? <b>", "url": "https://github.com/taskqorg/taskq/issues/12", "state": "ask",
            "revision": 6139994000u64, "parsed": true, "done": "checked", "text": "pick a store <i>", "choices": [{"k": 1, "text": "SQLite", "unavailable": null}, {"k": 3, "text": "Files", "unavailable": null}], "recommended": 3, "free_text": true}));
        assert_eq!((q[0]["state"].as_str(), q[0]["text"].as_str(), q[0]["choices"].as_array().map(Vec::len)), (Some("review"), Some("Merge now or wait?"), Some(0)));
        assert_eq!(b["failed"][0]["reason"], "Reason: CI <red> on every runner");
        assert!(doc["attention"]["current"].as_array().unwrap().iter().any(|i| i["what"] == "taskqorg/taskq#14: failed" && i["detail"] == "Reason: CI <red> on every runner"));
        // a failed snapshot reads unknown with its reason, never zero
        let d = &doc["boards"][2];
        assert_eq!((d["adapter"].as_str(), d["source"]["state"].as_str(), d["source"]["reason"].as_str()), (Some("taskq-snapshot"), Some("unknown"), Some("read failed (taskq timeout)")));
        assert!(d["in_work"].is_null() && d["questions"] == json!([]));
        assert_eq!(doc["boards"][1]["adapter"], "taskq-labels");
        assert_eq!(doc["sources"]["taskq"]["state"], "unknown");
    }

    /// A1: Held is counted from TaskQ's state vocabulary, never a DashQ list; a snapshot that adds a state needs no
    /// DashQ edit, and the label adapter counts `waiting` from TaskQ's canonical label (q-blocked), not a guessed one.
    #[test]
    fn counters_follow_the_taskq_state_vocabulary() {
        let mut inp: Value = serde_json::from_str(include_str!("../../fixture/input.json")).unwrap();
        let v = &mut inp["taskq"]["taskqorg/taskq"]["value"];
        v["totals"]["held"] = json!(2);
        let c = &build(&inp)["boards"][0]["counts"];
        assert_eq!((c["held"].as_u64(), c["closed"].clone()), (Some(2), Value::Null), "held from totals, closed stays unknown");
        let v = &mut inp["taskq"]["taskqorg/taskq"]["value"];
        v["totals"]["parked"] = json!(4);
        v["vocabulary"] = json!({"states": [{"id": "held", "name": "Held", "label": "q-held"}, {"id": "parked", "name": "Parked", "label": "q-parked"}]});
        let c = &build(&inp)["boards"][0]["counts"];
        assert_eq!(c, &json!({"closed": null, "held": 2, "parked": 4}), "the live vocabulary sets the counters");
        // the label adapter: the same board without a snapshot
        inp["taskq"].as_object_mut().unwrap().remove("taskqorg/taskq");
        let issues = inp["boards"]["taskqorg/taskq"]["value"]["issues"].as_array_mut().unwrap();
        issues.extend([json!({"number": 15, "title": "Blocked", "url": "https://github.com/taskqorg/taskq/issues/15", "labels": ["taskq", "q-blocked"]}),
            json!({"number": 16, "title": "Held", "url": "https://github.com/taskqorg/taskq/issues/16", "labels": ["taskq", "q-held"]})]);
        let b = &build(&inp)["boards"][0];
        assert_eq!((b["adapter"].as_str(), b["counts"]["waiting"].as_u64(), b["counts"]["held"].as_u64(), b["counts"]["doing"].as_u64()), (Some("taskq-labels"), Some(1), Some(1), Some(1)));
        let state = |n: u64| b["tasks"].as_array().unwrap().iter().find(|t| t["n"] == n).unwrap()["state"].clone();
        assert_eq!((state(15), state(16)), (json!("waiting"), json!("held")));
    }

    // Chain: enrolled taskq-board/1 task session -> board task row (#1565, #180). Risk: an off-host Codex session shows
    // no progress, an older or another board's record wins, a closed task keeps a stale row, a payload names another
    // host, a future time pins the newest choice, or a worker line reaches the row.
    #[test]
    fn taskq_session_progress_joins_the_board_task_row() {
        let mut inp: Value = serde_json::from_str(include_str!("../../fixture/input.json")).unwrap();
        let now = inp["now"].as_u64().unwrap();
        let tasks = build(&inp)["boards"][0]["tasks"].clone();
        let n = |state: &str| tasks.as_array().unwrap().iter().find(|t| t["state"] == state).unwrap()["n"].as_u64().unwrap();
        let (doing, ready) = (n("doing"), n("ready"));
        let tasks_of = |at: u64, line: Option<&str>| [doing, ready].map(|n| {
            let mut s = json!({"host": "spoof", "runtime": "codex", "stage": "doing", "at": at});
            if let Some(l) = line {
                s["line"] = json!(l);
            }
            json!({"n": n, "state": "doing", "session": s})
        });
        let rec = |repo: &str, at: u64, line: Option<&str>| json!({"product": "taskq", "connection": "active", "host_label": "win", "snapshot": {"schema": "taskq-board/1", "payload": {"schema": 1,
            "board": {"kind": "github", "host": "github.com", "repo": repo}, "tasks": tasks_of(at, line)}}});
        // A4: a snapshot with and without a worker line renders the same progress row
        let rows = [Some("Running tests"), None].map(|line| {
            inp["installations"] = json!({"rows": [rec("taskqorg/taskq", now - 90, Some("old")), rec("taskqorg/taskq", now - 30, line), rec("taskqorg/other", now, Some("other board")), rec("taskqorg/taskq", now + 60, Some("future"))]});
            let doc = build(&inp);
            let row = |n: u64| doc["boards"][0]["tasks"].as_array().unwrap().iter().find(|t| t["n"] == n).unwrap().clone();
            assert!(row(ready).get("session").is_none(), "only doing and review rows show progress");
            row(doing)["session"].clone()
        });
        assert_eq!(rows[0], json!({"host": "win", "runtime": "codex", "stage": "doing", "age_s": 30}));
        assert_eq!(rows[0], rows[1]);
    }

    #[test]
    fn source_states_never_turn_missing_into_ok() {
        let now = 10_000;
        let s = |v: Value| source(&v, now, 600, None)["state"].clone();
        assert_eq!(s(Value::Null), "unknown");
        assert_eq!(s(json!({"value": null, "at": null, "error": "timeout"})), "unknown");
        assert_eq!(s(json!({"value": 1, "at": 9000, "error": "timeout"})), "unconfirmed");
        assert_eq!(s(json!({"value": 1, "at": 9000, "error": null})), "stale");
        assert_eq!(s(json!({"value": 1, "at": 9900, "error": null, "incomplete": true})), "incomplete");
        assert_eq!(s(json!({"value": 1, "at": 9900, "error": null})), "ok");
        let r = source(&json!({"value": 1, "at": 9000, "error": "timeout"}), now, 600, None);
        assert_eq!(r["reason"], "read failed (timeout), showing 1970-01-01 02:30 UTC");
    }

    #[test]
    fn agentq_rows_build_the_fleet_and_join_machine_hosts() {
        let doc = build(&serde_json::from_str(include_str!("../../fixture/input.json")).unwrap());
        let ids: Vec<&str> = doc["hosts"].as_array().unwrap().iter().filter_map(|h| h["id"].as_str()).collect();
        assert_eq!(ids.iter().filter(|i| **i == "fra/native").count(), 1, "current sources name one host once");
        assert!(ids.contains(&"win-1/native"));
        let fleet = doc["agents"]["fleet"].as_array().unwrap();
        let (fra, win) = (&fleet[0], &fleet[1]);
        assert_eq!((fra["installation"].as_str(), fra["state"].as_str(), fra["version"].as_str()), (Some("inst-fra"), Some("ok"), Some("0.5.0")));
        assert_eq!((fra["runs"].as_u64(), fra["nonzero_exits"].as_u64(), fra["retrievals"].as_u64()), (Some(2), Some(1), Some(1)));
        assert_eq!((win["state"].as_str(), win["failed_retrievals"].as_u64()), (Some("stale"), Some(1)));
        let incidents = doc["attention"]["incidents"].as_array().unwrap();
        assert_eq!(incidents.iter().filter(|i| i["source"] == "agentq").count(), 1, "one event id is one incident");
        assert!(doc["attention"]["gaps"].as_array().unwrap().iter().any(|g| g["what"] == "agentq installation inst-win: stale"));
    }

    /// Chain hosts.md H1-H7 (#140): live-shaped sources give one Hosts row per machine and kind; taskq heartbeats join
    /// by their machine line, the freshest agentq installation fills the agentq columns and an older one is superseded
    /// with its reason. Risk: one machine split over several rows, or a fresh host with empty agentq columns.
    #[test]
    fn hosts_join_one_row_per_machine() {
        let mut fx: Value = serde_json::from_str(include_str!("../../fixture/hosts.json")).unwrap();
        let doc = build(&fx["input"]);
        if std::env::var_os("DASHQ_UPDATE_FIXTURE").is_some() {
            fx["hosts"] = doc["hosts"].clone();
            std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/fixture/hosts.json"), serde_json::to_string_pretty(&fx).unwrap() + "\n").unwrap();
        }
        assert_eq!(doc["hosts"], fx["hosts"], "rerun with DASHQ_UPDATE_FIXTURE=1 after an intended change");
        let hosts = doc["hosts"].as_array().unwrap();
        let ids: Vec<&str> = hosts.iter().filter_map(|h| h["id"].as_str()).collect();
        assert_eq!(ids, ["DESKTOP/native", "DESKTOP/wsl", "example-host/native", "macbook-m2.local/native", "wsl"]);
        for h in hosts.iter().filter(|h| h["freshness"]["state"] == "ok") {
            assert!(h["agentq"]["running"].is_string() && h["agentq"]["latest"].is_string() && h["agentq"]["behind"].is_boolean(), "{h}");
            assert!(h["reason"].is_null(), "{h}");
        }
        assert_eq!(hosts[3]["taskq"]["version"], "4.2.0", "H3: the newest version of two heartbeats");
        assert_eq!(hosts[4]["reason"], "no machine line: update taskq");
        let old = doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-win-old").unwrap();
        assert_eq!(old["reason"], "superseded by inst-win-new 0.6.0");
        assert!(!doc["attention"]["gaps"].to_string().contains("inst-win-old"), "a superseded installation is no gap");
        // D6: a failed, stale or capped Langfuse read never leaves a Hosts row green.
        let now = fx["input"]["now"].as_u64().unwrap();
        for (k, v, state) in [("error", json!("timeout"), "unconfirmed"), ("at", json!(now - 3600), "stale"), ("incomplete", json!(true), "incomplete")] {
            let mut inp = fx["input"].clone();
            inp["langfuse"][k] = v;
            let mac = build(&inp)["hosts"][3]["freshness"].clone();
            assert_eq!(mac["state"], state, "{k}: {mac}");
            assert!(mac["reason"].as_str().is_some_and(|r| r.starts_with("Langfuse: ")), "{k}: {mac}");
        }
    }

    /// Chain #116 A1: routing, hook and silent-compression failures of AgentQ installations reach Attention with host,
    /// client and reason, and clear once the installation routes and compresses again. Risk: a fleet that reads ok while
    /// every client bypasses AgentQ.
    #[test]
    fn agentq_coverage_failures_reach_attention_and_clear() {
        let now = 1_791_875_139;
        let at = |ago: u64| crate::source::iso(now - ago);
        let aq_row = |name: &str, inst: &str, host: &str, agent: Option<&str>| {
            let mut m = json!({"attributes.agentq.installation": inst, "attributes.host.name": host, "attributes.agentq.runtime": "linux"});
            if let Some(a) = agent {
                m["attributes.agentq.agent"] = json!(a);
            }
            json!({"id": format!("{name}-{inst}-{}", agent.unwrap_or("")), "name": name, "startTime": at(120), "traceId": "t1", "metadata": m})
        };
        let call = |host: &str, agent: &str| json!({"id": format!("g-{host}-{agent}"), "type": "GENERATION", "startTime": at(60), "traceId": "g1", "tags": [format!("host:{host}"), "runtime:native", format!("agent:{agent}")]});
        let cases: Value = serde_json::from_str(include_str!("../../fixture/agentq/configuration.json")).unwrap();
        let mut payload = cases["cases"]["profile_state_and_revision"].clone();
        payload["configuration"]["profiles"][1]["owned"] = json!(true);
        payload["configuration"]["profiles"][1]["hooks"] = json!("missing");
        let mut inp = json!({
            "now": now, "period": 1800, "filters": {}, "projects": ["taskqorg/dashq"],
            "boards": {"taskqorg/dashq": {"at": now, "value": {"issues": [], "date": now, "hosts": [{"title": "taskq host WIN-1", "body": "version 4.2.0
#7 a1", "updated_at": at(60)}]}}},
            "langfuse": {"at": now, "value": {
                "agentq.installation": [aq_row("agentq.installation", "inst-fra", "fra", None), aq_row("agentq.installation", "inst-win", "win-1", None)],
                "activity": [call("fra", "codex"), call("fra", "claude-code")],
                "agentq.compress": [aq_row("agentq.compress", "inst-fra", "fra", Some("claude"))]}},
            "installations": {"source": {"state": "ok"}, "rows": [{"installation": "inst-mac", "product": "agentq", "connection": "active", "host_label": "mac", "runtime": "macos",
                "observation": {"state": "ok"}, "snapshot": {"payload": payload}}]},
        });
        let rows = |inp: &Value| -> Vec<(String, String, String)> {
            let doc = build(inp);
            doc["attention"]["current"].as_array().unwrap().iter().filter(|i| i["source"] == "agentq").map(|i| (i["host"].as_str().unwrap().into(), i["client"].as_str().unwrap().into(), i["what"].as_str().unwrap().into())).collect()
        };
        let mut got = rows(&inp);
        got.sort();
        let want: Vec<(String, String, String)> = [
            ("fra/native", "Codex", "Codex: no compression records"),
            ("mac/native", "Claude Code", "Claude Code: hooks missing"),
            ("mac/native", "Claude Code", "Claude Code: routing direct"),
            ("mac/native", "Codex", "Codex: routing direct"),
            ("win-1/native", "TaskQ workers", "TaskQ workers: no activity or compression records"),
        ]
        .iter()
        .map(|(h, c, w)| (h.to_string(), c.to_string(), w.to_string()))
        .collect();
        assert_eq!(got, want);
        // D6: a cached Langfuse read after a timeout, or a capped query, never asserts absence; it is a gap per installation
        let gaps = |inp: &Value| -> Vec<(String, String)> {
            let doc = build(inp);
            doc["attention"]["gaps"].as_array().unwrap().iter().filter(|i| i["source"] == "agentq").map(|i| (i["host"].as_str().unwrap().into(), i["what"].as_str().unwrap().into())).collect()
        };
        let coverage_rows = |inp: &Value| rows(inp).into_iter().filter(|r| r.0 != "mac/native").collect::<Vec<_>>();
        let unknown = vec![("fra/native".to_string(), "AgentQ coverage unknown".to_string()), ("win-1/native".into(), "AgentQ coverage unknown".into())];
        let mut timed_out = inp.clone();
        timed_out["langfuse"]["error"] = json!("timeout");
        assert_eq!(coverage_rows(&timed_out), vec![]);
        assert_eq!(gaps(&timed_out), unknown);
        let mut capped = inp.clone();
        capped["langfuse"]["value"]["truncated"] = json!(["activity"]);
        capped["langfuse"]["incomplete"] = json!(true);
        assert_eq!(coverage_rows(&capped), vec![]);
        assert_eq!(gaps(&capped), unknown);
        // an unconfirmed board read never asserts that TaskQ workers run without records
        let mut board_down = inp.clone();
        board_down["boards"]["taskqorg/dashq"]["error"] = json!("timeout");
        assert!(!rows(&board_down).iter().any(|r| r.1 == "TaskQ workers"));
        assert_eq!(gaps(&board_down), vec![("win-1/native".to_string(), "TaskQ workers: liveness unknown".to_string())]);
        // recovery: the next complete read asserts again
        assert_eq!(rows(&inp).len(), 5);
        assert_eq!(gaps(&inp), vec![]);

        let doc = build(&inp);
        let win = doc["attention"]["current"].as_array().unwrap().iter().find(|i| i["host"] == "win-1/native").unwrap();
        assert_eq!(win["detail"], "TaskQ attempts taskqorg/dashq#7 run on this host, but no activity or compression records arrived in 30m (installation inst-win).");

        // agentq fixes routing: clients route, hooks registered, compression and activity records arrive
        for p in inp["installations"]["rows"][0]["snapshot"]["payload"]["configuration"]["profiles"].as_array_mut().unwrap() {
            p["routing"] = json!("routed");
            p["hooks"] = json!("registered");
        }
        let v = &mut inp["langfuse"]["value"];
        v["agentq.compress"].as_array_mut().unwrap().extend([aq_row("agentq.compress", "inst-fra", "fra", Some("codex")), aq_row("agentq.compress", "inst-win", "win-1", Some("codex"))]);
        v["activity"].as_array_mut().unwrap().push(call("win-1", "codex"));
        assert_eq!(rows(&inp), vec![]);
    }
/// D6 / specq Time: floor at minute, hour and day boundaries (including 90 s and 47 h).
#[test]
fn time_boundaries() {
    let cases: Vec<(u64, String)> = serde_json::from_str(include_str!("../../fixture/time.json")).unwrap();
    for (seconds, expected) in cases { assert_eq!(dur(seconds), expected, "{seconds} seconds"); }
}
