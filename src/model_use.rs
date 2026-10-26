//! Model use and API-equivalent estimates (dashq.md D24, #184): per-call AgentQ `agentq.model` spans (agentq#161)
//! priced by the owner's price table, and per-runtime field coverage including TaskQ stage records (D21). An
//! estimate is never spending; a missing price, token count or adapter reads unknown with its reason, never zero.

use crate::data::{aq, safe};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::Write;

pub const LABEL: &str = "API-equivalent estimate";
/// `agentq.model` token fields and the price-table rate that prices each, per million tokens.
const FIELDS: [(&str, &str); 4] = [("input_tokens", "input"), ("output_tokens", "output"), ("cache_read_tokens", "cache_read"), ("cache_creation_tokens", "cache_write")];
const RATES: [&str; 4] = ["input", "output", "cache_read", "cache_write"];

/// The config `prices` array, checked: `Err` names the bad key, never its value (D2).
pub fn prices(v: &Value) -> Result<Value, String> {
    let list = v.as_array().filter(|a| !a.is_empty()).ok_or("config: bad key prices")?;
    let mut seen = HashSet::new();
    for (i, p) in list.iter().enumerate() {
        let bad = |k: &str| format!("config: bad key prices.{i}.{k}");
        let o = p.as_object().ok_or(bad(""))?;
        if let Some(k) = o.keys().find(|k| !["model", "source", "effective", "currency"].contains(&k.as_str()) && !RATES.contains(&k.as_str())) {
            return Err(bad(k));
        }
        let model = p["model"].as_str().filter(|m| safe(m, 64, b"-._@")).ok_or(bad("model"))?;
        p["source"].as_str().filter(|s| s.starts_with("https://") && s.len() <= 300 && s.bytes().all(|b| b.is_ascii_graphic() && !b"\"'<>\\".contains(&b))).ok_or(bad("source"))?;
        let day = p["effective"].as_str().filter(|d| d.len() == 10 && day(d).is_some()).ok_or(bad("effective"))?;
        p["currency"].as_str().filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())).ok_or(bad("currency"))?;
        for r in RATES {
            let ok = p.get(r).map(|x| x.as_f64().is_some_and(|n| n.is_finite() && n >= 0.0));
            // input and output are required; cache rates are optional, and a cache token count without its rate is unknown
            if ok == Some(false) || (ok.is_none() && matches!(r, "input" | "output")) {
                return Err(bad(r));
            }
        }
        if !seen.insert((model, day)) {
            return Err(bad("effective"));
        }
    }
    Ok(v.clone())
}

/// Epoch seconds of a `YYYY-MM-DD` day; `None` unless it round-trips (no 2026-02-30).
fn day(d: &str) -> Option<u64> {
    crate::source::iso_secs(&format!("{d}T00:00:00Z")).filter(|t| crate::source::iso(*t).starts_with(d))
}

/// A Langfuse metadata number, sent as a number or a decimal string.
fn int(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_str()?.parse().ok())
}

fn float(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str()?.parse().ok()).filter(|n| n.is_finite() && *n >= 0.0)
}

/// The newest price entry of `model` effective on or before `at`.
fn price<'a>(prices: &'a [Value], model: &str, at: u64) -> Option<&'a Value> {
    prices.iter().filter(|p| p["model"] == model && p["effective"].as_str().and_then(day).is_some_and(|d| d <= at)).max_by_key(|p| p["effective"].as_str().and_then(day))
}

#[derive(Default)]
struct Sum {
    amounts: BTreeMap<String, f64>,
    known: u64,
    of: u64,
    reasons: BTreeSet<String>,
}

impl Sum {
    fn add(&mut self, r: Result<(String, f64), String>) {
        self.of += 1;
        match r {
            Ok((cur, x)) => {
                self.known += 1;
                *self.amounts.entry(cur).or_default() += x;
            }
            Err(e) => {
                self.reasons.insert(e);
            }
        }
    }
    /// Mixed currencies stay separate subtotals; no FX conversion.
    fn json(&self) -> Value {
        let amounts: BTreeMap<&String, f64> = self.amounts.iter().map(|(c, a)| (c, (a * 1e6).round() / 1e6)).collect();
        json!({"amounts": (!amounts.is_empty()).then_some(amounts), "known": self.known, "of": self.of, "reasons": self.reasons})
    }
}

#[derive(Default)]
struct Field {
    sum: u64,
    known: u64,
    of: u64,
    reasons: BTreeSet<String>,
}

impl Field {
    fn add(&mut self, v: Option<u64>, why: &str) {
        self.of += 1;
        match v {
            Some(n) => {
                self.known += 1;
                self.sum += n;
            }
            None => {
                self.reasons.insert(why.into());
            }
        }
    }
    fn json(&self) -> Value {
        json!({"sum": (self.known > 0).then_some(self.sum), "known": self.known, "of": self.of, "reasons": self.reasons})
    }
}

/// `/data.model_use` from `inp.langfuse` `agentq.model` rows, `inp.prices` and the D21 rows of `agent_work`.
pub fn build(inp: &Value, agent_work: &Value) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let lf = &inp["langfuse"];
    let f = &inp["filters"];
    let table: Vec<Value> = inp["prices"].as_array().cloned().unwrap_or_default();
    // only a capped `agentq.model` read makes this block incomplete, not another capped Langfuse read
    let mut snap = lf.clone();
    if !snap.is_null() {
        snap["incomplete"] = json!(lf["value"]["truncated"].as_array().is_some_and(|t| t.iter().any(|x| x == "agentq.model")));
    }
    let src = crate::data::source(&snap, now, 600, None);
    // one row per host and model; a resend or a Langfuse transient duplicate shares the event id and counts once
    let mut seen = HashSet::new();
    let mut rows: BTreeMap<(String, String), (u64, BTreeMap<&str, Field>, Sum, Sum)> = BTreeMap::new();
    let mut cover: BTreeMap<&str, Field> = BTreeMap::new();
    let mut priced = Sum::default();
    for r in lf["value"]["agentq.model"].as_array().into_iter().flatten().filter(|r| !r["id"].as_str().unwrap_or("").starts_with("t-")) {
        let host = aq(r, "host.name").as_str().filter(|h| safe(h, 64, b"_-")).unwrap_or("unknown");
        let host_id = format!("{host}/{}", if aq(r, "agentq.runtime") == "wsl" { "wsl" } else { "native" });
        let keep = |k: &str, v: &str| f[k].as_str().is_none_or(|x| x == v || (k == "host" && x.eq_ignore_ascii_case(&host_id)));
        let agent = aq(r, "agentq.agent").as_str().unwrap_or("");
        if !keep("host", host) || !keep("agent", agent) || !keep("session", aq(r, "agentq.session").as_str().unwrap_or("")) {
            continue;
        }
        let id = aq(r, "agentq.event_id").as_str().or(r["id"].as_str()).unwrap_or("").to_string();
        if !seen.insert(id) {
            continue;
        }
        let at = crate::data::row_at(r).unwrap_or(now);
        let model = aq(r, "agentq.model.name").as_str().filter(|m| safe(m, 64, b"-._@"));
        let why = |k: &str| aq(r, &format!("agentq.model.{k}_unknown")).as_str().filter(|w| safe(w, 64, b" -_")).unwrap_or("not reported").to_string();
        let e = rows.entry((host.into(), model.unwrap_or("unknown").into())).or_default();
        e.0 += 1;
        cover.entry("model").or_default().add(model.map(|_| 1), &why("name"));
        let mut tokens = BTreeMap::new();
        for (k, _) in FIELDS {
            let v = int(aq(r, &format!("agentq.model.{k}")));
            e.1.entry(k).or_default().add(v, &why(k));
            cover.entry(k).or_default().add(v, &why(k));
            tokens.insert(k, v);
        }
        // the estimate: every token count known and priced, at the price effective on the call's day
        let est = (|| {
            let m = model.ok_or("model not reported")?;
            if table.is_empty() {
                return Err("no price table configured".to_string());
            }
            let p = price(&table, m, at).ok_or(format!("no price for {m}"))?;
            let mut x = 0.0;
            for (k, rate) in FIELDS {
                let n = tokens[k].ok_or(format!("{k} not reported"))?;
                if n > 0 {
                    x += n as f64 * p[rate].as_f64().ok_or(format!("no {rate} price for {m}"))? / 1e6;
                }
            }
            Ok((p["currency"].as_str().unwrap_or("").to_string(), x))
        })();
        cover.entry("price").or_default().add(est.is_ok().then_some(1), est.as_ref().err().map_or("", |e| e));
        priced.add(est.clone());
        e.2.add(est);
        // reported cost: only what the client reported, never the estimate
        let cost = float(aq(r, "agentq.model.cost")).zip(aq(r, "agentq.model.cost_currency").as_str().filter(|c| c.len() == 3));
        e.3.add(cost.map(|(x, c)| (c.to_string(), x)).ok_or_else(|| why("cost")));
    }
    let calls: u64 = rows.values().map(|r| r.0).sum();
    let rows: Vec<Value> = rows
        .iter()
        .map(|((host, model), (n, fields, est, rep))| {
            json!({"host": host, "runtime": "claude", "model": model, "calls": n,
                "tokens": fields.iter().map(|(k, v)| (k.to_string(), v.json())).collect::<serde_json::Map<_, _>>(),
                "estimate": est.json(), "reported": rep.json()})
        })
        .collect();
    // per runtime: each source's per-field known/of; only a qualified adapter can read ok (A3)
    let stages = |rts: &[&str]| -> Option<Value> {
        let rs: Vec<&Value> = agent_work["rows"].as_array().into_iter().flatten().filter(|r| r["runtime"].as_str().is_some_and(|x| rts.contains(&x))).collect();
        if rs.is_empty() {
            return None;
        }
        let mut fields = json!({});
        for (k, key) in [("model", "models"), ("context_tokens", "tokens"), ("output_tokens", "output_tokens"), ("calls", "calls")] {
            let (known, of) = rs.iter().fold((0, 0), |(a, b), r| (a + r[key]["known"].as_u64().unwrap_or(0), b + r[key]["of"].as_u64().unwrap_or(0)));
            let reasons: BTreeSet<&str> = rs.iter().flat_map(|r| r[key]["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str)).collect();
            fields[k] = json!({"known": known, "of": of, "reasons": reasons});
        }
        fields["price"] = json!({"known": 0, "of": fields["context_tokens"]["of"], "reasons": ["context tokens not split by cache"]});
        Some(json!({"source": "taskq stages", "fields": fields}))
    };
    let unknown = |why: &str| json!({"state": "unknown", "reason": why});
    let agentq = (calls > 0).then(|| json!({"source": "agentq.model", "calls": calls, "fields": cover.iter().map(|(k, v)| (k.to_string(), v.json())).collect::<serde_json::Map<_, _>>()}));
    let claude_state = match (&agentq, src["state"].as_str()) {
        (_, Some(s)) if s != "ok" => json!({"state": s, "reason": src["reason"]}),
        (Some(_), _) => json!({"state": "ok", "reason": null}),
        (None, _) => unknown("no agentq.model calls in the period"),
    };
    let runtimes = json!([
        {"runtime": "claude", "status": claude_state, "sources": agentq.into_iter().chain(stages(&["claude"])).collect::<Vec<_>>()},
        {"runtime": "codex", "status": unknown("model accounting not qualified (no model id)"), "sources": stages(&["codex", "codex-app"]).into_iter().collect::<Vec<_>>()},
        {"runtime": "hermes", "status": unknown("no model accounting adapter"), "sources": []},
    ]);
    let prices = if table.is_empty() { json!({"state": "unknown", "reason": "no price table configured", "entries": []}) } else { json!({"state": "ok", "reason": null, "entries": table}) };
    json!({"source": src, "label": LABEL, "prices": prices, "runtimes": runtimes, "rows": rows, "estimate": priced.json()})
}

pub(crate) fn subtotal(s: &Value) -> String {
    let sums: Vec<String> = s["amounts"].as_object().into_iter().flatten().map(|(c, a)| format!("{} {c}", a.as_f64().map_or("—".into(), |a| if a > 0.0 && a < 0.01 { format!("{a:.6}") } else { format!("{a:.2}") }))).collect();
    if sums.is_empty() { "—".into() } else { format!("{}{}", if s["known"] == s["of"] { "" } else { "≥" }, sums.join(" + ")) }
}

fn amounts(s: &Value) -> String {
    let reasons: Vec<&str> = s["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let why = crate::page::esc(&if reasons.is_empty() { "no priced calls read".into() } else { reasons.join("; ") });
    let total = subtotal(s);
    match (s["amounts"].as_object().is_none_or(|s| s.is_empty()), s["known"] == s["of"]) {
        (true, _) => format!("<span class=\"unknown\" title=\"unknown: {why}\">—</span> <small>unknown: {why}</small>"),
        (false, true) => total,
        (false, false) => format!("{total} <small>({} of {} calls; {why})</small>", s["known"], s["of"]),
    }
}

pub(crate) fn known(f: &Value) -> String {
    let reasons: Vec<&str> = f["reasons"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let tail = if reasons.is_empty() { String::new() } else { format!(" <small>({})</small>", crate::page::esc(&reasons.join("; "))) };
    format!("{} of {}{tail}", f["known"].as_u64().unwrap_or(0), f["of"].as_u64().unwrap_or(0))
}

pub fn render(w: &mut String, doc: &Value) {
    let m = &doc["model_use"];
    if m.is_null() {
        return;
    }
    let esc = crate::page::esc;
    let s = &m["source"];
    let _ = writeln!(w, "<section id=\"modeluse\">\n<h2>Model use</h2>\n<p class=\"note\">{LABEL}s: reported tokens × the configured list price on the call's day. Not spending: actual expenses come from billing records. Currencies are never converted.</p>");
    if s["state"] != "ok" {
        let _ = writeln!(w, "<p class=\"note\">{} {}</p>", crate::page::badge(s["state"].as_str().unwrap_or("unknown"), s["state"].as_str().unwrap_or("unknown")), esc(s["reason"].as_str().unwrap_or("")));
    }
    let p = &m["prices"];
    match p["entries"].as_array().filter(|e| !e.is_empty()) {
        None => {
            let _ = writeln!(w, "<p class=\"note\">Prices unknown: {}.</p>", esc(p["reason"].as_str().unwrap_or("")));
        }
        Some(list) => {
            let _ = writeln!(w, "<h3>Price table (per million tokens)</h3>\n<table>\n<tr><th>Model</th><th>Effective</th><th>Currency</th><th>Input</th><th>Output</th><th>Cache read</th><th>Cache write</th><th>Source</th></tr>");
            for e in list {
                let r = |k: &str| e[k].as_f64().map_or("<span class=\"unknown\" title=\"unknown: not configured\">—</span>".into(), |x| x.to_string());
                let src = e["source"].as_str().unwrap_or("");
                let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td><a href=\"{}\">{}</a></td></tr>",
                    esc(e["model"].as_str().unwrap_or("")), esc(e["effective"].as_str().unwrap_or("")), esc(e["currency"].as_str().unwrap_or("")), r("input"), r("output"), r("cache_read"), r("cache_write"), esc(src), esc(src));
            }
            let _ = writeln!(w, "</table>");
        }
    }
    let _ = writeln!(w, "<h3>Coverage per runtime (known of observed)</h3>\n<table>\n<tr><th>Runtime</th><th>State</th><th>Source</th><th>Fields</th></tr>");
    for r in m["runtimes"].as_array().into_iter().flatten() {
        let st = &r["status"];
        let state = format!("{}{}", crate::page::badge(st["state"].as_str().unwrap_or("unknown"), st["state"].as_str().unwrap_or("unknown")), st["reason"].as_str().map(|x| format!(" <small>{}</small>", esc(x))).unwrap_or_default());
        let srcs = r["sources"].as_array().filter(|x| !x.is_empty());
        let rt = esc(r["runtime"].as_str().unwrap_or(""));
        match srcs {
            None => {
                let _ = writeln!(w, "<tr><td>{rt}</td><td>{state}</td><td>—</td><td><span class=\"unknown\" title=\"unknown: no source\">—</span></td></tr>");
            }
            Some(list) => {
                for x in list {
                    let fields: Vec<String> = x["fields"].as_object().into_iter().flatten().map(|(k, v)| format!("{} {}", esc(k), known(v))).collect();
                    let _ = writeln!(w, "<tr><td>{rt}</td><td>{state}</td><td>{}</td><td>{}</td></tr>", esc(x["source"].as_str().unwrap_or("")), fields.join("<br>"));
                }
            }
        }
    }
    let _ = writeln!(w, "</table>");
    let rows = m["rows"].as_array().filter(|r| !r.is_empty());
    if let Some(rows) = rows {
        let _ = writeln!(w, "<h3>Calls per host and model · total {}</h3>\n<table>\n<tr><th>Host</th><th>Runtime</th><th>Model</th><th>Calls</th><th>Tokens (sum, known of calls)</th><th>{LABEL}</th><th>Reported cost</th></tr>", amounts(&m["estimate"]));
        for r in rows {
            let tokens: Vec<String> = r["tokens"].as_object().into_iter().flatten().map(|(k, v)| format!("{} {} <small>({} of {})</small>", esc(k), v["sum"].as_u64().map_or("—".into(), |n| n.to_string()), v["known"], v["of"])).collect();
            let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                esc(r["host"].as_str().unwrap_or("")), esc(r["runtime"].as_str().unwrap_or("")), esc(r["model"].as_str().unwrap_or("")), r["calls"], tokens.join("<br>"), amounts(&r["estimate"]), amounts(&r["reported"]));
        }
        let _ = writeln!(w, "</table>");
    } else {
        let _ = writeln!(w, "<p class=\"note\">Model calls in this period unknown: no agentq.model calls read.</p>");
    }
    let _ = writeln!(w, "</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(id: &str, host: &str, model: Option<&str>, tokens: [Option<u64>; 4], at: u64) -> Value {
        let mut m = json!({"attributes.agentq.event_id": id, "attributes.host.name": host, "attributes.agentq.runtime": "macos", "attributes.agentq.agent": "claude",
            "attributes.agentq.session": "s1", "attributes.agentq.model.cost_unknown": "not reported"});
        if let Some(x) = model {
            m["attributes.agentq.model.name"] = json!(x);
        }
        for ((k, _), v) in FIELDS.iter().zip(tokens) {
            match v {
                // Langfuse returns some numbers as strings
                Some(n) => m[format!("attributes.agentq.model.{k}")] = json!(n.to_string()),
                None => m[format!("attributes.agentq.model.{k}_unknown")] = json!("not in transcript"),
            }
        }
        json!({"id": id, "name": "agentq.model", "startTime": crate::source::iso(at), "metadata": m})
    }

    // chain: config prices -> agentq.model spans -> /data.model_use -> page. Risk: a missing price or token read as
    // zero, a duplicate counted twice, an estimate shown as spending or mixed currencies added (A1, A3, A4)
    #[test]
    fn estimates_from_priced_calls() {
        let day = 1_792_713_600; // 2026-10-23
        let table = json!([
            {"model": "claude-opus-5-5", "source": "https://www.anthropic.com/pricing", "effective": "2026-10-01", "currency": "USD", "input": 5, "output": 25, "cache_read": 0.5, "cache_write": 6.25},
            {"model": "claude-opus-5-5", "source": "https://www.anthropic.com/pricing", "effective": "2026-11-01", "currency": "USD", "input": 50, "output": 250},
            {"model": "claude-haiku-5-5", "source": "https://example.test/eur", "effective": "2026-01-01", "currency": "EUR", "input": 1, "output": 5}
        ]);
        assert!(prices(&table).is_ok());
        let mut dup = table.clone();
        dup[1]["effective"] = json!("2026-10-01");
        assert_eq!(prices(&dup).unwrap_err(), "config: bad key prices.1.effective");
        for (k, v) in [("currency", json!("usd")), ("effective", json!("2026-02-30")), ("source", json!("http://x")), ("input", json!(-1)), ("output", Value::Null)] {
            let mut bad = table.clone();
            bad[0][k] = v;
            assert_eq!(prices(&bad).unwrap_err(), format!("config: bad key prices.0.{k}"));
        }
        let calls = json!([
            call("e1", "mac", Some("claude-opus-5-5"), [Some(1_000_000), Some(100_000), Some(2_000_000), Some(0)], day),
            call("e1", "mac", Some("claude-opus-5-5"), [Some(1_000_000), Some(100_000), Some(2_000_000), Some(0)], day),
            call("e2", "mac", Some("claude-opus-5-5"), [Some(10), None, Some(0), Some(0)], day),
            call("e3", "mac", Some("claude-sonnet-5-5"), [Some(10), Some(1), Some(0), Some(0)], day),
            call("e4", "fra", Some("claude-haiku-5-5"), [Some(1_000_000), Some(0), Some(0), Some(0)], day),
            call("e5", "fra", Some("claude-haiku-5-5"), [Some(1), Some(0), Some(5), Some(0)], day),
        ]);
        let inp = |prices: Value, filters: Value| json!({"now": day + 60, "period": 1800, "filters": filters, "prices": prices,
            "langfuse": {"at": day + 60, "value": {"agentq.model": calls, "truncated": []}}});
        let aw = json!({"rows": [{"runtime": "codex", "models": {"known": 0, "of": 2, "reasons": ["not-reported"]}, "tokens": {"known": 2, "of": 2, "reasons": []},
            "output_tokens": {"known": 2, "of": 2, "reasons": []}, "calls": {"known": 2, "of": 2, "reasons": []}}]});
        let m = build(&inp(table.clone(), json!({})), &aw);
        let mac = m["rows"].as_array().unwrap().iter().find(|r| r["host"] == "mac" && r["model"] == "claude-opus-5-5").unwrap();
        // 1 M input × 5 + 0.1 M output × 25 + 2 M cache read × 0.5 = 8.5 USD; the duplicate counts once
        assert_eq!((mac["calls"].as_u64(), mac["estimate"]["amounts"]["USD"].as_f64(), mac["estimate"]["known"].as_u64(), mac["estimate"]["of"].as_u64()), (Some(2), Some(8.5), Some(1), Some(2)));
        assert_eq!(mac["estimate"]["reasons"], json!(["output_tokens not reported"]));
        assert_eq!(mac["tokens"]["output_tokens"], json!({"sum": 100000, "known": 1, "of": 2, "reasons": ["not in transcript"]}));
        assert_eq!(mac["reported"], json!({"amounts": null, "known": 0, "of": 2, "reasons": ["not reported"]}), "reported cost is never the estimate");
        let son = m["rows"].as_array().unwrap().iter().find(|r| r["model"] == "claude-sonnet-5-5").unwrap();
        assert_eq!((son["estimate"]["amounts"].clone(), son["estimate"]["reasons"].clone()), (Value::Null, json!(["no price for claude-sonnet-5-5"])), "missing price is unknown, never 0");
        let fra = m["rows"].as_array().unwrap().iter().find(|r| r["host"] == "fra").unwrap();
        assert_eq!((fra["estimate"]["amounts"]["EUR"].as_f64(), fra["estimate"]["reasons"].clone()), (Some(1.0), json!(["no cache_read price for claude-haiku-5-5"])));
        assert_eq!(m["estimate"]["amounts"], json!({"EUR": 1.0, "USD": 8.5}), "currencies stay separate");
        let rts = m["runtimes"].as_array().unwrap();
        assert_eq!((rts[0]["status"]["state"].as_str(), rts[0]["sources"][0]["fields"]["price"]["known"].as_u64(), rts[0]["sources"][0]["fields"]["price"]["of"].as_u64()), (Some("ok"), Some(2), Some(5)));
        assert_eq!((rts[1]["status"]["state"].as_str(), rts[1]["sources"][0]["fields"]["model"]["known"].as_u64(), rts[1]["sources"][0]["fields"]["price"]["known"].as_u64()), (Some("unknown"), Some(0), Some(0)));
        assert_eq!((rts[2]["runtime"].as_str(), rts[2]["status"]["state"].as_str()), (Some("hermes"), Some("unknown")), "an unqualified runtime reads unknown");
        let later = build(&inp(table.clone(), json!({"host": "fra"})), &json!({}));
        assert_eq!(later["rows"].as_array().unwrap().len(), 1, "host filter");
        let none = build(&inp(Value::Null, json!({})), &json!({}));
        assert_eq!((none["prices"]["state"].as_str(), none["estimate"]["amounts"].clone(), none["estimate"]["reasons"].clone()), (Some("unknown"), Value::Null, json!(["no price table configured"])));
        let mut html = String::new();
        render(&mut html, &json!({"model_use": m}));
        assert!(html.contains("API-equivalent estimate") && html.contains("Not spending") && html.contains("≥1.00 EUR + 8.50 USD") && html.contains("unknown: no price for claude-sonnet-5-5"), "{html}");
        assert!(!html.contains("spent") && !html.contains(">0.00"), "{html}");
    }
}
