//! Public limits push (dashq.md D27): TaskQ `limit` heartbeat rows become the payload of the limits page Worker.
//! The payload is built field by field from validated numbers, so nothing else of a row can reach the public page.

use crate::{data, profile, runtimes, source};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// Seconds between passes: TaskQ polls each account every 120 s, so a faster pass finds nothing new.
pub const PERIOD: u64 = 120;
/// An unchanged snapshot is still pushed this often, so the page's `ts` keeps moving.
const HEARTBEAT: u64 = 600;
/// The age at which the Worker's page marks an account stale.
const STALE: u64 = 900;
/// A producer time further ahead than this is ignored, so a wrong clock cannot look fresh for ever.
const SKEW: u64 = 300;
/// Side-by-side tolerance: the two publishers sample minutes apart.
const CHECK_PERCENT: u64 = 5;
const CHECK_RESET: u64 = 600;

pub struct Mapped {
    runtime: usize,
    id: String,
    public_id: String,
    tag: Option<String>,
}

pub struct Config {
    pub url: String,
    pub token: Option<String>,
    accounts: Vec<Mapped>,
}

fn account_id(id: &str) -> bool {
    id.len() == 8 && id.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

impl Config {
    /// Errors name the bad key, never its value (D2).
    pub fn parse(v: &Value) -> Result<Config, String> {
        let bad = |k: &str| format!("config: bad key limits_push.{k}");
        let url = v["url"].as_str().filter(|u| u.starts_with("https://") && u.ends_with("/push") && u.matches("/push").count() == 1 && u.bytes().all(|b| b.is_ascii_graphic() && !b"\"\\@".contains(&b))).ok_or(bad("url"))?;
        // the token travels in a curl config line: only characters that need no escaping there
        let token = match v.get("token") {
            None => None,
            Some(t) => Some(t.as_str().filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_graphic() && !b"\"\\".contains(&b))).ok_or(bad("token"))?.to_string()),
        };
        let mut accounts = vec![];
        for a in v.get("accounts").map_or(Ok(&vec![]), |a| a.as_array().ok_or(bad("accounts")))? {
            let runtime = a["runtime"].as_str().and_then(runtimes::limit_runtime).ok_or(bad("accounts.runtime"))?;
            let id = a["id"].as_str().filter(|i| account_id(i)).ok_or(bad("accounts.id"))?;
            // the fingerprint is the private account id (A2): a public card id never repeats it
            let public_id = a["public_id"].as_str().filter(|p| data::safe(p, 64, b"_.-") && *p != id).ok_or(bad("accounts.public_id"))?;
            let tag = match a.get("tag") {
                None => None,
                Some(t) => Some(t.as_str().filter(|t| data::safe(t, 8, b"")).ok_or(bad("accounts.tag"))?.to_string()),
            };
            accounts.push(Mapped { runtime, id: id.into(), public_id: public_id.into(), tag });
        }
        Ok(Config { url: url.into(), token, accounts })
    }
}

struct Reading {
    at: u64,
    used: u64,
    reset: Option<u64>,
}

#[derive(Default)]
struct Account {
    windows: BTreeMap<String, Reading>,
}

type Accounts = BTreeMap<(usize, String), Account>;

fn time(t: &str) -> Option<u64> {
    source::iso_secs(t).filter(|s| source::iso(*s) == t)
}

/// Heartbeat rows of every readable board: accounts by runtime and id, and the runtimes with an id-less row.
/// A failed board read keeps its last value, whose rows keep their own old times (D6).
fn observe(boards: &Value, now: u64) -> (Accounts, BTreeSet<usize>) {
    let (mut accounts, mut idless) = (Accounts::new(), BTreeSet::new());
    let hosts = boards.as_object().into_iter().flat_map(|b| b.values()).flat_map(|b| b["value"]["hosts"].as_array().into_iter().flatten());
    for h in hosts.filter(|h| h["title"].as_str().is_some_and(|t| t.starts_with("taskq host "))) {
        for line in h["body"].as_str().unwrap_or("").lines() {
            let Some(w) = line.strip_prefix("limit ").map(|l| l.split(' ').collect::<Vec<_>>()) else { continue };
            // `limit <rt> until|probe <time>` is a hold line, not a usage row
            let [rt, _label, "id", id, rest @ ..] = w.as_slice() else { continue };
            let Some(runtime) = runtimes::limit_runtime(rt) else { continue };
            if !(*id == "-" || account_id(id)) || rest == ["unknown", "disabled"] {
                continue;
            }
            let window = match rest {
                ["window", name, "used", used, "reset", reset, "at", at] if data::safe(name, 40, b"_-") => {
                    let parsed = used.parse().ok().filter(|u| *u <= 100).zip(time(at).filter(|a| *a <= now + SKEW)).zip(if *reset == "-" { Some(None) } else { time(reset).map(Some) });
                    let Some(((used, at), reset)) = parsed else { continue };
                    Some((*name, Reading { at, used, reset }))
                }
                ["unknown", _, ..] => None,
                _ => continue,
            };
            if *id == "-" {
                idless.insert(runtime);
                continue;
            }
            let account = accounts.entry((runtime, id.to_string())).or_default();
            if let Some((name, r)) = window {
                // the freshest reading wins; a tie keeps the higher use, so the page never understates a limit
                if account.windows.get(name).is_none_or(|o| (r.at, r.used) > (o.at, o.used)) {
                    account.windows.insert(name.to_string(), r);
                }
            }
        }
    }
    (accounts, idless)
}

/// A window name as the family it counts for and the page's row.
fn classify(name: &str) -> (&str, &'static str) {
    for (prefix, span) in [("five_hour", "fiveHour"), ("seven_day", "weekly")] {
        if let Some(rest) = name.strip_prefix(prefix)
            && (rest.is_empty() || rest.starts_with('_'))
        {
            return (rest.trim_start_matches('_'), span);
        }
    }
    match name {
        "300m" => return ("", "fiveHour"),
        "10080m" => return ("", "weekly"),
        _ => {}
    }
    for (suffix, span) in [("-5h", "fiveHour"), ("-weekly", "weekly")] {
        if let Some(family) = name.strip_suffix(suffix) {
            return (family, span);
        }
    }
    // ponytail: the page has only 5h and 1w rows; an unknown window stays visible under its own name instead of vanishing
    (name, "weekly")
}

fn title(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
}

fn state(a: &Account, limits: &Value) -> Value {
    let Some(newest) = a.windows.values().map(|r| r.at).max() else {
        return json!({"status": "unknown"});
    };
    let families = limits["families"].as_array().map_or(&[][..], Vec::as_slice);
    let top = limits["top"].as_str().unwrap_or("");
    let mut s = json!({"status": "ok", "lastFetch": newest, "warnings": []});
    let mut additional: BTreeMap<(usize, String), Map<String, Value>> = BTreeMap::new();
    // a reading far behind the account's newest was superseded by a poll that no longer names this window
    for (name, r) in a.windows.iter().filter(|(_, r)| r.at + STALE >= newest) {
        let mut w = json!({"percentUsed": r.used, "percentLeft": 100 - r.used});
        if let Some(reset) = r.reset {
            w["resetsAt"] = json!(reset);
        }
        let (family, span) = classify(name);
        if family == top {
            s[span] = w.clone();
        }
        let known = families.iter().position(|f| f[0] == family);
        if family != top || known.is_some() {
            let label = known.and_then(|i| families[i][1].as_str()).map_or_else(|| title(family), String::from);
            additional.entry((known.unwrap_or(usize::MAX), label)).or_default().insert(span.into(), w);
        }
    }
    s["additional"] = additional.into_iter().map(|((_, label), mut row)| {
        row.insert("label".into(), json!(label));
        Value::Object(row)
    }).collect();
    s
}

fn card(runtime: usize, id: &str, tag: Option<&str>, account: &Account) -> Value {
    let limits = runtimes::runtimes()[runtime].limits;
    let mut c = json!({"id": id, "provider": limits["provider"], "label": limits["label"], "s": state(account, limits)});
    if let Some(tag) = tag {
        c["tag"] = json!(tag);
    }
    c
}

/// Accounts with no `limits_push.accounts` entry, as `<runtime> <fingerprint>`: operator output, never the payload.
pub fn unmapped(boards: &Value, cfg: &Config, now: u64) -> Vec<String> {
    observe(boards, now).0.keys().filter(|(r, id)| !cfg.accounts.iter().any(|m| m.runtime == *r && m.id == *id)).map(|(r, id)| format!("{} {id}", runtimes::runtimes()[*r].name)).collect()
}

/// The Worker's payload `{ts, accounts}`; `None` without a single window, so a read failure never blanks the page.
pub fn snapshot(boards: &Value, cfg: &Config, now: u64) -> Option<Value> {
    let (mut seen, idless) = observe(boards, now);
    let mut cards: Vec<(usize, Value)> = vec![];
    let mut numbered: BTreeMap<usize, usize> = BTreeMap::new();
    for m in &cfg.accounts {
        let account = seen.remove(&(m.runtime, m.id.clone())).unwrap_or_default();
        cards.push((m.runtime, card(m.runtime, &m.public_id, m.tag.as_deref(), &account)));
        *numbered.entry(m.runtime).or_default() += 1;
    }
    for ((runtime, _), account) in &seen {
        let n = numbered.entry(*runtime).or_default();
        *n += 1;
        let id = format!("{}-{n}", runtimes::runtimes()[*runtime].limits["provider"].as_str().unwrap_or_default());
        let tag = (*n > 1).then(|| n.to_string());
        cards.push((*runtime, card(*runtime, &id, tag.as_deref(), account)));
    }
    // an id-less failure names no account: it only keeps a runtime with no other card from vanishing
    for runtime in idless {
        if !cards.iter().any(|(r, _)| *r == runtime) {
            let id = format!("{}-1", runtimes::runtimes()[runtime].limits["provider"].as_str().unwrap_or_default());
            cards.push((runtime, card(runtime, &id, None, &Account::default())));
        }
    }
    if !cards.iter().any(|(_, c)| c["s"]["status"] == "ok") {
        return None;
    }
    cards.sort_by_key(|(r, _)| *r);
    Some(json!({"ts": now, "accounts": cards.into_iter().map(|(_, c)| c).collect::<Vec<_>>()}))
}

#[derive(Default)]
pub struct Pusher {
    sent: Option<(String, u64)>,
}

impl Pusher {
    /// One pass; `Ok(true)` when the Worker accepted a push. A failure keeps the previous state, so the next pass retries.
    pub fn pass(&mut self, cfg: &Config, boards: &Value, now: u64) -> Result<bool, String> {
        let Some(token) = &cfg.token else { return Ok(false) };
        let Some(snap) = snapshot(boards, cfg, now) else { return Ok(false) };
        let accounts = snap["accounts"].to_string();
        if self.sent.as_ref().is_some_and(|(a, at)| *a == accounts && now.saturating_sub(*at) < HEARTBEAT) {
            return Ok(false);
        }
        let t0 = std::time::Instant::now();
        let r = match source::post_status(&cfg.url, &[format!("X-Token: {token}"), "Content-Type: application/json".into()], &snap.to_string()) {
            Ok(200) => Ok(()),
            Ok(status) => Err(format!("HTTP {status}")),
            Err((reason, _)) => Err(reason),
        };
        profile::record("push.limits", t0, r.as_ref().err().map(String::as_str));
        r?;
        self.sent = Some((accounts, now));
        Ok(true)
    }
}

/// A card's rows under the keys the Worker's page and history use.
fn rows(c: &Value) -> BTreeMap<String, &Value> {
    let s = &c["s"];
    let mut out = BTreeMap::new();
    for (span, key) in [("fiveHour", "5h"), ("weekly", "1w")] {
        if s[span].is_object() {
            out.insert(key.to_string(), &s[span]);
        }
        for a in s["additional"].as_array().into_iter().flatten().filter(|a| a[span].is_object()) {
            out.insert(format!("{} {key}", a["label"].as_str().unwrap_or("model")), &a[span]);
        }
    }
    out
}

fn fresh(c: &Value, now: u64) -> bool {
    c["s"]["lastFetch"].as_u64().is_some_and(|at| now.saturating_sub(at) <= STALE)
}

/// Side-by-side check before cutover (D27 A3): every card and row of both payloads, fresh and within tolerance.
pub fn compare(ours: &Value, live: &Value, now: u64) -> (Vec<String>, bool) {
    let cards = |p: &'_ Value| -> BTreeMap<String, Value> {
        p["accounts"].as_array().into_iter().flatten().filter_map(|c| Some((c["id"].as_str()?.to_string(), c.clone()))).collect()
    };
    let (ours, live) = (cards(ours), cards(live));
    let (mut lines, mut failed, mut compared) = (vec![], false, 0);
    let mut fail = |lines: &mut Vec<String>, text: String| {
        failed = true;
        lines.push(format!("FAIL {text}"));
    };
    for id in ours.keys().chain(live.keys()).collect::<BTreeSet<_>>() {
        let (Some(o), Some(l)) = (ours.get(id), live.get(id)) else {
            fail(&mut lines, format!("{id}: only in {}", if ours.contains_key(id) { "dashq" } else { "the live page" }));
            continue;
        };
        for (who, c) in [("dashq", o), ("the live page", l)] {
            if !fresh(c, now) {
                fail(&mut lines, format!("{id}: {who} reading is stale or unknown"));
            }
        }
        let (o, l) = (rows(o), rows(l));
        for key in o.keys().chain(l.keys()).collect::<BTreeSet<_>>() {
            let (Some(a), Some(b)) = (o.get(key), l.get(key)) else {
                fail(&mut lines, format!("{id} {key}: only in {}", if o.contains_key(key) { "dashq" } else { "the live page" }));
                continue;
            };
            compared += 1;
            let text = format!("{id} {key}: dashq {} reset {} / live {} reset {}", a["percentUsed"], a["resetsAt"], b["percentUsed"], b["resetsAt"]);
            let near = |x: &Value, y: &Value, by: u64| x.as_u64().zip(y.as_u64()).is_some_and(|(x, y)| x.abs_diff(y) <= by);
            if near(&a["percentUsed"], &b["percentUsed"], CHECK_PERCENT) && near(&a["resetsAt"], &b["resetsAt"], CHECK_RESET) {
                lines.push(format!("ok {text}"));
            } else {
                fail(&mut lines, text);
            }
        }
    }
    let passed = !failed && compared > 0;
    lines.push(format!("limits check: {} ({compared} rows compared)", if passed { "PASS" } else { "FAIL" }));
    (lines, passed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};

    const NOW: u64 = 1_800_000_000;
    const FP_A: &str = "a1b2c3d4";
    const FP_B: &str = "0f0f0f0f";
    const FP_C: &str = "cafe0123";
    const FP_D: &str = "d00dfeed";

    fn t(s: u64) -> String {
        source::iso(s)
    }
    fn window(rt: &str, label: &str, id: &str, name: &str, used: u64, reset: u64, at: u64) -> String {
        format!("limit {rt} {label} id {id} window {name} used {used} reset {} at {}", t(reset), t(at))
    }
    fn boards(hosts: &[(&str, Vec<String>)]) -> Value {
        let hosts: Vec<Value> = hosts.iter().map(|(h, l)| json!({"title": format!("taskq host {h}"), "body": l.join("\n"), "updated_at": t(NOW)})).collect();
        json!({"org/board": {"at": NOW, "value": {"hosts": hosts}, "error": null}})
    }
    fn cfg(accounts: Value) -> Config {
        Config::parse(&json!({"url": "https://limits.example/push", "token": "invented-token", "accounts": accounts})).unwrap()
    }
    fn mapped() -> Config {
        cfg(json!([
            {"runtime": "codex", "id": FP_A, "public_id": "codex-111"},
            {"runtime": "codex", "id": FP_B, "public_id": "codex-222", "tag": "2"},
            {"runtime": "claude", "id": FP_C, "public_id": "claude"},
            {"runtime": "antigravity", "id": FP_D, "public_id": "gemini-333"},
        ]))
    }
    fn fleet() -> Value {
        boards(&[
            ("mac", vec![
                "machine mac".into(),
                format!("limit codex until {}", t(NOW + 60)),
                window("codex", "alex-kirs", FP_A, "10080m", 4, NOW + 9000, NOW - 300),
                window("codex", "alt", FP_B, "10080m", 7, NOW + 8000, NOW - 60),
                window("claude", "default", FP_C, "seven_day", 32, NOW + 7000, NOW - 60),
                window("claude", "default", FP_C, "seven_day_fable", 2, NOW + 7000, NOW - 60),
                window("antigravity", "default", FP_D, "gemini-5h", 0, NOW + 100, NOW - 60),
                window("antigravity", "default", FP_D, "gemini-weekly", 74, NOW + 200, NOW - 60),
                window("antigravity", "default", FP_D, "3p-weekly", 1, NOW + 300, NOW - 60),
            ]),
            ("pds18.example", vec![
                // the same account under another local label: the newer reading wins, the hosts merge
                window("codex", "tight", FP_A, "10080m", 6, NOW + 9000, NOW - 30),
                format!("limit claude default id {FP_C} unknown poll stale"),
                "limit claude spare id - unknown no login".into(),
            ]),
        ])
    }

    /// A1: rows of three runtimes become the Worker's existing payload shape; hosts sharing an id merge into one card.
    /// Risk: a card the page cannot draw, a split account, or a label or order change that breaks the same look.
    #[test]
    fn limits_snapshot_has_the_worker_shape_and_merges_hosts() {
        let s = snapshot(&fleet(), &mapped(), NOW).unwrap();
        assert_eq!(s, json!({"ts": NOW, "accounts": [
            {"id": "claude", "provider": "claude", "label": "cld", "s": {"status": "ok", "lastFetch": NOW - 60, "warnings": [],
                "weekly": {"percentUsed": 32, "percentLeft": 68, "resetsAt": NOW + 7000},
                "additional": [{"label": "Fable", "weekly": {"percentUsed": 2, "percentLeft": 98, "resetsAt": NOW + 7000}}]}},
            {"id": "codex-111", "provider": "codex", "label": "cod", "s": {"status": "ok", "lastFetch": NOW - 30, "warnings": [],
                "weekly": {"percentUsed": 6, "percentLeft": 94, "resetsAt": NOW + 9000}, "additional": []}},
            {"id": "codex-222", "provider": "codex", "label": "cod", "tag": "2", "s": {"status": "ok", "lastFetch": NOW - 60, "warnings": [],
                "weekly": {"percentUsed": 7, "percentLeft": 93, "resetsAt": NOW + 8000}, "additional": []}},
            {"id": "gemini-333", "provider": "gemini", "label": "agv", "s": {"status": "ok", "lastFetch": NOW - 60, "warnings": [],
                "fiveHour": {"percentUsed": 0, "percentLeft": 100, "resetsAt": NOW + 100},
                "weekly": {"percentUsed": 74, "percentLeft": 26, "resetsAt": NOW + 200},
                "additional": [
                    {"label": "Gemini Models", "fiveHour": {"percentUsed": 0, "percentLeft": 100, "resetsAt": NOW + 100}, "weekly": {"percentUsed": 74, "percentLeft": 26, "resetsAt": NOW + 200}},
                    {"label": "Claude and GPT models", "weekly": {"percentUsed": 1, "percentLeft": 99, "resetsAt": NOW + 300}}]}},
        ]}));
        // the same rows on a second board, as every project heartbeat carries them, change nothing
        let mut two = fleet();
        two["org/other"] = two["org/board"].clone();
        assert_eq!(snapshot(&two, &mapped(), NOW).unwrap(), s);
    }

    /// A1: a window name the page has no row for, and accounts with no mapping, stay visible.
    /// Risk: a new provider bucket that is the exhausted one vanishes, or two unmapped accounts share a card id.
    #[test]
    fn limits_unmapped_accounts_and_new_windows_stay_visible() {
        let b = boards(&[("mac", vec![
            window("codex", "a", FP_A, "primary", 9, NOW + 9, NOW - 1),
            window("codex", "b", FP_B, "300m", 8, NOW + 9, NOW - 1),
        ])]);
        let s = snapshot(&b, &cfg(json!([])), NOW).unwrap();
        let a = s["accounts"].as_array().unwrap();
        assert_eq!((&a[0]["id"], &a[0]["tag"]), (&json!("codex-1"), &Value::Null));
        assert_eq!(a[0]["s"]["fiveHour"]["percentUsed"], 8, "fingerprint order: {FP_B} first");
        assert_eq!((&a[1]["id"], &a[1]["tag"]), (&json!("codex-2"), &json!("2")));
        assert_eq!(a[1]["s"]["additional"][0]["label"], "Primary");
        assert_eq!(a[1]["s"]["additional"][0]["weekly"]["percentUsed"], 9);
        assert_eq!(unmapped(&b, &cfg(json!([])), NOW), [format!("codex {FP_B}"), format!("codex {FP_A}")]);
    }

    fn keys(v: &Value, path: &str, out: &mut BTreeSet<String>) {
        match v {
            Value::Object(o) => o.iter().for_each(|(k, x)| {
                let k = if path == "additional" && k == "label" { "additional.label".to_string() } else { k.clone() };
                out.insert(k.clone());
                keys(x, &k, out);
            }),
            Value::Array(a) => a.iter().for_each(|x| keys(x, path, out)),
            _ => {}
        }
    }

    /// A2: the public payload holds only short labels, percentages and resets, whatever a heartbeat line carries.
    /// Risk: an identity reaching a public page through a new field, a row field or an error reason.
    #[test]
    fn limits_payload_holds_no_identity() {
        let hostile = boards(&[("pds18.private-host.example", vec![
            "machine pds18.private-host.example".into(),
            "account codex secret-label on active".into(),
            "cli codex 1.2.3".into(),
            window("codex", "secret-label", FP_A, "10080m", 4, NOW + 9000, NOW - 60),
            format!("limit codex secret-label id {FP_B} unknown refused for someone@private.example token ghp_aaaaaaaaaaaaaaaaaaaaaaaa"),
            format!("limit codex a@private.example id {FP_C} window a@private.example used 1 reset - at {}", t(NOW)),
            format!("limit codex x id someone@private.example window 10080m used 1 reset - at {}", t(NOW)),
        ])]);
        let s = snapshot(&hostile, &cfg(json!([{"runtime": "codex", "id": FP_A, "public_id": "codex-111"}])), NOW).unwrap();
        let text = s.to_string();
        for leak in ["@", "private", "secret-label", "ghp_", "invented-token", "pds18", "refused", FP_A, FP_B, FP_C, "orgName", "email", "account\""] {
            assert!(!text.contains(leak), "{leak} leaked: {text}");
        }
        let mut found = BTreeSet::new();
        keys(&s, "", &mut found);
        let allowed = ["ts", "accounts", "id", "provider", "label", "tag", "s", "status", "lastFetch", "warnings", "fiveHour", "weekly", "additional", "percentUsed", "percentLeft", "resetsAt"];
        assert!(found.iter().all(|k| allowed.contains(&k.as_str())), "unexpected field in {found:?}");
        // a public card id can never repeat the account fingerprint
        let bad = json!({"url": "https://l.example/push", "accounts": [{"runtime": "codex", "id": FP_A, "public_id": FP_A}]});
        assert_eq!(Config::parse(&bad).err().unwrap(), "config: bad key limits_push.accounts.public_id");
    }

    /// A2 / R2: age comes from the producer, failure reads unknown and nothing is invented.
    /// Risk: a dead host's account looks fresh because DashQ pushed it just now, or a failed read blanks the page.
    #[test]
    fn limits_stale_and_unknown_accounts_stop_looking_fresh() {
        let dead = boards(&[("mac", vec![window("codex", "a", FP_A, "10080m", 4, NOW + 9, NOW - 4000)])]);
        let s = snapshot(&dead, &mapped(), NOW).unwrap();
        let by = |id: &str| s["accounts"].as_array().unwrap().iter().find(|c| c["id"] == id).unwrap().clone();
        assert_eq!(by("codex-111")["s"]["lastFetch"], NOW - 4000, "lastFetch is the producer's time, so the page marks it stale");
        for id in ["codex-222", "claude", "gemini-333"] {
            assert_eq!(by(id)["s"], json!({"status": "unknown"}), "{id}: a mapped account with no row never disappears or reads ok");
        }
        // a window a newer poll no longer names does not keep the card looking old or the old value alive
        let superseded = boards(&[("mac", vec![
            window("claude", "a", FP_C, "seven_day", 9, NOW + 9, NOW - 10),
            window("claude", "b", FP_C, "seven_day_old", 9, NOW + 9, NOW - 4000),
        ])]);
        assert_eq!(snapshot(&superseded, &cfg(json!([])), NOW).unwrap()["accounts"][0]["s"]["additional"], json!([]));
        // a future producer time is ignored instead of looking fresh for ever
        let ahead = boards(&[("mac", vec![window("codex", "a", FP_A, "10080m", 4, NOW + 9, NOW + 301)])]);
        assert!(snapshot(&ahead, &mapped(), NOW).is_none());
        // no window at all, as after a failed first read: nothing to push, the Worker's last payload ages on the page
        assert!(snapshot(&json!({"org/board": {"value": null, "error": "timeout"}}), &mapped(), NOW).is_none());
        assert!(snapshot(&boards(&[("mac", vec!["limit codex x id - unknown no login".into(), format!("limit codex x id {FP_A} unknown no login")])]), &mapped(), NOW).is_none());
        // a disabled account on its own makes no card; an id-less failure only keeps its runtime visible
        let mixed = boards(&[("mac", vec![
            window("codex", "a", FP_A, "10080m", 4, NOW + 9, NOW - 1),
            format!("limit codex off id {FP_B} unknown disabled"),
            "limit claude x id - unknown no login".into(),
            "limit claude y id - unknown no login".into(),
        ])]);
        let s = snapshot(&mixed, &cfg(json!([])), NOW).unwrap();
        assert_eq!(s["accounts"].as_array().unwrap().iter().map(|c| (c["id"].as_str().unwrap(), c["s"]["status"].as_str().unwrap())).collect::<Vec<_>>(), [("claude-1", "unknown"), ("codex-1", "ok")]);
    }

    /// One request per planned status; records head and body. The reply is plain text, as the Worker's is.
    fn stub(plan: Vec<(&'static str, &'static str)>) -> (String, std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/push", l.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(vec![]));
        let got = seen.clone();
        std::thread::spawn(move || {
            for (mut s, (status, text)) in l.incoming().flatten().zip(plan) {
                let mut r = BufReader::new(s.try_clone().unwrap());
                let (mut head, mut len) = (String::new(), 0);
                loop {
                    let mut line = String::new();
                    r.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        len = v.trim().parse().unwrap();
                    }
                    head += &line;
                }
                let mut body = vec![0; len];
                r.read_exact(&mut body).unwrap();
                got.lock().unwrap().push((head, String::from_utf8(body).unwrap()));
                write!(s, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len()).unwrap();
            }
        });
        (url, seen)
    }

    /// A1 chain through real curl: the Worker's plain-text `ok` is success, a refusal is retried, an unchanged snapshot waits.
    /// Risk: the JSON-only reader reported every accepted push as `invalid response`, or a token reaches an error or the body.
    #[test]
    fn limits_push_reaches_the_worker_route() {
        let (url, seen) = stub(vec![("401 Unauthorized", "nope"), ("200 OK", "ok"), ("200 OK", "ok")]);
        let mut c = mapped();
        c.url = url;
        let b = fleet();
        let mut p = Pusher::default();
        let refused = p.pass(&c, &b, NOW).unwrap_err();
        assert_eq!(refused, "HTTP 401");
        assert_eq!(p.pass(&c, &b, NOW + 1), Ok(true), "a refusal retries on the next pass");
        assert_eq!(p.pass(&c, &b, NOW + 2), Ok(false), "unchanged: no write");
        assert_eq!(p.pass(&c, &b, NOW + 1 + HEARTBEAT), Ok(true), "unchanged still moves ts");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        let (head, body) = &seen[1];
        assert!(head.starts_with("POST /push ") && head.contains("X-Token: invented-token"), "{head}");
        assert_eq!(serde_json::from_str::<Value>(body).unwrap()["accounts"].as_array().unwrap().len(), 4);
        assert!(!body.contains("invented-token"));
        // without a token nothing is sent: the side-by-side phase of the cutover (D27)
        let mut quiet = mapped();
        quiet.token = None;
        assert_eq!(Pusher::default().pass(&quiet, &b, NOW), Ok(false));
        assert_eq!(seen.len(), 3);
    }

    /// A3: the side-by-side check passes only when both payloads are fresh and agree, card by card and row by row.
    /// Risk: a green check that compared nothing, ignored a missing row or read a stale publisher.
    #[test]
    fn limits_check_compares_every_card_and_row() {
        let ours = snapshot(&fleet(), &mapped(), NOW).unwrap();
        let (lines, passed) = compare(&ours, &ours, NOW);
        assert!(passed, "{lines:?}");
        assert!(lines.last().unwrap().contains("PASS (9 rows compared)"), "{lines:?}");
        let change = |f: &dyn Fn(&mut Value)| {
            let mut live = ours.clone();
            f(&mut live);
            compare(&ours, &live, NOW)
        };
        for (why, broken) in [
            ("percent out of tolerance", change(&|l| l["accounts"][0]["s"]["weekly"]["percentUsed"] = json!(38))),
            ("reset out of tolerance", change(&|l| l["accounts"][1]["s"]["weekly"]["resetsAt"] = json!(NOW + 9601))),
            ("row missing", change(&|l| l["accounts"][0]["s"]["additional"] = json!([]))),
            ("card missing", change(&|l| { l["accounts"].as_array_mut().unwrap().pop(); })),
            ("live publisher stale", change(&|l| l["accounts"][1]["s"]["lastFetch"] = json!(NOW - 901))),
        ] {
            assert!(!broken.1, "{why} passed: {:?}", broken.0);
        }
        let (lines, passed) = change(&|l| l["accounts"][0]["s"]["weekly"]["percentUsed"] = json!(37));
        assert!(passed, "within tolerance: {lines:?}");
        assert!(!compare(&json!({"accounts": []}), &json!({"accounts": []}), NOW).1, "nothing compared never passes");
    }

    /// D2 / R7: a bad key is named, its value never; the path the check derives stays single-valued.
    #[test]
    fn limits_config_names_the_key_not_the_value() {
        for (v, key) in [
            (json!({"url": "http://l.example/push"}), "url"),
            (json!({"url": "https://l.example/other"}), "url"),
            (json!({"url": "https://user@l.example/push"}), "url"),
            (json!({"url": "https://l.example/push", "token": "has \"quote"}), "token"),
            (json!({"url": "https://l.example/push", "accounts": [{"runtime": "nope", "id": FP_A, "public_id": "x"}]}), "accounts.runtime"),
            (json!({"url": "https://l.example/push", "accounts": [{"runtime": "codex", "id": "someone@private.example", "public_id": "x"}]}), "accounts.id"),
            (json!({"url": "https://l.example/push", "accounts": [{"runtime": "codex", "id": FP_A, "public_id": "someone@private.example"}]}), "accounts.public_id"),
            (json!({"url": "https://l.example/push", "accounts": [{"runtime": "codex", "id": FP_A, "public_id": "x", "tag": "a@b"}]}), "accounts.tag"),
        ] {
            assert_eq!(Config::parse(&v).err().unwrap(), format!("config: bad key limits_push.{key}"));
        }
        assert!(Config::parse(&json!({"url": "https://l.example/push"})).unwrap().token.is_none());
    }
}
