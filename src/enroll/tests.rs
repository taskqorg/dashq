    use super::*;
    use std::path::PathBuf;

    const HOST: &str = "dash.example.com";
    const T0: i64 = 1_800_000_000;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dashq-enroll-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn setup(name: &str) -> (PathBuf, Result<Enroll, String>) {
        let d = dir(name);
        Enroll::admin(&d, "ws-1", "github|1").unwrap();
        let e = Enroll::open(&d, HOST);
        (d, e)
    }

    fn call(e: &Result<Enroll, String>, method: &str, path: &str, headers: &[(&str, &str)], body: &str, now: i64) -> (String, String, Value) {
        let h: Vec<(String, String)> = headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let (path, query) = path.split_once('?').unwrap_or((path, ""));
        let r = route(e, &Req { method, path, query, headers: &h, body: body.as_bytes() }, now).expect("enrollment route");
        let text = String::from_utf8(r.body).unwrap();
        (r.status.to_string(), r.headers, serde_json::from_str(&text).unwrap_or(Value::String(text)))
    }

    fn machine(e: &Result<Enroll, String>, path: &str, auth: Option<&str>, body: Value, now: i64) -> (u16, Value) {
        let bearer = auth.map(|a| format!("Bearer {a}"));
        let h: Vec<(&str, &str)> = bearer.iter().map(|b| ("authorization", b.as_str())).collect();
        let (s, _, v) = call(e, "POST", path, &h, &body.to_string(), now);
        (s[..3].parse().unwrap(), v)
    }

    fn page(e: &Result<Enroll, String>, subject: &str, code: &str, now: i64) -> (String, String) {
        let (s, _, v) = call(e, "GET", &format!("/connect?code={code}"), &[(SUBJECT, subject)], "", now);
        (s, v.as_str().unwrap().to_string())
    }

    /// Browser decision as the approval page posts it.
    fn decide(e: &Result<Enroll, String>, subject: &str, code: &str, decision: &str, workspace: &str, now: i64) -> (String, String) {
        let csrf = e.as_ref().unwrap().token(subject, &user_code(code).unwrap_or_default());
        let body = format!("code={code}&workspace={workspace}&decision={decision}&csrf={csrf}");
        let h = [(SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        let (s, _, v) = call(e, "POST", "/connect", &h, &body, now);
        (s, v.as_str().unwrap().to_string())
    }

    fn start(e: &Result<Enroll, String>, inst: &str, now: i64) -> (String, String) {
        let (s, v) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"]}), now);
        assert_eq!(s, 200, "{v}");
        assert_eq!((&v["verification_uri"], &v["expires_in"], &v["interval"]), (&json!("https://dash.example.com/connect"), &json!(600), &json!(5)));
        (v["device_code"].as_str().unwrap().into(), v["user_code"].as_str().unwrap().into())
    }

    /// The shared coreq client pairs through these handlers; the browser approves on the first wait.
    struct Wire<'a> {
        e: &'a Result<Enroll, String>,
        now: i64,
        code: Option<String>,
        calls: Vec<String>,
        /// Also ticks every requested operation capability at approval.
        tick: bool,
    }

    impl coreq::enroll::Wire for Wire<'_> {
        fn post(&mut self, destination: &str, route: &str, auth: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            assert_eq!(destination, "https://dash.example.com/machine");
            let route = &format!("/machine{route}");
            self.calls.push(route.into());
            Ok(machine(self.e, route, auth, body.clone(), self.now))
        }
        fn now(&self) -> u64 {
            self.now as u64
        }
        fn sleep(&mut self, secs: u64) {
            self.now += secs as i64;
            if let Some(code) = self.code.take() {
                let decision = if self.tick { "approve&capability.agentq.configure=grant" } else { "approve" };
                assert!(decide(self.e, "github|1", &code, decision, "ws-1", self.now).1.contains("Approved"));
            }
        }
        fn show(&mut self, uri: &str, code: &str) {
            assert_eq!(uri, "https://dash.example.com/connect");
            self.code = Some(code.into());
        }
    }

    #[test]
    fn coreq_client_pairs_once_and_the_grant_survives_restart() {
        let (d, e) = setup("client");
        let host = dir("client-host");
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!((&a["generation"], &a["workspace"], &a["product"]), (&json!(1), &json!("ws-1"), &json!("agentq")));
        assert_eq!(w.calls, ["/machine/connect/device", "/machine/connect/token", "/machine/connect/token", "/machine/connect/ack"]);
        // repeating enrollment returns the active connection without new requests or grants
        coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(w.calls.len(), 4);
        let cred = a["credential"].as_str().unwrap();
        assert!(cred.len() >= 43, "256-bit credential");
        drop(e);
        let e = Enroll::open(&d, HOST);
        let c = e.as_ref().unwrap().store.credential(&sha256(cred)).unwrap().unwrap();
        assert!(c.active && !c.revoked && c.grant.account == "github|1");
        // revoke survives restart; the reply repeats so offline retries end
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 60), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        drop(e);
        let e = Enroll::open(&d, HOST);
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1}), T0 + 61), (200, json!({"protocol": 1, "state": "revoked", "generation": 1})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some("forged"), json!({"protocol": 1}), T0 + 62), (401, json!({"protocol": 1, "error": "invalid_credential"})));
        assert_eq!(machine(&e, "/machine/connect/ack", None, json!({"protocol": 1}), T0 + 62).0, 401);
        assert!(e.as_ref().unwrap().store.authorize(&sha256(cred)).unwrap().is_none());
    }

    fn shipper_wire() -> Value {
        serde_json::from_str(include_str!("../../fixture/coreq-observation/shipper-wire.json")).unwrap()
    }

    #[test]
    fn discovered_links_preserve_identity_boundaries_and_finding_reasons() {
        let mut link=json!({"entity":"link","identity":"a".repeat(64),"relation":"uses","from_entity":"container","from_identity":"docker:web","to_entity":"folder","to_identity":"/srv/data"});
        assert!(inventory(&link));
        link["to_identity"]="/srv/../private".into();assert!(!inventory(&link));
        link["to_identity"]="/srv/data".into();link["to_entity"]="service".into();assert!(!inventory(&link));
        link["to_entity"]="folder".into();link["command"]="invented".into();assert!(!inventory(&link));
        assert!(inventory(&json!({"entity":"finding","identity":"sshhole:abc","status":"unknown","reason":"probe-unavailable"})));
        assert!(inventory(&json!({"entity":"process","identity":"worker:123:456","ram_used_bytes":1024})));
    }

    fn inventory_wire() -> Value {
        serde_json::from_str(include_str!("../../fixture/coreq-observation/inventory-wire.json")).unwrap()
    }

    fn inventory_device(name: &str) -> (Result<Enroll, String>, String) {
        let (_d, e) = setup(name);
        let host = dir(&format!("{name}-host"));
        let req = coreq::enroll::Request { installation: None, product: "serverq", host_label: "srv-1", schemas: &[crate::observations::SCHEMA], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let cred = a["credential"].as_str().unwrap().to_string();
        (e, cred)
    }

    // Defect: the generic allowlist answered `payload` for mount, project_root and folder paths, so the shipper's cursor
    // passed them: known loss of ServerQ inventory. Protects D28 Inventory with the unmodified `Store::ship` request.
    #[test]
    fn coreq_inventory_request_is_stored_and_exported_whole() {
        let (e, cred) = inventory_device("inventory");
        let body = inventory_wire();
        let sent = body["records"].as_array().unwrap();
        let entities: std::collections::BTreeSet<_> = sent.iter().map(|r| r["payload"]["observation"]["inventory"]["entity"].as_str().unwrap()).collect();
        assert_eq!(entities.into_iter().collect::<Vec<_>>(), ["container", "folder", "host", "mount", "project_root", "service"]);
        let all_accepted = vec![("accepted".to_string(), None); sent.len()];
        let (status, v) = machine(&e, "/machine/ingest", Some(&cred), body.clone(), T0);
        assert_eq!((status, receipts(&v)), (200, all_accepted.clone()), "{v}");
        // a lost reply repeats the same rows: the same receipts and nothing new
        let (status, v) = machine(&e, "/machine/ingest", Some(&cred), body.clone(), T0 + 60);
        assert_eq!((status, receipts(&v)), (200, all_accepted));
        let row = e.as_ref().unwrap().project(Scope::All).unwrap().into_iter().next().unwrap();
        assert_eq!((row.volume.as_ref().unwrap().rows, row.received_at), (sent.len() as i64, Some(T0)));
        let id = sent[1]["event_id"].as_str().unwrap();
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": id}), T0 + 60).1["state"], "accepted");
        let mut changed = sent[1].clone();
        changed["payload"]["observation"]["inventory"]["identity"] = json!("/other");
        assert_eq!(receipts(&machine(&e, "/machine/ingest", Some(&cred), json!({"protocol": 1, "records": [changed]}), T0 + 120).1)[0].1.as_deref(), Some("replay_mismatch"));
        // what was sent reads back unchanged, in its grant's scope only
        let scope = [(row.installation.clone(), row.workspace.clone())];
        let mut stored = Vec::new();
        Store::observations(&e.as_ref().unwrap().fleet_db, &scope, 0, i64::MAX, &[], |o| stored.push(serde_json::from_str::<Value>(&o.payload).unwrap())).unwrap();
        assert_eq!(stored.iter().collect::<Vec<_>>(), sent.iter().map(|r| &r["payload"]).collect::<Vec<_>>());
        let mut foreign = 0;
        Store::observations(&e.as_ref().unwrap().fleet_db, &[(row.installation.clone(), "other".to_string())], 0, i64::MAX, &[], |_| foreign += 1).unwrap();
        assert_eq!(foreign, 0);
        let at = sent[0]["observed_at"].as_i64().unwrap();
        let mut jsonl = Vec::new();
        export_observations(&e, &mut jsonl, "jsonl", at, at, Some("serverq")).unwrap();
        let exported: Vec<Value> = String::from_utf8(jsonl).unwrap().lines().skip(1).map(|l| serde_json::from_str(l).unwrap()).collect();
        assert_eq!(exported.len(), sent.len());
        for (x, r) in exported.iter().zip(sent) {
            assert_eq!(&x["observation"], &r["payload"]["observation"], "identity, measures, reasons and sample time survive");
            assert_eq!(x["observation"]["started_ms"], at * 1000);
        }
        let mut csv = Vec::new();
        export_observations(&e, &mut csv, "csv", at, at, None).unwrap();
        let csv = String::from_utf8(csv).unwrap();
        assert!(csv.lines().nth(1).unwrap().ends_with(",inventory,unknown"));
        for r in sent {
            let cell = r["payload"]["observation"]["inventory"].to_string().replace('"', "\"\"");
            assert!(csv.contains(&format!(",\"{cell}\",")), "{cell}");
        }
    }

    // Defect: a typed exception that leaks would carry contents, environment values or credentials off the host, or admit
    // a path anywhere else. Protects D28 Inventory and R1.
    #[test]
    fn inventory_keeps_refusing_secrets_contents_and_malformed_records() {
        let (e, cred) = inventory_device("inventory-refused");
        let base = inventory_wire()["records"].as_array().unwrap().iter().find(|r| r["payload"]["observation"]["inventory"]["identity"] == "/srv").unwrap().clone();
        let next = std::sync::atomic::AtomicU64::new(1);
        let answer = |mutate: &dyn Fn(&mut Value)| {
            let mut r = base.clone();
            // each request an hour later, so the device budget never answers for the validator
            let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            r["event_id"] = json!(format!("{n:032x}"));
            r["payload"]["id"] = r["event_id"].clone();
            mutate(&mut r["payload"]["observation"]);
            let (_, v) = machine(&e, "/machine/ingest", Some(&cred), json!({"protocol": 1, "records": [r]}), T0 + n as i64 * 3600);
            receipts(&v).remove(0)
        };
        let refused = |mutate: &dyn Fn(&mut Value), case: &str| assert_eq!(answer(mutate), ("rejected".to_string(), Some("payload".to_string())), "{case}");
        let inv = |o: &mut Value, k: &str, x: Value| o["inventory"][k] = x;
        assert_eq!(answer(&|_| {}).0, "accepted", "the base record is valid");
        for key in ["content", "file_content", "text", "env", "environment", "API_KEY", "token", "credential", "password", "secret", "prompt", "command", "stdout", "output", "path", "cost_usd", "other"] {
            for value in ["invented file content line", "API_KEY=invented-environment-value", "ghp_aaaaaaaaaaaaaaaaaaaaaaaa"] {
                refused(&|o| inv(o, key, json!(value)), key);
            }
        }
        let long_name = "x".repeat(257);
        let long_cyrillic = "я".repeat(129);
        let long_path = format!("/{}", "x".repeat(4096));
        for (entity, identity) in [
            ("service", "API_KEY=invented-environment-value"), ("service", "https://user:invented@example.test/repo"), ("container", "line one\nline two"), ("service", ""), ("service", long_name.as_str()), ("host", long_cyrillic.as_str()),
            ("service", "/srv/app"), ("container", r"C:\data"), ("host", r"\\nas\share"), ("service", "~/x"),
            ("folder", ""), ("folder", "relative/path"), ("folder", "~/x"), ("folder", "/a/../b"), ("folder", "/a/./b"), ("folder", "/a//b"), ("folder", "/srv\n/invented file content"), ("folder", long_path.as_str()),
            ("mount", "C:relative"), ("mount", r"C:\\"), ("mount", r"\\server"), ("mount", r"\\user@nas\share"), ("mount", r"\\user:invented@nas\share"), ("project_root", r"\\.\pipe\x"),
            ("project_root", r"\\?\C:relative"), ("project_root", r"\\?\UNC\server"), ("project_root", r"\\?\UNC\user@nas\share"), ("folder", r"\\?\Volume{not-a-guid}\"), ("folder", r"\\?\Volume{00000000-0000-0000-0000-000000000000}"), ("folder", r"\\?\pipe\x"),
            ("user", "x"),
        ] {
            refused(&|o| o["inventory"] = json!({"entity": entity, "identity": identity}), identity);
        }
        for (key, value, why) in [
            ("identity", json!(1), "identity type"), ("identity", Value::Null, "identity null"), ("disk_used_bytes", json!("1"), "measure text"), ("disk_used_bytes", json!(-1), "negative"), ("disk_used_bytes", json!(1.5), "fraction"),
            ("disk_used_bytes", json!(true), "bool"), ("disk_used_bytes", json!({}), "object"), ("cpu_used_milli", json!(1), "foreign measure"), ("status", json!("invented file content line"), "status text"), ("status", json!(1), "status type"),
            ("unknown", json!("reason"), "reasons type"), ("unknown", json!({"disk_used_bytes": 1}), "reason type"), ("unknown", json!({"cpu_used_milli": "no-cgroup"}), "foreign reason"), ("unknown", json!({"other": "reason"}), "unknown reason"),
            ("unknown", json!({"status": "invented file content"}), "reason text"),
        ] {
            refused(&|o| inv(o, key, value.clone()), why);
        }
        // a null needs its reason, and a collected value has none
        refused(&|o| { inv(o, "disk_used_bytes", Value::Null); inv(o, "unknown", json!({})); }, "null without reason");
        refused(&|o| o["inventory"] = json!({"entity": "folder", "identity": "/srv", "disk_used_bytes": 1, "unknown": {"disk_used_bytes": "x"}}), "value with reason");
        // the object lives on its own kind only, and that kind needs it
        refused(&|o| o["kind"] = json!("check"), "inventory on another kind");
        refused(&|o| { o.as_object_mut().unwrap().remove("inventory"); }, "kind without inventory");
        refused(&|o| o["inventory"] = json!("/srv"), "inventory is an object");
        // a path stays refused everywhere outside the typed field
        for key in ["operation", "source", "host", "project"] {
            refused(&|o| o[key] = json!("/var/lib"), key);
        }
        refused(&|o| o["detail"] = json!({"inventory": {"entity": "folder", "identity": "/srv"}}), "nested elsewhere");
        // generic metadata and other schemas gain nothing
        for schema in ["agentq-snapshot/1", "serverq-report/1"] {
            assert!(!snapshot_metadata(schema, &json!({"inventory": {"entity": "folder", "identity": "/srv"}})), "{schema}");
        }
        assert!(!metadata(&json!({"identity": "/srv"}), 0));
        assert!(!metadata(&json!({"path": "x"}), 0));
    }

    /// The largest path and name CoreQ documents are accepted; one byte more is not.
    #[test]
    fn inventory_identity_bounds_match_coreq() {
        let entity = |e: &str, i: String| json!({"entity": e, "identity": i});
        assert!(inventory(&entity("service", "x".repeat(256))));
        assert!(!inventory(&entity("service", "x".repeat(257))));
        assert!(inventory(&entity("folder", format!("/{}", "x".repeat(4095)))));
        assert!(!inventory(&entity("folder", format!("/{}", "x".repeat(4096)))));
        for good in ["/", "/srv/", r"C:\", "C:/", r"C:\данные", r"/posix\backslash", r"\\nas\share", r"\\nas\share\", r"\\nas\share\日本語", r"\\?\C:\", r"\\?\C:\данные\", r"\\?\UNC\nas\share", r"\\?\UNC\nas\share\日本語\", r"\\?\Volume{00000000-0000-0000-0000-000000000000}\", r"\\?\Volume{ABCDEF01-2345-6789-abcd-0123456789ab}\данные\"] {
            for e in ["mount", "project_root", "folder"] {
                assert!(inventory(&entity(e, good.to_string())), "{e}: {good}");
            }
        }
        for bad in [r"\\?\C:relative", r"\\?\UNC\server", "//", r"C:\\", "C://", "/srv//", r"C:\srv\\", r"\\nas\share\\", r"\\?\C:\\", r"\\?\UNC\", r"\\?\UNC\server\", r"\\?\UNC\server\\share", r"\\?\UNC\server\share\\", r"\\?\UNC\server\..", r"\\?\UNC\user@nas\share", r"\\?\C:\data\..", r"\\?\C:\data\.\x", r"\\?\Volume{00000000-0000-0000-0000-000000000000}", r"\\?\Volume{00000000-0000-0000-0000-000000000000}\\", r"\\?\Volume{not-a-guid}\", r"\\?\pipe\x", r"\\?\relative\path"] {
            assert!(!inventory(&entity("folder", bad.to_string())), "{bad}");
        }
    }

    fn receipts(v: &Value) -> Vec<(String, Option<String>)> {
        v["receipts"].as_array().unwrap().iter().map(|r| (r["state"].as_str().unwrap().to_string(), r["reason"].as_str().map(String::from))).collect()
    }

    // Defect: CoreQ's shipper names a schema no product prefix allows, and its oldest backlog would read `expired`, so
    // nothing would ever arrive. Protects D28 A1 with the unmodified request `Store::ship` sent.
    #[test]
    fn coreq_shipper_request_is_stored_once_under_the_enrollment_grant() {
        let (d, e) = setup("shipper");
        let host = dir("shipper-host");
        let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: "mac-1", schemas: &["taskq-board/1", crate::observations::SCHEMA], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let cred = a["credential"].as_str().unwrap();
        let body = shipper_wire();
        // the fixture's records are 72 days older than T0: older than the receipt window
        assert!(body["records"][0]["observed_at"].as_i64().unwrap() < T0 - 8 * 86400);
        let all_accepted = vec![("accepted".to_string(), None); 7];
        let (status, v) = machine(&e, "/machine/ingest", Some(cred), body.clone(), T0);
        assert_eq!((status, receipts(&v)), (200, all_accepted.clone()), "{v}");
        // a lost reply repeats the same rows: the same receipts and nothing new
        let (status, v) = machine(&e, "/machine/ingest", Some(cred), body.clone(), T0 + 60);
        assert_eq!((status, receipts(&v)), (200, all_accepted));
        let row = e.as_ref().unwrap().project(Scope::All).unwrap().into_iter().next().unwrap();
        assert_eq!((row.volume.as_ref().unwrap().rows, row.received_at), (7, Some(T0)), "a receipt keeps D11 silence accurate");
        let id = body["records"][0]["event_id"].as_str().unwrap();
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), T0 + 60).1["state"], "accepted");
        // every kind the shipper sends is readable back, whole
        let mut kinds = std::collections::BTreeSet::new();
        Store::observations(&e.as_ref().unwrap().fleet_db, &[(row.installation.clone(), row.workspace.clone())], 0, i64::MAX, &[], |o| {
            assert_eq!(o.product, "taskq");
            let stored: Value = serde_json::from_str(&o.payload).unwrap();
            kinds.insert(stored["observation"]["kind"].as_str().unwrap().to_string());
        })
        .unwrap();
        assert_eq!(kinds.iter().map(String::as_str).collect::<Vec<_>>(), ["agent.call", "agent.turn", "check", "operation", "service.pass", "stage", "transition"]);
        // a changed record, a priced record, a foreign id and a content key are refused alone
        let refused = |mutate: &dyn Fn(&mut Value), id: &str| {
            let mut r = body["records"][1].clone();
            r["event_id"] = json!(id);
            r["payload"]["id"] = json!(id);
            mutate(&mut r);
            let (_, v) = machine(&e, "/machine/ingest", Some(cred), json!({"protocol": 1, "records": [r]}), T0 + 120);
            receipts(&v).remove(0).1
        };
        let mut changed = body["records"][1].clone();
        changed["payload"]["seq"] = json!(99);
        assert_eq!(receipts(&machine(&e, "/machine/ingest", Some(cred), json!({"protocol": 1, "records": [changed]}), T0 + 120).1)[0].1.as_deref(), Some("replay_mismatch"));
        assert_eq!(refused(&|r| r["payload"]["observation"]["cost_usd"] = json!(1.5), "a1").as_deref(), Some("payload"), "records hold no price");
        assert_eq!(refused(&|r| r["payload"]["observation"]["unit_price"] = json!(1.5), "a2").as_deref(), Some("payload"));
        // Defect: only top-level keys were read, so a nested price was stored. Protects A3 "records hold no price".
        assert_eq!(refused(&|r| r["payload"]["observation"]["unknown"] = json!({"unit_price": 1.5}), "a7").as_deref(), Some("payload"));
        assert_eq!(refused(&|r| r["payload"]["observation"]["detail"] = json!([{"deep": {"Cost": 1.5}}]), "a8").as_deref(), Some("payload"));
        assert_eq!(refused(&|r| r["payload"]["id"] = json!("other"), "a3").as_deref(), Some("payload"));
        assert_eq!(refused(&|r| r["payload"]["observation"]["prompt"] = json!("hello"), "a4").as_deref(), Some("payload"));
        assert_eq!(refused(&|r| r["payload"]["observation"]["schema"] = json!(2), "a5").as_deref(), Some("payload"));
        assert_eq!(refused(&|r| r["payload"]["observation"]["outcome"] = Value::Null, "a6").as_deref(), Some("payload"));
        assert_eq!(e.as_ref().unwrap().project(Scope::All).unwrap()[0].volume.as_ref().unwrap().rows, 7);
        drop((e, d));
    }

    // Defect: a grant without the schema must hold the cursor; a shipper-only product must not gain other schemas.
    #[test]
    fn observation_schema_needs_the_grant_and_widens_no_product() {
        let (_d, e) = setup("shipper-scope");
        let host = dir("shipper-scope-host");
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let (_, v) = machine(&e, "/machine/ingest", Some(a["credential"].as_str().unwrap()), shipper_wire(), T0);
        assert!(receipts(&v).iter().all(|r| r.1.as_deref() == Some("scope")), "{v}");
        // no credential, no write
        assert_eq!(machine(&e, "/machine/ingest", None, shipper_wire(), T0).0, 401);
        let device = |product: &str, schemas: Value| machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": product, "installation": format!("{product}-1"), "runtime": "native", "host_label": "h", "schemas": schemas}), T0).0;
        assert_eq!(device("pilotq", json!([crate::observations::SCHEMA])), 200);
        assert_eq!(device("assetq", json!([crate::observations::SCHEMA])), 200);
        assert_eq!(device("pilotq", json!(["pilotq-snapshot/1"])), 400);
        assert_eq!(device("pilotq", json!([crate::observations::SCHEMA, "taskq-board/1"])), 400);
        assert_eq!(device("taskq", json!([crate::observations::SCHEMA, "serverq-report/1"])), 400);
        assert_eq!(device("unknownq", json!([crate::observations::SCHEMA])), 400);
    }

    /// A `taskq-telemetry/1` block (taskq#1353 S3) of `lines`, with optional sender `status`.
    fn block(first: u64, lines: &[Value], status: Option<Value>) -> Value {
        use std::io::Write;
        let plain: String = lines.iter().map(|l| format!("{l}\n")).collect();
        let mut z = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        z.write_all(plain.as_bytes()).unwrap();
        let mut p = json!({"schema": 1, "first": first, "last": first + lines.len() as u64 - 1, "rows": lines.len(), "bytes": plain.len(), "gzip": crate::base64(&z.finish().unwrap())});
        if let Some(s) = status {
            p["status"] = s;
        }
        p
    }

    /// #181: TaskQ `requests` rows count `method:route` keys; `:` passes in telemetry lines only, every segment
    /// still allowlisted, and stays refused in snapshot payloads.
    #[test]
    fn telemetry_counter_keys_allow_colon_only_in_lines() {
        let row = |k: &str| json!({"at_ms": 1, "op": "requests", "counts": {k: 3}});
        assert_eq!(telemetry(&block(1, &[row("GET:issues.N")], None)), Some(None));
        // taskq 4.6.31 still sends `GET:compare.:`; each bad line is dropped and counted, the block is kept
        for k in ["GET:text", "command:GET", "GET::x", ":GET", "GET:a/b", "GET:compare.:"] {
            let kept = telemetry(&block(1, &[row(k), row("GET:issues.N")], None)).flatten().unwrap_or_else(|| panic!("{k}"));
            assert_eq!((&kept["rows"], &kept["dropped"], &kept["last"]), (&json!(1), &json!(1), &json!(2)), "{k}");
        }
        assert!(!metadata(&json!({"counts": {"GET:issues.N": 3}}), 0));
    }

    /// The `/ingest` records taskq v4.6.30's own `telemetry_sync` posted (generator: fixture/taskq-telemetry/generate.rs).
    const TASKQ_BLOCKS: &str = include_str!("../../fixture/taskq-telemetry/telemetry-sync-v4.6.30.json");

    // Chain: taskq's wire records -> /machine/ingest -> replay -> changed replay -> forged cursor -> grant without
    // telemetry -> projection -> readback by event id.
    // Risk (#181 A1-A3, A6): every real block refused for its `{seq, id}` cursors, a replay stored twice, a cursor
    // that misnames its rows accepted, rows the sender never counted as a gap left invisible, or a readback that
    // differs from what was accepted.
    #[test]
    fn taskq_v4_6_30_blocks_conform() {
        let f: Value = serde_json::from_str(TASKQ_BLOCKS).unwrap();
        let recs = f["records"].as_array().unwrap().clone();
        let now = recs[2]["observed_at"].as_i64().unwrap() + 60;
        let (_d, e) = setup("tq-wire");
        let mut w = Wire { e: &e, now, code: None, calls: vec![], tick: false };
        let pair = |w: &mut Wire, label: &str, schemas: &[&str]| {
            let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: label, schemas, scope: None, capabilities: &[] };
            coreq::enroll::pair(&dir(label), "https://dash.example.com/machine", &req, w).unwrap()
        };
        let a = pair(&mut w, "tq-wire-host", &[TELEMETRY]);
        let cred = a["credential"].as_str().unwrap();
        let (code, got) = ingest(&e, cred, recs.clone(), now);
        assert_eq!((code, states(&got)), (200, vec!["accepted".to_string(); 3]), "{got}");
        assert_eq!(states(&ingest(&e, cred, recs.clone(), now + 1).1), ["accepted"; 3], "a replay returns the stored receipts");
        let mut changed = recs[0].clone();
        changed["payload"] = recs[1]["payload"].clone();
        let mut forged = recs[1].clone();
        forged["event_id"] = json!("t-forged");
        forged["payload"]["first"]["id"] = json!("0".repeat(32));
        assert_eq!(states(&ingest(&e, cred, vec![changed, forged], now + 2).1), ["rejected:replay_mismatch", "rejected:payload"]);
        let off = pair(&mut w, "tq-wire-off", &["taskq-board/1"]);
        assert_eq!(states(&ingest(&e, off["credential"].as_str().unwrap(), vec![recs[0].clone()], now).1), ["rejected:scope"]);
        let p = projection(&e, Some(Scope::All), None, now as u64 + 3).unwrap();
        let t = p["rows"].as_array().unwrap().iter().find(|r| r["installation"] == a["installation"]).unwrap()["telemetry"].clone();
        assert_eq!((&t["state"], &t["retained"]["first_seq"], &t["retained"]["last_seq"], &t["retained"]["rows"], &t["retained"]["blocks"]), (&json!("ok"), &json!(1), &json!(9), &json!(7), &json!(3)));
        // taskq pruned seq 6-7 after its cursor yet reported gap 0: only DashQ's hole shows them
        assert_eq!((&t["holes"], &t["hole_rows"], &f["status_after"][2]["gap_rows"]), (&json!([{"from_seq": 6, "to_seq": 7, "rows": 2}]), &json!(2), &json!(0)));
        assert_eq!((&t["sender"]["state"], &t["sender"]["reason"]), (&json!("unknown"), &json!("status not reported by taskq")), "v4.6.30 sends no status");
        let id = recs[2]["event_id"].as_str().unwrap();
        let b = telemetry_block(&e, &p["rows"], id);
        assert_eq!((&b["state"], &b["blocks"][0]["payload"], &b["blocks"][0]["received_at"], &b["blocks"][0]["receipt"]), (&json!("ok"), &recs[2]["payload"], &json!(now), &json!("accepted")));
        assert_eq!(b["blocks"][0]["coverage"], t);
        assert_eq!(telemetry_block(&e, &json!([]), id)["state"], "unknown", "outside the caller's rows nothing is returned");
        assert_eq!(telemetry_block(&e, &p["rows"], "t-none")["reason"], "no stored block with this event id");
    }

    /// The coreq client whose next `lose` replies are lost after the server committed them.
    struct Lossy<'a, 'b> {
        w: &'b mut Wire<'a>,
        lose: usize,
    }

    impl coreq::enroll::Wire for Lossy<'_, '_> {
        fn post(&mut self, d: &str, route: &str, auth: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            let r = self.w.post(d, route, auth, body);
            if self.lose > 0 {
                self.lose -= 1;
                return Err("timeout".into());
            }
            r
        }
        fn now(&self) -> u64 {
            self.w.now()
        }
        fn sleep(&mut self, secs: u64) {
            self.w.sleep(secs)
        }
        fn show(&mut self, uri: &str, code: &str) {
            self.w.show(uri, code)
        }
    }

    /// #161 A2/A4/A5 with the coreq reference client: a granted block is stored once across a lost reply and a
    /// restart, replays return the stored receipt, blocks outside the grant or the allowlist are refused, the
    /// projection shows taskq's sync counters (unknown with a reason when missing), and a revoke stops delivery.
    #[test]
    fn taskq_telemetry_blocks_through_the_coreq_client() {
        let (d, e) = setup("telemetry");
        let host = dir("telemetry-host");
        let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: "mac-1", schemas: &[TELEMETRY], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let inst = a["installation"].as_str().unwrap().to_string();
        let row = |e: &Result<Enroll, String>| projection(e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap()["telemetry"].clone();
        let t = row(&e);
        assert_eq!((&t["state"], &t["blocks"], &t["backlog_rows"], &t["reason"], &t["sender"]["state"], &t["retained"]), (&json!("unknown"), &json!(0), &Value::Null, &json!("no telemetry block yet"), &json!("unknown"), &Value::Null));
        let stored = |e: &Result<Enroll, String>| e.as_ref().unwrap().store.write(|tx| Ok(tx.query_row("SELECT COUNT(*) FROM telemetry", [], |r| r.get::<_, i64>(0))?)).unwrap();
        let line = |n: u64| json!({"at_ms": n, "op": "service.pass", "ms": 12, "outcome": "ok"});
        let status = json!({"backlog_rows": 5, "backlog_bytes": 900, "gap_rows": 2});
        let r1 = coreq::enroll::stamp(&host, TELEMETRY, "t-1", T0 as u64, block(1, &[line(1), line(2)], Some(status.clone()))).unwrap();
        // lost reply: the server committed, the client retries the same record and gets the stored receipt
        let got = coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut Lossy { w: &mut w, lose: 1 });
        assert!(matches!(got, Err(coreq::enroll::Delivery::Retry(_))), "{got:?}");
        let ok = json!([{"event_id": "t-1", "state": "accepted"}]);
        assert_eq!(json!(coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut w).unwrap()), ok);
        assert_eq!(stored(&e), 1);
        // restart of both sides: the client resends from its cursor, nothing is stored twice
        drop(w);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let mut w = Wire { e: &e, now: T0 + 10, code: None, calls: vec![], tick: false };
        assert_eq!(json!(coreq::enroll::deliver(&host, std::slice::from_ref(&r1), &mut Default::default(), &mut w).unwrap()), ok);
        assert_eq!(coreq::enroll::receipt(&host, &r1, &mut w).unwrap(), "accepted");
        let r2 = coreq::enroll::stamp(&host, TELEMETRY, "t-2", T0 as u64 + 10, block(3, &[line(3)], None)).unwrap();
        // allowlist inside the block: a line with a content field is dropped alone and counted (#181); a bad block
        // and a changed replay are refused, never stored
        let bad = [
            coreq::enroll::stamp(&host, TELEMETRY, "t-cmd", T0 as u64, block(4, &[json!({"op": "x", "command": "rm"}), line(5)], None)).unwrap(),
            coreq::enroll::stamp(&host, TELEMETRY, "t-rows", T0 as u64, {
                let mut p = block(4, &[line(4)], None);
                p["rows"] = json!(2);
                p
            })
            .unwrap(),
            coreq::enroll::stamp(&host, TELEMETRY, "t-1", T0 as u64, block(1, &[line(9)], None)).unwrap(),
        ];
        let got: Vec<Value> = coreq::enroll::deliver(&host, &[vec![r2.clone()], bad.to_vec()].concat(), &mut Default::default(), &mut w).unwrap();
        assert_eq!(json!(got.iter().map(|r| format!("{}{}", r["state"].as_str().unwrap(), r["reason"].as_str().map(|x| format!(":{x}")).unwrap_or_default())).collect::<Vec<_>>()),
            json!(["accepted", "accepted", "rejected:payload", "rejected:replay_mismatch"]));
        assert_eq!(stored(&e), 3);
        let kept = telemetry_block(&e, &projection(&e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"], "t-cmd")["blocks"][0]["payload"].clone();
        assert_eq!((&kept["rows"], &kept["dropped"], &kept["first"], &kept["last"]), (&json!(1), &json!(1), &json!(4), &json!(5)));
        assert!(!kept.to_string().contains("rm") && unbase64(kept["gzip"].as_str().unwrap()).is_some());
        // the newest block's counters are taskq's own; a block without status leaves them unknown with the reason
        let t = row(&e);
        assert_eq!((&t["state"], &t["blocks"], &t["last_synced"], &t["backlog_rows"], &t["reason"]), (&json!("ok"), &json!(3), &json!(T0 + 10), &Value::Null, &json!("status not reported by taskq")));
        assert_eq!(t["pruned"], json!([{"cause": "allowlist", "blocks": 1, "rows": 1, "through_seq": 5}]));
        let r3 = coreq::enroll::stamp(&host, TELEMETRY, "t-3", T0 as u64 + 20, block(4, &[line(4)], Some(status))).unwrap();
        coreq::enroll::deliver(&host, std::slice::from_ref(&r3), &mut Default::default(), &mut w).unwrap();
        let t = row(&e);
        assert_eq!((&t["backlog_rows"], &t["backlog_bytes"], &t["gap_rows"], &t["reason"]), (&json!(5), &json!(900), &json!(2), &Value::Null));
        // outside the grant: another taskq connection without the schema
        let host2 = dir("telemetry-host2");
        let req2 = coreq::enroll::Request { installation: None, product: "taskq", host_label: "mac-2", schemas: &["taskq-board/1"], scope: None, capabilities: &[] };
        let a2 = coreq::enroll::pair(&host2, "https://dash.example.com/machine", &req2, &mut w).unwrap();
        let out = coreq::enroll::stamp(&host2, TELEMETRY, "t-x", T0 as u64, block(1, &[line(1)], None)).unwrap();
        assert_eq!(json!(coreq::enroll::deliver(&host2, &[out], &mut Default::default(), &mut w).unwrap()), json!([{"event_id": "t-x", "state": "rejected", "reason": "scope"}]));
        let off = projection(&e, Some(Scope::All), None, T0 as u64 + 100).unwrap()["rows"].as_array().unwrap().iter().find(|r| r["installation"] == a2["installation"]).unwrap()["telemetry"].clone();
        assert_eq!(off, json!({"state": "off", "reason": "telemetry not in grant"}));
        // revoke: the client stops at once, the old credential stores nothing more
        coreq::enroll::disconnect(&host, &mut w).unwrap();
        let r4 = json!({"schema": TELEMETRY, "event_id": "t-4", "observed_at": T0 + 30, "payload": block(5, &[line(5)], None)});
        assert!(matches!(coreq::enroll::deliver(&host, std::slice::from_ref(&r4), &mut Default::default(), &mut w), Err(coreq::enroll::Delivery::Stopped(_))));
        assert_eq!(ingest(&e, a["credential"].as_str().unwrap(), vec![r4], T0 + 30), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(stored(&e), 4);
    }

    /// D26 A2-A4 / R1,R2,D11: real wire ingest, durable replay and scoped projection preserve turns without credentials or stage duplication.
    #[test]
    fn fleet_through_ingest_and_scoped_projection() {
        let f = crate::fleet::tests::fixture();
        let now = f["now"].as_u64().unwrap();
        let (_d, e) = setup("fleet");
        let host = dir("fleet-host");
        let mut w = Wire { e: &e, now: now as i64, code: None, calls: vec![], tick: false };
        let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: "pds18.ufobe.com", schemas: &[TELEMETRY], scope: None, capabilities: &[] };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let mut lines = f["fleet_records"][0]["records"].as_array().unwrap().clone();
        for (i,l) in lines.iter_mut().enumerate() { l["seq"] = json!(i+1); l["received_ms"] = json!(now*1000); }
        let rec = coreq::enroll::stamp(&host, TELEMETRY, "fleet-block", now, block(1,&lines,Some(json!({"backlog_rows":0,"backlog_bytes":0,"gap_rows":0})))).unwrap();
        for _ in 0..2 {
            let got=coreq::enroll::deliver(&host,std::slice::from_ref(&rec),&mut Default::default(),&mut w).unwrap();
            assert_eq!(got[0]["state"],"accepted");
        }
        let p=projection(&e,Some(Scope::Subject("github|1")),None,now).unwrap();
        // The reader must work while the shared ingestion/projection connection is occupied.
        {
            let store = e.as_ref().unwrap().store();
            let _writer = store.lock().unwrap();
            let blocks = Store::fleet_blocks(&paths(&_d).0, a["installation"].as_str().unwrap(), "ws-1", (now-7*86400) as i64).unwrap();
            assert_eq!(blocks.len(), 1);
        }
        let streams=fleet_records(&e,&p["rows"],(now-7*86400) as i64,false);
        assert_eq!(streams[0]["records"].as_array().unwrap().len(),6,"stage rows excluded; replay stores one block");
        let mut inp=f.clone(); inp["fleet_records"]=streams;
        let doc=crate::data::build(&inp);
        assert_eq!(doc["fleet"]["hosts"][0]["streams"][0]["accounts"][0]["tokens"]["input_tokens"]["observed"],100);
        let html=crate::page::render_view(&doc,&json!({}),"/fleet");
        assert!(html.contains("Tokens per project") && html.contains("invented-account") && html.contains("not-reported"));
        assert!(!html.contains(a["credential"].as_str().unwrap()));
        let denied=projection(&e,Some(Scope::Subject("github|other")),None,now).unwrap();
        assert_eq!(fleet_records(&e,&denied["rows"],0,false),json!([]));
        let revoked=p["rows"].as_array().unwrap().iter().cloned().map(|mut r|{r["connection"]=json!("revoked");r}).collect::<Vec<_>>();
        assert_eq!(fleet_records(&e,&json!(revoked),0,false),json!([]));
        if let Ok(path)=std::env::var("DASHQ_FLEET_EVIDENCE") {
            std::fs::create_dir_all(&path).unwrap();
            std::fs::write(std::path::Path::new(&path).join("page.html"),&html).unwrap();
            std::fs::write(std::path::Path::new(&path).join("page.json"),doc.to_string()).unwrap();
        }
    }

    /// Chain (#182 A1–A6): TaskQ stage rows in telemetry blocks -> coreq client -> /machine/ingest allowlist -> store
    /// -> [`stage_records`] -> `/data.agent_work` with a cutover log. Risk: stage rows dropped at ingest, nested stages
    /// or a log/telemetry overlap counted twice, elapsed time shown for open, adopted or gapped attempts, a
    /// cross-host attempt split in two, an unlinked record linked by guess.
    #[test]
    fn taskq_stage_replay_through_ingest() {
        let fx: Value = serde_json::from_str(include_str!("../../fixture/taskq-telemetry/stages.json")).unwrap();
        let t0 = fx["t0"].as_i64().unwrap();
        let (_d, e) = setup("stages");
        let mut w = Wire { e: &e, now: t0, code: None, calls: vec![], tick: false };
        for (n, inst) in fx["installations"].as_array().unwrap().iter().enumerate() {
            let host = dir(&format!("stages-host{n}"));
            let label = inst["label"].as_str().unwrap();
            let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: label, schemas: &[TELEMETRY], scope: None, capabilities: &[] };
            coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
            let recs: Vec<Value> = inst["blocks"].as_array().unwrap().iter().enumerate().map(|(i, b)| {
                let lines = b.as_array().unwrap();
                let mut p = block(lines[0]["seq"].as_u64().unwrap(), lines, None);
                p["last"] = lines[lines.len() - 1]["seq"].clone();
                coreq::enroll::stamp(&host, TELEMETRY, &format!("t-{label}-{i}"), t0 as u64, p).unwrap()
            }).collect();
            let got = coreq::enroll::deliver(&host, &recs, &mut Default::default(), &mut w).unwrap();
            assert!(got.iter().all(|r| r["state"] == "accepted"), "every stage row passes the allowlist: {got:?}");
        }
        let now = t0 as u64 + 60;
        let p = projection(&e, Some(Scope::All), None, now).unwrap();
        let stages = stage_records(&e, &p["rows"], t0 - 2 * 86400, false);
        assert_eq!(stages.as_array().unwrap().iter().map(|s| s["records"].as_array().unwrap().len()).sum::<usize>(), 19, "18 stages and one queue transition");
        // decoded once: a second read is served from the per-block memo, identical
        assert_eq!(stage_records(&e, &p["rows"], t0 - 2 * 86400, false), stages);
        // #260: while the service decodes the window a browser read names that instead of queueing behind the memo,
        // reads as unknown, and returns the records afterwards.
        e.as_ref().unwrap().warm_stages(true);
        let held = e.as_ref().unwrap().stages.lock().unwrap();
        let pending = stage_records(&e, &p["rows"], t0 - 2 * 86400, true);
        drop(held);
        assert!(pending.as_array().unwrap().iter().all(|s| s["error"] == "refresh in progress" && s["records"] == json!([])), "{pending}");
        let unread = crate::agent_work::build(&json!({"now": now, "period": 7200, "filters": {}, "projects": [], "agent_telemetry": pending}));
        assert!(unread["hosts"].as_array().unwrap().iter().all(|h| h["source"]["state"] != "ok"), "{}", unread["hosts"]);
        e.as_ref().unwrap().warm_stages(false);
        assert_eq!(stage_records(&e, &p["rows"], t0 - 2 * 86400, true), stages);
        let log: Vec<Value> = fx["log"]["mac"].as_array().unwrap().iter().filter_map(|l| serde_json::from_str(l.as_str()?.strip_prefix("taskq-stage ")?).ok()).collect();
        let snap = json!({"value": {"records": log, "corrupt": 0}, "at": now, "error": null, "incomplete": false});
        let inp = json!({"now": now, "period": 7200, "filters": {}, "projects": ["taskqorg/dashq"], "agent_telemetry": stages, "agent_work": {"mac": snap}});
        let a = crate::agent_work::build(&inp);
        let hosts: Vec<(&str, &str)> = a["hosts"].as_array().unwrap().iter().map(|h| (h["host"].as_str().unwrap(), h["source"]["state"].as_str().unwrap())).collect();
        assert_eq!(hosts, [("mac", "incomplete"), ("win", "ok")], "{}", a["hosts"]);
        assert_eq!((&a["hosts"][0]["source"]["reason"], &a["hosts"][0]["provenance"]), (&json!("2 telemetry rows not received"), &json!(["log", "telemetry"])));
        let by = |task: u64| a["attempts"].as_array().unwrap().iter().find(|t| t["task"] == task).unwrap().clone();
        let seen = |task: u64| { let t = by(task); (t["state"].as_str().unwrap().to_string(), t["reason"].as_str().map(String::from), t["elapsed_ms"].as_f64()) };
        assert_eq!(a["attempts_total"], 7);
        assert_eq!(seen(101), ("complete".into(), None, Some(670_000.0)), "nested start inside build counts once (stage sum 672 s)");
        assert_eq!(seen(102), ("partial".into(), Some("rows not received".into()), None));
        assert_eq!(seen(103), ("open".into(), Some("open-stage".into()), None));
        assert_eq!(seen(201), ("adopted".into(), Some("adopted".into()), None));
        assert_eq!(seen(301), ("complete".into(), None, Some(603_000.0)));
        assert_eq!((&by(301)["hosts"], &by(301)["runtimes"]), (&json!(["mac", "win"]), &json!(["codex"])), "one cross-host attempt, one row");
        let (t101, t401) = (by(101), by(401));
        assert_eq!((&t101["link"], &t101["project"], &t101["attempt"]), (&json!("https://github.com/taskqorg/dashq/issues/101"), &json!("taskqorg/dashq"), &json!(1001)));
        assert_eq!((&t401["link"], &t401["project"], &t401["hosts"], &t401["elapsed_ms"]), (&Value::Null, &Value::Null, &json!(["mac"]), &json!(101_000.0)), "no project: unlinked; no host: label");
        assert_eq!(seen(90), ("complete".into(), None, Some(100_000.0)), "the log fills the cutover");
        // #184 A2: wall time, summed overlapping execution and the queue wait stay apart; a missing wait says why
        assert_eq!((&t101["execution_ms"], &t101["queue_wait_s"], &t101["queue_wait_reason"]), (&json!(672_000.0), &json!(240), &Value::Null));
        assert_eq!((&by(102)["execution_ms"], &by(102)["queue_wait_s"], &by(102)["queue_wait_reason"]), (&Value::Null, &Value::Null, &json!("ready transition not retained")));
        assert_eq!((&t101["models"]["ids"], &t101["output_tokens"]["sum"], &t101["estimate"]["amounts"]), (&json!({"claude-opus-5-5": 1}), &json!(7000), &Value::Null), "stage tokens carry no cache split: no estimate");
        assert_eq!((&t101["tokens"], &by(102)["tokens"]["reasons"]), (&json!({"sum": 900_000, "known": 1, "of": 1, "reasons": []}), &json!(["no-usage"])));
        let row = |host: &str, rt: &str| a["rows"].as_array().unwrap().iter().find(|r| r["host"] == host && r["runtime"] == rt).unwrap().clone();
        let mc = row("mac", "claude");
        let claim = mc["stages"].as_array().unwrap().iter().find(|s| s["stage"] == "claim").unwrap().clone();
        assert_eq!(claim["count"], 5, "the claim of 1001 in both log and telemetry counts once");
        assert_eq!((&mc["time_ms"], &mc["elapsed"]), (&json!(871_000.0), &json!({"known": 3, "of": 5, "reasons": ["open-stage", "rows not received"]})));
        assert_eq!(mc["tokens"]["sum"], 908_000, "context tokens (cache included) summed once per build record");
        assert_eq!(row("win", "codex")["elapsed"], json!({"known": 1, "of": 2, "reasons": ["adopted"]}));
        let mut html = String::new();
        crate::agent_work::render(&mut html, &json!({"agent_work": a, "generated_at": now}));
        assert!(html.contains("<a href=\"https://github.com/taskqorg/dashq/issues/101\">Open task on board</a>") && html.contains("adopted: adopted") && html.contains("partial: rows not received") && html.contains("Unlinked task #401"), "{html}");
        let solo = crate::agent_work::build(&json!({"now": now, "period": 7200, "filters": {"host": "win"}, "agent_telemetry": stages}));
        assert_eq!((solo["hosts"].as_array().unwrap().len(), &solo["source"]["state"]), (1, &json!("ok")), "host filter; telemetry alone configures the block");
    }

    #[test]
    fn polling_lost_replies_and_expiry() {
        let (d, e) = setup("poll");
        assert_eq!(machine(&e, "/machine/connect/device", None, json!({"protocol": 2}), T0), (400, json!({"protocol": 1, "error": "unsupported_protocol"})));
        let (device, code) = start(&e, "inst-1", T0);
        let tok = |e: &Result<Enroll, String>, now| machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now);
        assert_eq!(tok(&e, T0 + 5).1["error"], "authorization_pending");
        assert_eq!(tok(&e, T0 + 7).1["error"], "slow_down");
        assert_eq!(tok(&e, T0 + 12).1["error"], "slow_down", "interval is now 10 s");
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 20).1.contains("Approved"));
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 21).1.contains("already approved"));
        // the token reply repeats after a lost response and a restart; the sealed copy is useless without the key
        let (s, first) = tok(&e, T0 + 30);
        assert_eq!(s, 200);
        drop(e);
        let raw = rusqlite::Connection::open(d.join("dashq.db")).unwrap().query_row("SELECT sealed FROM pairings", [], |r| r.get::<_, Vec<u8>>(0)).unwrap();
        assert!(!raw.windows(8).any(|w| first["credential"].as_str().unwrap().as_bytes().windows(8).any(|c| c == w)));
        let e = Enroll::open(&d, HOST);
        assert_eq!(tok(&e, T0 + 40), (200, first.clone()));
        let cred = first["credential"].as_str().unwrap();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 41), (200, json!({"protocol": 1, "state": "active", "generation": 1})));
        assert_eq!(machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 42).0, 200);
        assert_eq!(tok(&e, T0 + 43).1["error"], "invalid_grant");
        let sealed: Option<Vec<u8>> = rusqlite::Connection::open(d.join("dashq.db")).unwrap().query_row("SELECT sealed FROM pairings", [], |r| r.get(0)).unwrap();
        assert_eq!(sealed, None, "erased at acknowledgement");
        // approved but never acknowledged: expiry erases the token response
        let (device2, code2) = start(&e, "inst-2", T0 + 100);
        decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 110);
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 700).1["error"], "expired_token");
        let (_, code3) = start(&e, "inst-3", T0 + 800);
        assert!(decide(&e, "github|1", &code3, "approve", "ws-1", T0 + 1400).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": "nope"}), T0).1["error"], "invalid_grant");
    }

    #[test]
    fn configure_capability_needs_its_own_approval_and_never_widens() {
        // D16 / coreq v0.11.0: a requested operation capability is a separate, unticked effect on the approval page;
        // the token names only the approved subset, and the coreq client keeps only what it asked for and got
        let (_d, e) = setup("capability");
        let req = |inst: &str, product: &str, caps: Value| json!({"protocol": 1, "product": product, "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": [format!("{product}-snapshot/1")], "capabilities": caps});
        for (product, caps) in [("serverq", json!(["agentq.configure"])), ("agentq", json!(["agentq.run"])), ("agentq", json!(["agentq.configure", "agentq.configure"])), ("agentq", json!([])), ("agentq", json!("agentq.configure"))] {
            assert_eq!(machine(&e, "/machine/connect/device", None, req("inst-x", product, caps.clone()), T0), (400, json!({"protocol": 1, "error": "invalid_request"})), "{product} {caps}");
        }
        let token = |inst: &str, ticks: &str| {
            let (_, v) = machine(&e, "/machine/connect/device", None, req(inst, "agentq", json!(["agentq.configure"])), T0);
            let (device, code) = (v["device_code"].as_str().unwrap().to_string(), v["user_code"].as_str().unwrap().to_string());
            let p = page(&e, "github|1", &code, T0).1;
            assert!(p.contains("<input type=\"checkbox\" name=\"capability.agentq.configure\" value=\"grant\"> Allow <code>agentq.configure</code>"), "{p}");
            assert!(decide(&e, "github|1", &code, &format!("approve{ticks}"), "ws-1", T0 + 1).1.contains("Approved"));
            machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 10).1
        };
        // approving the connection alone grants no capability; a forged extra capability is ignored
        assert_eq!(token("inst-1", "&capability.agentq.run=grant")["capabilities"], json!([]));
        assert_eq!(token("inst-2", "&capability.agentq.configure=grant")["capabilities"], json!(["agentq.configure"]));
        // a request without capabilities keeps today's reply shape and page
        let (device, code) = start(&e, "inst-3", T0);
        assert!(!page(&e, "github|1", &code, T0).1.contains("Also requested"));
        decide(&e, "github|1", &code, "approve&capability.agentq.configure=grant", "ws-1", T0 + 1);
        assert!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 10).1.get("capabilities").is_none());
        // the shared client: approval without the tick leaves the active grant without the capability
        let host = dir("capability-host");
        let r = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &["agentq.configure"] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        assert_eq!(coreq::enroll::pair(&host, "https://dash.example.com/machine", &r, &mut w).unwrap()["capabilities"], json!([]));
    }

    #[test]
    fn board_scope_is_shown_granted_named_and_never_changed() {
        let (_d, e) = setup("board");
        let scope = json!({"name": "taskq-board/1", "value": "taskqorg/taskq"});
        let req = |inst: &str, scope: Value| json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1"], "scope": scope});
        for bad in [json!({"name": "taskq-board"}), json!({"name": "board", "value": "x"}), json!({"name": "taskq-board/1", "value": "a\nb"}), json!("taskq-board/1")] {
            assert_eq!(machine(&e, "/machine/connect/device", None, req("inst-x", bad.clone()), T0).1["error"], "invalid_request", "{bad}");
        }
        // A1: the page names the scope the device requested; a request without one renders as before
        let (_, v) = machine(&e, "/machine/connect/device", None, req("inst-1", scope.clone()), T0);
        let (device, code) = (v["device_code"].as_str().unwrap(), v["user_code"].as_str().unwrap());
        assert!(page(&e, "github|1", code, T0).1.contains("<dt>Board scope</dt><dd><code>taskq-board/1</code>: <code>taskqorg/taskq</code></dd>"));
        assert!(!page(&e, "github|1", &start(&e, "inst-2", T0).1, T0).1.contains("Board scope"));
        // A2: the grant keeps the approved scope and token, status and receipt replies name it
        assert!(decide(&e, "github|1", code, "approve", "ws-1", T0 + 5).1.contains("Approved"));
        let tok = |scope: Value| machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device, "scope": scope}), T0 + 10);
        assert_eq!(tok(json!({"name": "taskq-board/1", "value": "taskqorg/other"})), (403, json!({"protocol": 1, "error": "scope"})));
        let (s, t) = tok(scope.clone());
        assert_eq!((s, &t["scope"]), (200, &scope));
        let cred = t["credential"].as_str().unwrap();
        let (s, a) = machine(&e, "/machine/connect/ack", Some(cred), json!({"protocol": 1}), T0 + 11);
        assert_eq!((s, &a["scope"]), (200, &scope));
        let other = json!({"protocol": 1, "scope": {"name": "taskq-board/1", "value": "taskqorg/other"}, "records": [rec("e1", T0, json!({"n": 1}))]});
        assert_eq!(machine(&e, "/machine/ingest", Some(cred), other, T0 + 12), (403, json!({"protocol": 1, "error": "scope"})));
        assert_eq!(ingest(&e, cred, vec![rec("e1", T0, json!({"n": 1}))], T0 + 12).0, 200);
        let (s, r) = machine(&e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": "e1", "scope": scope}), T0 + 13);
        assert_eq!((s, &r["scope"]), (200, &scope));
        // revoke always ends the connection
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(cred), json!({"protocol": 1, "scope": {"name": "x/1", "value": "y"}}), T0 + 14).1["state"], "revoked");
    }

    #[test]
    fn browser_approval_denies_forgery_and_wrong_accounts() {
        let (_d, e) = setup("browser");
        let (device, code) = start(&e, "inst-1", T0);
        // no verified subject (proxy bypassed or header stripped): nothing shown or changed
        assert_eq!(call(&e, "GET", &format!("/connect?code={code}"), &[], "", T0).0, "401 Unauthorized");
        // an account without membership is never an administrator, however it arrived first
        let (s, body) = page(&e, "github|2", &code, T0);
        assert!(s == "403 Forbidden" && body.contains("not a member"), "{body}");
        let (s, body) = page(&e, "github|1", &code.to_lowercase().replace('-', ""), T0);
        assert!(s == "200 OK" && body.contains("agentq-snapshot/1") && body.contains(&code) && body.contains("type=\"hidden\" name=\"workspace\" value=\"ws-1\""), "{body}");
        // CSRF: foreign origin, missing token, a token minted for another account
        let csrf = e.as_ref().unwrap().token("github|1", &code);
        let form = format!("code={code}&workspace=ws-1&decision=approve&csrf={csrf}");
        let ok_h = [(SUBJECT, "github|1"), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        assert_eq!(call(&e, "POST", "/connect", &[ok_h[0], ("origin", "https://evil.example"), ok_h[2]], &form, T0).0, "403 Forbidden");
        assert_eq!(call(&e, "POST", "/connect", &[ok_h[0], ok_h[2]], &form, T0).0, "403 Forbidden");
        assert_eq!(call(&e, "POST", "/connect", &ok_h, &form.replace(&csrf, ""), T0).0, "403 Forbidden");
        let other = e.as_ref().unwrap().token("github|9", &code);
        assert_eq!(call(&e, "POST", "/connect", &ok_h, &form.replace(&csrf, &other), T0).0, "403 Forbidden");
        // re-pairing a connected installation says it replaces that connection
        assert!(!page(&e, "github|1", &code, T0).1.contains("already connected"));
        let (_, c0) = start(&e, "inst-0", T0);
        decide(&e, "github|1", &c0, "approve", "ws-1", T0);
        let (_, c1) = start(&e, "inst-0", T0);
        assert!(page(&e, "github|1", &c1, T0).1.contains("already connected"));
        // a forged subject header on a machine route grants nothing
        let forged = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 5);
        assert_eq!(forged.1["error"], "authorization_pending");
        // wrong workspace counts as a failed approval; five invalidate the code
        Enroll::admin(&_d, "ws-2", "github|2").unwrap();
        let s = &e.as_ref().unwrap().store;
        s.add_member("ws-2", "github|1", "member").unwrap();
        assert!(page(&e, "github|1", &code, T0).1.contains("<select name=\"workspace\""));
        for _ in 0..4 {
            assert_eq!(decide(&e, "github|2", &code, "approve", "ws-1", T0 + 10).0, "403 Forbidden");
        }
        assert_eq!(decide(&e, "github|1", &code, "deny", "ws-1", T0 + 11).0, "200 OK");
        assert!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 12).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), T0 + 20).1["error"], "access_denied");
        let (device2, code2) = start(&e, "inst-2", T0);
        for _ in 0..5 {
            decide(&e, "github|2", &code2, "approve", "ws-1", T0 + 10);
        }
        assert!(decide(&e, "github|1", &code2, "approve", "ws-1", T0 + 11).1.contains("unknown, expired"));
        assert_eq!(machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device2}), T0 + 20).1["error"], "invalid_grant");
        assert!(s.credential(b"none").unwrap().is_none());
    }

    #[test]
    fn setup_limits_and_methods() {
        let d = dir("setup");
        let none = Enroll::open(&d, HOST);
        assert!(none.is_err());
        assert_eq!(call(&none, "POST", "/machine/connect/device", &[], "{}", T0).0, "503 Service Unavailable");
        assert!(Enroll::admin(&d, "ws 1", "x").is_err() && Enroll::admin(&d, "ws", "a\nb").is_err());
        let (d, e) = setup("limits");
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(d.join("dashq.key")).unwrap().permissions()) & 0o777, 0o600);
        assert_eq!(call(&e, "GET", "/machine/connect/token", &[], "", T0).0, "405 Method Not Allowed");
        assert!(route(&e, &Req { method: "GET", path: "/data", query: "", headers: &[], body: b"" }, T0).is_none());
        // the old unprefixed machine paths are gone, no aliases (#110): the page router answers 404
        for p in MACHINE.map(|p| p.strip_prefix("/machine").unwrap()) {
            assert!(route(&e, &Req { method: "POST", path: p, query: "", headers: &[], body: b"{}" }, T0).is_none(), "{p}");
        }
        let dev = |ip: &str, inst: &str| {
            let h = [(CLIENT, ip)];
            call(&e, "POST", "/machine/connect/device", &h, &json!({"protocol": 1, "product": "agentq", "installation": inst, "runtime": "wsl", "host_label": "pc", "schemas": ["agentq-snapshot/1"]}).to_string(), T0)
        };
        for i in 0..10 {
            assert_eq!(dev("203.0.113.1", &format!("i{i}")).0, "200 OK");
        }
        let (s, h, v) = dev("203.0.113.1", "i10");
        assert_eq!((s.as_str(), h.as_str(), &v["error"]), ("429 Too Many Requests", "Retry-After: 60\r\n", &json!("rate_limited")));
        assert_eq!(dev("203.0.113.2", "i11").0, "200 OK");
        // scope: a product requests only its own schemas
        let bad = json!({"protocol": 1, "product": "serverq", "installation": "s1", "runtime": "native", "host_label": "pc", "schemas": ["agentq-snapshot/1"]});
        assert_eq!(machine(&e, "/machine/connect/device", None, bad, T0).1["error"], "invalid_request");
        assert_eq!(user_code("abcd efgh"), Some("ABCD-EFGH".into()));
        assert_eq!(user_code("ABCD-EFG1"), None);
    }

    #[test]
    fn proxy_fixture_exempts_only_machine_routes() {
        let c = include_str!("../../fixture/Caddyfile");
        let (head, guarded) = c.split_once("forward_auth").unwrap();
        // client copies of the trusted headers are stripped before anything else
        assert!(head.contains("request_header -X-Dashq-Subject") && head.contains("request_header -X-Dashq-Client"));
        let paths: Vec<&str> = head.lines().filter_map(|l| l.trim().strip_prefix("path ")).flat_map(str::split_whitespace).collect();
        // one prefix for every machine route, the resource exchange (D14) included (#110)
        assert_eq!(paths, ["/machine/*"]);
        assert!(MACHINE.iter().chain(&[crate::policy::EXCHANGE, crate::link::SEND, crate::link::RECV]).all(|p| p.starts_with("/machine/")));
        assert!(head.contains("method POST") && !head.contains("/data"));
        assert!(guarded.contains("copy_headers") && guarded.contains(">X-Dashq-Subject") && guarded.contains("reverse_proxy 127.0.0.1:18882"));
    }

    /// Approved and acknowledged connection of `inst` in `ws` as `subject`; returns the credential.
    fn connect(e: &Result<Enroll, String>, inst: &str, subject: &str, ws: &str, now: i64) -> String {
        let (device, code) = start(e, inst, now);
        assert!(decide(e, subject, &code, "approve", ws, now).1.contains("Approved"));
        let cred = machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        cred
    }

    fn rec(id: &str, observed: i64, payload: Value) -> Value {
        json!({"schema": "agentq-snapshot/1", "event_id": id, "observed_at": observed, "payload": payload})
    }

    fn ingest(e: &Result<Enroll, String>, cred: &str, records: Vec<Value>, now: i64) -> (u16, Value) {
        machine(e, "/machine/ingest", Some(cred), json!({"protocol": 1, "records": records}), now)
    }

    fn states(v: &Value) -> Vec<String> {
        v["receipts"].as_array().unwrap().iter().map(|r| format!("{}{}", r["state"].as_str().unwrap(), r["reason"].as_str().map(|x| format!(":{x}")).unwrap_or_default())).collect()
    }

    /// Exact match where `"$name"` binds on first use and a new binding never repeats another's value (coreq traces).
    fn matches(want: &Value, got: &Value, vars: &mut HashMap<String, Value>, at: &str) {
        match (want, got) {
            (Value::String(s), _) if s.starts_with('$') => match vars.get(s) {
                Some(v) => assert_eq!(v, got, "{at}: {s}"),
                None => {
                    assert!(!vars.values().any(|v| v == got), "{at}: {s} repeats a bound value");
                    vars.insert(s.clone(), got.clone());
                }
            },
            (Value::Object(w), Value::Object(g)) => {
                assert_eq!(w.keys().collect::<std::collections::BTreeSet<_>>(), g.keys().collect(), "{at}: keys");
                w.iter().for_each(|(k, v)| matches(v, &g[k], vars, &format!("{at}.{k}")));
            }
            (Value::Array(w), Value::Array(g)) => {
                assert_eq!(w.len(), g.len(), "{at}: length");
                w.iter().zip(g).enumerate().for_each(|(i, (w, g))| matches(w, g, vars, &format!("{at}[{i}]")));
            }
            _ => assert_eq!(want, got, "{at}"),
        }
    }

    fn subst(v: &Value, vars: &HashMap<String, Value>) -> Value {
        match v {
            Value::String(s) if s.starts_with('$') => vars.get(s).cloned().unwrap_or_else(|| panic!("unbound {s}")),
            Value::Object(m) => Value::Object(m.iter().map(|(k, v)| (k.clone(), subst(v, vars))).collect()),
            Value::Array(a) => Value::Array(a.iter().map(|v| subst(v, vars)).collect()),
            _ => v.clone(),
        }
    }

    /// coreq wire v1 conformance traces (fixture/enrollment: coreq v0.7.0 tests/enrollment, unchanged; agentq-operations
    /// from v0.11.0) run against these handlers; `{"server": "project"}` is the dashboard projection pass and
    /// `{"server": "queue"}` queues through the store's own authority check.
    #[test]
    fn coreq_conformance_traces() {
        let traces = [
            include_str!("../../fixture/enrollment/agentq-connect.json"),
            include_str!("../../fixture/enrollment/agentq-pairing.json"),
            include_str!("../../fixture/enrollment/agentq-reject.json"),
            include_str!("../../fixture/enrollment/serverq-connect.json"),
            include_str!("../../fixture/enrollment/agentq-operations.json"),
        ];
        for (n, text) in traces.iter().enumerate() {
            let trace: Value = serde_json::from_str(text).unwrap();
            assert_eq!(trace["contract"], coreq::enroll::CONTRACT);
            let d = dir(&format!("trace-{n}"));
            for ws in ["ws-1", "ws-2"] {
                Enroll::admin(&d, ws, "acct-1").unwrap();
            }
            let e = Enroll::open(&d, HOST);
            let mut vars = HashMap::new();
            let mut now = 0;
            for (i, step) in trace["steps"].as_array().unwrap().iter().enumerate() {
                let at_s = format!("{} step {i}", trace["about"]);
                now = step["at"].as_i64().unwrap_or(now);
                if step["server"] == "project" {
                    e.as_ref().unwrap().project(Scope::All).unwrap();
                } else if step["server"] == "queue" {
                    // the queuing account is the authority's actor; DashQ checks it against its own grants and members
                    let op = &step["operation"];
                    let got = e.as_ref().unwrap().store.queue(op["authority"]["actor"].as_str().unwrap(), op, now).unwrap();
                    assert_eq!(if got == store::Queued::Queued { "queued" } else { "refused" }, step["result"], "{at_s}: {got:?}");
                } else if let Some(action) = step["browser"].as_str() {
                    let s = subst(step, &vars);
                    // a trace approval approves what the device requested: here that is ticking each capability
                    let action = if action == "approve" { "approve&capability.agentq.configure=grant" } else { action };
                    let (_, body) = decide(&e, s["account"].as_str().unwrap(), s["user_code"].as_str().unwrap(), action, s["workspace"].as_str().unwrap(), now);
                    let got = if body.contains("Approved:") {
                        "approved"
                    } else if body.contains("already approved") {
                        "already approved"
                    } else if body.contains("Denied:") {
                        "denied"
                    } else {
                        "invalid"
                    };
                    assert_eq!(got, step["result"], "{at_s}: {body}");
                } else {
                    let auth = step.get("auth").map(|a| subst(a, &vars));
                    let (status, resp) = machine(&e, step["route"].as_str().unwrap(), auth.as_ref().and_then(Value::as_str), subst(&step["request"], &vars), now);
                    assert_eq!(status, step["status"], "{at_s}: {resp}");
                    matches(&step["response"], &resp, &mut vars, &at_s);
                    assert_eq!(coreq::enroll::check(&resp), Ok(()), "{at_s}");
                    if let Some(v) = step.get("verify") {
                        let got = coreq::enroll::verify(&resp, v["installation"].as_str().unwrap(), v["generation"].as_u64().unwrap(), v["schema"].as_str().unwrap(), v["event_id"].as_str().unwrap());
                        assert_eq!(got.err().unwrap_or("connected".into()), step["client"], "{at_s}");
                    }
                }
            }
        }
    }

    /// AgentQ's published snapshots, with and without the profile projection (agentq#101), pass the allowlist whole.
    #[test]
    fn agentq_profile_snapshots_are_accepted() {
        let (_d, e) = setup("agentq-profiles");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", T0);
        let cases: Value = serde_json::from_str(include_str!("../../fixture/agentq/configuration.json")).unwrap();
        let records: Vec<Value> = cases["cases"].as_object().unwrap().iter().map(|(k, p)| rec(k, T0, p.clone())).collect();
        let n = records.len();
        assert_eq!(states(&ingest(&e, &cred, records, T0).1), vec!["accepted"; n]);
    }

    #[test]
    fn agentq_events_enter_the_outbox_beside_snapshots() {
        let (_d, e) = setup("events");
        let (s, v) = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": "inst-1", "runtime": "native", "host_label": "mac-1", "schemas": ["agentq-snapshot/1", "agentq-event/1"]}), T0);
        assert_eq!(s, 200, "{v}");
        assert!(decide(&e, "github|1", v["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": v["device_code"]}), T0).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
        let ev = |id: &str, p: Value| json!({"schema": "agentq-event/1", "event_id": id, "observed_at": T0, "payload": p});
        let records = vec![
            ev("r1", json!({"name": "agentq.run", "metadata": {"exit": 0}})),
            ev("r2", json!({"name": "agentq.unknown", "metadata": {}})),
            ev("r3", json!({"name": "agentq.run", "metadata": {"prompt": "hi"}})),
            rec("s1", T0, json!({"version": "1.0.0"})),
            // OTLP cannot time it: refused before acknowledgement, never an overflow in the forwarder (#37)
            json!({"schema": "agentq-event/1", "event_id": "r4", "observed_at": i64::MAX, "payload": {"name": "agentq.run", "metadata": {}}}),
        ];
        assert_eq!(states(&ingest(&e, &cred, records, T0).1), ["accepted", "rejected:payload", "rejected:payload", "accepted", "rejected:payload"]);
        let store = &e.as_ref().unwrap().store;
        assert_eq!(store.outbox(9).unwrap().iter().map(|r| r.event_id.as_str()).collect::<Vec<_>>(), ["r1"]);
        // an export never becomes the snapshot or a history sample
        assert_eq!(store.project(Scope::All, renders).unwrap()[0].snapshot.as_ref().unwrap().event_id, "s1");
        assert_eq!(store.history("inst-1").unwrap().len(), 1);
    }

    #[test]
    fn board_records_outside_the_granted_board_are_refused() {
        // Chain: taskq board grant -> ingest -> projection. Risk: a record of another board (taskq#913 probe:
        // taskqorg/coreq under a taskqorg/taskq grant) is stored and shown under this grant.
        let (_d, e) = setup("board-records");
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/taskq"});
        let req = |inst: &str, scope: Value| json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["taskq-board/1"], "scope": scope});
        let join = |inst: &str, scope: Value| {
            let v = machine(&e, "/machine/connect/device", None, req(inst, scope), T0).1;
            assert!(decide(&e, "github|1", v["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
            let t = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": v["device_code"]}), T0).1;
            let cred = t["credential"].as_str().unwrap().to_string();
            assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
            cred
        };
        let board = |id: &str, repo: &str| json!({"schema": "taskq-board/1", "event_id": id, "observed_at": T0, "payload": {"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": repo}}});
        let cred = join("inst-t", scope);
        let (s, v) = ingest(&e, &cred, vec![board("in", "taskqorg/taskq"), board("out", "taskqorg/coreq"), json!({"schema": "taskq-board/1", "event_id": "none", "observed_at": T0, "payload": {"n": 1}})], T0);
        assert_eq!((s, states(&v)), (200, vec!["accepted".to_string(), "rejected:scope".into(), "rejected:scope".into()]), "{v}");
        let rows = e.as_ref().unwrap().project(Scope::All).unwrap();
        assert_eq!(rows.iter().map(|r| r.snapshot.as_ref().map(|s| s.event_id.as_str())).collect::<Vec<_>>(), [Some("in")]);
        // a grant without a board covers no board record
        let bare = join("inst-u", Value::Null);
        assert_eq!(states(&ingest(&e, &bare, vec![board("x", "taskqorg/taskq")], T0).1), ["rejected:scope"]);
    }

    #[test]
    fn ingest_takes_ownership_from_the_grant_and_allowlists_payloads() {
        let (_d, e) = setup("ingest");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", T0);
        // authority fields in a payload are data: the grant decides installation and workspace
        let forged = json!({"installation": "inst-9", "workspace": "ws-9", "version": "1.2.0", "cpu": {"percent": 12.5}, "input_tokens": 10});
        let bad = [
            json!({"prompt": "hi"}),
            json!({"nested": {"stdout": "x"}}),
            json!({"log_path": "x"}),
            json!({"where": "/Users/me"}),
            json!({"where": "C:\\Users"}),
            json!({"note": "a\nb"}),
            json!({"long": "x".repeat(257)}),
            json!("not an object"),
            json!({"a": {"b": {"c": {"d": {"e": {"f": 1}}}}}}),
            json!({"a": {"b": {"c": {"d": {"e": [[1]]}}}}}),
            json!({"a": {"b": {"c": {"d": {"e": [{"f": 1}]}}}}}),
        ];
        let mut records = vec![rec("ev-1", T0, forged.clone())];
        records.extend(bad.iter().enumerate().map(|(i, p)| rec(&format!("bad-{i}"), T0, p.clone())));
        records.push(json!({"schema": "serverq-report/1", "event_id": "other", "observed_at": T0, "payload": {}}));
        records.push(json!({"schema": "agentq-snapshot/1", "event_id": "no-time", "payload": {}}));
        let (s, v) = ingest(&e, &cred, records, T0);
        assert_eq!(s, 200, "{v}");
        let mut want = vec!["accepted".to_string()];
        want.extend(std::iter::repeat_n("rejected:payload".to_string(), bad.len()));
        want.extend(["rejected:scope".into(), "rejected:payload".into()]);
        assert_eq!(states(&v), want);
        let rows = e.as_ref().unwrap().project(Scope::All).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].installation.as_str(), rows[0].workspace.as_str()), ("inst-1", "ws-1"));
        assert_eq!(serde_json::from_str::<Value>(&rows[0].snapshot.as_ref().unwrap().payload).unwrap(), forged);
        // stable IDs: a replay repeats the receipt, changed content under the same ID fails
        assert_eq!(states(&ingest(&e, &cred, vec![rec("ev-1", T0, forged.clone())], T0 + 1).1), ["accepted"]);
        assert_eq!(states(&ingest(&e, &cred, vec![rec("ev-1", T0, json!({"version": "9"}))], T0 + 2).1), ["rejected:replay_mismatch"]);
        assert_eq!(states(&ingest(&e, &cred, vec![rec("old", T0 - 9 * 86400, json!({}))], T0 + 3).1), ["rejected:expired"]);
        assert_eq!(e.as_ref().unwrap().store.history("inst-1").unwrap().len(), 1);
        // malformed batches are refused whole
        assert_eq!(ingest(&e, &cred, vec![json!({"schema": "agentq-snapshot/1"})], T0 + 4), (400, json!({"protocol": 1, "error": "invalid_request"})));
        assert_eq!(ingest(&e, &cred, (0..101).map(|i| rec(&format!("m{i}"), T0, json!({}))).collect(), T0 + 5).0, 400);
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1}), T0 + 6).0, 400);
        // per-device rate: burst 10, then 429 with Retry-After
        let codes: Vec<u16> = (0..12).map(|_| ingest(&e, &cred, vec![], T0 + 100).0).collect();
        assert_eq!(codes, [[200; 10].as_slice(), &[429, 429]].concat());
        let (s, h, _) = call(&e, "POST", "/machine/ingest", &[("authorization", &format!("Bearer {cred}"))], r#"{"protocol": 1, "records": []}"#, T0 + 100);
        assert_eq!((s.as_str(), h.as_str()), ("429 Too Many Requests", "Retry-After: 1\r\n"));
        assert_eq!(ingest(&e, &cred, vec![], T0 + 101).0, 200);
    }

    #[test]
    fn projection_isolates_workspaces_and_members_and_keeps_expected_installations() {
        let (d, e) = setup("scope");
        Enroll::admin(&d, "ws-2", "github|2").unwrap();
        let s = &e.as_ref().unwrap().store;
        s.add_member("ws-1", "github|3", "member").unwrap();
        let admin = connect(&e, "inst-a", "github|1", "ws-1", T0);
        let member = connect(&e, "inst-m", "github|3", "ws-1", T0);
        let other = connect(&e, "inst-o", "github|2", "ws-2", T0);
        // approved, never acknowledged: expected, pending, no data
        let (device, code) = start(&e, "inst-p", T0);
        decide(&e, "github|1", &code, "approve", "ws-1", T0);
        for (c, id) in [(&admin, "a-1"), (&member, "m-1"), (&other, "o-1")] {
            assert_eq!(states(&ingest(&e, c, vec![rec(id, T0, json!({"v": 1}))], T0).1), ["accepted"]);
        }
        let seen = |scope| -> Vec<(String, &'static str, Option<String>)> {
            e.as_ref().unwrap().project(scope).unwrap().into_iter().map(|r| (r.installation, r.connection, r.snapshot.map(|s| s.event_id))).collect()
        };
        let a = |i: &str, c, ev: Option<&str>| (i.to_string(), c, ev.map(String::from));
        assert_eq!(seen(Scope::Subject("github|1")), [a("inst-a", "active", Some("a-1")), a("inst-m", "active", Some("m-1")), a("inst-p", "pending", None)]);
        assert_eq!(seen(Scope::Subject("github|3")), [a("inst-m", "active", Some("m-1"))]);
        assert_eq!(seen(Scope::Subject("github|2")), [a("inst-o", "active", Some("o-1"))]);
        assert!(seen(Scope::Subject("github|9")).is_empty());
        assert_eq!(seen(Scope::All).len(), 4);
        // a device reads only its own receipts
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&other), json!({"protocol": 1, "event_id": "a-1"}), T0).0, 404);
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&admin), json!({"protocol": 1, "event_id": "a-1"}), T0).1["state"], "projected");
        // revoked: kept as revoked, not dropped; a new approval into another workspace starts empty there
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(&admin), json!({"protocol": 1}), T0).0, 200);
        assert_eq!(ingest(&e, &admin, vec![rec("a-2", T0, json!({}))], T0 + 1), (401, json!({"protocol": 1, "error": "revoked"})));
        s.add_member("ws-2", "github|1", "member").unwrap();
        let moved = connect(&e, "inst-a", "github|1", "ws-2", T0 + 2);
        assert_eq!(seen(Scope::Subject("github|2")), [a("inst-a", "active", None), a("inst-o", "active", Some("o-1"))]);
        assert_eq!(states(&ingest(&e, &moved, vec![rec("a-1", T0 + 3, json!({"v": 2}))], T0 + 3).1), ["accepted"]);
        let ws1: Vec<_> = seen(Scope::Subject("github|1")).into_iter().filter(|r| r.0 == "inst-a").collect();
        assert_eq!(ws1, [a("inst-a", "revoked", Some("a-1")), a("inst-a", "active", Some("a-1"))]);
        let rows = e.as_ref().unwrap().project(Scope::Subject("github|1")).unwrap();
        let payloads: Vec<(&str, &str)> = rows.iter().filter(|r| r.installation == "inst-a").map(|r| (r.workspace.as_str(), r.snapshot.as_ref().unwrap().payload.as_str())).collect();
        assert_eq!(payloads, [("ws-1", r#"{"v":1}"#), ("ws-2", r#"{"v":2}"#)]);
        // a live grant is never moved to another workspace silently
        let (_, code) = start(&e, "inst-o", T0 + 4);
        assert_eq!(decide(&e, "github|1", &code, "approve", "ws-1", T0 + 4).0, "409 Conflict");
        drop(device);
    }

    /// Test config without Langfuse or GitHub and with an unreadable serverq report: every optional source fails.
    fn app(e: Result<Enroll, String>, langfuse: Option<(String, String)>) -> crate::App {
        let cfg = crate::Config { assetq: None, limits_push: None, prices: Value::Null, langfuse, forward: false, links: json!({}), github_token: None, projects: vec![], taskq_bin: None, serverq_report: "/nonexistent/report.json".into(), serverq_host: "fra/native".into(), public_host: HOST.into(), answer: None, task_controls: None, pm_requests: false, resources: false, agentq_configure: false, taskq_logs: vec![], limits: crate::store::LIMITS };
        crate::App { cfg, cache: crate::source::Cache::default(), enroll: e, answers: Default::default(), resources: Default::default(), export: Default::default(), link: Default::default() }
    }

    fn data(app: &crate::App, query: &str, scope: Option<Scope>) -> (&'static str, Value) {
        let (s, _, body) = crate::route(app, "GET", "/data", query, scope);
        (s, serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn client_receipt_and_scoped_data_project_the_same_snapshot_through_outages() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("e2e");
        let host = dir("e2e-host");
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now, code: None, calls: vec![], tick: false };
        let conn = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        let payload = json!({"version": "1.2.0", "cpu": {"percent": 7}});
        let r = coreq::enroll::stamp(&host, "agentq-snapshot/1", "snap-1", now as u64, payload.clone()).unwrap();
        let got = coreq::enroll::deliver(&host, std::slice::from_ref(&r), &mut Default::default(), &mut w).unwrap();
        assert_eq!(got, [json!({"event_id": "snap-1", "state": "accepted"})]);
        assert_eq!(coreq::enroll::receipt(&host, &r, &mut w), Ok("accepted"));
        // Langfuse unreachable, GitHub and serverq missing: the snapshot still projects and /data answers 200
        let app = app(e, Some(("https://127.0.0.1:9".into(), "Basic x".into())));
        let (s, doc) = data(&app, "", Some(Scope::Subject("github|1")));
        assert_eq!(s, "200 OK", "{doc}");
        assert_eq!((doc["sources"]["langfuse"]["state"].as_str(), doc["sources"]["github"]["reason"].as_str()), (Some("unknown"), Some("not configured")));
        assert_eq!(doc["sources"]["enrollment"]["state"], "ok");
        let row = &doc["installations"]["rows"][0];
        assert_eq!((&row["installation"], &row["generation"], &row["connection"]), (&conn["installation"], &json!(1), &json!("active")));
        assert_eq!(row["snapshot"], json!({"event_id": "snap-1", "schema": "agentq-snapshot/1", "payload": payload}));
        assert_eq!(row["observation"]["state"], "ok");
        // the client's verified receipt now names the snapshot that /data showed
        let mut w = Wire { e: &app.enroll, now, code: None, calls: vec![], tick: false };
        assert_eq!(coreq::enroll::receipt(&host, &r, &mut w), Ok("projected"));
        // scope: another account, a remote request without a subject, a host filter
        assert_eq!(data(&app, "", Some(Scope::Subject("github|2"))).1["installations"]["rows"], json!([]));
        let (_, anon) = data(&app, "", None);
        assert_eq!((anon["installations"]["rows"].clone(), anon["sources"]["enrollment"]["reason"].as_str()), (json!([]), Some("no signed-in subject")));
        assert_eq!(data(&app, "host=fra%2Fnative", Some(Scope::All)).1["installations"]["rows"], json!([]));
        assert_eq!(data(&app, &format!("host=mac-1%2F{}", coreq::enroll::runtime()), Some(Scope::All)).1["installations"]["rows"].as_array().unwrap().len(), 1);
        assert!(crate::actor(Some(HOST), HOST, None).is_none());
        assert!(matches!(crate::actor(Some(HOST), HOST, Some("github|1")), Some(Scope::Subject("github|1"))));
        assert!(matches!(crate::actor(Some("127.0.0.1:18882"), HOST, None), Some(Scope::All)));
        // enrollment not set up: the capability stays inactive, no installations section or source
        let (_, doc) = data(&self::app(Err("not set up".into()), None), "", Some(Scope::All));
        assert!(doc.get("installations").is_none() && doc["sources"].get("enrollment").is_none());
    }

    /// Approved and acknowledged connection of a standalone product installation; returns the credential.
    fn join(e: &Result<Enroll, String>, product: &str, inst: &str, runtime: &str, label: &str, schema: &str, now: i64) -> String {
        let (s, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": product, "installation": inst, "runtime": runtime, "host_label": label, "schemas": [schema]}), now);
        assert_eq!(s, 200, "{d}");
        assert!(decide(e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        cred
    }

    /// #242 A1/A3. Defect: an installation that stopped reporting raised nothing (ServerQ on Windows and WSL went
    /// silent for 16 days unnoticed, a TaskQ never linked), and a retired one stayed in every active view.
    /// Protection: no report for over an hour reads `silent` and is an Attention request with a reconnect target;
    /// a 30-day silence is `retired`: gone from hosts, checks and Attention, still listed with its history; a
    /// revoked connection is never silent.
    #[test]
    fn silent_installations_ask_for_a_reconnect_and_retired_ones_leave_active_views() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("silent");
        let schema = "serverq-report/1";
        let linux = include_str!("../../fixture/serverq/linux.json");
        let (hour, month) = (SILENT_SECS as i64, RETIRE_SECS as i64);
        let send = |cred: &str, id: &str, at: i64| {
            let (s, v) = ingest(&e, cred, vec![json!({"schema": schema, "event_id": id, "observed_at": at, "payload": serverq_report(linux, at)})], at);
            assert_eq!((s, states(&v)), (200, vec!["accepted".to_string()]), "{v}");
        };
        let fresh = join(&e, "serverq", "sq-fresh", "native", "fresh", schema, now - 600);
        send(&fresh, "r-fresh", now - 60);
        // just under the hour still reports; just over is silent
        let edge = join(&e, "serverq", "sq-edge", "native", "edge", schema, now - 2 * hour);
        send(&edge, "r-edge", now - hour + 120);
        let quiet = join(&e, "serverq", "sq-quiet", "wsl", "DESKTOP", schema, now - 2 * hour);
        send(&quiet, "r-quiet", now - hour - 120);
        // approved and acknowledged, never reported
        join(&e, "taskq", "tq-never", "wsl", "pds18", "taskq-board/1", now - 2 * hour);
        let old = join(&e, "serverq", "sq-old", "native", "old", schema, now - month - 7200);
        send(&old, "r-old", now - month - 3600);
        let gone = join(&e, "serverq", "sq-gone", "native", "gone", schema, now - 2 * hour);
        send(&gone, "r-gone", now - 2 * hour);
        assert!(e.as_ref().unwrap().store().revoke("sq-gone", now).unwrap());

        let mut app = app(e, None);
        app.cfg.serverq_report = String::new();
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let rows = doc["installations"]["rows"].as_array().unwrap();
        let activity = |inst: &str| rows.iter().find(|r| r["installation"] == inst).unwrap()["activity"].clone();
        let states: Vec<_> = ["sq-fresh", "sq-edge", "sq-quiet", "tq-never", "sq-old", "sq-gone"].map(|i| activity(i)["state"].as_str().unwrap().to_string()).into();
        assert_eq!(states, ["reporting", "reporting", "silent", "silent", "retired", "revoked"]);
        assert_eq!(activity("sq-quiet")["last_report"], now - hour - 120);
        assert!(activity("tq-never")["reason"].as_str().unwrap().starts_with("no report since approval"), "{}", activity("tq-never"));
        assert!(activity("sq-old")["reason"].as_str().unwrap().ends_with("retired from active views, history kept"));

        // Attention: exactly the two silent installations, each naming host, age and its Connections row
        let silent: Vec<&Value> = doc["attention"]["requests"].as_array().unwrap().iter().filter(|r| r["kind"] == "silent").collect();
        assert_eq!(silent.len(), 2, "{silent:?}");
        let quiet_req = silent.iter().find(|r| r["installation"] == "sq-quiet").unwrap();
        assert_eq!((quiet_req["host"].as_str(), quiet_req["source"].as_str(), quiet_req["action"].as_str()), (Some("DESKTOP/wsl"), Some("serverq"), Some("Reconnect")));
        assert_eq!(quiet_req["target"], "/integrations?period=1800#inst-sq-quiet-ws-1");
        assert!(quiet_req["age_s"].as_u64().is_some_and(|a| a > SILENT_SECS), "{quiet_req}");
        assert_eq!(quiet_req["reason"], "serverq silent: no report for 1h");
        assert!(silent.iter().any(|r| r["installation"] == "tq-never" && r["since"].is_null()));

        // A3: the retired installation leaves hosts, checks and versions, but keeps its row and history
        assert!(!doc["checks"]["installations"].as_array().unwrap().iter().any(|c| c["installation"] == "sq-old"));
        assert!(!doc["hosts"].as_array().unwrap().iter().any(|h| h["id"].as_str().is_some_and(|id| id.starts_with("old/"))), "{}", doc["hosts"]);
        assert!(!doc["versions"]["rows"].as_array().unwrap().iter().any(|r| r["installation"] == "sq-old"));
        assert!(doc["checks"]["installations"].as_array().unwrap().iter().any(|c| c["installation"] == "sq-quiet"), "a silent one stays visible");
        assert_eq!(app.enroll.as_ref().unwrap().store().history("sq-old").unwrap().len(), 1);
        let html = crate::page::render_view(&doc, &json!({}), "/integrations");
        assert!(html.contains("Silent · no report for 1h") && html.contains("Retired · no report for 30d"), "Connections names both");
        assert!(html.contains("data-inactive=\"true\" id=\"select-inst-sq-old-ws-1\""), "retired is behind the filter");
        assert!(html.contains("data-inactive=\"false\" id=\"select-inst-sq-quiet-ws-1\""), "silent with a current observation stays in the default list");
        assert!(html.contains("data-inactive=\"true\" id=\"select-inst-sq-gone-ws-1\""), "revoked stays behind the filter");
    }

    /// Enrollment E1/E6 for TaskQ (#125): a schema 1 `taskq-board/1` observation, ask-card choice text included, is a
    /// supported versions row, its board summary renders in Connections and its receipt reads `projected`; another
    /// payload schema stays `accepted` and its versions row unsupported. Risk: the D11 choice-text exception widens to
    /// other fields or schemas, or refuses the canonical TaskQ observation.
    #[test]
    fn taskq_board_snapshots_render_and_unknown_schemas_stay_unsupported() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("taskq");
        let schema = "taskq-board/1";
        let join_board = |inst: &str| {
            let scope = json!({"name": schema, "value": "github:github.com/taskqorg/taskq"});
            let d = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "win", "schemas": [schema], "scope": scope}), now).1;
            assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
            let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
            assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
            cred
        };
        let (good, v2) = (join_board("tq-win"), join_board("tq-v2"));
        let board = json!({"kind": "github", "host": "github.com", "repo": "taskqorg/taskq"});
        let choices = json!([{"number": 1, "text": "Approve narrowly scoped choice content in D11 ★", "recommended": true}, {"number": 2, "text": "Defer choice support", "recommended": false}]);
        let observation = |tasks: Value| json!({"schema": 1, "installation": "tq-win", "board": board, "observed": now, "complete": true, "reason": null, "totals": {"doing": 2, "ask": 1}, "tasks": tasks, "observations": [
            {"host":"win","kind":"online","state":"online","producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+290,"complete":true,"reason":null},
            {"host":"win","kind":"login","state":"logged in","claude":"logged in","codex":"logged in","producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+590,"complete":true,"reason":null},
            {"host":"win","kind":"queue","state":"busy","running":1,"queued":2,"producer":"aaaaaaaaaaaa","observed":now-10,"valid_until":now+290,"complete":true,"reason":null}]});
        let record = |id: &str, payload: Value| json!({"schema": schema, "event_id": id, "observed_at": now, "payload": payload});
        // text outside `tasks[].choices[].text`, or choice text that is not short single-line metadata, stays refused
        let refused = [
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "text": "raw card", "choices": choices}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "message": "x", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "two
lines", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "/etc/passwd", "recommended": true}]}])),
            observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": [{"number": 1, "text": "x".repeat(257), "recommended": true}]}])),
            json!({"schema": 1, "board": board, "text": "free text", "tasks": []}),
        ];
        let records = refused.into_iter().enumerate().map(|(i, p)| record(&format!("bad-{i}"), p)).collect();
        assert_eq!(states(&ingest(&e, &good, records, now).1), vec!["rejected:payload"; 6]);
        for (cred, id, payload) in [
            (&good, "b-1", observation(json!([{"n": 7, "state": "ask", "revision": 3, "choices": choices}, {"n": 8, "state": "doing", "revision": 4, "choices": []}]))),
            (&v2, "b-2", json!({"schema": 2, "board": board, "tasks": []})),
        ] {
            let (s, v) = ingest(&e, cred, vec![record(id, payload)], now);
            assert_eq!((s, states(&v)), (200, vec!["accepted".to_string()]), "{v}");
        }
        // the exception is TaskQ's: the same choice shape in another schema is refused
        let agent = join(&e, "agentq", "aq-1", "native", "mac", "agentq-snapshot/1", now);
        let (_, v) = ingest(&e, &agent, vec![json!({"schema": "agentq-snapshot/1", "event_id": "a-1", "observed_at": now, "payload": {"tasks": [{"choices": choices}]}})], now);
        assert_eq!(states(&v), ["rejected:payload"]);
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let receipt = |cred: &str, id: &str| machine(&app.enroll, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        assert_eq!((receipt(&good, "b-1"), receipt(&v2, "b-2")), (json!("projected"), json!("accepted")));
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "tq-win").unwrap().clone();
        assert_eq!(row["snapshot"]["payload"]["tasks"][0]["choices"], choices);
        let host_observations = |d: &Value| d["hosts"].as_array().unwrap().iter().find(|h|h["id"]=="win").unwrap()["taskq"]["observations"].clone();
        let obs = host_observations(&doc);
        assert_eq!(obs.as_array().unwrap().len(),3,"stored observations reach /data without optional sources");
        assert!(obs.as_array().unwrap().iter().any(|o|o["kind"]=="online" && o["state"]=="online" && o["valid_until"]==now+290));
        let report = app.doc(1800,json!({}),Some(Scope::All));
        assert_eq!(host_observations(&report),obs,"local report and HTTP share the projection");
        let scoped = data(&app,"host=win",Some(Scope::All)).1;
        assert_eq!(host_observations(&scoped),obs,"host filter retains board observations");
        assert!(data(&app,"",None).1["hosts"].as_array().unwrap().iter().all(|h|h["taskq"]["observations"].as_array().unwrap().is_empty()),"no observation escapes browser scope");
        let html = crate::page::render(&doc,&json!({}));
        assert!(html.contains("valid until") && html.contains("aaaaaaaaaaaa") && html.contains("logged in"));
        if let Some(dir) = std::env::var_os("DASHQ_HOST_OBSERVATIONS_EVIDENCE") {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("page.html"),html).unwrap();
        }

        let ver = |inst: &str| doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap().clone();
        assert_eq!((ver("tq-win")["provenance"].as_str(), ver("tq-win")["observation"]["state"].as_str(), ver("tq-win")["reason"].as_str()), (Some("enrollment"), Some("ok"), None));
        assert_eq!((ver("tq-v2")["provenance"].as_str(), ver("tq-v2")["reason"].as_str()), (Some("unsupported"), Some("unsupported snapshot schema taskq-board/1")));
        let html = crate::page::render(&doc, &json!({}));
        assert!(html.contains("Board github:github.com/taskqorg/taskq · doing 2 · ask 1 · 2 tasks observed"), "board summary");
    }

    /// #180 A2/A3: a `taskq-board/1` task `session` keeps host, runtime, stage and time; its worker `line` is dropped
    /// before storing, a retry of the same record keeps its receipt, and a snapshot stored with a line before #180
    /// projects without it. Risk: worker text reaches the store, `/data`, `dashq report` or the page.
    #[test]
    fn taskq_session_line_is_never_stored_or_projected() {
        let now = crate::source::now() as i64;
        let (d, e) = setup("taskq-line");
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/taskq"});
        let dev = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": "tq-1", "runtime": "native", "host_label": "win", "schemas": ["taskq-board/1"], "scope": scope}), now).1;
        assert!(decide(&e, "github|1", dev["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": dev["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let session = json!({"host": "win", "runtime": "codex", "stage": "doing", "at": now});
        let mut with_line = session.clone();
        with_line["line"] = json!("Running tests in the worker");
        let payload = |s: &Value| json!({"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": "taskqorg/taskq"}, "complete": true, "totals": {"doing": 1},
            "tasks": [{"n": 7, "state": "doing", "revision": 1, "session": s}]});
        let rec = json!({"schema": "taskq-board/1", "event_id": "b-1", "observed_at": now, "payload": payload(&with_line)});
        assert_eq!(states(&ingest(&e, &cred, vec![rec.clone()], now).1), ["accepted"]);
        let db = rusqlite::Connection::open(d.join("dashq.db")).unwrap();
        let stored: String = db.query_row("SELECT payload FROM snapshots WHERE event_id = 'b-1'", [], |r| r.get(0)).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&stored).unwrap(), payload(&session));
        assert_eq!(states(&ingest(&e, &cred, vec![rec], now).1), ["accepted"], "a retry is the same content");
        // a snapshot stored before the change keeps its line until retention ages it out; no view projects it
        db.execute("UPDATE snapshots SET payload = ?1 WHERE event_id = 'b-1'", [payload(&with_line).to_string()]).unwrap();
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "tq-1").unwrap().clone();
        assert_eq!(row["snapshot"]["payload"], payload(&session));
        assert!(!doc.to_string().contains("Running tests"), "no line in /data or dashq report");
        assert!(!crate::page::render(&doc, &json!({})).contains("Running tests"), "no line on the page");
    }

    /// #160 A1/A2: TaskQ sends its board and the release fixture `enrollment_record` (taskq v4.6.25
    /// resource-policy-fixtures, pinned in fixture/taskq-resources) in one batch. Both are accepted, the board stays
    /// the snapshot so the connection reads Connected and versions stay supported, and the projection shows as project
    /// limits aged by its status time. Risk: a resources record replacing the board snapshot (`unsupported snapshot
    /// schema`, pending readback), or a target of another installation or generation being stored.
    #[test]
    fn taskq_resources_record_shows_project_limits_beside_the_board() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("resources");
        let fixture: Value = serde_json::from_str(include_str!("../../fixture/taskq-resources/resource-policy-v1.json")).unwrap();
        let mut res = fixture["enrollment_record"].clone();
        let inst = res["payload"]["targets"][0]["target"]["installation_id"].as_str().unwrap().to_string();
        // move the fixture to now, keeping the status readback 61 s older than the send
        let shift = now - res["observed_at"].as_i64().unwrap();
        res["observed_at"] = json!(now);
        res["payload"]["observed_at"] = json!(res["payload"]["observed_at"].as_i64().unwrap() + shift);
        let scope = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/dashq"});
        let d = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac", "schemas": ["taskq-board/1", RESOURCES], "scope": scope}), now).1;
        assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let board = json!({"schema": "taskq-board/1", "event_id": "b-1", "observed_at": now - 1, "payload": {"schema": 1, "board": {"kind": "github", "host": "github.com", "repo": "taskqorg/dashq"}, "complete": true, "totals": {"doing": 1}, "tasks": []}});
        let variant = |id: &str, f: &dyn Fn(&mut Value)| {
            let mut r = res.clone();
            r["event_id"] = json!(id);
            f(&mut r);
            r
        };
        let records = vec![
            board,
            res.clone(),
            variant("other-inst", &|r| r["payload"]["targets"][0]["target"]["installation_id"] = json!("b".repeat(64))),
            variant("other-gen", &|r| r["payload"]["targets"][0]["target"]["generation"] = json!(2)),
            variant("raw", &|r| r["payload"]["targets"][0]["sample"] = json!({"path": "/etc"})),
            variant("schema", &|r| r["payload"]["schema"] = json!("taskq.resources/2")),
        ];
        let (s, v) = ingest(&e, &cred, records, now);
        assert_eq!((s, states(&v)), (200, vec!["accepted", "accepted", "rejected:scope", "rejected:scope", "rejected:payload", "rejected:payload"].into_iter().map(String::from).collect::<Vec<_>>()), "{v}");
        let app = app(e, None);
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let row = doc["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap().clone();
        assert_eq!((&row["projected"], &row["snapshot"]["schema"]), (&json!(true), &json!("taskq-board/1")));
        let l = &row["project_limits"];
        // age from the document's own clock: the route reads the wall clock again, maybe one second after `now`
        let age = doc["generated_at"].as_i64().unwrap() - res["payload"]["observed_at"].as_i64().unwrap();
        assert!((61..=62).contains(&age), "{age}");
        assert_eq!((&l["state"], &l["age_s"], &l["reason"]), (&json!("ok"), &json!(age), &Value::Null), "{l}");
        assert_eq!(l["targets"], json!([{"project_id": "project-1", "runtime_id": "codex", "policy_schemas": ["taskq.project-limits/1"], "current_revision": "1".repeat(64), "limit": 1,
            "effective_revision": null, "effective": null, "blockers": [{"code": "sample", "knowledge": "unknown"}], "completeness": "unknown"}]));
        let ver = doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst.as_str()).unwrap().clone();
        assert_eq!((ver["provenance"].as_str(), ver["reason"].as_str()), (Some("enrollment"), None));
        let html = crate::page::render(&doc, &json!({}));
        assert!(html.contains("✓ Connected") && !html.contains("unsupported snapshot schema"), "{html}");
        assert!(html.contains(&format!("Project limits: observed {} ago", crate::data::dur(age as u64))), "{html}");
        assert!(html.contains("project-1 · codex: limit 1 · effective <span class=\"unknown\" title=\"unknown: sample unknown\">—</span> · unknown"), "{html}");
    }

    /// A ServerQ projection fixture (serverq.md § 3) moved so its report time is `at`; every time keeps its offset.
    fn serverq_report(fixture: &str, at: i64) -> Value {
        let mut p: Value = serde_json::from_str(fixture).unwrap();
        let shift = at - p["observed_at"].as_i64().unwrap();
        fn walk(v: &mut Value, shift: i64) {
            match v {
                Value::Object(o) => o.iter_mut().for_each(|(k, x)| match x.as_i64() {
                    Some(t) if ["observed_at", "first_seen", "last_seen", "verified_at"].contains(&k.as_str()) => *x = json!(t + shift),
                    _ => walk(x, shift),
                }),
                _ => {}
            }
        }
        walk(&mut p, shift);
        p
    }

    /// Enrollment E1/E8 for ServerQ (#122): ServerQ-only installations on Linux (WSL) and Windows under one host label
    /// fill their own hosts rows and Server checks, stay fresh by producer time within the serverq limit, and their
    /// receipts read `projected` only after a view rendered them; an unsupported schema never does.
    #[test]
    fn serverq_snapshots_render_per_installation_and_drive_projected_receipts() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("serverq");
        let schema = "serverq-report/1";
        let (linux, windows) = (include_str!("../../fixture/serverq/linux.json"), include_str!("../../fixture/serverq/windows.json"));
        let wsl = join(&e, "serverq", "sq-wsl", "wsl", "DESKTOP", schema, now);
        let native = join(&e, "serverq", "sq-native", "native", "DESKTOP", schema, now);
        // a lost identity on the same label and runtime: an older, stale report of its own
        let lost = join(&e, "serverq", "sq-lost", "native", "DESKTOP", schema, now);
        let other = join(&e, "taskq", "tq-1", "native", "DESKTOP", "taskq-snapshot/1", now);
        // the right schema name around a payload the view cannot render: another payload schema, no rules
        let v2 = join(&e, "serverq", "sq-v2", "native", "other", schema, now);
        let bare = join(&e, "serverq", "sq-bare", "native", "other", schema, now);
        let send = |cred: &str, id: &str, schema: &str, at: i64, payload: Value| {
            let (s, v) = ingest(&e, cred, vec![json!({"schema": schema, "event_id": id, "observed_at": at, "payload": payload})], now);
            assert_eq!((s, states(&v)), (200, vec!["accepted".to_string()]), "{v}");
        };
        send(&wsl, "r-wsl", schema, now - 180, serverq_report(linux, now - 180));
        // ServerQ sends no heartbeat between hourly checks: 7000 s is fresh, 7300 s is stale
        send(&native, "r-native", schema, now - 7000, serverq_report(windows, now - 7000));
        send(&lost, "r-lost", schema, now - 7300, serverq_report(linux, now - 7300));
        send(&other, "t-1", "taskq-snapshot/1", now, json!({"schema": 1}));
        send(&v2, "r-v2", schema, now, json!({"schema": 2, "product": "serverq", "rules": {}}));
        send(&bare, "r-bare", schema, now, json!({"schema": 1, "product": "serverq"}));
        let receipt = |cred: &str, id: &str| machine(&e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        assert_eq!(receipt(&wsl, "r-wsl"), "accepted", "no view has read it yet");

        let mut app = app(e, None);
        app.cfg.serverq_report = String::new();
        let (s, doc) = data(&app, "", Some(Scope::All));
        assert_eq!(s, "200 OK", "{doc}");
        let e = &app.enroll;
        let receipt = |cred: &str, id: &str| machine(e, "/machine/connection/receipt", Some(cred), json!({"protocol": 1, "event_id": id}), now).1["state"].clone();
        for (cred, id) in [(&wsl, "r-wsl"), (&native, "r-native"), (&lost, "r-lost")] {
            assert_eq!(receipt(cred, id), "projected", "{id}");
        }
        assert_eq!(receipt(&other, "t-1"), "accepted", "no view renders taskq-snapshot/1");
        assert_eq!((receipt(&v2, "r-v2"), receipt(&bare, "r-bare")), (json!("accepted"), json!("accepted")), "an unsupported payload never reads projected");
        let rows = doc["installations"]["rows"].as_array().unwrap();
        let row = |inst: &str| rows.iter().find(|r| r["installation"] == inst).unwrap().clone();
        let projected = |inst: &str| row(inst)["projected"].clone();
        assert_eq!([projected("sq-native"), projected("tq-1"), projected("sq-v2"), projected("sq-bare")], [true, false, false, false].map(|b| json!(b)));
        assert_eq!((row("sq-native")["observation"]["state"].as_str(), row("sq-lost")["observation"]["state"].as_str()), (Some("ok"), Some("stale")));

        // hosts: one row per label and runtime; the newest observation owns a shared row
        let host = |id: &str| doc["hosts"].as_array().unwrap().iter().find(|h| h["id"] == id).unwrap()["serverq"].clone();
        let (h_wsl, h_native) = (host("DESKTOP/wsl"), host("DESKTOP/native"));
        assert_eq!((h_wsl["status"].as_str(), h_wsl["state"].as_str(), h_wsl["installation"].as_str(), h_wsl["at"].as_i64()), (Some("warn"), Some("ok"), Some("sq-wsl"), Some(now - 180)));
        // age is read at /data time, a second or more after `now` on a slow host
        assert!(h_wsl["age_s"].as_i64().is_some_and(|a| (180..190).contains(&a)), "{h_wsl}");
        assert_eq!((h_native["status"].as_str(), h_native["state"].as_str(), h_native["installation"].as_str()), (Some("unknown"), Some("ok"), Some("sq-native")));

        // Server checks: each installation apart, per-rule status, reason and producer time
        let checks = doc["checks"]["installations"].as_array().unwrap();
        assert_eq!(checks.len(), 5);
        for inst in ["sq-v2", "sq-bare"] {
            let c = checks.iter().find(|c| c["installation"] == inst).unwrap();
            assert_eq!((c["source"]["state"].as_str(), c["source"]["reason"].as_str()), (Some("unknown"), Some("unsupported serverq projection (schema 1 expected)")), "{c}");
        }
        let check = |inst: &str| checks.iter().find(|c| c["installation"] == inst).unwrap().clone();
        let win = check("sq-native");
        assert_eq!((win["host"].as_str(), win["version"].as_str(), win["source"]["state"].as_str()), (Some("DESKTOP/native"), Some("1.3.0"), Some("ok")));
        assert_eq!(win["results"].as_array().unwrap().len(), 5);
        for r in win["results"].as_array().unwrap() {
            assert_eq!((r["status"].as_str(), r["reason"].as_str(), r["observed_at"].as_i64()), (Some("unknown"), Some("unsupported-os"), Some(now - 7000)), "{r}");
        }
        let manual: Vec<&str> = win["manual_rules"].as_array().unwrap().iter().map(|m| m["status"].as_str().unwrap()).collect();
        assert_eq!(manual, ["not verified"; 4], "manual rules are never verified");
        let lin = check("sq-wsl");
        let rule = |name: &str| lin["results"].as_array().unwrap().iter().find(|r| r["check"] == name).unwrap().clone();
        let mut failed = rule("failed-services");
        let source = failed.as_object_mut().unwrap().remove("source").unwrap();
        assert_eq!(source["state"], "ok");
        assert_eq!(source["at"], now - 180);
        assert_eq!(failed, json!({"check": "failed-services", "status": "warn", "reason": "failed-units", "observed_at": now - 180, "evidence": {"units": 1}}));
        assert_eq!(rule("bounded-journal")["observed_at"], now - 180 - 3600, "a cached rule keeps its own observation time");
        assert_eq!(lin["incidents"], json!([{"check": "failed-services", "state": "observed", "first_seen": now - 180 - 7200, "last_seen": now - 180, "occurrences": 3}]));
        assert_eq!(check("sq-lost")["source"]["state"], "stale");

        // Attention: the warning is current, unknown rules are gaps, each naming its installation
        let att = &doc["attention"];
        assert!(att["current"].as_array().unwrap().iter().any(|i| i["installation"] == "sq-wsl" && i["what"] == "failed-services: warn" && i["host"] == "DESKTOP/wsl"));
        assert_eq!(att["gaps"].as_array().unwrap().iter().filter(|i| i["installation"] == "sq-native" && i["detail"] == "unsupported-os").count(), 5);
        // versions: the enrolled ServerQ version, an unknown schema stays unsupported
        let ver = |inst: &str| doc["versions"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap().clone();
        assert_eq!((ver("sq-native")["provenance"].as_str(), ver("sq-native")["running"]["version"].as_str()), (Some("enrollment"), Some("1.3.0")));
        assert_eq!(ver("tq-1")["provenance"], "unsupported");

        // the page renders what /data says; equal host labels stay apart
        let html = crate::page::render(&doc, &json!({}));
        let section = &html[html.find("<section id=\"checks\">").unwrap()..];
        for s in ["DESKTOP/wsl · ServerQ 1.3.0 · <code>sq-wsl</code>", "DESKTOP/native · ServerQ 1.3.0 · <code>sq-native</code>", "<code>sq-lost</code>", "unsupported-os", "failed-units", "Last known ok", "not verified"] {
            assert!(section.contains(s), "{s}");
        }
        assert!(html.contains("<section id=\"host-grid\">") && !html.contains("no serverq report for this host"));
    }

    /// H1/H2 chain: ingest -> retained store -> scoped projection -> Attention/page, without any task write.
    #[test]
    fn serverq_episode_chain_keeps_identity_recovery_and_recurrence() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("finding-40");
        let schema = "serverq-report/1";
        let a = join(&e, "serverq", "sq-a", "native", "same", schema, now);
        let b = join(&e, "serverq", "sq-b", "native", "same", schema, now);
        let mut app = app(e, None);
        app.cfg.serverq_report = String::new();
        let e = &app.enroll;
        let report = |at: i64, status: &str, first: i64, verified: Option<i64>| json!({
            "schema": 1, "product": "serverq", "catalog": crate::findings::catalog_hash(), "observed_at": at,
            "rules": {"disk-headroom": {"status": status, "observed_at": at, "evidence": {"available": 20, "total": 100}}},
            "incidents": {"disk-headroom": {"state": if verified.is_some() { "verified" } else { "observed" }, "first_seen": first, "last_seen": first, "verified_at": verified, "occurrences": 1}}
        });
        let send = |cred: &str, id: &str, at: i64, p: Value| {
            assert_eq!(ingest(e, cred, vec![json!({"schema": schema, "event_id": id, "observed_at": at, "payload": p})], now).0, 200);
        };
        let first = now - 500;
        send(&a, "warn-a", now - 400, report(now - 400, "warn", first, None));
        send(&b, "warn-b", now - 400, report(now - 400, "warn", first, None));
        // A newer incomplete frame must not erase the earlier open episode.
        send(&a, "missing", now - 300, json!({"schema": 1, "product": "serverq", "rules": {}, "incidents": {}, "complete": false}));
        let check = |doc: &Value, inst: &str| doc["checks"]["installations"].as_array().unwrap().iter().find(|c| c["installation"] == inst).unwrap().clone();
        let doc = data(&app, "", Some(Scope::All)).1;
        assert_eq!(check(&doc, "sq-a")["findings"][0]["state"], "open");
        assert!(doc["attention"]["gaps"].as_array().unwrap().iter().any(|g| g["installation"] == "sq-a" && g["episode"]["first_seen"] == first));
        let mut unknown = report(now - 200, "unknown", first, None);
        send(&a, "unknown", now - 200, unknown.clone());
        unknown["rules"]["disk-headroom"]["observed_at"] = json!(now - 8000);
        unknown["rules"]["disk-headroom"]["status"] = json!("ok");
        unknown["incidents"]["disk-headroom"]["state"] = json!("verified");
        unknown["incidents"]["disk-headroom"]["verified_at"] = json!(now - 100);
        send(&a, "cached-pass", now - 100, unknown);
        let cached = check(&data(&app, "", Some(Scope::All)).1, "sq-a");
        assert_eq!(cached["findings"][0]["state"], "open");
        assert_eq!(cached["results"][0]["source"]["state"], "stale");
        // The full page also exposes the stale rule as last known, never a fresh green pass.
        let fragment = crate::page::render(&data(&app, "", Some(Scope::All)).1, &json!({}));
        assert!(fragment.contains("Last known ok") && fragment.contains("rule observation stale"));
        send(&a, "pass", now - 50, report(now - 50, "ok", first, Some(now - 50)));
        // Received later, observed earlier: cannot undo the matching pass.
        send(&a, "late-warn", now - 350, report(now - 350, "warn", first, None));
        let doc = data(&app, "", Some(Scope::All)).1;
        assert_eq!(check(&doc, "sq-a")["findings"][0]["state"], "verified");
        assert_eq!(check(&doc, "sq-b")["findings"][0]["state"], "open");
        // A3/H2: reconstruct history through the real retained-store boundary after expiry.
        let aged = projection(e, Some(Scope::All), None, (now + 7201) as u64).unwrap();
        let samples = finding_samples(e, &aged["rows"]);
        for inst in ["sq-a", "sq-b"] {
            let row = aged["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap();
            let retained = samples.as_array().unwrap().iter().find(|r| r["installation"] == inst).unwrap();
            let findings = crate::findings::build(row, retained, (now + 7201) as u64);
            assert_eq!(findings["episodes"][0]["state"], if inst == "sq-a" { "verified" } else { "open" });
            assert_eq!(findings["episodes"][0]["source"]["state"], "stale");
        }
        send(&a, "recurrence", now - 10, report(now - 10, "warn", now - 10, None));
        let doc = data(&app, "", Some(Scope::All)).1;
        let c = check(&doc, "sq-a");
        assert_eq!(c["findings"].as_array().unwrap().len(), 2);
        assert_eq!(c["findings"][0]["state"], "verified");
        assert_eq!(c["findings"][1]["state"], "open");
        let html = crate::page::render(&doc, &json!({}));
        for expected in ["verified", "Catalog matched", "minimum_percent: 15", "installation <code>sq-a</code>", "installation <code>sq-b</code>", "available 20"] {
            assert!(html.contains(expected), "{expected}");
        }
        if let Some(dir) = std::env::var_os("DASHQ_FINDINGS_EVIDENCE") {
            let dir = std::path::PathBuf::from(dir);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("page.html"), html.replace("<link rel=\"stylesheet\" href=\"/style.css\">", &format!("<style>{}</style>", crate::style_css())).replace("<script src=\"/app.js\" defer></script>", "")).unwrap();
            std::fs::write(dir.join("data.json"), serde_json::to_vec_pretty(&doc).unwrap()).unwrap();
        }
    }

    #[test]
    fn late_stale_and_future_observations_keep_truthful_freshness() {
        let now = crate::source::now() as i64;
        let (_d, e) = setup("fresh");
        let cred = connect(&e, "inst-1", "github|1", "ws-1", now);
        let app = app(e, None);
        let e = &app.enroll;
        let obs = |q: &str| data(&app, q, Some(Scope::All)).1["installations"]["rows"][0].clone();
        // expected installation without any event
        assert_eq!(obs("")["observation"], json!({"state": "unknown", "at": null, "age_s": null, "reason": "no snapshot yet", "incomplete": false}));
        // observed 400 s ago: stale by producer time, whatever the arrival time
        assert_eq!(states(&ingest(e, &cred, vec![rec("s-1", now - 400, json!({"v": 1}))], now).1), ["accepted"]);
        let o = obs("");
        assert_eq!((o["observation"]["state"].as_str(), o["observation"]["at"].as_i64()), (Some("stale"), Some(now - 400)));
        // a late sample joins history only and is never projected as current
        assert_eq!(states(&ingest(e, &cred, vec![rec("s-0", now - 900, json!({"v": 0}))], now + 1).1), ["accepted"]);
        assert_eq!(obs("")["snapshot"]["event_id"], "s-1");
        assert_eq!(machine(e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-0"}), now + 2).1["state"], "accepted");
        assert_eq!(machine(e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 2).1["state"], "projected");
        // future time: shown as clock-uncertain, never fresh, and the next real sample replaces it
        ingest(e, &cred, vec![rec("s-f", now + 3600, json!({"v": 9}))], now + 3);
        let o = obs("");
        assert_eq!((o["snapshot"]["event_id"].as_str(), o["observation"]["state"].as_str(), o["observation"]["age_s"].clone()), (Some("s-f"), Some("unknown"), Value::Null));
        ingest(e, &cred, vec![rec("s-2", now, json!({"v": 2}))], now + 4);
        let o = obs("");
        assert_eq!((o["snapshot"]["event_id"].as_str(), o["observation"]["state"].as_str()), (Some("s-2"), Some("ok")));
    }

    fn get(e: &Result<Enroll, String>, subject: &str, path: &str, now: i64) -> (String, String, String) {
        let (s, h, v) = call(e, "GET", path, &[(SUBJECT, subject)], "", now);
        (s, h, v.as_str().unwrap_or_default().to_string())
    }

    fn revoke(e: &Result<Enroll, String>, subject: &str, token_subject: &str, inst: &str, ws: &str, now: i64) -> (String, String) {
        let csrf = e.as_ref().unwrap().token_for(&revoke_purpose(token_subject, inst, ws));
        let h = [(SUBJECT, subject), ("origin", "https://dash.example.com"), ("content-type", "application/x-www-form-urlencoded")];
        let (s, _, v) = call(e, "POST", "/connect", &h, &format!("decision=revoke&installation={inst}&workspace={ws}&csrf={csrf}"), now);
        (s, v.as_str().unwrap().to_string())
    }

    #[test]
    fn approval_follows_projection_to_overview_and_lifecycle_reads_stay_scoped() {
        let now = crate::source::now() as i64;
        let (d, e) = setup("life");
        Enroll::admin(&d, "ws-2", "github|2").unwrap();
        e.as_ref().unwrap().store.add_member("ws-1", "github|3", "member").unwrap();
        // approval page follows this generation to the overview at 2 s
        let (device, code) = start(&e, "inst-1", now);
        let (_, body) = decide(&e, "github|1", &code, "approve", "ws-1", now);
        let wait = format!("/connect?wait=inst-1&workspace=ws-1&generation=1&since={now}");
        assert!(body.contains(&format!("content=\"2; url={}\"", wait.replace('&', "&amp;"))), "{body}");
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now).0, 200);
        let (s, _, b) = get(&e, "github|1", &wait, now + 2);
        assert!(s == "200 OK" && b.contains("Waiting for the first snapshot") && b.contains("http-equiv=\"refresh\""), "{b}");
        // another account cannot follow or even see it
        assert_eq!(get(&e, "github|2", &wait, now + 2).0, "404 Not Found");
        assert_eq!(states(&ingest(&e, &cred, vec![rec("s-1", now, json!({"v": 1}))], now + 3).1), ["accepted"]);
        let (s, h, _) = get(&e, "github|1", &wait, now + 4);
        assert_eq!((s.as_str(), h.as_str()), ("303 See Other", "Location: /#inst-inst-1-ws-1\r\n"));
        assert_eq!(machine(&e, "/machine/connection/receipt", Some(&cred), json!({"protocol": 1, "event_id": "s-1"}), now + 4).1["state"], "projected");
        assert!(get(&e, "github|1", &wait.replace("generation=1", "generation=7"), now + 4).2.contains("replaced or revoked"));
        // a host that never sends: progress ends after 60 s with an honest state
        let other = connect(&e, "inst-2", "github|2", "ws-2", now);
        let w2 = format!("/connect?wait=inst-2&workspace=ws-2&generation=1&since={now}");
        assert!(get(&e, "github|2", &w2, now + 30).2.contains("Waiting for the first snapshot"));
        assert!(get(&e, "github|2", &w2, now + 61).2.contains("No snapshot arrived within 60 s"));
        // history warmup until 3 samples; export gaps per workspace; data age apart from both
        let app = app(e, None);
        let e = &app.enroll;
        let row = |scope, inst: &str| data(&app, "", scope).1["installations"]["rows"].as_array().unwrap().iter().find(|r| r["installation"] == inst).cloned();
        let r1 = row(Some(Scope::All), "inst-1").unwrap();
        assert_eq!((r1["history"].clone(), r1["observation"]["state"].clone()), (json!({"samples": 1, "state": "warmup", "reason": "collecting history: 1 of 3 samples"}), json!("ok")));
        ingest(e, &cred, vec![rec("s-2", now + 1, json!({"v": 2})), rec("s-3", now + 2, json!({"v": 3}))], now + 5);
        assert_eq!(row(Some(Scope::All), "inst-1").unwrap()["history"], json!({"samples": 3, "state": "ok", "reason": null}));
        let s = &e.as_ref().unwrap().store;
        s.accept(&sha256(&cred), &[Event { id: "x-1", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        s.accept(&sha256(&other), &[Event { id: "x-2", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }, Event { id: "x-3", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        s.prune(now + 7 * 86400 + 1).unwrap();
        assert_eq!(row(Some(Scope::All), "inst-1").unwrap()["export"], json!({"gap_events": 1, "state": "gap", "reason": "events expired before export: 1"}));
        assert_eq!(row(Some(Scope::All), "inst-2").unwrap()["export"]["gap_events"], 2);
        // scoped JSON and HTML: no other workspace's rows, evidence IDs or gaps; no subject: nothing
        let (_, doc) = data(&app, "", Some(Scope::Subject("github|2")));
        let text = doc["installations"].to_string();
        assert!(!text.contains("inst-1") && !text.contains("s-3") && text.contains("inst-2"), "{text}");
        let html = |scope| String::from_utf8(crate::route(&app, "GET", "/", "", scope).2).unwrap();
        let h2 = html(Some(Scope::Subject("github|2")));
        assert!(h2.contains("id=\"inst-inst-2-ws-2\"") && !h2.contains("inst-1") && !h2.contains("s-3"));
        let h1 = html(Some(Scope::Subject("github|1")));
        assert!(h1.contains("<code>s-3</code>") && h1.contains("gap · events expired before export: 1") && !h1.contains("inst-2"));
        assert!(h1.contains("href=\"/connect?revoke=inst-1&amp;workspace=ws-1\""));
        assert!(!html(None).contains("inst-"));
        let all = html(Some(Scope::All));
        assert!(all.contains("inst-inst-1-ws-1") && all.contains("inst-inst-2-ws-2"));
        // revoke: confirmation names the installation and effect; only the workspace admin or its approver
        assert_eq!(get(e, "github|2", "/connect?revoke=inst-1&workspace=ws-1", now).0, "404 Not Found");
        let (s1, _, b) = get(e, "github|1", "/connect?revoke=inst-1&workspace=ws-1", now);
        assert!(s1 == "200 OK" && b.contains("Revoke <strong>mac-1 (native)</strong>") && b.contains("Accepted history stays"), "{b}");
        assert_eq!(revoke(e, "github|2", "github|1", "inst-1", "ws-1", now).0, "403 Forbidden");
        assert!(revoke(e, "github|2", "github|2", "inst-1", "ws-1", now).1.contains("Nothing to revoke"));
        assert!(revoke(e, "github|3", "github|3", "inst-1", "ws-1", now).1.contains("Nothing to revoke"), "a member revokes only its own approvals");
        let (_, _, wrong_origin) = call(e, "POST", "/connect", &[(SUBJECT, "github|1"), ("origin", "https://evil.example"), ("content-type", "application/x-www-form-urlencoded")], "decision=revoke&installation=inst-1&workspace=ws-1", now);
        assert!(wrong_origin.as_str().unwrap().contains("did not come from"));
        assert!(revoke(e, "github|1", "github|1", "inst-1", "ws-1", now).1.contains("Revoked: installation <code>inst-1</code>"));
        assert_eq!(ingest(e, &cred, vec![rec("s-4", now, json!({}))], now + 6), (401, json!({"protocol": 1, "error": "revoked"})));
        let r1 = row(Some(Scope::All), "inst-1").unwrap();
        assert_eq!((r1["connection"].as_str(), r1["snapshot"]["event_id"].as_str()), (Some("revoked"), Some("s-3")));
        let h1 = html(Some(Scope::Subject("github|1")));
        assert!(h1.contains("history kept") && !h1.contains("revoke=inst-1"));
        assert!(get(e, "github|1", "/connect?revoke=inst-1&workspace=ws-1", now).2.contains("already revoked"));
        assert!(revoke(e, "github|1", "github|1", "inst-1", "ws-1", now).1.contains("Nothing to revoke"));
        // the local report reads the store only with the host administrator's private key
        #[cfg(unix)]
        {
            std::fs::set_permissions(d.join("dashq.key"), std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
            assert_eq!(Enroll::open(&d, HOST).err().unwrap(), "dashq.key: mode must be 0600");
        }
    }
    /// L1-L3: approval -> activation -> durable snapshot -> matching projection; receipt age is not
    /// producer freshness, reconnect cannot borrow an old generation, and lost identity never merges.
    #[test]
    fn connections_lifecycle_reference_chain() {
        let now = crate::source::now() as i64;
        let (_, e) = setup("connections-reference");
        let emit = |name: &str, html: &str| {
            if let Some(dir) = std::env::var_os("DASHQ_CONNECTION_EVIDENCE") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(Path::new(&dir).join(format!("{name}.html")), html).unwrap();
            }
        };
        emit("entry", &get(&e, "github|1", "/connect", now).2);
        let (device, code) = start(&e, "inst-reference", now);
        let review = page(&e, "github|1", &code, now).1;
        emit("approval", &review);
        let (_, approved) = decide(&e, "github|1", &code, "approve", "ws-1", now);
        assert!(!approved.contains("is connected"), "approval alone is not connection: {approved}");
        assert!(review.contains("https://dash.example.com") && review.contains("observation only"));
        emit("approved", &approved);
        let wait = format!("/connect?wait=inst-reference&workspace=ws-1&generation=1&since={now}");
        let pending = get(&e, "github|1", &wait, now + 2).2;
        assert!(pending.contains("credential activation"));
        emit("activation", &pending);
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": device}), now + 5).1["credential"].as_str().unwrap().to_string();
        machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), now + 5);
        let pending = get(&e, "github|1", &wait, now + 6).2;
        assert!(pending.contains("Waiting for the first snapshot"));
        emit("snapshot", &pending);
        let timeout = get(&e, "github|1", &wait, now + 61).2;
        assert!(timeout.contains("Check again for 60 s") && !timeout.contains("http-equiv=\"refresh\""));
        emit("timeout", &timeout);
        let payload = json!({"version": "1.2.3", "agentq.resource.cpu_unknown": "warmup"});
        ingest(&e, &cred, vec![rec("reference-1", now - 1000, payload.clone())], now);
        // Same delivery retry retains one installation and snapshot; its original observation time stays old.
        ingest(&e, &cred, vec![rec("reference-1", now - 1000, payload)], now + 11);
        assert_eq!(get(&e, "github|1", &wait, now + 12).0, "303 See Other");
        let p = projection(&e, Some(Scope::Subject("github|1")), None, (now + 12) as u64).unwrap();
        let r = &p["rows"][0];
        assert_eq!(p["rows"].as_array().unwrap().len(), 1);
        assert_eq!((&r["projected"], &r["observation"]["state"], &r["export"]["state"]), (&json!(true), &json!("stale"), &json!("unknown")));
        assert_eq!(r["received_at"], now);
        assert_eq!(r["history"]["samples"], 1);
        let app = app(e, None);
        let e = &app.enroll;
        let render = || String::from_utf8(crate::route(&app, "GET", "/", "", Some(Scope::Subject("github|1"))).2).unwrap();
        let stale = render();
        assert!(stale.contains("✓ Connected") && stale.contains("stale") && stale.contains("export forwarding not qualified"));
        emit("stale", &stale);
        emit("revoke", &get(e, "github|1", "/connect?revoke=inst-reference&workspace=ws-1", now).2);
        revoke(e, "github|1", "github|1", "inst-reference", "ws-1", now + 13);
        emit("revoked", &render());
        assert_eq!(ingest(e, &cred, vec![rec("old-credential", now, json!({}))], now + 14).0, 401);
        let next = connect(e, "inst-reference", "github|1", "ws-1", now + 20);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 21) as u64).unwrap();
        assert_eq!(p["rows"].as_array().unwrap().len(), 1);
        assert_eq!((&p["rows"][0]["generation"], &p["rows"][0]["projected"]), (&json!(2), &json!(false)));
        assert!(!render().contains("✓ Connected"), "new generation cannot inherit the previous readback");
        let wait2 = format!("/connect?wait=inst-reference&workspace=ws-1&generation=2&since={now}");
        assert_eq!(get(e, "github|1", &wait2, now + 21).0, "200 OK", "old history is not new readback");
        ingest(e, &next, vec![rec("reference-2", now, json!({"version": "1.2.3", "agentq.resource.cpu_unknown": "warmup"}))], now);
        assert_eq!(get(e, "github|1", &wait2, now + 22).0, "303 See Other");
        emit("connected", &render());
        connect(e, "inst-lost-identity", "github|1", "ws-1", now + 25);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 26) as u64).unwrap();
        assert_eq!(p["rows"].as_array().unwrap().len(), 2, "equal host labels keep distinct identities");
        emit("identity", &render());
        // Standalone products do not require AgentQ or Langfuse and keep their own scope.
        for product in ["serverq", "taskq"] {
            let schema = format!("{product}-snapshot/1");
            let (_, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": product, "installation": product, "runtime": "wsl", "host_label": "same-long-host-label", "schemas": [schema], "scope": {"name": "board/1", "value": "example/project"}}), now + 30);
            decide(e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", now + 31);
        }
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 32) as u64).unwrap();
        for r in p["rows"].as_array().unwrap().iter().filter(|r| r["product"] != "agentq") {
            assert_eq!(r["export"]["state"], "not_applicable");
            assert_eq!(r["scope"]["value"], "example/project");
        }
        emit("list", &render());
        let mut quiet = app.doc(1800, json!({}), Some(Scope::Subject("github|1")));
        quiet["generated_at"] = json!(now + 400);
        quiet["installations"] = projection(e, Some(Scope::Subject("github|1")), None, (now + 400) as u64).unwrap();
        let quiet = crate::page::render(&quiet, &json!({}));
        assert!(quiet.contains("No recent receipt") && quiet.contains("Host activity unknown"));
        emit("no-contact", &quiet);
        let store = &e.as_ref().unwrap().store;
        store.accept(&sha256(&next), &[Event { id: "export-gap", schema: "agentq-snapshot/1", observed: now, kind: Kind::Export, payload: "{}" }], now).unwrap();
        store.prune(now + 7 * 86400 + 1).unwrap();
        let gap = render();
        assert!(gap.contains("gap · events expired before export: 1") && gap.contains("✓ Connected"));
        emit("export-gap", &gap);
        // Separately approved operations from D16 stay separate from the observation scope.
        let (_, d) = machine(e, "/machine/connect/device", None, json!({"protocol": 1, "product": "agentq", "installation": "inst-scoped", "runtime": "native", "host_label": "scoped-host", "schemas": ["agentq-snapshot/1"], "capabilities": ["agentq.configure"]}), now + 40);
        let code = d["user_code"].as_str().unwrap();
        emit("approval-capability", &page(e, "github|1", code, now + 41).1);
        decide(e, "github|1", code, "approve&capability.agentq.configure=grant", "ws-1", now + 41);
        let p = projection(e, Some(Scope::Subject("github|1")), None, (now + 42) as u64).unwrap();
        let r = p["rows"].as_array().unwrap().iter().find(|r| r["installation"] == "inst-scoped").unwrap();
        assert_eq!(r["capabilities"], json!(["agentq.configure"]));
        assert!(render().contains("Separately approved operations</dt><dd>agentq.configure"));
    }

    /// L3/E7 owner boundary: the real coreq client retains a revoke-only request offline. DashQ
    /// cannot know that local state; acknowledgement, restart and new approval preserve one identity.
    #[test]
    fn connections_offline_revoke_host_and_server_chain() {
        struct Offline;
        impl coreq::enroll::Wire for Offline {
            fn post(&mut self, _: &str, route: &str, _: Option<&str>, _: &Value) -> Result<(u16, Value), String> {
                assert_eq!(route, "/connection/revoke");
                Err("offline".into())
            }
            fn now(&self) -> u64 { T0 as u64 }
            fn sleep(&mut self, _: u64) {}
            fn show(&mut self, _: &str, _: &str) { panic!("no new approval") }
        }
        let (d, e) = setup("offline-life");
        let host = dir("offline-life-host");
        let req = coreq::enroll::Request { installation: None, product: "agentq", host_label: "same-host", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &[] };
        let mut w = Wire { e: &e, now: T0, code: None, calls: vec![], tick: false };
        let grant = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(coreq::enroll::disconnect(&host, &mut Offline), Err("1 revoke pending".into()));
        assert!(coreq::enroll::active(&host).unwrap().is_none());
        assert_eq!(coreq::enroll::stamp(&host, "agentq-snapshot/1", "off", T0 as u64, json!({})), Err("disconnected".into()));
        assert_eq!(projection(&e, Some(Scope::All), None, T0 as u64).unwrap()["rows"][0]["connection"], "active");
        coreq::enroll::retry_revokes(&host, &mut w).unwrap();
        coreq::enroll::retry_revokes(&host, &mut w).unwrap();
        assert_eq!(w.calls.iter().filter(|s| *s == "/machine/connection/revoke").count(), 1);
        drop(e);
        let e = Enroll::open(&d, HOST);
        assert_eq!(projection(&e, Some(Scope::All), None, T0 as u64).unwrap()["rows"][0]["connection"], "revoked");
        let mut w = Wire { e: &e, now: T0 + 60, code: None, calls: vec![], tick: false };
        let new = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(new["installation"], grant["installation"]);
        assert_eq!(new["generation"], 2);
        assert_eq!(e.as_ref().unwrap().project(Scope::All).unwrap().len(), 1);
    }

    /// An AgentQ host paired through the coreq client with `agentq.configure` ticked and one profile snapshot
    /// (profiles 0123456789abcdef, 1111111111111111, 3333333333333333); returns its client folder.
    fn configurable(e: &Result<Enroll, String>, inst: &str, now: i64) -> PathBuf {
        let host = dir(&format!("{inst}-host"));
        let req = coreq::enroll::Request { installation: Some(inst), product: "agentq", host_label: "mac-1", schemas: &["agentq-snapshot/1"], scope: None, capabilities: &["agentq.configure"] };
        let mut w = Wire { e, now, code: None, calls: vec![], tick: true };
        let a = coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        assert_eq!(a["capabilities"], json!(["agentq.configure"]));
        let snap: Value = serde_json::from_str(include_str!("../../fixture/agentq/configuration.json")).unwrap();
        assert_eq!(states(&ingest(e, a["credential"].as_str().unwrap(), vec![rec("s1", now, snap["cases"]["profile_projection"].clone())], now).1), ["accepted"]);
        host
    }

    fn op_id(n: u8) -> String {
        format!("{n:032x}")
    }

    const ROUTING: Change = Change { profile: "0123456789abcdef", field: "routing", value: "direct" };
    const PLAN: Plan = Plan { revision: "00112233445566ff", digest: "9d029e2b923a7a5b63e1b3e2c306898c454f27df8026fd6e061b3421b274d937" };

    // chain: D16 A1/A2, a typed configure request is queued only under server-set authority (actor, workspace,
    // profiles), reaches the AgentQ host through the shared coreq client, and its result commits before the receipt
    // and repeats after a lost reply; AgentQ's published envelopes fit the transport
    #[test]
    fn configure_operations_carry_server_authority_to_the_host() {
        let (d, e) = setup("configure");
        let x = e.as_ref().unwrap();
        let host = configurable(&e, "inst-c1", T0);
        let fx: Value = serde_json::from_str(include_str!("../../fixture/agentq/configuration-fixtures.json")).unwrap();
        let c = |profile, field, value| Change { profile, field, value };
        // malformed requests never queue: id, empty, unknown value, duplicate target, bad plan
        let bad_plan = Plan { revision: "r1", digest: "nothex" };
        for (op, plan, changes) in [("XYZ".to_string(), None, vec![ROUTING]), (op_id(1), None, vec![]), (op_id(1), None, vec![c("0123456789abcdef", "routing", "uninstall")]),
            (op_id(1), None, vec![ROUTING, ROUTING]), (op_id(1), Some(bad_plan), vec![ROUTING])] {
            assert_eq!(x.configure("github|1", "inst-c1", &op, plan, &changes, T0), Err("invalid"));
        }
        // authority is the server's: no grant, a non-member, a member who did not approve, a profile not in the snapshot
        x.store.add_member("ws-1", "github|2", "member").unwrap();
        assert_eq!(x.configure("github|1", "inst-none", &op_id(1), None, &[ROUTING], T0), Err("no_grant"));
        assert_eq!(x.configure("github|9", "inst-c1", &op_id(1), None, &[ROUTING], T0), Err("authority"));
        assert_eq!(x.configure("github|2", "inst-c1", &op_id(1), None, &[ROUTING], T0), Err("authority"));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[c("9999999999999999", "routing", "direct")], T0), Err("profile_out_of_scope"));
        // an observation grant carries no configure authority, with or without a snapshot
        let observe = connect(&e, "inst-o1", "github|1", "ws-1", T0);
        assert_eq!(x.configure("github|1", "inst-o1", &op_id(1), None, &[ROUTING], T0), Err("no_profiles"));
        let snap: Value = serde_json::from_str(include_str!("../../fixture/agentq/configuration.json")).unwrap();
        ingest(&e, &observe, vec![rec("o1", T0, snap["cases"]["profile_projection"].clone())], T0);
        assert_eq!(x.configure("github|1", "inst-o1", &op_id(1), None, &[ROUTING], T0), Err("scope"));
        // the plan: AgentQ's request envelope with authority from the grant and snapshot, digest per coreq
        let plan = x.configure("github|1", "inst-c1", &op_id(1), None, &[ROUTING], T0).unwrap();
        let actor = actor_id("github|1");
        assert!(actor.starts_with("actor-") && actor.len() == 30 && actor_id("acct-1") == "acct-1");
        assert_eq!(plan["authority"], json!({"actor": actor, "workspace": "ws-1", "profiles": ["0123456789abcdef", "1111111111111111", "3333333333333333"]}));
        let keys = |v: &Value| v.as_object().unwrap().keys().cloned().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(keys(&plan["payload"]), keys(&fx["requests"]["plan"]));
        assert_eq!((&plan["payload"]["expires_at"], &plan["digest"]), (&json!(T0 + 300), &json!(coreq::enroll::digest(&plan["payload"]))));
        // immutable identity: a retry returns the stored operation, changed content or another actor is refused
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[ROUTING], T0 + 5), Ok(plan.clone()));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(1), None, &[c("0123456789abcdef", "routing", "routed")], T0 + 5), Err("replay_mismatch"));
        assert_eq!(x.configure("acct-1", "inst-c1", &op_id(1), None, &[ROUTING], T0 + 5), Err("replay_mismatch"));
        // one unresolved mutation per installation; plans are not mutations
        let apply = x.configure("github|1", "inst-c1", &op_id(2), Some(PLAN), &[ROUTING], T0).unwrap();
        assert_eq!(keys(&apply["payload"]), keys(&fx["requests"]["apply"]));
        assert_eq!(x.configure("github|1", "inst-c1", &op_id(3), Some(PLAN), &[ROUTING], T0), Err("busy"));
        x.configure("github|1", "inst-c1", &op_id(4), None, &[ROUTING], T0).unwrap();
        assert_eq!(x.readback("github|1", &op_id(2), T0).unwrap().unwrap()["state"], "queued");
        // the host pulls through the coreq client; a lost pull reply pulls the same operations again
        let mut w = Wire { e: &e, now: T0 + 10, code: None, calls: vec![], tick: false };
        let mut b = coreq::enroll::Backoff::default();
        let got = coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap();
        assert_eq!((got.operations.iter().map(|o| o["operation"].clone()).collect::<Vec<_>>(), got.rejected.len()), (vec![json!(op_id(1)), json!(op_id(2)), json!(op_id(4))], 0));
        assert_eq!(coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap(), got);
        let op = &got.operations[1];
        // payload claims never add authority: only the authority's actor, workspace and profiles pass
        assert_eq!(coreq::enroll::admit(op, Some(&actor), Some("ws-1"), &["0123456789abcdef"]), Ok(()));
        assert_eq!(coreq::enroll::admit(op, Some("acct-1"), None, &["0123456789abcdef"]), Err("wrong_actor"));
        assert_eq!(coreq::enroll::admit(op, None, Some("ws-2"), &["0123456789abcdef"]), Err("wrong_workspace"));
        assert_eq!(coreq::enroll::admit(op, None, None, &["9999999999999999"]), Err("profile_out_of_scope"));
        assert_eq!(x.readback("github|1", &op_id(2), T0 + 10).unwrap().unwrap()["state"], "delivered");
        // every published AgentQ plan and result envelope is storable metadata
        for r in fx["results"].as_array().unwrap().iter().filter(|r| r["valid"] == true).map(|r| &r["result"]).chain(fx["cases"].as_array().unwrap().iter().filter(|c| c["expect"]["kind"] == "plan").map(|c| &c["expect"])) {
            assert!(result_metadata(&json!({"result": r})), "{r}");
        }
        // the result commits before its receipt; a lost receipt repeats it; a changed result is refused
        let result = json!({"result": fx["results"][0]["result"]});
        let receipt = json!({"protocol": 1, "operation": op_id(2), "state": "accepted"});
        assert_eq!(coreq::enroll::post_result(&host, op, result.clone(), &mut b, &mut w).unwrap(), receipt);
        assert_eq!(coreq::enroll::post_result(&host, op, result.clone(), &mut b, &mut w).unwrap(), receipt);
        let changed = coreq::enroll::post_result(&host, op, json!({"result": fx["results"][1]["result"]}), &mut b, &mut w).unwrap();
        assert_eq!(changed, json!({"protocol": 1, "operation": op_id(2), "state": "rejected", "reason": "replay_mismatch"}));
        // readback: the actor and the workspace administrator; another member reads nothing
        let back = x.readback("github|1", &op_id(2), T0 + 11).unwrap().unwrap();
        assert_eq!((&back["state"], &back["result"], &back["authority"]), (&json!("resulted"), &result, &apply["authority"]));
        assert_eq!(x.readback("github|2", &op_id(2), T0 + 11).unwrap(), None);
        // resolved: the next mutation queues; the resulted one is no longer pending
        x.configure("github|1", "inst-c1", &op_id(5), Some(PLAN), &[ROUTING], T0 + 11).unwrap();
        let ids = |p: &coreq::enroll::Pulled| p.operations.iter().map(|o| o["operation"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        assert_eq!(ids(&coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap()), [op_id(1), op_id(4), op_id(5)]);
        // a restart keeps operations and results
        drop(w);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let mut w = Wire { e: &e, now: T0 + 12, code: None, calls: vec![], tick: false };
        assert_eq!(ids(&coreq::enroll::pull(&host, "agentq.configure", &mut b, &mut w).unwrap()), [op_id(1), op_id(4), op_id(5)]);
        assert_eq!(e.as_ref().unwrap().readback("github|1", &op_id(2), T0 + 12).unwrap().unwrap()["result"], result);
    }

    // chain: D16 A3, expiry, revocation and results commit in either order with the actual outcome: an undelivered
    // operation never runs or blocks after expiry or revoke, a delivered one stays unresolved (across restart) until
    // its result or a revoke, and a committed result is never undone by a later revoke
    #[test]
    fn operation_expiry_revocation_and_restart_orderings() {
        let (d, e) = setup("op-races");
        let host = configurable(&e, "inst-r1", T0);
        let cred = coreq::enroll::active(&host).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        let pull = |e: &Result<Enroll, String>, cred: &str, now| machine(e, "/machine/operations", Some(cred), json!({"protocol": 1, "capability": "agentq.configure"}), now);
        let post = |e: &Result<Enroll, String>, cred: &str, op: &Value, now| {
            machine(e, "/machine/operations/result", Some(cred), json!({"protocol": 1, "operation": op["operation"], "digest": op["digest"], "result": {"outcome": "applied"}}), now)
        };
        let state = |e: &Result<Enroll, String>, id: &str, now| e.as_ref().unwrap().readback("github|1", id, now).unwrap().unwrap()["state"].clone();
        let x = e.as_ref().unwrap();
        // expiry before delivery: never delivered, never blocks the next mutation
        x.configure("github|1", "inst-r1", &op_id(1), Some(PLAN), &[ROUTING], T0).unwrap();
        assert_eq!(pull(&e, &cred, T0 + 300), (200, json!({"protocol": 1, "operations": []})));
        assert_eq!(state(&e, &op_id(1), T0 + 300), "expired");
        let op2 = x.configure("github|1", "inst-r1", &op_id(2), Some(PLAN), &[ROUTING], T0 + 300).unwrap();
        // delivery before expiry: unresolved past expiry, also after a restart, until its result arrives
        assert_eq!(pull(&e, &cred, T0 + 301).1["operations"], json!([op2]));
        assert_eq!(x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 700), Err("busy"));
        drop(e);
        let e = Enroll::open(&d, HOST);
        let x = e.as_ref().unwrap();
        assert_eq!((state(&e, &op_id(2), T0 + 700), x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 700)), (json!("delivered"), Err("busy")));
        assert_eq!(post(&e, &cred, &op2, T0 + 701).1["state"], "accepted", "a late result of a delivered operation is its actual outcome");
        let op3 = x.configure("github|1", "inst-r1", &op_id(3), Some(PLAN), &[ROUTING], T0 + 701).unwrap();
        // result then revoke: the committed result stays; the revoked credential can neither pull nor post
        assert_eq!(pull(&e, &cred, T0 + 702).1["operations"], json!([op3]));
        assert_eq!(post(&e, &cred, &op3, T0 + 703).1["state"], "accepted");
        assert_eq!(machine(&e, "/machine/connection/revoke", Some(&cred), json!({"protocol": 1}), T0 + 704).0, 200);
        assert_eq!(state(&e, &op_id(3), T0 + 705), "resulted");
        assert_eq!(pull(&e, &cred, T0 + 705), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!(post(&e, &cred, &op3, T0 + 705).0, 401);
        assert_eq!(x.configure("github|1", "inst-r1", &op_id(4), None, &[ROUTING], T0 + 705), Err("no_grant"));
        // revoke then result: the delivered operation reads unknown, the undelivered one revoked; neither is undone
        let host2 = configurable(&e, "inst-r2", T0 + 800);
        let cred2 = coreq::enroll::active(&host2).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        let op5 = x.configure("github|1", "inst-r2", &op_id(5), Some(PLAN), &[ROUTING], T0 + 800).unwrap();
        assert_eq!(pull(&e, &cred2, T0 + 801).1["operations"], json!([op5]));
        x.configure("github|1", "inst-r2", &op_id(6), None, &[ROUTING], T0 + 802).unwrap();
        assert!(x.store.revoke("inst-r2", T0 + 803).unwrap());
        assert_eq!(post(&e, &cred2, &op5, T0 + 804), (401, json!({"protocol": 1, "error": "revoked"})));
        assert_eq!((state(&e, &op_id(5), T0 + 804), state(&e, &op_id(6), T0 + 804)), (json!("unknown"), json!("revoked")));
        // reconnecting the installation: the revoke resolved the delivered apply (#147), so a new apply is accepted,
        // also after restart
        configurable(&e, "inst-r2", T0 + 805);
        drop(e);
        let e = Enroll::open(&d, HOST);
        let x = e.as_ref().unwrap();
        assert!(x.configure("github|1", "inst-r2", &op_id(7), Some(PLAN), &[ROUTING], T0 + 807).is_ok());
        // an undelivered apply of a revoked generation never runs, so it does not block the reconnected installation
        configurable(&e, "inst-r4", T0 + 810);
        x.configure("github|1", "inst-r4", &op_id(8), Some(PLAN), &[ROUTING], T0 + 810).unwrap();
        assert!(x.store.revoke("inst-r4", T0 + 811).unwrap());
        configurable(&e, "inst-r4", T0 + 812);
        assert!(x.configure("github|1", "inst-r4", &op_id(9), Some(PLAN), &[ROUTING], T0 + 813).is_ok());
        // another installation's credential never reaches these operations
        let host3 = configurable(&e, "inst-r3", T0 + 900);
        let cred3 = coreq::enroll::active(&host3).unwrap().unwrap()["credential"].as_str().unwrap().to_string();
        assert_eq!(post(&e, &cred3, &op5, T0 + 901).1["reason"], "unknown_operation");
        assert_eq!(pull(&e, &cred3, T0 + 901).1["operations"], json!([]));
        assert_eq!(machine(&e, "/machine/operations/result", Some(&cred3), json!({"protocol": 1, "operation": op_id(5), "digest": op5["digest"], "result": {"path": "/etc"}}), T0 + 902).0, 400);
    }

    /// #159 A1/A2 chain: TaskQ's `enrollment_record` fixture becomes the per-project editor groups of an active
    /// connection, with the current and effective revisions the write is reviewed against and applied by; an older
    /// record never replaces the view, and a revoked connection shows no editor. Risk: an editor for a foreign or
    /// revoked target, or a stale record winning. Writes `DASHQ_LIMITS_EVIDENCE` for tests/limits-browser.cjs.
    #[test]
    fn taskq_resources_record_becomes_project_limit_groups() {
        let f: Value = serde_json::from_str(include_str!("../../fixture/taskq-resources/resource-policy-v1.json")).unwrap();
        let inst = f["enrollment_record"]["payload"]["targets"][0]["target"]["installation_id"].as_str().unwrap().to_string();
        let (_d, e) = setup("limit-groups");
        let board = json!({"name": "taskq-board/1", "value": "github:github.com/taskqorg/dashq"});
        let (_, d) = machine(&e, "/machine/connect/device", None, json!({"protocol": 1, "product": "taskq", "installation": inst, "runtime": "native", "host_label": "mac-1", "schemas": ["taskq-board/1", RESOURCES], "scope": board}), T0);
        assert!(decide(&e, "github|1", d["user_code"].as_str().unwrap(), "approve", "ws-1", T0).1.contains("Approved"));
        let cred = machine(&e, "/machine/connect/token", None, json!({"protocol": 1, "device_code": d["device_code"]}), T0).1["credential"].as_str().unwrap().to_string();
        assert_eq!(machine(&e, "/machine/connect/ack", Some(&cred), json!({"protocol": 1}), T0).0, 200);
        let record = |id: &str, observed: i64, payload: Value| json!({"schema": RESOURCES, "event_id": id, "observed_at": observed, "payload": payload});
        let mut payload = f["enrollment_record"]["payload"].clone();
        payload["observed_at"] = json!(T0 - 5);
        let mut old = payload.clone();
        old["targets"][0]["current_revision"] = json!("0".repeat(64));
        assert_eq!(states(&ingest(&e, &cred, vec![record("r-1", T0, payload.clone()), record("r-0", T0 - 60, old)], T0).1), vec!["accepted", "accepted"]);
        let doc = projection(&e, Some(Scope::All), None, T0 as u64).unwrap();
        let l = &doc["rows"][0]["project_limits"];
        assert_eq!(l["targets"][0]["current_revision"], "1".repeat(64), "an older record never replaces the view");
        assert_eq!(l["groups"], json!([{"key": format!("{inst}:ws-1:project-1"), "observed_at": T0 - 5, "labels": {"host": "mac-1", "project": "project-1"}, "targets": [payload["targets"][0]]}]));
        let html = crate::page::render(&json!({"generated_at": T0, "installations": doc, "resources": {"rows": []}}), &json!({}));
        assert!(html.contains(" · unknown · revision <code title=\"1111111111111111111111111111111111111111111111111111111111111111\">111111111111</code> · effective revision unknown</p>"), "{html}");
        assert!(html.contains("class=\"node-policy\" data-taskq-groups=\"[{&quot;key&quot;:&quot;"), "{html}");
        if let Ok(out) = std::env::var("DASHQ_LIMITS_EVIDENCE") {
            std::fs::write(out, &html).unwrap();
        }
        // revoked: the readback stays as history, the editor goes
        e.as_ref().unwrap().store().revoke(&inst, T0 + 1).unwrap();
        let doc = projection(&e, Some(Scope::All), None, T0 as u64 + 2).unwrap();
        assert_eq!(doc["rows"][0]["project_limits"]["groups"], json!([]));
        let html = crate::page::render(&json!({"generated_at": T0 + 2, "installations": doc, "resources": {"rows": []}}), &json!({}));
        assert!(!html.contains("node-policy"));
    }

/// #262 A1-A3 / R2,D6,D26: a bad stored block must not hide valid rows or claim healthy coverage, even after restart.
#[test]
    fn fleet_invalid_stored_block_keeps_records_after_restart() {
        let f = crate::fleet::tests::fixture();
        let now = f["now"].as_u64().unwrap();
        let (d, mut e) = setup("fleet-invalid");
        let host = dir("fleet-invalid-host");
        let mut w = Wire { e: &e, now: now as i64, code: None, calls: vec![], tick: false };
        let req = coreq::enroll::Request { installation: None, product: "taskq", host_label: "pds18.ufobe.com", schemas: &[TELEMETRY], scope: None, capabilities: &[] };
        coreq::enroll::pair(&host, "https://dash.example.com/machine", &req, &mut w).unwrap();
        for (i, index) in [0, 2, 3].into_iter().enumerate() {
            let mut line = f["fleet_records"][0]["records"][index].clone();
            line["seq"] = json!(i + 1);
            line["received_ms"] = json!(now * 1000);
            let rec = coreq::enroll::stamp(&host, TELEMETRY, &format!("fleet-{i}"), now, block(i as u64 + 1, &[line], None)).unwrap();
            assert_eq!(coreq::enroll::deliver(&host, &[rec], &mut Default::default(), &mut w).unwrap()[0]["state"], "accepted");
        }
        drop(w);
        let db = rusqlite::Connection::open(d.join("dashq.db")).unwrap();
        let id: i64 = db.query_row("SELECT id FROM telemetry WHERE event_id = 'fleet-1'", [], |r| r.get(0)).unwrap();
        db.execute("UPDATE telemetry SET payload = '{' WHERE id = ?1", [id]).unwrap();
        drop(db);
        for restarted in [false, true] {
            if restarted {
                drop(e);
                e = Enroll::open(&d, HOST);
            }
            let p = projection(&e, Some(Scope::Subject("github|1")), None, now).unwrap();
            let streams = fleet_records(&e, &p["rows"], (now - 7 * 86400) as i64, false);
            assert_eq!(streams[0]["records"].as_array().map(Vec::len), Some(2), "valid blocks on both sides survive; restarted={restarted}: {streams}");
            assert_eq!(streams[0]["known_loss"], json!([{"block_id": id, "reason": format!("stored block {id} invalid")}]));
            assert!(streams[0]["error"].is_null(), "a decode loss is not a failed store read");
            let mut inp = f.clone();
            inp["fleet_records"] = streams;
            let doc = crate::data::build(&inp);
            for win in doc["fleet"]["hosts"][0]["streams"].as_array().unwrap() {
                assert_eq!(win["source"]["state"], "unknown");
                assert_eq!(win["source"]["reason"], format!("stored block {id} invalid"));
                assert_eq!(win["known_loss"][0]["block_id"], id);
                assert_eq!(win["projects"][0]["tokens"]["input_tokens"]["observed"], 100);
                assert_eq!(win["completed_observed"], 1);
                assert!(win["workers_started_observed"].is_null(), "the invalid block never becomes a zero count");
            }
            let html = crate::page::render_view(&doc, &json!({}), "/fleet");
            assert!(html.contains(&format!("stored block {id} invalid")) && html.contains("invented-account"));
            assert_eq!(fleet_records(&e, &p["rows"], (now - 7 * 86400) as i64, false), inp["fleet_records"], "cached reads retain known loss");
        }
    }

    /// while a source read hangs, the source reads unknown, and a first read in progress is not "all sources failed".
    #[test]
    fn browser_routes_do_not_wait_for_a_hung_source_read() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        // accepts the connection and never answers: curl waits out its 10 s timeout
        let hung = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("https://{}", hung.local_addr().unwrap());
        let (returned, result) = channel();
        std::thread::spawn(move || {
            let app = app(Err("no enrollment".into()), Some((base, "invented".into())));
            let page = crate::route(&app, "GET", "/", "", None);
            let data = crate::route(&app, "GET", "/data", "", None);
            returned.send((page.0, data.0, String::from_utf8(data.2).unwrap())).unwrap();
        });
        let (page, status, body) = result.recv_timeout(Duration::from_secs(5)).expect("the source read held the routes");
        assert_eq!((page, status), ("200 OK", "200 OK"), "{body}");
        let doc: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(doc["sources"]["langfuse"]["state"], "unknown", "{}", doc["sources"]);
    }

    /// #260 A2 / D4,D6,D11: every HTML view and /data retains aged Fleet data during a blocked read, within actor scope.
    #[test]
    fn fleet_pending_read_keeps_routes_and_scope_available() {
        use std::sync::mpsc::channel;
        use std::time::Duration;
        let (_d, e) = setup("fleet-pending");
        // Cross a second boundary without sleeping: route reads must retain the advancing producer age.
        let now = crate::source::now() - 1;
        let installation = "invented-fleet";
        let enrollment = e.as_ref().unwrap();
        enrollment.store.grant(&store::Installation { id: installation, product: "taskq", runtime: "native", label: "invented-host" }, "ws-1", "github|1", r#"["taskq-telemetry/1"]"#, b"invented-verifier", now as i64).unwrap();
        let key = format!("fleet:{}", json!([installation, "ws-1"]));
        let mut records = crate::fleet::tests::fixture()["fleet_records"][0]["records"].clone();
        for record in records.as_array_mut().unwrap() { record["observation"]["started_ms"] = json!((now-120)*1000); }
        enrollment.fleet_cache.get(&key, 0, || Ok((json!({"records": records, "telemetry": {"last_synced": now-120}}), false)));
        let (started, running) = channel();
        let (release, wait) = channel();
        enrollment.fleet_cache.refresh(&key, 30, move || {
            started.send(()).unwrap();
            wait.recv().unwrap();
            Err(("store unavailable".into(), None))
        });
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let app = app(e, None);
        let (returned, result) = channel();
        std::thread::scope(|threads| {
            let app = &app;
            threads.spawn(move || {
                let pages: Vec<_> = crate::page::VIEWS.iter().map(|(path, _)| crate::route(app, "GET", path, "", Some(Scope::Subject("github|1")))).collect();
                let data = data(app, "", Some(Scope::Subject("github|1")));
                returned.send((pages, data)).unwrap();
            });
            let response = result.recv_timeout(Duration::from_secs(5));
            release.send(()).unwrap();
            let (pages, (status, doc)) = response.expect("Fleet held browser routes behind its read");
            assert!(pages.iter().all(|p| p.0 == "200 OK"));
            assert_eq!(status, "200 OK");
            let window = doc["fleet"]["hosts"].as_array().unwrap().iter().flat_map(|h| h["streams"].as_array().unwrap()).next().expect("cached Fleet stream missing");
            let source = &window["source"];
            assert_eq!(source["state"], "unconfirmed");
            assert_eq!(source["reason"], "refresh in progress");
            assert_eq!(source["at"], now-120);
            assert!(source["age_s"].as_u64().unwrap() >= 120);
            assert_eq!(window["accounts"][0]["tokens"]["input_tokens"]["observed"], 100);
            let fleet = String::from_utf8(pages.iter().find(|p| String::from_utf8_lossy(&p.2).contains("data-view=\"/fleet\"")).unwrap().2.clone()).unwrap();
            let age = fleet.split_once("source age ").unwrap().1.split_once(" s").unwrap().0.parse::<u64>().unwrap();
            assert!((120..=120 + crate::source::now().saturating_sub(now)).contains(&age), "unexpected producer age: {age}");
            assert!(fleet.contains("refresh in progress"));
        });
        let (_, denied) = data(&app, "", Some(Scope::Subject("github|other")));
        let (_, anonymous) = data(&app, "", None);
        app.enroll.as_ref().unwrap().store.revoke(installation, now as i64).unwrap();
        let (_, revoked) = data(&app, "", Some(Scope::Subject("github|1")));
        for doc in [denied, anonymous, revoked] {
            assert!(doc["fleet"]["hosts"].as_array().into_iter().flatten().all(|h| h["streams"].as_array().is_none_or(|s| s.is_empty())));
            assert!(!doc["fleet"].to_string().contains("invented-account"));
        }
    }
