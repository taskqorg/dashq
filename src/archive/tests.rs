//! D31 boundaries: grant isolation, durable replay, audited readers, retention, deletion, recovery and inert output.
use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
const NOW: i64 = 1_800_000_000;
const OWNER: &str = "github|invented-owner";
const CRED: &str = "invented-archive-credential";
struct Fixture {
    dir: PathBuf,
    e: Result<Enroll, String>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "dashq-archive-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        Enroll::admin(&dir, "ws", OWNER).unwrap();
        let e = Enroll::open(&dir, "dash.example.test");
        let f = Self { dir, e };
        f.grant("inst", "ws", CRED, WIRE);
        f
    }
    fn e(&self) -> &Enroll {
        self.e.as_ref().unwrap()
    }
    fn root(&self) -> PathBuf {
        self.dir.join("objects")
    }
    fn objects(&self) -> objects::Objects {
        objects::Objects::open(&self.root(), self.e().archive_key()).unwrap()
    }
    fn grant(&self, inst: &str, ws: &str, cred: &str, schema: &str) {
        self.e()
            .store()
            .grant(
                &store::Installation {
                    id: inst,
                    product: "agentq",
                    runtime: "native",
                    label: "invented-host",
                },
                ws,
                OWNER,
                &json!([schema]).to_string(),
                &enroll::sha256(cred),
                NOW,
            )
            .unwrap();
        self.e().store().ack(&enroll::sha256(cred), NOW).unwrap();
    }
    fn ingest(&self, b: &Value, now: i64) -> Res<Value> {
        ingest(self.e(), &self.objects(), &enroll::sha256(CRED), b, now)
    }
    fn request(
        &self,
        method: &str,
        path: &str,
        query: &str,
        subject: Option<&str>,
        body: &[u8],
        extra: &[(&str, &str)],
    ) -> Resp {
        let mut headers: Vec<_> = extra
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        if let Some(s) = subject {
            headers.push((enroll::SUBJECT.into(), s.into()));
        }
        route(
            &self.e,
            Some(&self.root()),
            &Req {
                method,
                path,
                query,
                headers: &headers,
                body,
            },
            NOW,
        )
        .unwrap()
    }
    fn audit(&self) -> i64 {
        self.e()
            .store()
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM archive.archive_audit", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
}
fn body(session: &str, source: &str, bytes: &[u8]) -> Value {
    let mut b = json!({"protocol":1,"schema":WIRE,"manifest":{"project":"example/project","task":283,"attempt":"attempt-1","actor":"manager","entry":"taskq","runtime":"codex","cli_version":"invented-version","model":"invented-model","effort":"high","policy":{"write":"project"},"account_label":"invented-account","host":"invented-host","session":session,"parent_session":null,"instructions_hash":null},"capture":"launched","source":source,"format":"codex-rollout","start":0,"end":bytes.len(),"sha256":hash_bytes(bytes),"bytes":crate::base64(bytes),"scrubbed":0,"observed_at":NOW});
    rehash(&mut b);
    b
}
fn rehash(b: &mut Value) {
    b["manifest_sha256"] = json!(coreq::enroll::digest(&b["manifest"]));
}
fn record(session: &str) -> Vec<u8> {
    format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session}\"}}}}\n{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"type\":\"output_text\",\"text\":\"invented-private-canary <script>alert(1)</script>\"}}]}}}}\n").into_bytes()
}
fn form(e: &Enroll, mut pairs: HashMap<String, String>, subject: &str) -> Vec<u8> {
    pairs.insert("csrf".into(), e.token_for(&format!("archive\n{subject}")));
    pairs
        .into_iter()
        .map(|(k, v)| format!("{}={}", crate::source::enc(&k), crate::source::enc(&v)))
        .collect::<Vec<_>>()
        .join("&")
        .into_bytes()
}

#[test]
fn ingest_replay_ranges_integrity_and_content_stays_out_of_sqlite() {
    let f = Fixture::new("ranges");
    let raw = record("session-1");
    let mut b = body("session-1", "source-1", &raw[..19]);
    let id = f.ingest(&b, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(f.ingest(&b, NOW).unwrap()["end"], 19);
    assert_eq!(
        std::fs::read_dir(f.root())
            .unwrap()
            .filter(|e| digest(&e.as_ref().unwrap().file_name().to_string_lossy()))
            .count(),
        1,
        "replay stores no second object"
    );
    let mut changed = b.clone();
    changed["manifest"]["model"] = json!("another");
    rehash(&mut changed);
    assert_eq!(f.ingest(&changed, NOW), Err(Failure("replay_mismatch")));
    changed = b.clone();
    changed["bytes"] = json!(crate::base64(b"wrong"));
    assert_eq!(f.ingest(&changed, NOW), Err(Failure("invalid_request")));
    let tail = &raw[19..];
    b["bytes"] = json!(crate::base64(tail));
    b["sha256"] = json!(hash_bytes(tail));
    b["start"] = json!(20);
    b["end"] = json!(raw.len() + 1);
    assert_eq!(f.ingest(&b, NOW), Err(Failure("range_gap")));
    b["start"] = json!(19);
    b["end"] = json!(raw.len());
    f.ingest(&b, NOW).unwrap();
    let (s, src, read) =
        read_source(f.e(), &f.objects(), &id, "source-1", OWNER, "test", NOW).unwrap();
    assert_eq!(read, raw);
    let (mapped, pending) = steps(&f.objects(), &s, &src, &read).unwrap();
    assert_eq!(pending, 0);
    assert!(
        mapped
            .iter()
            .any(|s| s.kind == agentq_runtime::agent::history::StepKind::Message)
    );
    assert_eq!(f.audit(), 1);
    {
        let c = f.e().store().lock().unwrap();
        assert_eq!(c.query_row("SELECT COUNT(*) FROM main.sqlite_master WHERE type='table' AND name='archive_sessions'",[],|r|r.get::<_,i64>(0)).unwrap(),0,"telemetry database holds no archive index");
        assert_eq!(
            c.query_row("SELECT outcome FROM archive.archive_audit", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "read"
        );
    }
    for file in std::fs::read_dir(&f.dir)
        .unwrap()
        .chain(std::fs::read_dir(f.root()).unwrap())
    {
        let path = file.unwrap().path();
        if path.is_file() {
            assert!(
                !std::fs::read(path)
                    .unwrap()
                    .windows(b"invented-private-canary".len())
                    .any(|w| w == b"invented-private-canary")
            );
        }
    }
    let c = f.e().store().lock().unwrap();
    for table in [
        "receipts",
        "snapshots",
        "history",
        "outbox",
        "telemetry",
        "observations",
    ] {
        assert_eq!(
            c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    let hash = hash_bytes(&raw[..19]);
    let mut damaged = std::fs::read(f.root().join(&hash)).unwrap();
    damaged[15] ^= 1;
    std::fs::write(f.root().join(&hash), damaged).unwrap();
    drop(c);
    assert_eq!(
        read_source(f.e(), &f.objects(), &id, "source-1", OWNER, "test", NOW).unwrap_err(),
        Failure("unavailable")
    );
}

#[test]
fn enrollment_authority_manual_opt_in_revocation_and_browser_roles() {
    let f = Fixture::new("authority");
    let b = body("s", "source", &record("s"));
    let wire = b.to_string();
    let r = f.request(
        "POST",
        "/machine/archive/1",
        "",
        None,
        wire.as_bytes(),
        &[("authorization", &format!("Bearer {CRED}"))],
    );
    assert_eq!(r.status, "200 OK");
    let id: Value = serde_json::from_slice(&r.body).unwrap();
    let id = id["id"].as_str().unwrap();
    assert_eq!(
        f.request(
            "GET",
            "/archive/raw",
            &format!("id={id}&source=source"),
            None,
            b"",
            &[]
        )
        .status,
        "403 Forbidden",
        "loopback never grants content access"
    );
    f.e().store().add_member("ws", "member", "admin").unwrap();
    assert_eq!(
        f.request(
            "GET",
            "/archive/raw",
            &format!("id={id}&source=source"),
            Some("member"),
            b"",
            &[]
        )
        .status,
        "404 Not Found",
        "generic admin is not the archive owner"
    );
    role(f.e(), "ws", "member", false, true).unwrap();
    let raw = f.request(
        "GET",
        "/archive/raw",
        &format!("id={id}&source=source"),
        Some("member"),
        b"",
        &[],
    );
    assert_eq!(raw.status, "200 OK");
    assert!(raw.headers.contains("attachment"));
    assert_eq!(f.audit(), 1);
    role(f.e(), "ws", "member", false, false).unwrap();
    assert_eq!(
        f.request(
            "GET",
            "/archive/raw",
            &format!("id={id}&source=source"),
            Some("member"),
            b"",
            &[]
        )
        .status,
        "404 Not Found"
    );
    let device=json!({"protocol":1,"product":"agentq","installation":"manual-request","runtime":"native","host_label":"invented-host","schemas":[WIRE],"capabilities":["agent-history.manual"]}).to_string();
    let started = enroll::route(
        &f.e,
        &Req {
            method: "POST",
            path: "/machine/connect/device",
            query: "",
            headers: &[],
            body: device.as_bytes(),
        },
        NOW,
    )
    .unwrap();
    assert_eq!(started.status, "200 OK");
    let pair: Value = serde_json::from_slice(&started.body).unwrap();
    let subject_headers = vec![(enroll::SUBJECT.into(), OWNER.into())];
    let code = format!(
        "code={}",
        crate::source::enc(pair["user_code"].as_str().unwrap())
    );
    let approval = enroll::route(
        &f.e,
        &Req {
            method: "GET",
            path: "/connect",
            query: &code,
            headers: &subject_headers,
            body: b"",
        },
        NOW,
    )
    .unwrap();
    let approval = String::from_utf8(approval.body).unwrap();
    assert!(
        approval.contains("Archive content:")
            && approval.contains("archive content and selected metadata")
    );
    assert!(
        approval.contains(
            "<input type=\"checkbox\" name=\"capability.agent-history.manual\" value=\"grant\">"
        ),
        "manual capture must remain a separate unticked approval"
    );
    f.grant("observation", "ws", "observer", crate::observations::SCHEMA);
    assert_eq!(
        f.request(
            "POST",
            "/machine/archive/1",
            "",
            None,
            wire.as_bytes(),
            &[("authorization", "Bearer observer")]
        )
        .status,
        "403 Forbidden"
    );
    let mut manual = b.clone();
    manual["capture"] = json!("manual");
    assert_eq!(f.ingest(&manual, NOW), Err(Failure("manual_disabled")));
    f.e().store().write(|tx|Ok(tx.execute("UPDATE grants SET capabilities='[\"agent-history.manual\"]' WHERE installation='inst'",[]).map(|_|())?)).unwrap();
    assert!(f.ingest(&manual, NOW).is_ok());
    f.e()
        .store()
        .revoke_credential(&enroll::sha256(CRED), NOW)
        .unwrap();
    assert_eq!(
        f.request(
            "POST",
            "/machine/archive/1",
            "",
            None,
            wire.as_bytes(),
            &[("authorization", &format!("Bearer {CRED}"))]
        )
        .status,
        "401 Unauthorized"
    );
    // A reader keeps historical content after the producer disconnects, but another workspace never receives it.
    role(f.e(), "other", "foreign", true, true).unwrap();
    assert_eq!(
        f.request(
            "GET",
            "/archive/raw",
            &format!("id={id}&source=source"),
            Some("foreign"),
            b"",
            &[]
        )
        .status,
        "404 Not Found"
    );
    assert_eq!(
        f.request(
            "GET",
            "/archive/raw",
            &format!("id={id}&source=source"),
            Some(OWNER),
            b"",
            &[]
        )
        .status,
        "200 OK"
    );
}

#[test]
fn deletion_retention_shared_media_and_replay_tombstones() {
    let f = Fixture::new("delete");
    let raw = record("s");
    let a = body("s", "source", &raw);
    let id = f.ingest(&a, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let mut media = body("s", &hash_bytes(b"invented-media"), b"invented-media");
    media["format"] = json!("media");
    media["media"] = json!(true);
    media["total"] = media["end"].clone();
    f.ingest(&media, NOW).unwrap();
    f.ingest(&media, NOW).unwrap();
    let mut other = media.clone();
    other["manifest"]["session"] = json!("s2");
    rehash(&mut other);
    let id2 = f.ingest(&other, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let cmd = HashMap::from([
        ("id".into(), id.clone()),
        ("confirm".into(), "delete".into()),
    ]);
    mutate(f.e(), &f.objects(), &cmd, OWNER, false).unwrap();
    assert_eq!(f.ingest(&a, NOW), Err(Failure("deleted")));
    assert!(!f.root().join(hash_bytes(&raw)).exists());
    assert!(f.root().join(hash_bytes(b"invented-media")).exists());
    assert_eq!(
        read_source(
            f.e(),
            &f.objects(),
            &id2,
            &hash_bytes(b"invented-media"),
            OWNER,
            "raw",
            NOW
        )
        .unwrap()
        .2,
        b"invented-media"
    );
    assert_eq!(
        read_source(
            f.e(),
            &f.objects(),
            &id2,
            &hash_bytes(b"invented-media"),
            OWNER,
            "raw",
            NOW + RETENTION
        )
        .unwrap_err(),
        Failure("expired")
    );
    prune(f.e(), &f.root(), NOW + RETENTION).unwrap();
    assert_eq!(
        std::fs::read_dir(f.root())
            .unwrap()
            .filter(|e| digest(&e.as_ref().unwrap().file_name().to_string_lossy()))
            .count(),
        0
    );
    let c = f.e().store().lock().unwrap();
    assert_eq!(
        c.query_row("SELECT COUNT(*) FROM archive.archive_sessions", [], |r| r
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        2
    );
    drop(c);
    assert_eq!(f.ingest(&other, NOW + RETENTION), Err(Failure("expired")));
    let project = HashMap::from([
        ("workspace".into(), "ws".into()),
        ("project".into(), "example/project".into()),
        ("confirm".into(), "delete".into()),
    ]);
    mutate(f.e(), &f.objects(), &project, OWNER, false).unwrap();
    assert_eq!(f.ingest(&other, NOW + RETENTION), Err(Failure("deleted")));
}

#[test]
fn viewer_search_full_output_unknown_raw_naming_and_csrf() {
    let f = Fixture::new("viewer");
    let raw = record("s");
    let b = body("s", "source", &raw);
    let id = f.ingest(&b, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let home = f.request("GET", "/archive", "", Some(OWNER), b"", &[]);
    assert_eq!(home.status, "200 OK");
    let h = String::from_utf8(home.body).unwrap();
    assert!(
        h.contains("unknown")
            && h.contains("invented-model")
            && !h.contains("invented-private-canary")
    );
    let detail = f.request(
        "GET",
        "/archive",
        &format!("id={id}"),
        Some(OWNER),
        b"",
        &[],
    );
    assert_eq!(detail.status, "200 OK");
    let detail = String::from_utf8(detail.body).unwrap();
    assert!(detail.contains("Message") && !detail.contains("<script>alert(1)"));
    let (s, src, bytes) =
        read_source(f.e(), &f.objects(), &id, "source", OWNER, "test", NOW).unwrap();
    let step = steps(&f.objects(), &s, &src, &bytes)
        .unwrap()
        .0
        .into_iter()
        .find(|s| s.kind == agentq_runtime::agent::history::StepKind::Message)
        .unwrap();
    let full = f.request(
        "GET",
        "/archive/step",
        &format!("id={id}&source=source&step={}", step.id),
        Some(OWNER),
        b"",
        &[],
    );
    let full = String::from_utf8(full.body).unwrap();
    assert!(
        full.contains("invented-private-canary &lt;script&gt;")
            && !full.contains("<script>alert(1)")
    );
    let search = form(
        f.e(),
        HashMap::from([("q".into(), "invented-private-canary".into())]),
        OWNER,
    );
    assert_eq!(
        f.request("POST", "/archive/search", "", Some(OWNER), &search, &[])
            .status,
        "403 Forbidden"
    );
    let searched = f.request(
        "POST",
        "/archive/search",
        "",
        Some(OWNER),
        &search,
        &[("origin", "https://dash.example.test")],
    );
    let searched = String::from_utf8(searched.body).unwrap();
    assert!(
        searched.contains(&id) && !searched.contains("invented-private-canary"),
        "search text never enters URL or result snippets"
    );
    let quoted=(json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"literal \"quoted\" phrase"}]}}).to_string()+"\n").into_bytes();
    let quoted_id = f
        .ingest(&body("quoted", "quoted-source", &quoted), NOW)
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let escaped_search = form(
        f.e(),
        HashMap::from([("q".into(), "literal \"quoted\" phrase".into())]),
        OWNER,
    );
    let escaped_result = f.request(
        "POST",
        "/archive/search",
        "",
        Some(OWNER),
        &escaped_search,
        &[("origin", "https://dash.example.test")],
    );
    assert_eq!(escaped_result.status, "200 OK");
    assert!(
        String::from_utf8(escaped_result.body)
            .unwrap()
            .contains(&quoted_id),
        "search includes decoded step text, not only JSON-escaped raw bytes"
    );
    let console_record=(json!({"type":"user","sessionId":"console","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"console-call","content":""}]},"toolUseResult":{"stdout":"first line\nsecond line <script>alert(1)</script>","stderr":"invented stderr","exitCode":0}}).to_string()+"\n").into_bytes();
    let mut console_upload = body("console", "console-source", &console_record);
    console_upload["format"] = json!("claude-native");
    console_upload["manifest"]["runtime"] = json!("claude");
    rehash(&mut console_upload);
    let console_id = f.ingest(&console_upload, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (cs, source, bytes) = read_source(
        f.e(),
        &f.objects(),
        &console_id,
        "console-source",
        OWNER,
        "test",
        NOW,
    )
    .unwrap();
    let shell = steps(&f.objects(), &cs, &source, &bytes)
        .unwrap()
        .0
        .into_iter()
        .find(|s| s.kind == agentq_runtime::agent::history::StepKind::Shell)
        .unwrap();
    let console = f.request(
        "GET",
        "/archive/step",
        &format!("id={console_id}&source=console-source&step={}", shell.id),
        Some(OWNER),
        b"",
        &[],
    );
    assert_eq!(console.status, "200 OK");
    let console = String::from_utf8(console.body).unwrap();
    assert!(
        console.contains("first line\nsecond line &lt;script&gt;")
            && console.contains("Standard error")
            && !console.contains("<script>alert(1)"),
        "console text keeps line breaks and remains inert"
    );
    let named = form(
        f.e(),
        HashMap::from([
            ("id".into(), id.clone()),
            ("name".into(), "<img onerror=alert(1)>".into()),
        ]),
        OWNER,
    );
    assert_eq!(
        f.request(
            "POST",
            "/archive/name",
            "",
            Some(OWNER),
            &named,
            &[("origin", "https://dash.example.test")]
        )
        .status,
        "200 OK"
    );
    let name = f.request("GET", "/archive", "", Some(OWNER), b"", &[]);
    let name = String::from_utf8(name.body).unwrap();
    assert!(name.contains("&lt;img onerror=alert(1)&gt;") && !name.contains("<img onerror"));
    let mut unknown = body("unknown", "source", b"invented unsupported log\n");
    unknown["format"] = json!("invented-unknown-format");
    let unknown_id = f.ingest(&unknown, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let unknown = f.request(
        "GET",
        "/archive",
        &format!("id={unknown_id}"),
        Some(OWNER),
        b"",
        &[],
    );
    assert!(
        String::from_utf8(unknown.body)
            .unwrap()
            .contains("Steps unknown")
    );
    let invalid_delete = form(f.e(), HashMap::from([("id".into(), id.clone())]), OWNER);
    assert_eq!(
        f.request(
            "POST",
            "/archive/delete",
            "",
            Some(OWNER),
            &invalid_delete,
            &[("origin", "https://dash.example.test")]
        )
        .status,
        "400 Bad Request"
    );
    if let Some(dir) = std::env::var_os("DASHQ_ARCHIVE_EVIDENCE") {
        let dir = PathBuf::from(dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (file, html) in [
            ("list", h),
            ("detail", detail),
            ("step", full),
            ("search", searched),
            ("console", console),
        ] {
            let html = html.replace(
                "<link rel=\"stylesheet\" href=\"/style.css\">",
                &format!("<style>{}</style>", crate::style_css()),
            );
            std::fs::write(dir.join(format!("{file}.html")), html).unwrap();
        }
    }
}

#[test]
fn audit_failure_refuses_content_and_backup_restore_verifies_objects() {
    let f = Fixture::new("restore");
    let raw = record("s");
    let b = body("s", "source", &raw);
    let id = f.ingest(&b, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let backup = f.dir.join("backup.db");
    f.e().store().backup(&backup).unwrap();
    let restore = f.dir.join("restore");
    std::fs::create_dir(&restore).unwrap();
    store::Store::restore(&backup, &restore.join("dashq.db"), store::LIMITS).unwrap();
    let mut key = objects::private_file(&restore.join("dashq.key")).unwrap();
    std::io::Write::write_all(&mut key, &std::fs::read(f.dir.join("dashq.key")).unwrap()).unwrap();
    drop(key);
    let restored = Enroll::open(&restore, "dash.example.test").unwrap();
    let target = objects::Objects::open(&restore.join("objects"), restored.archive_key()).unwrap();
    objects::private_file(&restore.join("objects/archive.db")).unwrap();
    f.e()
        .store()
        .lock()
        .unwrap()
        .execute(
            "VACUUM archive INTO ?1",
            [restore.join("objects/archive.db").to_string_lossy()],
        )
        .unwrap();
    for path in std::fs::read_dir(f.root()).unwrap() {
        let p = path.unwrap();
        if !digest(&p.file_name().to_string_lossy()) {
            continue;
        }
        let mut file = objects::private_file(&restore.join("objects").join(p.file_name())).unwrap();
        std::io::Write::write_all(&mut file, &std::fs::read(p.path()).unwrap()).unwrap();
    }
    assert_eq!(
        read_source(&restored, &target, &id, "source", OWNER, "restore", NOW)
            .unwrap()
            .2,
        raw
    );
    let wrong = objects::Objects::open(&restore.join("objects"), b"invented-wrong-key").unwrap();
    assert!(wrong.read(&hash_bytes(&raw)).is_err());
    f.e().store().lock().unwrap().execute_batch("CREATE TRIGGER archive.refuse_audit BEFORE INSERT ON archive_audit BEGIN SELECT RAISE(ABORT,'invented refusal'); END;").unwrap();
    assert_eq!(
        read_source(f.e(), &f.objects(), &id, "source", OWNER, "raw", NOW).unwrap_err(),
        Failure("unavailable")
    );
    let orphan = hash_bytes(b"orphan");
    f.objects().put(&orphan, b"orphan").unwrap();
    prune(f.e(), &f.root(), NOW).unwrap();
    assert!(!f.root().join(orphan).exists());
    assert!(f.root().join(hash_bytes(&raw)).exists());
}

#[test]
fn comparison_joins_only_exact_scoped_session_and_preserves_unknown() {
    let f = Fixture::new("compare");
    let b = body("s", "source", &record("s"));
    let id = f.ingest(&b, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let c = f.e().store().lock().unwrap();
    let s = session(&c, &id, OWNER, false).unwrap();
    assert_eq!(view::metrics(&c, &s).unwrap()["duration_us"], Value::Null);
    drop(c);
    let payload=json!({"observation":{"kind":"agent.turn","session_id":"s","project":"example/project","task":283,"attempt":"attempt-1","duration_us":100,"outcome":"ok","input_tokens":7,"cache_read_tokens":0,"cache_write_tokens":0,"output_tokens":3}}).to_string();
    f.e().store().write(|tx| {
        for (inst,ws,event) in [("inst","ws","one"),("foreign","ws","two"),("inst","foreign","three")] {
            tx.execute("INSERT INTO observations(installation,workspace,generation,product,event_id,observed,received,kind,payload) VALUES (?1,?2,1,'agentq',?3,?4,?4,'agent.turn',?5)",params![inst,ws,event,NOW,payload])?;
        }
        let mut wrong:Value=serde_json::from_str(&payload).unwrap();wrong["observation"]["attempt"]=json!("another-attempt");
        tx.execute("INSERT INTO observations(installation,workspace,generation,product,event_id,observed,received,kind,payload) VALUES ('inst','ws',1,'agentq','wrong-attempt',?1,?1,'agent.turn',?2)",params![NOW,wrong.to_string()])?;
        Ok(())
    }).unwrap();
    let c = f.e().store().lock().unwrap();
    let joined = view::metrics(&c, &s).unwrap();
    assert_eq!(joined["records"], 1);
    assert_eq!(joined["duration_us"], 100);
    assert_eq!(joined["tokens"]["input_tokens"]["observed"], 7);
    drop(c);
    let mut no_task = body("no-task", "source", &record("no-task"));
    no_task["manifest"]["task"] = Value::Null;
    no_task["manifest"]["attempt"] = Value::Null;
    rehash(&mut no_task);
    let no_task_id = f.ingest(&no_task, NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let invalid=json!({"observation":{"kind":"agent.turn","session_id":"no-task","project":"example/project","task":false,"attempt":null,"duration_us":999,"outcome":"ok"}}).to_string();
    f.e().store().write(|tx|{tx.execute("INSERT INTO observations(installation,workspace,generation,product,event_id,observed,received,kind,payload) VALUES ('inst','ws',1,'agentq','invalid-task',?1,?1,'agent.turn',?2)",params![NOW,invalid])?;Ok(())}).unwrap();
    let c = f.e().store().lock().unwrap();
    let s = session(&c, &no_task_id, OWNER, false).unwrap();
    assert_eq!(
        view::metrics(&c, &s).unwrap()["duration_us"],
        Value::Null,
        "invalid task identity must not match a missing task"
    );
}

// D31 full-output boundary: a native record larger than the legacy runtime page limit must remain readable.
#[test]
fn full_native_console_crosses_upload_ranges_and_unknown_steps_stay_raw() {
    let f = Fixture::new("full-console");
    let output = "x".repeat(17 * 1024 * 1024) + "\nlast line with \"quotes\"";
    let input=json!({"type":"assistant","sessionId":"large","message":{"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"Bash","input":{"command":"invented-command"}}]}}).to_string();
    let result=json!({"type":"user","sessionId":"large","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"call-1","content":""}]},"toolUseResult":{"stdout":output,"stderr":"invented-stderr","exitCode":0}}).to_string();
    let unknown = "{\"type\":\"future-native-record\",\"invented\":\"raw stays exact\"}\n";
    let raw = format!("{input}\n{result}\n{unknown}").into_bytes();
    let mut start = 0;
    let mut id = String::new();
    for chunk in raw.chunks(512 * 1024) {
        let mut b = body("large", "claude-source", chunk);
        b["format"] = json!("claude-native");
        b["manifest"]["runtime"] = json!("claude");
        rehash(&mut b);
        b["start"] = json!(start);
        b["end"] = json!(start + chunk.len());
        id = f.ingest(&b, NOW).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        start += chunk.len();
    }
    let (s, src, bytes) = read_source(
        f.e(),
        &f.objects(),
        &id,
        "claude-source",
        OWNER,
        "test",
        NOW,
    )
    .unwrap();
    assert!(bytes == raw, "archive reconstruction changed native bytes");
    let (steps, pending) = steps(&f.objects(), &s, &src, &bytes).unwrap();
    assert_eq!(pending, 0);
    let shell = steps
        .iter()
        .find(|s| s.native_id.as_deref() == Some("call-1") && s.data["stdout"].is_string())
        .unwrap();
    assert!(
        shell.data["stdout"].as_str() == Some(output.as_str()),
        "native stdout was clipped or changed"
    );
    assert_eq!(shell.data["stderr"], "invented-stderr");
    assert!(steps.iter().any(
        |s| s.kind == agentq_runtime::agent::history::StepKind::Unknown
            && s.source["raw"] == unknown
    ));
}

// The existing HTTP body bound must not become a whole-video bound, and no partial/incorrect media is readable.
#[test]
fn chunked_media_is_invisible_until_whole_object_verified() {
    let f = Fixture::new("media-chunks");
    let raw = [
        vec![11; 512 * 1024],
        vec![22; 512 * 1024],
        vec![33; 512 * 1024],
    ]
    .concat();
    let whole = hash_bytes(&raw);
    let mut id = String::new();
    let make = |start: usize, bytes: &[u8]| {
        let mut b = body("video", &whole, bytes);
        b["media"] = json!(true);
        b["format"] = json!("media");
        b["total"] = json!(raw.len());
        b["start"] = json!(start);
        b["end"] = json!(start + bytes.len());
        b
    };
    for (n, chunk) in raw.chunks(512 * 1024).take(2).enumerate() {
        let b = make(n * 512 * 1024, chunk);
        assert!(b.to_string().len() < enroll::MAX_BODY);
        let accepted = f.ingest(&b, NOW).unwrap();
        assert_eq!(accepted["complete"], false);
        id = accepted["id"].as_str().unwrap().to_string();
        assert_eq!(
            read_source(f.e(), &f.objects(), &id, &whole, OWNER, "raw", NOW).unwrap_err(),
            Failure("incomplete")
        );
    }
    assert_eq!(f.audit(), 0, "pending media bytes are not read");
    let pending = f.request(
        "GET",
        "/archive",
        &format!("id={id}"),
        Some(OWNER),
        b"",
        &[],
    );
    let pending = String::from_utf8(pending.body).unwrap();
    assert!(pending.contains("Media pending") && !pending.contains("Download exact raw bytes"));
    let mut altered = make(512 * 1024, &raw[512 * 1024..1024 * 1024]);
    altered["total"] = json!(raw.len() + 1);
    assert_eq!(f.ingest(&altered, NOW), Err(Failure("replay_mismatch")));
    let bad = make(1024 * 1024, &vec![99; 512 * 1024]);
    assert_eq!(f.ingest(&bad, NOW), Err(Failure("invalid_request")));
    let last = make(1024 * 1024, &raw[1024 * 1024..]);
    let accepted = f.ingest(&last, NOW).unwrap();
    assert_eq!(accepted["complete"], true);
    assert_eq!(
        f.ingest(&make(0, &raw[..512 * 1024]), NOW).unwrap()["complete"],
        true
    );
    assert!(
        read_source(f.e(), &f.objects(), &id, &whole, OWNER, "raw", NOW)
            .unwrap()
            .2
            == raw
    );
}

// Resumes can change conditions; byte bindings, not current labels or observation arrival order, select versions.
#[test]
fn manifest_versions_are_append_only_and_comparison_uses_turn_positions() {
    let f = Fixture::new("manifest-versions");
    let line = b"{\"type\":\"future\"}\n";
    let mut first = body("resumed", "source", line);
    first["manifest"]["model"] = json!("model-a");
    rehash(&mut first);
    let mut second = first.clone();
    second["start"] = json!(line.len());
    second["end"] = json!(2 * line.len());
    second["manifest"]["model"] = json!("model-b");
    second["manifest"]["effort"] = json!("low");
    second["manifest"]["policy"] = json!({"write":"all"});
    rehash(&mut second);
    let mut third = first.clone();
    third["start"] = json!(2 * line.len());
    third["end"] = json!(3 * line.len());
    third["manifest"]["project"] = json!("other/project");
    third["manifest"]["task"] = json!(284);
    third["manifest"]["attempt"] = json!("attempt-2");
    rehash(&mut third);
    let mut fourth = first.clone();
    fourth["start"] = json!(3 * line.len());
    fourth["end"] = json!(4 * line.len());
    let mut uploads = [first, second, third, fourth];
    let mut id = String::new();
    for (n, b) in uploads.iter_mut().enumerate() {
        b["turns"] = json!([{"event_id":format!("turn-{n}"),"start":n*line.len()}]);
        id = f.ingest(b, NOW + n as i64).unwrap()["id"]
            .as_str()
            .unwrap()
            .to_string();
        let payload=json!({"observation":{"kind":"agent.turn","session_id":"resumed","project":b["manifest"]["project"],"task":b["manifest"]["task"],"attempt":b["manifest"]["attempt"],"duration_us":(n+1)*100,"outcome":"ok","input_tokens":(n+1)*10,"cache_read_tokens":0,"cache_write_tokens":0,"output_tokens":1}}).to_string();
        f.e().store().write(|tx|{tx.execute("INSERT INTO observations(installation,workspace,generation,product,event_id,observed,received,kind,payload) VALUES ('inst','ws',1,'agentq',?1,?2,?2,'agent.turn',?3)",params![format!("turn-{n}"),NOW,payload])?;Ok(())}).unwrap();
    }
    let c = f.e().store().lock().unwrap();
    let all = versions(&c, &id).unwrap();
    assert_eq!(all.len(), 3);
    let first_hash = uploads[0]["manifest_sha256"].as_str().unwrap();
    assert_eq!(
        all.iter()
            .find(|v| v.hash == first_hash)
            .unwrap()
            .positions
            .len(),
        2,
        "returning to earlier conditions keeps both activations"
    );
    for (n, expected) in [(0, 50), (1, 20), (2, 30)] {
        let mut s = session(&c, &id, OWNER, false).unwrap();
        s.manifest = uploads[n]["manifest"].clone();
        s.version = uploads[n]["manifest_sha256"].as_str().map(String::from);
        let m = view::metrics(&c, &s).unwrap();
        assert_eq!(m["tokens"]["input_tokens"]["observed"], expected);
        assert_eq!(m["duration_us"], expected * 10);
    }
    drop(c);
    let mut changed = uploads[0].clone();
    changed["manifest"]["model"] = json!("rewrite");
    rehash(&mut changed);
    assert_eq!(f.ingest(&changed, NOW), Err(Failure("replay_mismatch")));
    let payload=json!({"observation":{"kind":"agent.turn","session_id":"resumed","project":"example/project","task":283,"attempt":"attempt-1","duration_us":500,"outcome":"ok","input_tokens":50,"cache_read_tokens":0,"cache_write_tokens":0,"output_tokens":1}}).to_string();
    f.e().store().write(|tx|{tx.execute("INSERT INTO observations(installation,workspace,generation,product,event_id,observed,received,kind,payload) VALUES ('inst','ws',1,'agentq','late-turn',?1,?1,'agent.turn',?2)",params![NOW,payload])?;Ok(())}).unwrap();
    {
        let c = f.e().store().lock().unwrap();
        let s = session(&c, &id, OWNER, false).unwrap();
        let m = view::metrics(&c, &s).unwrap();
        assert_eq!(m["unattributed"], 1);
        assert_eq!(m["duration_us"], Value::Null);
        assert_eq!(m["tokens"]["input_tokens"]["state"], "unknown");
    }
    let mut replay = uploads[1].clone();
    replay["turns"] = json!([{"event_id":"late-turn","start":line.len()}]);
    f.ingest(&replay, NOW + 5).unwrap();
    {
        let c = f.e().store().lock().unwrap();
        let mut s = session(&c, &id, OWNER, false).unwrap();
        s.manifest = uploads[1]["manifest"].clone();
        s.version = uploads[1]["manifest_sha256"].as_str().map(String::from);
        let m = view::metrics(&c, &s).unwrap();
        assert_eq!(m["duration_us"], 700);
        assert_eq!(m["tokens"]["input_tokens"]["observed"], 70);
    }
    replay["turns"] = json!([{"event_id":"late-turn","start":0}]);
    assert_eq!(f.ingest(&replay, NOW + 6), Err(Failure("replay_mismatch")));
    let html = f.request("GET", "/archive", "model=model-b", Some(OWNER), b"", &[]);
    let html = String::from_utf8(html.body).unwrap();
    assert!(html.contains("model-b") && !html.contains("model-a"));
    let project = HashMap::from([
        ("workspace".into(), "ws".into()),
        ("project".into(), "other/project".into()),
        ("confirm".into(), "delete".into()),
    ]);
    mutate(f.e(), &f.objects(), &project, OWNER, false).unwrap();
    assert_eq!(
        f.ingest(&uploads[0], NOW),
        Err(Failure("deleted")),
        "project deletion includes every historical manifest"
    );
}

#[test]
fn archive_v1_index_migration_preserves_bytes_and_first_manifest() {
    let f = Fixture::new("v1-migration");
    let raw = record("s");
    let id = f.ingest(&body("s", "source", &raw), NOW).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    f.e().store().lock().unwrap().execute_batch("DROP TABLE archive.archive_turns; DROP TABLE archive.archive_versions; DROP TABLE archive.archive_manifests; ALTER TABLE archive.archive_sources DROP COLUMN total; ALTER TABLE archive.archive_sources DROP COLUMN complete; ALTER TABLE archive.archive_ranges DROP COLUMN manifest; PRAGMA archive.user_version=1;").unwrap();
    let e = Enroll::open(&f.dir, "dash.example.test").unwrap();
    let o = objects::Objects::open(&f.root(), e.archive_key()).unwrap();
    assert_eq!(
        read_source(&e, &o, &id, "source", OWNER, "migration", NOW)
            .unwrap()
            .2,
        raw
    );
    let c = e.store().lock().unwrap();
    let v = versions(&c, &id).unwrap();
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].positions, vec![("source".into(), 0)]);
    assert_eq!(
        c.query_row("PRAGMA archive.user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}
