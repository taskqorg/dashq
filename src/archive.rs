//! D31: content stays out of dashboard projections and telemetry; a separate SQLite file holds index and audit.
use crate::{
    enroll::{self, Enroll, Req, Resp},
    store,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
mod objects;
mod view;

pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS archive_owners(workspace TEXT PRIMARY KEY, subject TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS archive_readers(workspace TEXT NOT NULL, subject TEXT NOT NULL, PRIMARY KEY(workspace,subject));
";
const INDEX: &str = "
CREATE TABLE IF NOT EXISTS archive.archive_sessions(id TEXT PRIMARY KEY, workspace TEXT NOT NULL, installation TEXT NOT NULL, session TEXT NOT NULL, project TEXT NOT NULL, manifest TEXT NOT NULL, name TEXT NOT NULL DEFAULT '', created INTEGER NOT NULL, received INTEGER NOT NULL, deleted INTEGER NOT NULL DEFAULT 0, UNIQUE(workspace,installation,session));
CREATE TABLE IF NOT EXISTS archive.archive_sources(session TEXT NOT NULL REFERENCES archive_sessions(id), source TEXT NOT NULL, format TEXT NOT NULL, media INTEGER NOT NULL, total INTEGER, complete INTEGER NOT NULL DEFAULT 1, PRIMARY KEY(session,source));
CREATE TABLE IF NOT EXISTS archive.archive_ranges(session TEXT NOT NULL, source TEXT NOT NULL, start INTEGER NOT NULL, end INTEGER NOT NULL, hash TEXT NOT NULL, received INTEGER NOT NULL, observed INTEGER NOT NULL, scrubbed INTEGER NOT NULL, retained INTEGER NOT NULL DEFAULT 1, manifest TEXT NOT NULL, PRIMARY KEY(session,source,start), FOREIGN KEY(session,source) REFERENCES archive_sources(session,source));
CREATE TABLE IF NOT EXISTS archive.archive_manifests(session TEXT NOT NULL REFERENCES archive_sessions(id), hash TEXT NOT NULL, manifest TEXT NOT NULL, PRIMARY KEY(session,hash));
CREATE TABLE IF NOT EXISTS archive.archive_versions(session TEXT NOT NULL, source TEXT NOT NULL, start INTEGER NOT NULL, hash TEXT NOT NULL, PRIMARY KEY(session,source,start), FOREIGN KEY(session,source) REFERENCES archive_sources(session,source));
CREATE TABLE IF NOT EXISTS archive.archive_turns(session TEXT NOT NULL, source TEXT NOT NULL, event_id TEXT NOT NULL, start INTEGER NOT NULL, PRIMARY KEY(session,source,event_id), FOREIGN KEY(session,source) REFERENCES archive_sources(session,source));
CREATE TABLE IF NOT EXISTS archive.archive_audit(id INTEGER PRIMARY KEY, actor TEXT NOT NULL, workspace TEXT NOT NULL, session TEXT NOT NULL, source TEXT NOT NULL, action TEXT NOT NULL, at INTEGER NOT NULL, outcome TEXT NOT NULL DEFAULT 'pending', reason TEXT);
";
fn migrate_index(c: &Connection) -> Res<()> {
    for (table, column, definition) in [
        ("archive_sources", "total", "INTEGER"),
        ("archive_sources", "complete", "INTEGER NOT NULL DEFAULT 1"),
        ("archive_ranges", "manifest", "TEXT NOT NULL DEFAULT ''"),
    ] {
        let has = c.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1,'archive') WHERE name=?2)",
            [table, column],
            |r| r.get::<_, bool>(0),
        )?;
        if !has {
            c.execute_batch(&format!(
                "ALTER TABLE archive.{table} ADD COLUMN {column} {definition}"
            ))?;
        }
    }
    let rows = {
        let mut q = c.prepare("SELECT id,manifest FROM archive.archive_sessions")?;
        q.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?
    };
    for (id, raw) in rows {
        let m: Value = serde_json::from_str(&raw).map_err(|_| Failure("unavailable"))?;
        let hash = coreq::enroll::digest(&m);
        c.execute(
            "INSERT OR IGNORE INTO archive.archive_manifests VALUES (?1,?2,?3)",
            params![id, hash, raw],
        )?;
        c.execute("INSERT OR IGNORE INTO archive.archive_versions SELECT session,source,MIN(start),?2 FROM archive.archive_ranges WHERE session=?1 GROUP BY session,source",params![id,hash])?;
        c.execute(
            "UPDATE archive.archive_ranges SET manifest=?2 WHERE session=?1 AND manifest=''",
            params![id, hash],
        )?;
    }
    c.execute("UPDATE archive.archive_sources SET total=(SELECT MAX(end) FROM archive.archive_ranges r WHERE r.session=archive_sources.session AND r.source=archive_sources.source) WHERE media=1 AND total IS NULL",[])?;
    Ok(())
}

pub const WIRE: &str = "agent-history/1";
const RETENTION: i64 = 365 * 86400;
#[derive(Debug, PartialEq)]
struct Failure(&'static str);
type Res<T> = Result<T, Failure>;
impl From<&'static str> for Failure {
    fn from(s: &'static str) -> Self {
        Self(s)
    }
}
impl From<rusqlite::Error> for Failure {
    fn from(_: rusqlite::Error) -> Self {
        Self("unavailable")
    }
}
impl From<store::Error> for Failure {
    fn from(_: store::Error) -> Self {
        Self("unavailable")
    }
}
impl From<std::io::Error> for Failure {
    fn from(_: std::io::Error) -> Self {
        Self("unavailable")
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn hash_bytes(bytes: &[u8]) -> String {
    hex(ring::digest::digest(&ring::digest::SHA256, bytes).as_ref())
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn text(v: &Value) -> bool {
    v.as_str()
        .is_some_and(|s| !s.is_empty() && !s.chars().any(char::is_control))
}
fn integer(v: &Value) -> Option<i64> {
    v.as_u64().and_then(|n| i64::try_from(n).ok())
}

pub fn config(v: &Value) -> Result<Option<std::path::PathBuf>, String> {
    match v.get("archive") {
        None => Ok(None),
        Some(a) => a["root"]
            .as_str()
            .filter(|s| {
                let p = Path::new(s);
                p.is_absolute()
                    && p.parent().is_some()
                    && !p
                        .components()
                        .any(|c| matches!(c, std::path::Component::ParentDir))
            })
            .map(|s| Some(s.into()))
            .ok_or_else(|| "config: bad key archive.root".into()),
    }
}

/// Host-administrator role management, under the same protected config/key as enrollment.
pub fn role(
    e: &Enroll,
    workspace: &str,
    subject: &str,
    owner: bool,
    allow: bool,
) -> Result<(), String> {
    if !enroll::id(workspace, 64) || !enroll::valid_subject(subject) {
        return Err("archive: bad workspace or subject".into());
    }
    e.store().write(|tx| {
        if owner {tx.execute("INSERT INTO archive_owners VALUES (?1,?2) ON CONFLICT(workspace) DO UPDATE SET subject=excluded.subject",[workspace,subject])?;}
        else if allow {tx.execute("INSERT OR IGNORE INTO archive_readers VALUES (?1,?2)",[workspace,subject])?;}
        else {tx.execute("DELETE FROM archive_readers WHERE workspace=?1 AND subject=?2",[workspace,subject])?;}
        Ok(())
    }).map_err(|_|"archive: role unavailable".into())
}

fn owner(c: &Connection, ws: &str, subject: &str) -> Res<bool> {
    Ok(c.query_row(
        "SELECT EXISTS(SELECT 1 FROM archive_owners WHERE workspace=?1 AND subject=?2)",
        [ws, subject],
        |r| r.get(0),
    )?)
}
fn reader(c: &Connection, ws: &str, subject: &str) -> Res<bool> {
    Ok(owner(c, ws, subject)?
        || c.query_row(
            "SELECT EXISTS(SELECT 1 FROM archive_readers WHERE workspace=?1 AND subject=?2)",
            [ws, subject],
            |r| r.get(0),
        )?)
}

fn valid_manifest(m: &Value) -> bool {
    const KEYS: [&str; 15] = [
        "project",
        "task",
        "attempt",
        "actor",
        "entry",
        "runtime",
        "cli_version",
        "model",
        "effort",
        "policy",
        "account_label",
        "host",
        "session",
        "parent_session",
        "instructions_hash",
    ];
    m.as_object()
        .is_some_and(|o| o.len() == KEYS.len() && KEYS.iter().all(|k| o.contains_key(*k)))
        && ["project", "actor", "entry", "runtime", "host", "session"]
            .iter()
            .all(|k| text(&m[*k]))
        && [
            "cli_version",
            "model",
            "effort",
            "account_label",
            "parent_session",
        ]
        .iter()
        .all(|k| m[*k].is_null() || text(&m[*k]))
        && ["task", "attempt"]
            .iter()
            .all(|k| m[*k].is_null() || text(&m[*k]) || m[*k].is_u64())
        && (m["instructions_hash"].is_null() || m["instructions_hash"].as_str().is_some_and(digest))
}

fn ingest(
    e: &Enroll,
    objects: &objects::Objects,
    verifier: &[u8],
    b: &Value,
    now: i64,
) -> Res<Value> {
    if b["protocol"] != 1 || b["schema"] != WIRE || !valid_manifest(&b["manifest"]) {
        return Err("invalid_request".into());
    }
    let m = &b["manifest"];
    let manifest_hash = coreq::enroll::digest(m);
    if b["manifest_sha256"] != manifest_hash {
        return Err("invalid_request".into());
    }
    let (
        Some(source),
        Some(format),
        Some(start),
        Some(end),
        Some(hash),
        Some(encoded),
        Some(scrubbed),
        Some(observed),
    ) = (
        b["source"].as_str().filter(|_| text(&b["source"])),
        b["format"].as_str().filter(|_| text(&b["format"])),
        integer(&b["start"]),
        integer(&b["end"]),
        b["sha256"].as_str().filter(|s| digest(s)),
        b["bytes"].as_str(),
        integer(&b["scrubbed"]),
        integer(&b["observed_at"]),
    )
    else {
        return Err("invalid_request".into());
    };
    let bytes = enroll::unbase64(encoded).ok_or(Failure("invalid_request"))?;
    if end <= start
        || end - start != bytes.len() as i64
        || hash_bytes(&bytes) != hash
        || !matches!(b["capture"].as_str(), Some("launched" | "manual"))
        || (!b["media"].is_null() && !b["media"].is_boolean())
    {
        return Err("invalid_request".into());
    }
    let media = b["media"] == true;
    let total = if media {
        Some(
            integer(&b["total"])
                .filter(|n| *n > 0 && end <= *n)
                .ok_or(Failure("invalid_request"))?,
        )
    } else {
        None
    };
    if media && (!digest(source) || format != "media") {
        return Err("invalid_request".into());
    }
    let mut c = e.store().lock()?;
    objects.index(&c)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let g = store::authorize(&tx, verifier)?.ok_or(Failure("revoked"))?;
    if !tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM activations WHERE installation=?1 AND generation=?2)",
        params![g.installation, g.generation],
        |r| r.get::<_, bool>(0),
    )? {
        return Err("not_active".into());
    }
    if !serde_json::from_str::<Vec<String>>(&g.scopes)
        .unwrap_or_default()
        .iter()
        .any(|s| s == WIRE)
    {
        return Err("scope".into());
    }
    if b["capture"] == "manual"
        && !g
            .capabilities
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
            .is_some_and(|v| v.iter().any(|s| s == "agent-history.manual"))
    {
        return Err("manual_disabled".into());
    }
    let id = hash_bytes(
        json!([g.workspace, g.installation, m["session"]])
            .to_string()
            .as_bytes(),
    );
    if tx
        .query_row(
            "SELECT deleted FROM archive.archive_sessions WHERE id=?1",
            [&id],
            |r| r.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false)
    {
        return Err("deleted".into());
    }
    let existing:Option<(String,bool,Option<i64>,bool)>=tx.query_row("SELECT format,media,total,complete FROM archive.archive_sources WHERE session=?1 AND source=?2",[&id,source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    if existing
        .as_ref()
        .is_some_and(|(f, m, t, _)| f != format || *m != media || *t != total)
    {
        return Err("replay_mismatch".into());
    }
    let receipt = |complete| json!({"protocol":1,"schema":WIRE,"state":"accepted","id":id,"source":source,"start":start,"end":end,"sha256":hash,"manifest_sha256":manifest_hash,"complete":complete});
    let old:Option<(i64,String,bool,i64,String)>=tx.query_row("SELECT end,hash,retained,received,manifest FROM archive.archive_ranges WHERE session=?1 AND source=?2 AND start=?3",params![id,source,start],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    if let Some((old_end, old_hash, retained, received, old_manifest)) = old {
        if old_end != end || old_hash != hash || old_manifest != manifest_hash {
            return Err("replay_mismatch".into());
        }
        if !retained || now.saturating_sub(received) >= RETENTION {
            return Err("expired".into());
        }
        objects.read(hash)?;
        bind_turns(&tx, &id, source, b, end, media)?;
        let complete = existing.as_ref().is_some_and(|e| e.3);
        tx.commit()?;
        return Ok(receipt(complete));
    }
    let position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(end),0) FROM archive.archive_ranges WHERE session=?1 AND source=?2",
        [&id, source],
        |r| r.get(0),
    )?;
    if start != position {
        return Err(if start > position {
            "range_gap"
        } else {
            "replay_mismatch"
        }
        .into());
    }
    let complete = !media || total == Some(end);
    if media && complete {
        let mut digest = ring::digest::Context::new(&ring::digest::SHA256);
        let mut q=tx.prepare("SELECT hash,start,end,retained,received FROM archive.archive_ranges WHERE session=?1 AND source=?2 ORDER BY start")?;
        let parts = q
            .query_map([&id, source], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, bool>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut offset = 0;
        for (hash, from, to, retained, received) in parts {
            if !retained || now.saturating_sub(received) >= RETENTION {
                return Err("expired".into());
            }
            let part = objects.read(&hash)?;
            if from != offset || to - from != part.len() as i64 {
                return Err("unavailable".into());
            }
            digest.update(&part);
            offset = to;
        }
        if offset != start {
            return Err("unavailable".into());
        }
        digest.update(&bytes);
        if hex(digest.finish().as_ref()) != source {
            return Err("invalid_request".into());
        }
    }
    objects.put(hash, &bytes)?;
    tx.execute("INSERT OR IGNORE INTO archive.archive_sessions(id,workspace,installation,session,project,manifest,created,received) VALUES (?1,?2,?3,?4,?5,?6,?7,?7)",params![id,g.workspace,g.installation,m["session"].as_str(),m["project"].as_str(),m.to_string(),now])?;
    tx.execute("INSERT OR IGNORE INTO archive.archive_sources(session,source,format,media,total,complete) VALUES (?1,?2,?3,?4,?5,?6)",params![id,source,format,media,total,complete])?;
    tx.execute(
        "INSERT OR IGNORE INTO archive.archive_manifests VALUES (?1,?2,?3)",
        params![id, manifest_hash, m.to_string()],
    )?;
    let previous:Option<String>=tx.query_row("SELECT hash FROM archive.archive_versions WHERE session=?1 AND source=?2 ORDER BY start DESC LIMIT 1",[&id,source],|r|r.get(0)).optional()?;
    if previous.as_deref() != Some(&manifest_hash) {
        tx.execute(
            "INSERT INTO archive.archive_versions VALUES (?1,?2,?3,?4)",
            params![id, source, start, manifest_hash],
        )?;
    }
    tx.execute("INSERT INTO archive.archive_ranges(session,source,start,end,hash,received,observed,scrubbed,manifest) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,source,start,end,hash,now,observed,scrubbed,manifest_hash])?;
    tx.execute(
        "UPDATE archive.archive_sources SET complete=?3 WHERE session=?1 AND source=?2",
        params![id, source, complete],
    )?;
    bind_turns(&tx, &id, source, b, end, media)?;
    tx.execute(
        "UPDATE archive.archive_sessions SET received=?2 WHERE id=?1",
        params![id, now],
    )?;
    tx.commit()?;
    Ok(receipt(complete))
}

fn bind_turns(c: &Connection, id: &str, source: &str, b: &Value, end: i64, media: bool) -> Res<()> {
    let turns = match b.get("turns") {
        None => return Ok(()),
        Some(v) => v.as_array().ok_or(Failure("invalid_request"))?,
    };
    if media && !turns.is_empty() {
        return Err("invalid_request".into());
    }
    for turn in turns {
        let event = turn["event_id"]
            .as_str()
            .filter(|_| text(&turn["event_id"]))
            .ok_or(Failure("invalid_request"))?;
        let start = integer(&turn["start"])
            .filter(|n| *n < end)
            .ok_or(Failure("invalid_request"))?;
        let old:Option<i64>=c.query_row("SELECT start FROM archive.archive_turns WHERE session=?1 AND source=?2 AND event_id=?3",[id,source,event],|r|r.get(0)).optional()?;
        if old.is_some_and(|n| n != start) {
            return Err("replay_mismatch".into());
        }
        c.execute(
            "INSERT OR IGNORE INTO archive.archive_turns VALUES (?1,?2,?3,?4)",
            params![id, source, event, start],
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct Version {
    hash: String,
    manifest: Value,
    positions: Vec<(String, i64)>,
}
fn versions(c: &Connection, id: &str) -> Res<Vec<Version>> {
    let mut q=c.prepare("SELECT v.hash,m.manifest,v.source,v.start FROM archive.archive_versions v JOIN archive.archive_manifests m ON m.session=v.session AND m.hash=v.hash WHERE v.session=?1 ORDER BY v.source,v.start")?;
    let rows = q
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut out: Vec<Version> = Vec::new();
    for (hash, m, source, start) in rows {
        if let Some(v) = out.iter_mut().find(|v| v.hash == hash) {
            v.positions.push((source, start));
        } else {
            out.push(Version {
                hash,
                manifest: serde_json::from_str(&m).map_err(|_| Failure("unavailable"))?,
                positions: vec![(source, start)],
            });
        }
    }
    Ok(out)
}
fn version_at(c: &Connection, id: &str, source: &str, start: i64) -> Res<Option<String>> {
    Ok(c.query_row("SELECT hash FROM archive.archive_versions WHERE session=?1 AND source=?2 AND start<=?3 ORDER BY start DESC LIMIT 1",params![id,source,start],|r|r.get(0)).optional()?)
}

#[derive(Clone, Debug)]
struct Session {
    id: String,
    ws: String,
    installation: String,
    manifest: Value,
    name: String,
    received: i64,
    deleted: bool,
    version: Option<String>,
}
fn session(c: &Connection, id: &str, subject: &str, write: bool) -> Res<Session> {
    let row=c.query_row("SELECT id,workspace,installation,manifest,name,received,deleted FROM archive.archive_sessions WHERE id=?1",[id],session_row).optional()?.ok_or(Failure("not_found"))?;
    if !(if write {
        owner(c, &row.ws, subject)?
    } else {
        reader(c, &row.ws, subject)?
    }) {
        return Err("not_found".into());
    }
    Ok(row)
}
fn session_row(r: &rusqlite::Row) -> rusqlite::Result<Session> {
    let manifest: String = r.get(3)?;
    Ok(Session {
        id: r.get(0)?,
        ws: r.get(1)?,
        installation: r.get(2)?,
        manifest: serde_json::from_str(&manifest).unwrap_or(Value::Null),
        name: r.get(4)?,
        received: r.get(5)?,
        deleted: r.get(6)?,
        version: None,
    })
}

#[derive(Debug)]
struct Source {
    id: String,
    format: String,
    media: bool,
    end: i64,
    expired: bool,
    observed: i64,
    scrubbed: i64,
    received: i64,
    total: Option<i64>,
    complete: bool,
}
fn sources(c: &Connection, s: &Session, now: i64) -> Res<Vec<Source>> {
    let mut q=c.prepare("SELECT s.source,s.format,s.media,COALESCE(MAX(r.end),0),COALESCE(MIN(r.retained),0),COALESCE(MIN(r.received),0),COALESCE(MAX(r.observed),0),COALESCE(SUM(r.scrubbed),0),COALESCE(MAX(r.received),0),s.total,s.complete FROM archive.archive_sources s LEFT JOIN archive.archive_ranges r ON r.session=s.session AND r.source=s.source WHERE s.session=?1 GROUP BY s.source,s.format,s.media ORDER BY s.source")?;
    Ok(q.query_map([&s.id], |r| {
        Ok(Source {
            id: r.get(0)?,
            format: r.get(1)?,
            media: r.get(2)?,
            end: r.get(3)?,
            expired: r.get::<_, i64>(4)? == 0 || now.saturating_sub(r.get(5)?) >= RETENTION,
            observed: r.get(6)?,
            scrubbed: r.get(7)?,
            received: r.get(8)?,
            total: r.get(9)?,
            complete: r.get(10)?,
        })
    })?
    .collect::<Result<_, _>>()?)
}

/// Hold the existing writer across authorization, audit commit and decryption; deletion cannot race a read.
fn read_source(
    e: &Enroll,
    o: &objects::Objects,
    id: &str,
    source: &str,
    subject: &str,
    action: &str,
    now: i64,
) -> Res<(Session, Source, Vec<u8>)> {
    let mut c = e.store().lock()?;
    o.index(&c)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let s = session(&tx, id, subject, false)?;
    if s.deleted {
        return Err("deleted".into());
    }
    let src = sources(&tx, &s, now)?
        .into_iter()
        .find(|s| s.id == source)
        .ok_or(Failure("not_found"))?;
    if src.expired {
        return Err("expired".into());
    }
    if src.media && !src.complete {
        return Err("incomplete".into());
    }
    tx.execute("INSERT INTO archive.archive_audit(actor,workspace,session,source,action,at) VALUES (?1,?2,?3,?4,?5,?6)",params![hash_bytes(subject.as_bytes()),s.ws,s.id,source,action,now])?;
    let audit = tx.last_insert_rowid();
    tx.commit()?;
    let read = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let result = (|| -> Res<(Session, Source, Vec<u8>)> {
        let s = session(&read, id, subject, false)?;
        if s.deleted {
            return Err("deleted".into());
        }
        let src = sources(&read, &s, now)?
            .into_iter()
            .find(|s| s.id == source)
            .ok_or(Failure("not_found"))?;
        if src.expired {
            return Err("expired".into());
        }
        if src.media && !src.complete {
            return Err("incomplete".into());
        }
        let mut q=read.prepare("SELECT hash,start,end FROM archive.archive_ranges WHERE session=?1 AND source=?2 AND retained=1 ORDER BY start")?;
        let ranges = q
            .query_map([id, source], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut bytes = Vec::new();
        for (hash, start, end) in ranges {
            if start != bytes.len() as i64 {
                return Err("unavailable".into());
            }
            let chunk = o.read(&hash)?;
            if chunk.len() as i64 != end - start {
                return Err("unavailable".into());
            }
            bytes.extend(chunk);
        }
        if bytes.len() as i64 != src.end {
            return Err("unavailable".into());
        }
        if src.media && (src.total != Some(bytes.len() as i64) || hash_bytes(&bytes) != src.id) {
            return Err("unavailable".into());
        }
        Ok((s, src, bytes))
    })();
    read.execute(
        "UPDATE archive.archive_audit SET outcome=?2,reason=?3 WHERE id=?1",
        params![
            audit,
            if result.is_ok() { "read" } else { "failed" },
            result.as_ref().err().map(|e| e.0)
        ],
    )?;
    read.commit()?;
    result
}

fn steps(
    o: &objects::Objects,
    s: &Session,
    src: &Source,
    bytes: &[u8],
) -> Res<(Vec<agentq_runtime::agent::history::Step>, u64)> {
    let tmp = o.reconstruct(bytes)?;
    let mut history = agentq_runtime::agent::history::History::new(
        &src.format,
        &src.id,
        s.manifest["session"]
            .as_str()
            .ok_or(Failure("unavailable"))?,
    )
    .map_err(|_| Failure("mapping_unknown"))?;
    let page = history
        .steps(&tmp.path, bytes.len().max(1) as u64)
        .map_err(|_| Failure("mapping_unknown"))?;
    let mut latest = BTreeMap::new();
    for step in page.steps {
        let e = latest
            .entry(step.id.clone())
            .or_insert_with(|| step.clone());
        if step.revision >= e.revision {
            *e = step;
        }
    }
    let mut out: Vec<_> = latest.into_values().collect();
    out.sort_by_key(|s| s.revision);
    Ok((out, page.pending_bytes))
}

pub fn prune(e: &Enroll, root: &Path, now: i64) -> Result<(), String> {
    let run = || -> Res<()> {
        let o = objects::Objects::open(root, e.archive_key())?;
        let mut c = e.store().lock()?;
        o.index(&c)?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE archive.archive_ranges SET retained=0 WHERE retained=1 AND received<=?1",
            [now.saturating_sub(RETENTION)],
        )?;
        tx.commit()?;
        let gc = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        o.collect(&gc)?;
        gc.commit()?;
        Ok(())
    };
    run().map_err(|_| "archive unavailable".into())
}

fn mutate(
    e: &Enroll,
    o: &objects::Objects,
    b: &HashMap<String, String>,
    subject: &str,
    name: bool,
) -> Res<()> {
    let mut c = e.store().lock()?;
    o.index(&c)?;
    let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if name {
        let s = session(&tx, b.get("id").map_or("", String::as_str), subject, true)?;
        if s.deleted {
            return Err("deleted".into());
        }
        let name = b.get("name").ok_or(Failure("invalid_request"))?;
        if name.chars().any(char::is_control) {
            return Err("invalid_request".into());
        }
        tx.execute(
            "UPDATE archive.archive_sessions SET name=?2 WHERE id=?1",
            [&s.id, name],
        )?;
    } else {
        if b.get("confirm").map(String::as_str) != Some("delete") {
            return Err("invalid_request".into());
        }
        let ids = if let Some(id) = b.get("id").filter(|s| !s.is_empty()) {
            vec![session(&tx, id, subject, true)?.id]
        } else {
            let (Some(ws), Some(project)) = (b.get("workspace"), b.get("project")) else {
                return Err("invalid_request".into());
            };
            if !owner(&tx, ws, subject)? {
                return Err("not_found".into());
            }
            let mut q = tx.prepare(
                "SELECT s.id FROM archive.archive_sessions s WHERE s.workspace=?1 AND EXISTS(SELECT 1 FROM archive.archive_manifests m WHERE m.session=s.id AND json_extract(m.manifest,'$.project')=?2)",
            )?;
            q.query_map([ws, project], |r| r.get(0))?
                .collect::<Result<Vec<String>, _>>()?
        };
        for id in ids {
            tx.execute(
                "UPDATE archive.archive_sessions SET deleted=1 WHERE id=?1",
                [&id],
            )?;
            tx.execute(
                "UPDATE archive.archive_ranges SET retained=0 WHERE session=?1",
                [&id],
            )?;
        }
    }
    tx.commit()?;
    if !name {
        let gc = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        o.collect(&gc)?;
        gc.commit()?;
    }
    Ok(())
}

fn fail(code: &'static str) -> Resp {
    let status = match code {
        "invalid_request" => "400 Bad Request",
        "invalid_credential" | "revoked" => "401 Unauthorized",
        "scope" | "not_active" | "manual_disabled" | "forbidden" => "403 Forbidden",
        "not_found" => "404 Not Found",
        "method_not_allowed" => "405 Method Not Allowed",
        "replay_mismatch" | "range_gap" | "deleted" | "expired" | "incomplete" => "409 Conflict",
        _ => "503 Service Unavailable",
    };
    reply(
        status,
        "application/json",
        json!({"protocol":1,"error":code}).to_string().into_bytes(),
        false,
    )
}
fn reply(status: &'static str, ctype: &'static str, body: Vec<u8>, forms: bool) -> Resp {
    Resp {
        status,
        ctype,
        body,
        headers: String::new(),
        forms,
    }
}

pub fn route(e: &Result<Enroll, String>, root: Option<&Path>, r: &Req, now: i64) -> Option<Resp> {
    if ![
        "/machine/archive/1",
        "/archive",
        "/archive/step",
        "/archive/raw",
        "/archive/search",
        "/archive/name",
        "/archive/delete",
    ]
    .contains(&r.path)
    {
        return None;
    }
    let machine = r.path == "/machine/archive/1";
    let post = machine
        || matches!(
            r.path,
            "/archive/search" | "/archive/name" | "/archive/delete"
        );
    if r.method != if post { "POST" } else { "GET" } {
        return Some(fail("method_not_allowed"));
    }
    let run = || -> Res<Resp> {
        let e = e.as_ref().map_err(|_| Failure("unavailable"))?;
        let root = root.ok_or(Failure("unavailable"))?;
        if machine {
            let cred = r
                .header("authorization")
                .and_then(|s| s.strip_prefix("Bearer "))
                .filter(|s| !s.is_empty())
                .ok_or(Failure("invalid_credential"))?;
            let verifier = enroll::sha256(cred);
            let c = e
                .store()
                .credential(&verifier)?
                .ok_or(Failure("invalid_credential"))?;
            if c.revoked {
                return Err("revoked".into());
            }
            if !c.active {
                return Err("not_active".into());
            }
            if !serde_json::from_str::<Vec<String>>(&c.grant.scopes)
                .unwrap_or_default()
                .iter()
                .any(|s| s == WIRE)
            {
                return Err("scope".into());
            }
            if let Err(wait) = e.spend(&c.grant, now) {
                let mut x = reply(
                    "429 Too Many Requests",
                    "application/json",
                    json!({"protocol":1,"error":"rate_limited"})
                        .to_string()
                        .into_bytes(),
                    false,
                );
                x.headers = format!("Retry-After: {wait}\r\n");
                return Ok(x);
            }
            let b: Value =
                serde_json::from_slice(r.body).map_err(|_| Failure("invalid_request"))?;
            let o = objects::Objects::open(root, e.archive_key())?;
            return Ok(reply(
                "200 OK",
                "application/json",
                ingest(e, &o, &verifier, &b, now)?.to_string().into_bytes(),
                false,
            ));
        }
        let subject = r
            .header(enroll::SUBJECT)
            .filter(|s| enroll::valid_subject(s))
            .ok_or(Failure("forbidden"))?;
        let b = if post {
            if r.header("origin") != Some(format!("https://{}", e.public_host()).as_str()) {
                return Err("forbidden".into());
            }
            let b = enroll::form(r.body);
            if !e.token_ok(
                &format!("archive\n{subject}"),
                b.get("csrf").map_or("", String::as_str),
            ) {
                return Err("forbidden".into());
            }
            b
        } else {
            enroll::form(r.query.as_bytes())
        };
        let o = objects::Objects::open(root, e.archive_key())?;
        {
            let c = e.store().lock()?;
            o.index(&c)?;
        }
        if matches!(r.path, "/archive/name" | "/archive/delete") {
            mutate(e, &o, &b, subject, r.path == "/archive/name")?;
            return Ok(reply(
                "200 OK",
                "text/html; charset=utf-8",
                view::page(
                    e,
                    subject,
                    "<p>Saved. <a href=\"/archive\">Return to archive</a></p>",
                )
                .into_bytes(),
                true,
            ));
        }
        view::route(e, &o, r.path, &b, subject, now)
    };
    Some(run().unwrap_or_else(|e| {
        let mut response=fail(e.0);
        if !machine && r.path!="/archive/raw" {
            response.ctype="text/html; charset=utf-8";
            let message=match e.0 {
                "forbidden"=>"Sign in through the dashboard. Content requires the archive owner or a separate archive-reader role.",
                "not_found"=>"This session or step is unavailable in your scope.",
                "expired"=>"Content expired after 365 days; its index remains.",
                "deleted"=>"Content was permanently deleted; replay is refused.",
                "mapping_unknown"=>"Step mapping is unknown. Open the run to download its exact raw source.",
                "invalid_request"=>"The submitted archive request is invalid.",
                _=>"The archive is not configured or its storage is unavailable. No content was returned.",
            };
            response.body=format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><link rel=\"stylesheet\" href=\"/style.css\"><title>Session archive</title></head><body class=\"archive-page\"><main><h1>Session archive</h1><p>{message}</p><p><a href=\"/archive\">Archive</a> · <a href=\"/\">Overview</a></p></main></body></html>").into_bytes();
        }
        response
    }))
}

#[cfg(test)]
mod tests;
