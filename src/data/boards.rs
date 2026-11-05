use super::*;

pub(super) struct Read {
    pub boards: Vec<Value>,
    pub heartbeats: BTreeMap<String, Value>,
    pub gh_states: Vec<(String, Value)>,
    pub taskq_states: Vec<(String, Value)>,
    pub working: BTreeMap<String, Vec<String>>,
    pub unconfirmed: BTreeMap<String, String>,
    pub taskq_hosts: BTreeMap<(String, String), String>,
}

pub(super) fn read(inp: &Value, machines: &Machines, hosts: &mut BTreeMap<String, Value>, current: &mut Vec<Value>) -> Read {
    let now = inp["now"].as_u64().unwrap_or(0);
    let mut heartbeats: BTreeMap<String, Value> = BTreeMap::new();
    // Heartbeats and boards (D4.2).
    let releases = &inp["releases"];
    let latest = |repo: &str| releases[repo]["value"].clone();
    let mut boards = Vec::new();
    let mut gh_states = Vec::new();
    let mut taskq_states = Vec::new();
    // live taskq attempts per lowercase Hosts row id from fresh heartbeats: agents run there (#116);
    // `unconfirmed`: hosts whose heartbeat came from a failed, stale or cached board read, with that reason
    let mut working: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut unconfirmed: BTreeMap<String, String> = BTreeMap::new();
    let mut taskq_hosts = BTreeMap::new();
    for repo in inp["projects"].as_array().into_iter().flatten().filter_map(Value::as_str) {
        let snap = &inp["boards"][repo];
        let src = source(snap, now, GITHUB_LIMIT, None);
        gh_states.push((repo.to_string(), src.clone()));
        if let Some(r) = releases.get(repo) {
            let s = source(r, now, 900, None);
            if s["state"] != "ok" {
                gh_states.push((format!("{repo} release"), s));
            }
        }
        let v = &snap["value"];
        let issues = v["issues"].as_array();
        let with = |labels: &[&str]| issues.map(|is| is.iter().filter(|i| i["labels"].as_array().is_some_and(|l| l.iter().any(|x| labels.iter().any(|w| x == *w)))).collect::<Vec<_>>());
        let count = |labels: &[&str]| with(labels).map_or(Value::Null, |v| json!(v.len()));
        let list = |labels: &[&str]| with(labels).map_or(json!([]), |v| v.iter().map(|i| json!({"number": i["number"], "title": i["title"], "url": i["url"]})).collect());
        let mut attempts = Vec::new();
        let date = v["date"].as_u64();
        for (index, hb) in v["hosts"].as_array().into_iter().flatten().enumerate() {
            let Some(name) = hb["title"].as_str().and_then(|t| t.strip_prefix("taskq host ")).filter(|n| safe(n, 64, b"_.-")) else { continue };
            let body = hb["body"].as_str().unwrap_or("");
            // H2: the `machine <host.name>[ wsl]` line names the row; without it a label that is a machine joins its native row
            let machine = body.lines().find_map(|l| l.strip_prefix("machine ")).and_then(|m| {
                let mut p = m.split_whitespace();
                p.next().filter(|n| safe(n, 64, b"_.-")).map(|n| (n, p.next() == Some("wsl")))
            });
            let (row, joined) = match machine {
                Some((n, wsl)) => (machines.id(n, if wsl { "wsl" } else { "native" }), true),
                None if machines.known(name) => (machines.id(name, "native"), true),
                None => (name.to_string(), false),
            };
            taskq_hosts.insert((repo.to_ascii_lowercase(), name.to_ascii_lowercase()), row.clone());
            let ttl = body.lines().find_map(|l| l.strip_prefix("ttl ")?.trim().parse().ok()).unwrap_or(HEARTBEAT_TTL);
            let first = attempts.len();
            for l in body.lines() {
                if let Some((n, id)) = l.strip_prefix('#').and_then(|l| l.split_once(' '))
                    && let Ok(n) = n.parse::<u64>()
                {
                    attempts.push(json!({"host": name, "host_id": row, "task": n, "attempt": id.trim()}));
                }
            }
            // Age at collection uses only the board clock.
            let at = hb["updated_at"].as_str().and_then(iso_secs);
            let age = date.zip(at).map(|(d, u)| d.saturating_sub(u));
            let cached_age = age.zip(snap["at"].as_u64()).map(|(a, read_at)| a.saturating_add(now.saturating_sub(read_at)));
            for a in &mut attempts[first..] {
                a["current"] = json!(src["state"] == "ok" && cached_age.is_some_and(|a| a <= ttl));
            }
            if src["state"] != "ok" {
                unconfirmed.entry(row.to_ascii_lowercase()).or_insert_with(|| format!("{repo}: {}", src["reason"].as_str().unwrap_or("board read not confirmed")));
            } else if cached_age.is_some_and(|a| a <= ttl) {
                let tasks = working.entry(row.to_ascii_lowercase()).or_default();
                tasks.extend(attempts[first..].iter().map(|a| format!("{repo}#{}", a["task"])));
            }
            let version = semver(&json!(body.lines().find_map(|l| l.strip_prefix("version ")).map(str::trim)));
            // taskq's own installation id (taskq.md § 4 `service`), never the host name; older heartbeats have none
            let service = body.lines().find_map(|l| l.strip_prefix("service ")).map(str::trim).filter(|s| s.len() == 12 && s.bytes().all(|b| b.is_ascii_hexdigit()));
            let key = format!("{repo}:{}", service.map_or_else(|| format!("unidentified:{index}"), String::from));
            if heartbeats.get(&key).is_none_or(|o: &Value| o["age_s"].as_u64().is_some_and(|a| cached_age.is_some_and(|x| x < a))) {
                heartbeats.insert(key, json!({"service": service, "workspace": repo, "host": row, "at": at, "age_s": cached_age, "ttl": ttl, "version": version, "source": src, "clis": crate::fleet::cli_lines(body)}));
            }
            let h = host_entry(hosts, &row);
            if !joined {
                h["reason"] = json!("no machine line: update taskq");
            }
            // H3: several heartbeats on one row show the newest version and the freshest state
            if h["taskq"]["version"].is_null() || behind(&version, &h["taskq"]["version"]) == true {
                h["taskq"]["version"] = version.clone();
            }
            if h["taskq"]["age_s"].as_u64().is_some_and(|a| age.is_none_or(|x| x >= a)) {
                continue;
            }
            let fresh = age.is_some_and(|a| a <= ttl);
            let t = &mut h["taskq"];
            t["age_s"] = json!(age);
            t["state"] = json!(if age.is_none() { "unknown" } else if fresh { "ok" } else { "stale" });
            t["reason"] = if fresh { Value::Null } else { json!(age.map_or("heartbeat age unknown".into(), |a| format!("heartbeat {} old (ttl {})", dur(a), dur(ttl)))) };
        }
        // TaskQ's state vocabulary (crate::vocab): counter order and canonical labels; Closed has no authoritative total yet: unknown
        let states = crate::vocab::pinned_states();
        let l = |id: &str| states.iter().find(|s| s.id == id).map_or("", |s| s.label.as_str());
        let mut board = json!({
            "project": repo, "url": format!("https://github.com/{repo}/issues"), "source": src, "adapter": "taskq-labels",
            "in_work": issues.map(|_| json!(with(&[l("doing"), l("review")]).unwrap().len())), "waiting": count(&[l("ask")]), "ready": count(&[l("ready")]),
            "questions": list(&[l("ask")]), "failed": list(&[l("failed")]), "attempts": attempts, "latest": latest(repo),
        });
        let mut counts = json!({"closed": null});
        for s in states {
            counts[&s.id] = count(&[&s.label]);
        }
        board["counts"] = counts;
        board["tasks"] = issues.into_iter().flatten().map(|i| {
            let state = states.iter().find(|s| i["labels"].as_array().is_some_and(|ls| ls.iter().any(|l| l == &s.label))).map(|s| &s.id);
            json!({"n": i["number"], "title": i["title"], "url": i["url"], "state": state, "revision": null})
        }).collect::<Vec<_>>().into();
        // taskq-snapshot (adapters.md S3 R1) replaces counts, questions and failures when configured; heartbeats stay
        if let Some(ts) = inp["taskq"].get(repo) {
            let tsrc = source(ts, now, GITHUB_LIMIT, None);
            taskq_states.push((repo.to_string(), tsrc.clone()));
            let tasks = ts["value"]["tasks"].as_array();
            let total = |k: &[&str]| -> Value { k.iter().map(|k| ts["value"]["totals"][k].as_u64()).sum::<Option<u64>>().map_or(Value::Null, |n| json!(n)) };
            let pick = |f: &dyn Fn(&Value) -> bool, m: &dyn Fn(&Value) -> Value| -> Value { tasks.into_iter().flatten().filter(|t| f(t)).map(m).collect::<Vec<_>>().into() };
            board["source"] = tsrc;
            board["adapter"] = json!("taskq-snapshot");
            // D13: preserve canonical question identities/limits/evidence; no second parser or session authority.
            board["tasks"] = tasks.into_iter().flatten().map(|t| json!({
                "n": t["n"], "title": t["title"], "url": t["url"], "state": t["state"],
                "revision": t["revision"], "controls": t["controls"], "question": t["question"], "card": t["card"], "admission": t["admission"]
            })).collect::<Vec<_>>().into();
            board["in_work"] = total(&["doing", "review"]);
            board["waiting"] = total(&["ask"]);
            board["ready"] = total(&["ready"]);
            // the snapshot's own state list sets the counters, Held included (A1); an older taskq reads the pin
            board["counts"] = json!({"closed": null});
            for st in crate::vocab::states(&ts["value"]) {
                board["counts"][&st.id] = total(&[&st.id]);
            }
            board["questions"] = pick(&|t| t["state"] == "ask" || (t["state"] == "review" && t["question"].is_object()), &question);
            board["failed"] = pick(&|t| t["state"] == "failed", &|t| json!({"number": t["n"], "title": t["title"], "url": t["url"], "reason": t["failed"]}));
        }
        // TaskQ R6 3 (#1565): a live session's progress comes from its host's enrolled `taskq-board/1` task `session`
        // (runtime, stage, update time; never a worker line, #180); the newest per task wins, shown on doing and review rows only.
        // the host shown is the enrolled row's label, not the payload's; a future `at` cannot pin newest-wins
        let mut progress: BTreeMap<u64, (&Value, &str)> = BTreeMap::new();
        // D23: the newest enrolled admission per task; a future time cannot win
        let mut admitted: BTreeMap<u64, &Value> = BTreeMap::new();
        for r in inp["installations"]["rows"].as_array().into_iter().flatten()
            .filter(|r| r["product"] == "taskq" && r["connection"] != "revoked" && r["snapshot"]["schema"] == "taskq-board/1")
        {
            let p = &r["snapshot"]["payload"];
            let label = r["host_label"].as_str().filter(|h| safe(h, 64, b"_.-")).unwrap_or("unknown");
            if !p["board"]["repo"].as_str().is_some_and(|x| x.eq_ignore_ascii_case(repo)) {
                continue;
            }
            for t in p["tasks"].as_array().into_iter().flatten() {
                if let (Some(n), Some(o)) = (t["n"].as_u64(), t["admission"]["observed"].as_u64().filter(|&o| o <= now + 60))
                    && admitted.get(&n).is_none_or(|a| a["observed"].as_u64() < Some(o))
                {
                    admitted.insert(n, &t["admission"]);
                }
                let (Some(n), Some(at)) = (t["n"].as_u64(), t["session"]["at"].as_u64().filter(|&at| at <= now)) else { continue };
                if progress.get(&n).is_none_or(|(v, _)| v["at"].as_u64() < Some(at)) {
                    progress.insert(n, (&t["session"], label));
                }
            }
        }
        let snapshot = board["adapter"] == "taskq-snapshot";
        for t in board["tasks"].as_array_mut().unwrap() {
            if matches!(t["state"].as_str(), Some("ready" | "waiting")) {
                let own = t.get("admission").filter(|a| snapshot && a.is_object()).cloned();
                let raw = own.as_ref().map(|a| (a, "taskq-snapshot")).or_else(|| t["n"].as_u64().and_then(|n| admitted.get(&n)).map(|a| (*a, "taskq-board/1")));
                t["admission"] = crate::explain::admission(raw, now);
            } else if let Some(o) = t.as_object_mut() {
                o.remove("admission");
            }
            if let Some(&(s, host)) = t["n"].as_u64().and_then(|n| progress.get(&n)).filter(|_| matches!(t["state"].as_str(), Some("doing" | "review"))) {
                t["session"] = json!({"host": host, "runtime": s["runtime"], "stage": s["stage"], "age_s": now - s["at"].as_u64().unwrap()});
            }
        }
        for i in board["failed"].as_array().unwrap() {
            let detail = i["reason"].as_str().filter(|r| !r.is_empty()).map_or(i["title"].clone(), |r| json!(r));
            current.push(json!({"source": "taskq", "host": null, "what": format!("{repo}#{}: failed", i["number"]), "since": null, "detail": detail, "trace_id": null, "url": i["url"]}));
        }
        boards.push(board);
    }
    Read { boards, heartbeats, gh_states, taskq_states, working, unconfirmed, taskq_hosts }
}

/// D8 / #178: newest board-scoped evidence, never inferred from activity or shared queue totals.
pub(super) fn taskq_observations(inp: &Value, now: u64, machines: &Machines, aliases: &BTreeMap<(String, String), String>, hosts: &mut BTreeMap<String, Value>) {
    let mut latest: BTreeMap<(String, String, String, String), Value> = BTreeMap::new();
    for r in inp["installations"]["rows"].as_array().into_iter().flatten().filter(|r|
        r["product"] == "taskq" && r["connection"] == "active" && r["projected"] != false && r["snapshot"]["schema"] == "taskq-board/1") {
        let p = &r["snapshot"]["payload"];
        let b = &p["board"];
        let Some(repo) = b["repo"].as_str() else { continue };
        if p["schema"] != 1 || b["kind"] != "github" || b["host"] != "github.com" { continue }
        for o in p["observations"].as_array().into_iter().flatten() {
            let (Some(host), Some(kind)) = (o["host"].as_str().filter(|h| safe(h,64,b"_.-")), o["kind"].as_str().filter(|k| matches!(*k,"online"|"login"|"queue"))) else { continue };
            let row = aliases.get(&(repo.to_ascii_lowercase(), host.to_ascii_lowercase())).cloned()
                .unwrap_or_else(|| if machines.known(host) { machines.id(host,"native") } else { host.to_string() });
            let key = (repo.to_ascii_lowercase(), r["workspace"].as_str().unwrap_or("").into(), row.to_ascii_lowercase(), kind.into());
            let mut v = o.clone();
            v["board"] = b.clone(); v["workspace"] = r["workspace"].clone(); v["installation"] = r["installation"].clone();
            let rank = |v: &Value| (v["observed"].as_u64().is_some_and(|t|t<=now), v["observed"].as_u64());
            if let Some(old) = latest.get_mut(&key) {
                if rank(old) > rank(&v) { continue }
                if rank(old) == rank(&v) {
                    let mut a = old.clone(); let mut b = v.clone();
                    for x in [&mut a, &mut b] { x.as_object_mut().unwrap().remove("installation"); }
                    if a != b { old["state"] = json!("unknown"); old["complete"] = json!(false); old["reason"] = json!("conflicting observations"); }
                    continue;
                }
            }
            latest.insert(key, v);
            host_entry(hosts, &row);
        }
    }
    for ((repo, workspace, row, _), v) in latest.clone() {
        for kind in ["online", "login", "queue"] {
            latest.entry((repo.clone(), workspace.clone(), row.clone(), kind.into())).or_insert_with(|| json!({
                "host":v["host"],"kind":kind,"board":v["board"],"workspace":workspace,"installation":null,
                "producer":null,"observed":null,"valid_until":null,"state":"unknown","complete":false,"reason":"no observation"}));
        }
    }
    for ((_, _, row, kind), mut o) in latest {
        let valid = match kind.as_str() {
            "online" => matches!(o["state"].as_str(),Some("online"|"offline")),
            "login" => matches!(o["state"].as_str(),Some("logged in"|"not logged in")),
            _ => matches!(o["state"].as_str(),Some("busy"|"idle")) && o["running"].as_u64().is_some() && o["queued"].as_u64().is_some(),
        };
        let reason = if o["observed"].as_u64().is_some_and(|t|t>now) { Some("clock uncertain") }
            else if o["valid_until"].as_u64().is_some_and(|t|now>t) { Some("expired") }
            else if o["complete"] != true || o["state"] == "unknown" { Some(o["reason"].as_str().unwrap_or("incomplete observation")) }
            else if o["observed"].as_u64().is_none_or(|t|t==0) || o["valid_until"].as_u64().is_none_or(|t|Some(t)<o["observed"].as_u64()) { Some("invalid observation time") }
            else if kind == "login" && crate::runtimes::runtimes().iter().filter_map(|r| r.login).any(|k| !matches!(o[k].as_str(),Some("logged in"|"not logged in"|"not installed"))) { Some("runtime login unknown") }
            else if !valid || !o["producer"].as_str().is_some_and(|p|p.len()==12 && p.bytes().all(|b|b.is_ascii_hexdigit())) { Some("invalid observation") }
            else { None };
        if let Some(reason) = reason.map(str::to_string) {
            o["state"] = json!("unknown"); o["complete"] = json!(false); o["reason"] = json!(reason);
            for k in ["running","queued"] { if o.get(k).is_some() { o[k] = Value::Null; } }
            for k in crate::runtimes::codes() { if o.get(k).is_some() { o[k] = json!("unknown"); } }
        }
        let h = hosts.get_mut(&row).unwrap();
        if !h["taskq"]["observations"].is_array() { h["taskq"]["observations"] = json!([]); }
        h["taskq"]["observations"].as_array_mut().unwrap().push(o);
    }
}
