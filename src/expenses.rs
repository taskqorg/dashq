//! Actual expenses (dashq.md D25, #185): owner-supplied billing records, never estimates. Amounts are summed exactly
//! in millionths; a higher revision supersedes, an identical record counts once, currencies stay separate subtotals
//! and only explicit rule records allocate. An invalid file reads unknown with the bad key, never a partial sum.

use crate::data::{safe, safe_id};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;

pub const LABEL: &str = "Actual expenses";
const CATEGORIES: [&str; 4] = ["api", "subscription", "infrastructure", "unknown"];

type Money = BTreeMap<String, i64>;

#[derive(Clone, PartialEq)]
struct Rec {
    id: String,
    rev: u64,
    source: String,
    period: (String, String),
    cur: String,
    amt: i64,
    cat: String,
}

/// The file at `path` with its modification time, as a source snapshot; content is checked by `build`.
pub fn read(path: &str) -> Value {
    let at = std::fs::metadata(path).ok().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs());
    match std::fs::read_to_string(path).map_err(|_| "file unreadable").and_then(|s| serde_json::from_str::<Value>(&s).map_err(|_| "invalid JSON")) {
        Ok(v) => json!({"at": at, "value": v}),
        Err(e) => json!({"error": format!("expenses: {e}")}),
    }
}

/// A decimal amount (number or string, at most 6 places, credits negative) in millionths.
fn micros(v: &Value) -> Option<i64> {
    let s = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    let (neg, digits) = s.strip_prefix('-').map_or((false, s.as_str()), |d| (true, d));
    let (int, frac) = digits.split_once('.').unwrap_or((digits, ""));
    let ok = |d: &str, max: usize| d.len() <= max && d.bytes().all(|b| b.is_ascii_digit());
    if int.is_empty() || !ok(int, 12) || !ok(frac, 6) || (digits.contains('.') && frac.is_empty()) {
        return None;
    }
    let m = int.parse::<i64>().ok()? * 1_000_000 + format!("{frac:0<6}").parse::<i64>().ok()?;
    Some(if neg { -m } else { m })
}

fn dec(m: i64) -> String {
    let s = format!("{}{}.{:06}", if m < 0 { "-" } else { "" }, m.unsigned_abs() / 1_000_000, m.unsigned_abs() % 1_000_000);
    let t = s.trim_end_matches('0');
    // at least two places: 12.50, 0.000001
    if t.len() - t.find('.').unwrap() < 3 { format!("{:.2}", m as f64 / 1e6) } else { t.to_string() }
}

fn money(m: &Money) -> Value {
    m.iter().map(|(c, a)| (c.clone(), json!(*a as f64 / 1e6))).collect::<serde_json::Map<_, _>>().into()
}

fn add(m: &mut Money, cur: &str, a: i64) {
    *m.entry(cur.into()).or_default() += a;
}

type Parsed = (Vec<Value>, Vec<Rec>, Vec<Value>);

/// Checks the file; `Err` names the bad key, never its value.
fn parse(v: &Value) -> Result<Parsed, String> {
    let bad = |k: String| format!("expenses: bad key {k}");
    if v["schema"] != 1 {
        return Err(bad("schema".into()));
    }
    let cur = |x: &Value| x.as_str().filter(|c| c.len() == 3 && c.bytes().all(|b| b.is_ascii_uppercase())).map(String::from);
    let period = |x: &Value| -> Option<(String, String)> {
        let d = |k: &str| x[k].as_str().filter(|d| d.len() == 10 && crate::source::iso_secs(&format!("{d}T00:00:00Z")).is_some_and(|t| crate::source::iso(t).starts_with(d))).map(String::from);
        d("from").zip(d("to")).filter(|(f, t)| f <= t)
    };
    let list = |k: &str, required: bool| -> Result<Vec<Value>, String> {
        match &v[k] {
            Value::Null if !required => Ok(vec![]),
            Value::Array(a) if !a.is_empty() || !required => Ok(a.clone()),
            _ => Err(bad(k.into())),
        }
    };
    let sources = list("sources", true)?;
    let mut ids = Vec::new();
    for (i, s) in sources.iter().enumerate() {
        let k = |f: &str| bad(format!("sources.{i}.{f}"));
        let id = s["id"].as_str().filter(|x| safe_id(x) && !ids.contains(x)).ok_or(k("id"))?;
        ids.push(id);
        s["provider"].as_str().filter(|x| safe(x, 64, b"-_.")).ok_or(k("provider"))?;
        s["kind"].as_str().filter(|x| matches!(*x, "export" | "invoice")).ok_or(k("kind"))?;
        period(&s["period"]).ok_or(k("period"))?;
        let t = s["totals"].as_object().filter(|t| !t.is_empty()).ok_or(k("totals"))?;
        if t.iter().any(|(c, a)| cur(&json!(c)).is_none() || micros(a).is_none()) {
            return Err(k("totals"));
        }
    }
    let mut recs: Vec<Rec> = Vec::new();
    for (i, r) in list("records", true)?.iter().enumerate() {
        let k = |f: &str| bad(format!("records.{i}.{f}"));
        let rec = Rec {
            id: r["billing_id"].as_str().filter(|x| safe_id(x)).ok_or(k("billing_id"))?.into(),
            rev: r["revision"].as_u64().filter(|n| *n >= 1).ok_or(k("revision"))?,
            source: r["source"].as_str().filter(|x| ids.contains(x)).ok_or(k("source"))?.into(),
            period: period(&r["period"]).ok_or(k("period"))?,
            cur: cur(&r["currency"]).ok_or(k("currency"))?,
            amt: micros(&r["amount"]).ok_or(k("amount"))?,
            cat: r["category"].as_str().filter(|x| CATEGORIES.contains(x)).ok_or(k("category"))?.into(),
        };
        // the same billing id and revision with other content is a conflict, not a correction
        if recs.iter().any(|x| x.id == rec.id && x.rev == rec.rev && *x != rec) {
            return Err(k("revision"));
        }
        recs.push(rec);
    }
    let rules = list("rules", false)?;
    for (i, r) in rules.iter().enumerate() {
        let k = |f: &str| bad(format!("rules.{i}.{f}"));
        r["billing_id"].as_str().filter(|x| safe_id(x)).ok_or(k("billing_id"))?;
        r["revision"].as_u64().filter(|n| *n >= 1).ok_or(k("revision"))?;
        r["target"].as_str().filter(|x| safe(x, 64, b"-_./ ")).ok_or(k("target"))?;
        micros(&r["amount"]).ok_or(k("amount"))?;
    }
    Ok((sources, recs, rules))
}

/// `/data.expenses` from `inp.expenses` (a `read` snapshot, `null` when not configured).
pub fn build(inp: &Value) -> Value {
    let now = inp["now"].as_u64().unwrap_or(0);
    let snap = &inp["expenses"];
    let parsed = if snap["value"].is_null() { None } else { Some(parse(&snap["value"])) };
    let src = match &parsed {
        Some(Err(e)) => crate::data::source(&json!({"error": e}), now, u64::MAX, None),
        _ => crate::data::source(snap, now, u64::MAX, None),
    };
    let Some(Ok((sources, all, rules))) = parsed else {
        return json!({"source": src, "label": LABEL, "records": null, "periods": null, "totals": null, "unknown_basis": null, "allocation": null, "sources": null});
    };
    let mut distinct: Vec<&Rec> = Vec::new();
    for r in &all {
        if !distinct.contains(&r) {
            distinct.push(r);
        }
    }
    let mut top: HashMap<&str, u64> = HashMap::new();
    for r in &distinct {
        let e = top.entry(&r.id).or_default();
        *e = (*e).max(r.rev);
    }
    let current: Vec<&Rec> = distinct.iter().copied().filter(|r| top[r.id.as_str()] == r.rev).collect();
    let (mut periods, mut totals, mut unknown, mut known): (BTreeMap<(String, String), (BTreeMap<String, Money>, Money)>, BTreeMap<String, Money>, Money, Money) = Default::default();
    for r in &current {
        let p = periods.entry(r.period.clone()).or_default();
        if r.cat == "unknown" {
            add(&mut p.1, &r.cur, r.amt);
            add(&mut unknown, &r.cur, r.amt);
        } else {
            add(p.0.entry(r.cat.clone()).or_default(), &r.cur, r.amt);
            add(totals.entry(r.cat.clone()).or_default(), &r.cur, r.amt);
            add(&mut known, &r.cur, r.amt);
        }
    }
    // explicit rule records only; whatever no rule takes stays unallocated
    let (mut targets, mut used, mut rejected): (BTreeMap<String, Money>, HashMap<&str, i64>, Vec<Value>) = Default::default();
    let mut unallocated = known.clone();
    for r in &rules {
        let (id, rev, target, amt) = (r["billing_id"].as_str().unwrap_or(""), r["revision"].as_u64().unwrap_or(0), r["target"].as_str().unwrap_or(""), micros(&r["amount"]).unwrap_or(0));
        let rec = current.iter().find(|x| x.id == id);
        let why = match rec {
            None => Some("no such billing id"),
            Some(x) if x.rev != rev => Some("revision superseded"),
            Some(x) if x.cat == "unknown" => Some("unknown basis is not allocated"),
            Some(x) if (used.get(id).copied().unwrap_or(0) + amt).abs() > x.amt.abs() || (amt != 0 && amt.signum() != x.amt.signum()) => Some("exceeds the record amount"),
            _ => None,
        };
        match (why, rec) {
            (None, Some(x)) => {
                *used.entry(id).or_default() += amt;
                add(targets.entry(target.into()).or_default(), &x.cur, amt);
                add(&mut unallocated, &x.cur, -amt);
            }
            (w, _) => rejected.push(json!({"billing_id": id, "revision": rev, "target": target, "reason": w})),
        }
    }
    // each source reconciles on its own distinct records, whatever later revisions supersede
    let sources: Vec<Value> = sources
        .iter()
        .map(|s| {
            let id = s["id"].as_str().unwrap_or("");
            let mut recs = Money::new();
            for r in distinct.iter().filter(|r| r.source == id) {
                add(&mut recs, &r.cur, r.amt);
            }
            let total: Money = s["totals"].as_object().into_iter().flatten().map(|(c, a)| (c.clone(), micros(a).unwrap_or(0))).collect();
            let mut diff = Money::new();
            for c in recs.keys().chain(total.keys()) {
                let d = recs.get(c).copied().unwrap_or(0) - total.get(c).copied().unwrap_or(0);
                if d != 0 {
                    diff.insert(c.clone(), d);
                }
            }
            json!({"id": id, "provider": s["provider"], "kind": s["kind"], "period": s["period"], "totals": money(&total), "records": money(&recs),
                "state": if diff.is_empty() { "reconciled" } else { "mismatch" }, "difference": money(&diff)})
        })
        .collect();
    json!({
        "source": src, "label": LABEL,
        "records": {"count": all.len(), "distinct": distinct.len(), "current": current.len(), "duplicates": all.len() - distinct.len(), "superseded": distinct.len() - current.len()},
        "periods": periods.iter().map(|((f, t), (cats, unk))| json!({"period": {"from": f, "to": t},
            "categories": cats.iter().map(|(c, m)| (c.clone(), money(m))).collect::<serde_json::Map<_, _>>(), "unknown_basis": money(unk)})).collect::<Vec<_>>(),
        "totals": totals.iter().map(|(c, m)| (c.clone(), money(m))).collect::<serde_json::Map<_, _>>(),
        "unknown_basis": money(&unknown),
        "allocation": {"targets": targets.iter().map(|(t, m)| json!({"target": t, "amounts": money(m)})).collect::<Vec<_>>(), "unallocated": money(&unallocated), "rejected": rejected},
        "sources": sources,
    })
}

fn show(m: &Value) -> String {
    let parts: Vec<String> = m.as_object().into_iter().flatten().filter_map(|(c, a)| Some(format!("{} {c}", dec((a.as_f64()? * 1e6).round() as i64)))).collect();
    if parts.is_empty() { "—".into() } else { parts.join(" + ") }
}

pub fn render(w: &mut String, doc: &Value) {
    let x = &doc["expenses"];
    if x.is_null() || x["source"]["reason"] == "not configured" {
        return;
    }
    let esc = crate::page::esc;
    let s = &x["source"];
    let _ = writeln!(w, "<section id=\"expenses\">\n<h2>{LABEL}</h2>\n<p class=\"note\">From owner-supplied billing records only; no estimate enters these subtotals. Currencies are never converted.</p>");
    if s["state"] != "ok" {
        let st = s["state"].as_str().unwrap_or("unknown");
        let _ = writeln!(w, "<p class=\"note\">{} {}</p>\n</section>", crate::page::badge(st, st), esc(s["reason"].as_str().unwrap_or("")));
        return;
    }
    let n = &x["records"];
    let _ = writeln!(w, "<p class=\"note\">{} records · {} distinct · {} current · {} duplicates counted once · {} superseded by a higher revision</p>", n["count"], n["distinct"], n["current"], n["duplicates"], n["superseded"]);
    let _ = writeln!(w, "<table>\n<tr><th>Billing period</th><th>API</th><th>Subscription</th><th>Infrastructure</th><th>Unknown basis</th></tr>");
    let cell = |m: &Value, c: &str| show(&m["categories"][c]);
    for p in x["periods"].as_array().into_iter().flatten() {
        let _ = writeln!(w, "<tr><td>{} – {}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", esc(p["period"]["from"].as_str().unwrap_or("")), esc(p["period"]["to"].as_str().unwrap_or("")),
            cell(p, "api"), cell(p, "subscription"), cell(p, "infrastructure"), show(&p["unknown_basis"]));
    }
    let t = &x["totals"];
    let _ = writeln!(w, "<tr><th>Total</th><th>{}</th><th>{}</th><th>{}</th><th>{}</th></tr>\n</table>", show(&t["api"]), show(&t["subscription"]), show(&t["infrastructure"]), show(&x["unknown_basis"]));
    let a = &x["allocation"];
    let _ = writeln!(w, "<h3>Allocation (explicit rules only)</h3>\n<table>\n<tr><th>Target</th><th>Amount</th></tr>");
    for r in a["targets"].as_array().into_iter().flatten() {
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td></tr>", esc(r["target"].as_str().unwrap_or("")), show(&r["amounts"]));
    }
    let _ = writeln!(w, "<tr><td>Unallocated</td><td>{}</td></tr>\n</table>", show(&a["unallocated"]));
    for r in a["rejected"].as_array().into_iter().flatten() {
        let _ = writeln!(w, "<p class=\"note\">Rule {} rev {} → {} not applied: {}</p>", esc(r["billing_id"].as_str().unwrap_or("")), r["revision"], esc(r["target"].as_str().unwrap_or("")), esc(r["reason"].as_str().unwrap_or("")));
    }
    let _ = writeln!(w, "<h3>Reconciliation per source</h3>\n<table>\n<tr><th>Source</th><th>Provider</th><th>Kind</th><th>Period</th><th>Source total</th><th>Records</th><th>State</th></tr>");
    for r in x["sources"].as_array().into_iter().flatten() {
        let st = r["state"].as_str().unwrap_or("");
        let state = if st == "reconciled" { crate::page::badge("ok", "reconciled") } else { format!("{} <small>difference {}</small>", crate::page::badge("warn", "mismatch"), show(&r["difference"])) };
        let _ = writeln!(w, "<tr><td>{}</td><td>{}</td><td>{}</td><td>{} – {}</td><td>{}</td><td>{}</td><td>{state}</td></tr>", esc(r["id"].as_str().unwrap_or("")), esc(r["provider"].as_str().unwrap_or("")),
            esc(r["kind"].as_str().unwrap_or("")), esc(r["period"]["from"].as_str().unwrap_or("")), esc(r["period"]["to"].as_str().unwrap_or("")), show(&r["totals"]), show(&r["records"]));
    }
    let _ = writeln!(w, "</table>\n</section>");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> Value {
        serde_json::from_str(include_str!("../fixture/expenses.json")).unwrap()
    }

    // chain: owner billing file -> /data.expenses -> page. Risk: a correction or duplicate counted twice, currencies
    // added together, an unknown basis or unruled amount allocated, a source total silently not matching (A1-A5)
    #[test]
    fn expenses_from_billing_records() {
        let m = build(&json!({"now": 100, "expenses": {"at": 90, "value": file()}}));
        assert_eq!(m["source"]["state"], "ok");
        assert_eq!(m["records"], json!({"count": 6, "distinct": 5, "current": 4, "duplicates": 1, "superseded": 1}));
        // sub-1 revision 2 (a -5 USD credit) supersedes revision 1; EUR is never added to USD
        assert_eq!(m["totals"], json!({"api": {"USD": 100.1}, "infrastructure": {"EUR": 39.9}, "subscription": {"USD": -5.0}}));
        assert_eq!(m["unknown_basis"], json!({"USD": 10.0}), "unknown basis stays its own bucket");
        let a = &m["allocation"];
        assert_eq!(a["targets"], json!([{"target": "taskqorg/dashq", "amounts": {"USD": 60.0}}]));
        assert_eq!(a["unallocated"], json!({"EUR": 39.9, "USD": 35.1}));
        let why: Vec<&str> = a["rejected"].as_array().unwrap().iter().map(|r| r["reason"].as_str().unwrap()).collect();
        assert_eq!(why, ["exceeds the record amount", "revision superseded", "unknown basis is not allocated"]);
        // the Anthropic invoice reconciles on its own records: 100.10 once + 30 (revision 1) = 130.10
        let s = m["sources"].as_array().unwrap();
        assert_eq!((s[0]["state"].as_str(), s[1]["state"].as_str()), (Some("reconciled"), Some("reconciled")));
        assert_eq!((s[2]["state"].as_str(), s[2]["difference"].clone()), (Some("mismatch"), json!({"USD": -1.0})), "{}", s[2]);
        let mut html = String::new();
        render(&mut html, &json!({"expenses": m}));
        assert!(html.contains("Actual expenses") && html.contains("100.10 USD") && html.contains("39.90 EUR") && html.contains("mismatch") && html.contains("Unallocated</td><td>39.90 EUR + 35.10 USD"), "{html}");
        // invalid input is unknown with the key, never a partial sum
        for (k, f) in [("records.0.amount", json!("1.0000001")), ("records.0.currency", json!("usd")), ("records.0.source", json!("nope")), ("records.0.revision", json!(0))] {
            let mut bad = file();
            bad["records"][0][k.rsplit('.').next().unwrap()] = f;
            let m = build(&json!({"now": 100, "expenses": {"at": 90, "value": bad}}));
            assert_eq!((m["source"]["state"].as_str(), m["source"]["reason"].as_str(), m["totals"].clone()), (Some("unknown"), Some(format!("read failed (expenses: bad key {k})").as_str()), Value::Null));
        }
        let mut conflict = file();
        conflict["records"][1]["amount"] = json!("100.11");
        assert_eq!(build(&json!({"expenses": {"value": conflict}}))["source"]["reason"], "read failed (expenses: bad key records.1.revision)");
        assert_eq!(build(&json!({}))["source"]["reason"], "not configured");
        assert_eq!((micros(&json!("12.5")), micros(&json!("-0.000001")), micros(&json!(1e-7)), micros(&json!("1.")), dec(12_500_000), dec(1)), (Some(12_500_000), Some(-1), None, None, "12.50".into(), "0.000001".into()));
    }
}
