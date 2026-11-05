//! D28: the reader of the global observation store. Every answer is computed from the stored records at read time;
//! nothing here is stored, and prices never enter a record.
use crate::model_use::{LABEL, Sum, estimate};
use crate::store::Observation;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;

pub const SCHEMA: &str = "coreq-observation/1";
const VERSION: u64 = 1;
const EXPORT: &str = "dashq-observations/1";
const PARITY: &str = "dashq-parity/1";
/// Rows of one answer in `/data`; `total` keeps the exact count.
const ROWS: usize = 200;
/// Stage records before the window that still count as an attempt's evidence, as D21 reads two days.
pub const EARLIER_SECS: i64 = 2 * 86400;
/// The CSV columns taken from the shared record, after the store's own.
const FIELDS: [&str; 26] = [
    "kind", "operation", "outcome", "error", "started_ms", "duration_us", "project", "task", "attempt", "actor", "entry", "pipeline", "pipeline_revision", "step", "units", "unit_kind",
    "disk_bytes", "runtime", "runtime_version", "host", "product", "model", "input_tokens", "cache_read_tokens", "cache_write_tokens", "output_tokens",
];

/// One stored record, decoded; a record that no longer decodes has a null `value`.
pub struct Line {
    pub row: Observation,
    pub value: Value,
}

impl Line {
    pub fn new(row: Observation) -> Line {
        let value = serde_json::from_str(&row.payload).unwrap_or(Value::Null);
        Line { row, value }
    }
    fn o(&self) -> &Value {
        &self.value["observation"]
    }
    fn product(&self) -> &str {
        self.o()["product"].as_str().unwrap_or(&self.row.product)
    }
    /// Producer time in seconds.
    fn at(&self) -> i64 {
        self.o()["started_ms"].as_i64().map_or(self.row.observed, |ms| ms / 1000)
    }
}

/// Calls, duration and outcome of a group of one record kind ([`grouped`], the `work` rows), since `agent.turn` and
/// `agent.call` describe the same tokens and time; a missing duration stays unknown, never zero.
#[derive(Default)]
struct Tally {
    calls: u64,
    us: u64,
    known: u64,
    max: Option<u64>,
    outcomes: BTreeMap<String, u64>,
    units: BTreeMap<String, u64>,
    last: i64,
}

impl Tally {
    fn add(&mut self, l: &Line) {
        let o = l.o();
        self.calls += 1;
        if let Some(us) = o["duration_us"].as_u64() {
            self.known += 1;
            self.us = self.us.saturating_add(us);
            self.max = Some(self.max.map_or(us, |m| m.max(us)));
        }
        *self.outcomes.entry(o["outcome"].as_str().unwrap_or("unknown").into()).or_default() += 1;
        if let (Some(u), Some(k)) = (o["units"].as_u64(), o["unit_kind"].as_str()) {
            *self.units.entry(k.into()).or_default() += u;
        }
        self.last = self.last.max(l.at());
    }
    fn json(&self) -> Value {
        json!({"calls": self.calls, "duration_us": {"sum": (self.known > 0).then_some(self.us), "max": self.max, "known": self.known, "of": self.calls}, "outcomes": self.outcomes, "units": self.units, "last_at": self.last})
    }
}

/// One question's answer: its rows, newest first and at most [`ROWS`], or unknown with the reason (specq Telemetry by
/// design: "an answer, or unknown with a reason").
fn answer(mut rows: Vec<Value>, why: &str) -> Value {
    rows.sort_by_key(|r| std::cmp::Reverse(r["last_at"].as_i64().unwrap_or(0)));
    let total = rows.len();
    rows.truncate(ROWS);
    json!({"state": if total == 0 { "unknown" } else { "ok" }, "reason": (total == 0).then_some(why), "rows": rows, "total": total, "capped": total > ROWS})
}

fn grouped<'a>(lines: impl Iterator<Item = &'a Line>, key: impl Fn(&Line) -> Value) -> BTreeMap<String, (Value, Tally)> {
    let mut groups: BTreeMap<String, (Value, Tally)> = BTreeMap::new();
    for l in lines {
        let mut k = key(l);
        k["kind"] = l.o()["kind"].clone();
        groups.entry(k.to_string()).or_insert_with(|| (k, Tally::default())).1.add(l);
    }
    groups
}

/// A group's key object with the tally's fields next to it.
fn row((key, tally): (Value, Tally)) -> Value {
    let mut r = tally.json();
    if let (Some(r), Some(k)) = (r.as_object_mut(), key.as_object()) {
        r.extend(k.clone());
    }
    r
}

/// The one errors query (D28): every failure, unstable operation and attempt without a terminal record in the window,
/// by product, operation and error class; `refused` is counted apart. `earlier` holds the stage records just before
/// the window, evidence only: an attempt is listed for a record inside it.
pub fn errors(lines: &[Line], earlier: &[Line]) -> Value {
    let mut rows: BTreeMap<(&str, &str, &str, &str), (u64, i64)> = BTreeMap::new();
    let mut refused = 0u64;
    for l in lines {
        let o = l.o();
        let class = match o["outcome"].as_str() {
            Some("ok" | "cancelled" | "ask") => continue,
            Some("refused") => {
                refused += 1;
                continue;
            }
            Some("failed") => "failure",
            Some("retried") => "unstable",
            // a span's opening marker is no result; its missing terminal stays with the span
            _ if o["kind"] == "method.start" => continue,
            _ => "unknown",
        };
        let e = rows.entry((class, l.product(), o["operation"].as_str().unwrap_or("unknown"), o["error"].as_str().unwrap_or("unknown"))).or_default();
        e.0 += 1;
        e.1 = e.1.max(l.at());
    }
    let identity = |l: &Line| {
        let o = l.o();
        (o["kind"] == "stage" && o["task"].is_u64() && o["attempt"].is_u64()).then(|| json!([l.row.installation, l.row.workspace, o["project"], o["task"], o["attempt"]]).to_string())
    };
    let mapped = |l: &Line| json!({"stage": l.o()["operation"], "at_ms": l.o()["started_ms"], "open": l.o()["open"]});
    let mut attempts: BTreeMap<String, (&Line, Vec<Value>)> = BTreeMap::new();
    for l in lines {
        if let Some(k) = identity(l) {
            let e = attempts.entry(k).or_insert_with(|| (l, vec![]));
            if l.at() > e.0.at() {
                e.0 = l;
            }
            e.1.push(mapped(l));
        }
    }
    for l in earlier {
        if let Some(e) = identity(l).and_then(|k| attempts.get_mut(&k)) {
            e.1.push(mapped(l));
        }
    }
    let mut open: Vec<Value> = attempts
        .values()
        .filter_map(|(l, recs)| {
            let why = crate::agent_work::terminal_gap(&recs.iter().collect::<Vec<_>>())?;
            let o = l.o();
            Some(json!({"product": l.product(), "installation": l.row.installation, "workspace": l.row.workspace, "project": o["project"], "task": o["task"], "attempt": o["attempt"], "host": o["host"], "reason": why, "last_at": l.at()}))
        })
        .collect();
    open.sort_by_key(|r| std::cmp::Reverse(r["last_at"].as_i64().unwrap_or(0)));
    let total = open.len();
    open.truncate(ROWS);
    let mut listed: Vec<Value> = rows.into_iter().map(|((class, product, operation, error), (count, last))| json!({"class": class, "product": product, "operation": operation, "error": error, "count": count, "last_at": last})).collect();
    listed.sort_by_key(|r| std::cmp::Reverse(r["last_at"].as_i64().unwrap_or(0)));
    json!({"rows": listed, "attempts": open, "attempts_total": total, "refused": refused})
}

/// API-equivalent estimate at read time (D24, D28): `agent.call` only, since `agent.turn` describes the same tokens.
fn cost(lines: &[Line], prices: &[Value]) -> Value {
    let mut total = Sum::default();
    let mut models: BTreeMap<String, (u64, Sum)> = BTreeMap::new();
    for l in lines.iter().filter(|l| l.o()["kind"] == "agent.call") {
        let o = l.o();
        let model = o["model"].as_str();
        let at = l.at().max(0) as u64;
        let est = if o["unit_kind"].as_str().is_some_and(|k| k != "tokens") {
            Err("provider credits are not priced".to_string())
        } else {
            let n = |k: &str| o[k].as_u64();
            estimate(prices, model, at, &[("input_tokens", "input", n("input_tokens")), ("output_tokens", "output", n("output_tokens")), ("cache_read_tokens", "cache_read", n("cache_read_tokens")), ("cache_write_tokens", "cache_write", n("cache_write_tokens"))])
        };
        total.add(est.clone());
        let e = models.entry(model.unwrap_or("unknown").into()).or_default();
        e.0 += 1;
        e.1.add(est);
    }
    json!({"label": LABEL, "estimate": total.json(), "models": models.iter().map(|(m, (calls, s))| json!({"model": m, "calls": calls, "estimate": s.json()})).collect::<Vec<_>>()})
}

/// `agent.turn` and `agent.call` describe the same tokens: one object each, no field adds them.
fn totals(lines: &[Line]) -> Value {
    let kind = |k: &str| {
        let rows: Vec<&Value> = lines.iter().filter(|l| l.o()["kind"] == k).map(Line::o).collect();
        json!({"records": rows.len(), "tokens": crate::fleet::tokens(&rows)})
    };
    json!({"agent.turn": kind("agent.turn"), "agent.call": kind("agent.call")})
}

/// `/data.observations` without its `source` and `store`: one read of the window's records. `earlier`: see [`errors`].
pub fn query(lines: &[Line], earlier: &[Line], prices: &[Value], from: i64, to: i64) -> Value {
    let kind = |k: &'static str| lines.iter().filter(move |l| l.o()["kind"] == k);
    let operations = grouped(lines.iter(), |l| json!({"product": l.product(), "operation": l.o()["operation"]}));
    let work = lines.iter().filter(|l| !l.o()["task"].is_null() && matches!(l.o()["kind"].as_str(), Some("agent.turn" | "agent.call" | "stage")));
    // `agent.turn`, `agent.call` and `stage` each keep their own tally and tokens: no field of a row adds them
    let mut who: BTreeMap<String, (Value, BTreeMap<&str, (Tally, Vec<&Value>)>)> = BTreeMap::new();
    for l in work {
        let o = l.o();
        let k = json!({"project": o["project"], "task": o["task"], "attempt": o["attempt"], "actor": o["actor"], "product": l.product()});
        let e = who.entry(k.to_string()).or_insert_with(|| (k, BTreeMap::new())).1.entry(o["kind"].as_str().unwrap_or("")).or_default();
        e.0.add(l);
        e.1.push(o);
    }
    let work: Vec<Value> = who
        .into_values()
        .map(|(mut r, kinds)| {
            r["last_at"] = json!(kinds.values().map(|(t, _)| t.last).max());
            for (field, kind) in [("turns", "agent.turn"), ("calls", "agent.call"), ("stages", "stage")] {
                let (tally, records) = kinds.get(kind).map_or((Tally::default().json(), &[][..]), |(t, o)| (t.json(), &o[..]));
                r[field] = tally;
                r[field]["records"] = json!(records.len());
                r[field]["tokens"] = crate::fleet::tokens(records);
            }
            r
        })
        .collect();
    let board = lines.iter().filter(|l| l.o()["operation"].as_str().is_some_and(|o| o.starts_with("board.")));
    let board = grouped(board, |l| json!({"product": l.product(), "host": l.o()["host"], "task": l.o()["task"]}));
    let stages = grouped(kind("stage"), |l| json!({"stage": l.o()["operation"]}));
    let mut waits: BTreeMap<String, (u64, u64, u64, i64)> = BTreeMap::new();
    for l in kind("transition") {
        let o = l.o();
        let e = waits.entry(o["from"].as_str().unwrap_or("unknown").into()).or_default();
        e.1 += 1;
        if let Some(s) = o["previous_seconds"].as_u64() {
            e.0 += s;
            e.2 += 1;
        }
        e.3 = e.3.max(l.at());
    }
    let waits: Vec<Value> = waits.into_iter().map(|(from, (sum, of, known, last))| json!({"from": from, "wait_s": (known > 0).then_some(sum), "known": known, "of": of, "last_at": last})).collect();
    let pipelines = grouped(lines.iter().filter(|l| !l.o()["pipeline"].is_null()), |l| json!({"pipeline": l.o()["pipeline"], "pipeline_revision": l.o()["pipeline_revision"], "step": l.o()["step"], "method": l.o()["operation"]}));
    let rows = |g: BTreeMap<String, (Value, Tally)>| g.into_values().map(row).collect::<Vec<_>>();
    json!({
        "window": {"from": from, "to": to, "seconds": to - from},
        "records": lines.len(),
        "unreadable": lines.iter().filter(|l| l.value.is_null()).count(),
        "newest_observed": lines.iter().map(Line::at).max(),
        "totals": totals(lines),
        "cost": cost(lines, prices),
        "errors": errors(lines, earlier),
        "operations": answer(rows(operations), "no observations in window"),
        "work": answer(work, "no task records in window"),
        "board_calls": answer(rows(board), "no board.* records in window"),
        "stages": answer(rows(stages), "no stage records in window"),
        "queue_waits": answer(waits, "no transition records in window"),
        "pipelines": answer(rows(pipelines), "no pipeline records in window"),
    })
}

fn csv(v: &Value) -> String {
    let s = match v {
        Value::Null => return String::new(),
        Value::String(s) => s.clone(),
        v => v.to_string(),
    };
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s }
}

/// The export header: one schema version per file, stating the window it covers.
pub fn export_header(w: &mut impl Write, format: &str, from: i64, to: i64, rows: u64) -> std::io::Result<()> {
    if format == "csv" {
        writeln!(w, "# export={EXPORT} schema={SCHEMA} version={VERSION} from={from} to={to} rows={rows}")?;
        let store = ["installation", "workspace", "generation", "grant_product", "received", "seq", "id", "received_ms"];
        return writeln!(w, "{},{},inventory,unknown", store.join(","), FIELDS.join(","));
    }
    writeln!(w, "{}", json!({"export": EXPORT, "schema": SCHEMA, "version": VERSION, "from": from, "to": to, "rows": rows, "format": "jsonl"}))
}

/// One exported record.
pub fn export_row(w: &mut impl Write, format: &str, l: &Line) -> std::io::Result<()> {
    let r = &l.row;
    let (seq, id, received_ms) = (&l.value["seq"], &l.value["id"], &l.value["received_ms"]);
    if format == "csv" {
        let store = [json!(r.installation), json!(r.workspace), json!(r.generation), json!(r.product), json!(r.received), seq.clone(), id.clone(), received_ms.clone()];
        let cells: Vec<String> = store.iter().map(csv).chain(FIELDS.iter().map(|f| csv(&l.o()[*f]))).chain([csv(&l.o()["inventory"]), csv(&l.o()["unknown"])]).collect();
        return writeln!(w, "{}", cells.join(","));
    }
    writeln!(w, "{}", json!({"installation": r.installation, "workspace": r.workspace, "generation": r.generation, "product": r.product, "received": r.received, "seq": seq, "id": id, "received_ms": received_ms, "observation": l.o()}))
}

/// One compared path: `parity` only with old evidence and nothing missing from the new path (R2).
fn compared(product: &str, comparison: &str, old_path: &str, new_path: &str, old: Result<u64, String>, new: u64, matched: u64) -> Value {
    let (state, reason, old, only_old) = match old {
        Err(why) => ("unknown", Some(why), Value::Null, Value::Null),
        Ok(0) => ("unknown", Some("old path has no records in this window".to_string()), json!(0), json!(0)),
        Ok(n) if n > matched => ("gap", Some(format!("{} old records missing from the new path", n - matched)), json!(n), json!(n - matched)),
        Ok(n) => ("parity", None, json!(n), json!(0)),
    };
    json!({"product": product, "state": state, "reason": reason, "comparison": comparison, "old_path": old_path, "new_path": new_path, "old": old, "new": new, "matched": old.as_u64().map(|_| matched), "only_old": only_old, "only_new": old.as_u64().map(|_| new - matched)})
}

/// The old paths as DashQ reads them, or why they cannot be read.
pub struct OldPaths {
    /// Observation ids in TaskQ's `taskq-telemetry/1` blocks.
    pub taskq: Result<BTreeSet<String>, String>,
    /// Langfuse `agentq.model` spans.
    pub agentq: Result<u64, String>,
    /// Per-rule results of `serverq-report/1` history.
    pub serverq: Result<u64, String>,
}

/// `dashq parity` (D28): old and new paths per product for one window. Langfuse stays until this reads `parity`.
pub fn parity(new: &[Line], old: OldPaths, from: i64, to: i64) -> Value {
    let ids: BTreeSet<&str> = new.iter().filter(|l| l.row.product == "taskq").filter_map(|l| l.value["id"].as_str()).collect();
    let matched = old.taskq.as_ref().map_or(0, |o| o.iter().filter(|i| ids.contains(i.as_str())).count() as u64);
    let taskq = compared("taskq", "observation id", "taskq-telemetry/1 blocks", "coreq-observation/1 of TaskQ grants", old.taskq.map(|o| o.len() as u64), ids.len() as u64, matched);
    let calls = new.iter().filter(|l| l.o()["kind"] == "agent.call").count() as u64;
    let agentq = compared("agentq", "count", "Langfuse agentq.model", "agent.call of every grant", old.agentq.clone(), calls, old.agentq.as_ref().map_or(0, |o| calls.min(*o)));
    let checks = new.iter().filter(|l| l.row.product == "serverq" && l.o()["kind"] == "check").count() as u64;
    let serverq = compared("serverq", "count", "serverq-report/1 rule results", "check of ServerQ grants", old.serverq.clone(), checks, old.serverq.as_ref().map_or(0, |o| checks.min(*o)));
    let parity = [&taskq, &agentq, &serverq].iter().all(|r| r["state"] == "parity");
    let count = |p: &str| new.iter().filter(|l| l.row.product == p).count();
    let mut products = vec![taskq, agentq, serverq];
    for p in ["pilotq", "assetq"] {
        products.push(json!({"product": p, "state": "unknown", "reason": "DashQ reads no old path of this product", "old": null, "new": count(p)}));
    }
    for p in ["coreq", "dashq"] {
        products.push(json!({"product": p, "state": "not_applicable", "reason": "no separate old path", "old": null, "new": count(p)}));
    }
    json!({"report": PARITY, "window": {"from": from, "to": to, "seconds": to - from}, "products": products, "parity": parity,
        "langfuse": {"stays": !parity, "reason": if parity { "parity shown; deleting Langfuse is a later task citing this report" } else { "no parity yet for taskq, agentq and serverq in this window" }}})
}

/// The Langfuse `agentq.model` count of one `source::langfuse` read: one event id counts once, like D24.
pub fn langfuse_calls(read: &Value) -> Result<u64, String> {
    if read["truncated"].as_array().is_some_and(|t| t.iter().any(|x| x == "agentq.model")) {
        return Err("Langfuse agentq.model read capped".into());
    }
    let rows = read["agentq.model"].as_array().ok_or("Langfuse read has no agentq.model")?;
    let ids: BTreeSet<&str> = rows.iter().filter(|r| !r["id"].as_str().unwrap_or("").starts_with("t-")).filter_map(|r| crate::data::aq(r, "agentq.event_id").as_str().or(r["id"].as_str())).collect();
    Ok(ids.len() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(installation: &str, product: &str, o: Value) -> Line {
        let observed = o["started_ms"].as_i64().unwrap_or(0) / 1000;
        let id = format!("{installation}-{}-{}-{}-{}", o["kind"].as_str().unwrap_or(""), o["operation"].as_str().unwrap_or(""), o["started_ms"], o["attempt"]);
        let value = json!({"seq": 1, "id": id, "received_ms": observed * 1000, "observation": o});
        Line { row: Observation { installation: installation.into(), workspace: "w".into(), generation: 1, product: product.into(), observed, received: observed, payload: value.to_string() }, value }
    }

    fn base(kind: &str, operation: &str, outcome: &str, error: &str, at: i64) -> Value {
        json!({"schema": 1, "product": "taskq", "kind": kind, "operation": operation, "outcome": outcome, "error": error, "started_ms": at * 1000, "duration_us": 2000, "project": "o/r", "task": 7, "attempt": 3, "actor": "worker", "host": "mac", "runtime": "claude"})
    }

    fn call(model: Option<&str>, tokens: [Option<u64>; 4], at: i64) -> Value {
        let mut o = base("agent.call", "agent.call", "ok", "none", at);
        o["product"] = json!("agentq");
        o["model"] = json!(model);
        for (k, n) in ["input_tokens", "output_tokens", "cache_read_tokens", "cache_write_tokens"].iter().zip(tokens) {
            o[*k] = json!(n);
        }
        o
    }

    fn prices(input: f64) -> Vec<Value> {
        vec![json!({"model": "m1", "source": "https://example.invalid/p", "effective": "2020-01-01", "currency": "USD", "input": input, "output": 2.0, "cache_read": 0.5, "cache_write": 1.0})]
    }

    const AT: i64 = 1_800_000_000;

    // Defect: without a read-time table a price change would rewrite records. Protects D28 cost and R2 unknowns.
    #[test]
    fn cost_is_computed_at_read_time_and_turns_are_never_added_to_calls() {
        let mut turn = base("agent.turn", "agent.turn", "ok", "none", AT);
        turn["input_tokens"] = json!(1000);
        let lines = [
            line("i", "taskq", call(Some("m1"), [Some(1_000_000), Some(1_000_000), Some(0), Some(0)], AT)),
            line("i", "taskq", call(Some("m9"), [Some(1), Some(1), Some(0), Some(0)], AT + 1)),
            line("i", "taskq", call(Some("m1"), [None, Some(1), Some(0), Some(0)], AT + 2)),
            line("i", "taskq", turn),
        ];
        let stored: Vec<String> = lines.iter().map(|l| l.row.payload.clone()).collect();
        let cheap = query(&lines, &[], &prices(1.0), AT - 10, AT + 10);
        let dear = query(&lines, &[], &prices(4.0), AT - 10, AT + 10);
        assert_eq!(cheap["cost"]["label"], "API-equivalent estimate");
        assert_eq!(cheap["cost"]["estimate"]["amounts"]["USD"], 3.0);
        assert_eq!(dear["cost"]["estimate"]["amounts"]["USD"], 6.0, "a new price needs no rewrite");
        assert_eq!((&cheap["cost"]["estimate"]["known"], &cheap["cost"]["estimate"]["of"]), (&json!(1), &json!(3)), "the turn is never priced");
        let reasons = cheap["cost"]["estimate"]["reasons"].to_string();
        assert!(reasons.contains("no price for m9") && reasons.contains("input_tokens not reported"), "{reasons}");
        assert_eq!(stored, lines.iter().map(|l| l.row.payload.clone()).collect::<Vec<_>>());
        assert!(!stored.iter().any(|p| p.contains("price") || p.contains("USD")));
        // the same tokens stay in two objects with no sum across them
        assert_eq!(cheap["totals"]["agent.turn"]["tokens"]["input_tokens"]["observed"], 1000);
        assert_eq!(cheap["totals"]["agent.call"]["tokens"]["input_tokens"]["observed"], 1_000_001);
        assert_eq!(cheap["totals"].as_object().unwrap().len(), 2);
        let none = query(&lines, &[], &[], AT - 10, AT + 10);
        assert!(none["cost"]["estimate"]["amounts"].is_null());
        assert!(none["cost"]["estimate"]["reasons"].to_string().contains("no price table configured"));
        let mut credits = call(Some("m1"), [Some(1), Some(1), Some(0), Some(0)], AT);
        credits["unit_kind"] = json!("credits");
        let c = query(&[line("i", "taskq", credits)], &[], &prices(1.0), AT - 10, AT + 10);
        assert!(c["cost"]["estimate"]["reasons"].to_string().contains("provider credits are not priced"));
    }

    // Defect: `agent.turn` and `agent.call` describe the same tokens and time, so one row adding them read 370 tokens for
    // 185 (review of attempt 6663625241). Protects A3 in every group, including one operation name used by both kinds.
    #[test]
    fn turn_and_call_units_and_time_are_never_added_in_any_group() {
        let same = |kind: &str| {
            let mut o = base(kind, "agent.run", "ok", "none", AT);
            o["units"] = json!(185);
            o["unit_kind"] = json!("tokens");
            o["pipeline"] = json!("p1");
            o["input_tokens"] = json!(185);
            o
        };
        let lines = [line("i", "taskq", same("agent.turn")), line("i", "taskq", same("agent.call")), line("i", "taskq", base("stage", "build", "ok", "none", AT))];
        let q = query(&lines, &[], &[], AT - 10, AT + 10);
        assert_eq!(q["work"]["total"], 1, "one project, task, attempt and actor");
        let who = &q["work"]["rows"][0];
        for k in ["turns", "calls"] {
            assert_eq!((&who[k]["records"], &who[k]["units"]["tokens"], &who[k]["duration_us"]["sum"], &who[k]["tokens"]["input_tokens"]["observed"]), (&json!(1), &json!(185), &json!(2000), &json!(185)), "{k}: {who}");
        }
        assert_eq!((&who["stages"]["records"], &who["stages"]["duration_us"]["sum"]), (&json!(1), &json!(2000)));
        assert!(who.get("units").is_none() && who.get("duration_us").is_none() && who.get("outcomes").is_none(), "no field adds the kinds: {who}");
        assert_eq!(who["last_at"], AT);
        for group in ["operations", "pipelines"] {
            let rows: Vec<&Value> = q[group]["rows"].as_array().unwrap().iter().filter(|r| r["units"]["tokens"].is_number()).collect();
            assert_eq!(rows.len(), 2, "{group}: {}", q[group]);
            assert!(rows.iter().all(|r| r["units"]["tokens"] == 185 && r["duration_us"]["sum"] == 2000), "{group}: {rows:?}");
        }
    }

    // Defect: a failure could hide in a separate detector, or `refused` read as failure. Protects the one errors query.
    #[test]
    fn one_query_lists_failures_unstable_operations_and_open_attempts() {
        let open_only = {
            let mut o = base("stage", "claim", "ok", "none", AT);
            o["attempt"] = json!(4);
            o
        };
        let earlier_build = {
            let mut o = base("stage", "build", "ok", "none", AT - 100);
            o["attempt"] = json!(5);
            o
        };
        let late_close = {
            let mut o = base("stage", "close", "ok", "none", AT);
            o["attempt"] = json!(5);
            o
        };
        let lines = [
            line("i", "taskq", base("operation", "board.issue.read", "failed", "timeout", AT)),
            line("i", "taskq", base("operation", "board.issue.read", "failed", "timeout", AT + 1)),
            line("i", "taskq", base("service.pass", "sync.pass", "retried", "timeout", AT)),
            line("i", "taskq", base("operation", "gate", "refused", "cooldown", AT)),
            line("i", "taskq", base("operation", "fine", "ok", "none", AT)),
            line("i", "taskq", base("operation", "stopped", "cancelled", "none", AT)),
            line("i", "taskq", base("method.start", "m", "unknown", "none", AT)),
            line("i", "taskq", base("operation", "lost", "unknown", "none", AT)),
            line("i", "taskq", base("stage", "build", "failed", "timeout", AT)),
            line("i", "taskq", open_only),
            line("i", "taskq", late_close),
        ];
        let e = errors(&lines, &[line("i", "taskq", earlier_build)]);
        let got: Vec<(String, String, u64)> = e["rows"].as_array().unwrap().iter().map(|r| (r["class"].as_str().unwrap().into(), r["operation"].as_str().unwrap().into(), r["count"].as_u64().unwrap())).collect();
        assert!(got.contains(&("failure".into(), "board.issue.read".into(), 2)), "{got:?}");
        assert!(got.contains(&("unstable".into(), "sync.pass".into(), 1)));
        assert!(got.contains(&("unknown".into(), "lost".into(), 1)));
        assert!(!got.iter().any(|(_, o, _)| ["gate", "fine", "stopped", "m"].contains(&o.as_str())), "{got:?}");
        assert_eq!(e["refused"], 1, "refused is counted apart");
        // attempt 4 has no terminal record; attempt 3 failed but ended; attempt 5 ended just before the window
        assert_eq!(e["attempts_total"], 1, "{}", e["attempts"]);
        assert_eq!((&e["attempts"][0]["attempt"], &e["attempts"][0]["reason"]), (&json!(4), &json!("no terminal record")));
        // the same attempt inside another installation is its own attempt
        let other = [line("j", "taskq", base("stage", "claim", "ok", "none", AT))];
        assert_eq!(errors(&other, &[line("i", "taskq", base("stage", "build", "ok", "none", AT - 5))])["attempts_total"], 1);
    }

    // Defect: a question with no records could read as zero. Protects specq "Questions are tests" and R2.
    #[test]
    fn every_telemetry_question_answers_or_reads_unknown() {
        let mut moved = base("transition", "ready", "ok", "none", AT);
        moved["from"] = json!("ready");
        moved["previous_seconds"] = json!(30);
        let mut no_entry = moved.clone();
        no_entry["previous_seconds"] = Value::Null;
        no_entry["started_ms"] = json!((AT + 1) * 1000);
        let mut turn = base("agent.turn", "agent.turn", "ok", "none", AT);
        turn["input_tokens"] = json!(10);
        let mut asset = base("operation", "render", "retried", "timeout", AT);
        asset["pipeline"] = json!("p1");
        asset["pipeline_revision"] = json!("r1");
        asset["step"] = json!("s1");
        asset["units"] = json!(5);
        asset["unit_kind"] = json!("credits");
        let lines = [line("i", "taskq", base("operation", "board.issue.read", "ok", "none", AT)), line("i", "taskq", base("stage", "build", "ok", "none", AT)), line("i", "taskq", moved), line("i", "taskq", no_entry), line("i", "taskq", turn), line("i", "taskq", asset)];
        let q = query(&lines, &[], &[], AT - 10, AT + 10);
        // failures and unstable operations: asked of the same document
        assert_eq!(q["errors"]["rows"][0]["class"], "unstable");
        // calls, duration and outcome per operation
        let op = q["operations"]["rows"].as_array().unwrap().iter().find(|r| r["operation"] == "board.issue.read").unwrap();
        assert_eq!((&op["calls"], &op["duration_us"]["sum"], &op["outcomes"]["ok"]), (&json!(1), &json!(2000), &json!(1)));
        // who worked on a task
        let who = &q["work"]["rows"][0];
        assert_eq!((&who["project"], &who["task"], &who["attempt"], &who["actor"]), (&json!("o/r"), &json!(7), &json!(3), &json!("worker")));
        assert_eq!(who["turns"]["tokens"]["input_tokens"]["observed"], 10);
        // board calls per product, host and task
        assert_eq!((&q["board_calls"]["rows"][0]["host"], &q["board_calls"]["rows"][0]["task"], &q["board_calls"]["rows"][0]["calls"]), (&json!("mac"), &json!(7), &json!(1)));
        // time per stage and wait per queue state; a missing entry time stays unknown, never zero
        assert_eq!(q["stages"]["rows"][0]["duration_us"]["sum"], 2000);
        assert_eq!((&q["queue_waits"]["rows"][0]["wait_s"], &q["queue_waits"]["rows"][0]["known"], &q["queue_waits"]["rows"][0]["of"]), (&json!(30), &json!(1), &json!(2)));
        // pipelines by method, cost, time, retries and outcome
        let p = &q["pipelines"]["rows"][0];
        assert_eq!((&p["method"], &p["units"]["credits"], &p["outcomes"]["retried"]), (&json!("render"), &json!(5), &json!(1)));
        let empty = query(&[], &[], &[], AT - 10, AT + 10);
        for k in ["operations", "work", "board_calls", "stages", "queue_waits", "pipelines"] {
            assert_eq!(empty[k]["state"], "unknown", "{k}");
            assert!(empty[k]["reason"].is_string(), "{k}");
        }
        assert!(empty["newest_observed"].is_null());
        assert_eq!(empty["totals"]["agent.call"]["tokens"]["input_tokens"]["state"], "unknown", "no record is never zero");
    }

    // Defect: an export without window or version cannot be compared later. Protects A5.
    #[test]
    fn export_states_schema_version_and_window_in_both_formats() {
        let mut o = base("operation", "a,b\"c", "ok", "none", AT);
        o["actor"] = Value::Null;
        o["unknown"] = json!({"actor": "not-collected"});
        let l = line("i", "taskq", o);
        let mut out = Vec::new();
        export_header(&mut out, "jsonl", 1, 2, 1).unwrap();
        export_row(&mut out, "jsonl", &l).unwrap();
        let text = String::from_utf8(out).unwrap();
        let mut lines = text.lines();
        assert_eq!(serde_json::from_str::<Value>(lines.next().unwrap()).unwrap(), json!({"export": "dashq-observations/1", "schema": "coreq-observation/1", "version": 1, "from": 1, "to": 2, "rows": 1, "format": "jsonl"}));
        let row: Value = serde_json::from_str(lines.next().unwrap()).unwrap();
        assert_eq!((&row["installation"], &row["observation"]["operation"]), (&json!("i"), &json!("a,b\"c")));
        let mut out = Vec::new();
        export_header(&mut out, "csv", 1, 2, 1).unwrap();
        export_row(&mut out, "csv", &l).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "# export=dashq-observations/1 schema=coreq-observation/1 version=1 from=1 to=2 rows=1");
        assert!(lines[1].starts_with("installation,workspace,generation,grant_product,received,seq,id,received_ms,kind,operation,"));
        assert!(lines[1].ends_with(",unknown"));
        assert!(lines[2].contains(",\"a,b\"\"c\","), "{}", lines[2]);
        assert!(lines[2].ends_with("\"{\"\"actor\"\":\"\"not-collected\"\"}\""), "a null keeps its reason: {}", lines[2]);
    }

    fn ids(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    // Defect: Langfuse could be deleted on missing evidence. Protects A6 and R2.
    #[test]
    fn parity_requires_old_evidence_and_nothing_missing() {
        let lines: Vec<Line> = vec![line("t", "taskq", base("operation", "a", "ok", "none", AT)), line("t", "taskq", base("operation", "b", "ok", "none", AT)), line("a", "agentq", call(Some("m1"), [Some(1); 4], AT)), line("s", "serverq", base("check", "disk", "ok", "none", AT))];
        let id = |i: usize| lines[i].value["id"].as_str().unwrap();
        let state = |r: &Value, p: &str| r["products"].as_array().unwrap().iter().find(|x| x["product"] == p).unwrap().clone();
        // all paths agree; the new path holding more does not fail parity
        let r = parity(&lines, OldPaths { taskq: Ok(ids(&[id(0)])), agentq: Ok(1), serverq: Ok(1) }, AT - 10, AT + 10);
        assert_eq!((&r["parity"], &r["langfuse"]["stays"]), (&json!(true), &json!(false)), "{r}");
        let t = state(&r, "taskq");
        assert_eq!((&t["state"], &t["old"], &t["new"], &t["matched"], &t["only_old"], &t["only_new"]), (&json!("parity"), &json!(1), &json!(2), &json!(1), &json!(0), &json!(1)));
        assert_eq!(r["window"], json!({"from": AT - 10, "to": AT + 10, "seconds": 20}));
        // an old record the new path lacks is a gap, and Langfuse stays
        let r = parity(&lines, OldPaths { taskq: Ok(ids(&[id(0), "not-shipped"])), agentq: Ok(1), serverq: Ok(1) }, AT - 10, AT + 10);
        assert_eq!((&state(&r, "taskq")["state"], &state(&r, "taskq")["only_old"], &r["parity"], &r["langfuse"]["stays"]), (&json!("gap"), &json!(1), &json!(false), &json!(true)));
        let r = parity(&lines, OldPaths { taskq: Ok(ids(&[id(0)])), agentq: Ok(2), serverq: Ok(1) }, AT - 10, AT + 10);
        assert_eq!((&state(&r, "agentq")["state"], &r["langfuse"]["stays"]), (&json!("gap"), &json!(true)));
        // an unreadable or empty old path is unknown, never parity
        let r = parity(&lines, OldPaths { taskq: Ok(ids(&[id(0)])), agentq: Err("Langfuse not configured".into()), serverq: Ok(0) }, AT - 10, AT + 10);
        assert_eq!((&state(&r, "agentq")["state"], &state(&r, "agentq")["reason"], &state(&r, "agentq")["old"]), (&json!("unknown"), &json!("Langfuse not configured"), &Value::Null));
        assert_eq!((&state(&r, "serverq")["state"], &r["langfuse"]["stays"]), (&json!("unknown"), &json!(true)));
        assert_eq!(state(&r, "pilotq")["state"], "unknown");
        assert_eq!(state(&r, "coreq")["state"], "not_applicable");
        let r = parity(&[], OldPaths { taskq: Ok(ids(&[])), agentq: Ok(0), serverq: Ok(0) }, 0, 1);
        assert_eq!(r["parity"], false, "nothing compared is no parity");
    }

    #[test]
    fn langfuse_read_counts_each_event_once_and_a_capped_read_is_unknown() {
        let span = |id: &str, ev: &str| json!({"id": id, "metadata": {"attributes.agentq.event_id": ev}});
        assert_eq!(langfuse_calls(&json!({"truncated": [], "agentq.model": [span("s1", "e1"), span("s2", "e1"), span("s3", "e2"), span("t-1", "e3")]})), Ok(2));
        assert!(langfuse_calls(&json!({"truncated": ["agentq.model"], "agentq.model": []})).is_err());
        assert!(langfuse_calls(&json!({"truncated": []})).is_err());
    }
}
