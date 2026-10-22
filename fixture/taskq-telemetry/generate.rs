// Generator of telemetry-sync-v4.6.30.json (#181 A2); DashQ never builds it. Append this file to src/telemetry_sync.rs
// of a taskqorg/taskq v4.6.30 checkout (09cb0d7) and run `DASHQ181=<empty dir> cargo test --bin taskq dashq181 --
// --nocapture`. `<dir>/fixture.json` holds the /ingest records taskq's unmodified `run()` posted; the committed file
// adds only the `source` key. With a copy of a host's `telemetry.log/universal.sqlite3` under
// `<dir>/install/telemetry.log`, it also prints that store's block volume (#181 A4) and writes
// `<dir>/real-blocks.jsonl`, which stays on that host.

#[cfg(test)]
mod dashq181 {
    use super::*;
    use std::io::Read as _;

    #[derive(Default)]
    struct Capture { records: Vec<Value> }
    impl Wire for Capture {
        fn post(&mut self, _: &str, route: &str, _: Option<&str>, body: &Value) -> Result<(u16, Value), String> {
            assert_eq!(route, "/ingest");
            let r = body["records"][0].clone();
            let id = r["event_id"].clone();
            self.records.push(r);
            Ok((200, json!({"protocol": 1, "receipts": [{"event_id": id, "state": "accepted"}]})))
        }
        fn now(&self) -> u64 { 0 }
        fn sleep(&mut self, _: u64) {}
        fn show(&mut self, _: &str, _: &str) {}
    }

    fn connect(install: &Path) {
        fs::create_dir_all(enrollment::dir(install)).unwrap();
        let conn = json!({"destination": "https://dash.example.invalid/machine", "credential": "c", "generation": 1, "installation": "i",
            "scope": {"name": enrollment::SCHEMA, "value": "github:github.com/o/a"}, "schemas": [enrollment::SCHEMA, SCHEMA]});
        fs::write(enrollment::dir(install).join("connection.json"), json!({"installation": "i", "pending": null, "active": conn, "revoke": []}).to_string()).unwrap();
    }

    fn go(install: &Path, c: &mut Capture) -> Value {
        run(install, c, &mut Backoff::default(), Instant::now() + Duration::from_secs(600)).0
    }

    #[test]
    fn dashq181() {
        let out = PathBuf::from(std::env::var("DASHQ181").unwrap());
        // fixture: TaskQ's own test rows; block 1 = seq 1..3, block 2 = seq 4..5, then seq 6..7 pruned before upload, block 3 = 8..9
        let install = out.join("fixture-install");
        let _ = fs::remove_dir_all(&install);
        connect(&install);
        fs::create_dir_all(install.join("telemetry.log")).unwrap();
        let store = coreq::telemetry::Store::open(&install.join("telemetry.log/universal.sqlite3")).unwrap();
        let e = || coreq::telemetry::Event::new(json!({"product": "taskq", "version": "1.0.0", "operation": "op", "kind": "operation",
            "trace": "t", "span": "s", "started_ms": 1000, "duration_us": 10, "outcome": "ok", "error": "none"})).unwrap();
        let mut c = Capture::default();
        store.append(&[e(), e(), e()]).unwrap();
        let s1 = go(&install, &mut c);
        store.append(&[e(), e()]).unwrap();
        let s2 = go(&install, &mut c);
        store.append(&[e(), e(), e(), e()]).unwrap();
        drop(store);
        let db = rusqlite::Connection::open(install.join("telemetry.log/universal.sqlite3")).unwrap();
        db.execute("DELETE FROM events WHERE seq IN (6, 7)", []).unwrap();
        drop(db);
        let s3 = go(&install, &mut c);
        fs::write(out.join("fixture.json"), serde_json::to_string_pretty(&json!({"records": c.records, "status_after": [s1, s2, s3]})).unwrap()).unwrap();
        // measurement: every retained row of a real store copy, cut into blocks exactly as the sender does
        let real = out.join("install");
        connect(&real);
        let mut m = Capture::default();
        let st = go(&real, &mut m);
        let (mut rows, mut raw, mut gz, mut kinds, mut lo, mut hi) = (0u64, 0u64, 0u64, BTreeMap::new(), i64::MAX, 0i64);
        let mut bad = fs::File::create(out.join("real-blocks.jsonl")).unwrap();
        for r in &m.records {
            let p = &r["payload"];
            rows += p["rows"].as_u64().unwrap();
            raw += p["bytes"].as_u64().unwrap();
            gz += r.to_string().len() as u64;
            let z = base64::engine::general_purpose::STANDARD.decode(p["gzip"].as_str().unwrap()).unwrap();
            let mut text = String::new();
            flate2::read::GzDecoder::new(&z[..]).read_to_string(&mut text).unwrap();
            for l in text.lines() {
                let v: Value = serde_json::from_str(l).unwrap();
                *kinds.entry(v["observation"]["kind"].as_str().unwrap_or("?").to_string()).or_insert(0u64) += 1;
                let t = v["received_ms"].as_i64().unwrap();
                (lo, hi) = (lo.min(t), hi.max(t));
            }
            writeln!(bad, "{r}").unwrap();
        }
        println!("DASHQ181 status={st} blocks={} rows={rows} raw={raw} record_bytes={gz} span_ms={} kinds={kinds:?}", m.records.len(), hi - lo);
    }
    use std::collections::BTreeMap;
}
