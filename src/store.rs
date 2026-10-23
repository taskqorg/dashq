//! Enrollment store (docs/design/enrollment.md C4/C5, dashq.md D11): one SQLite file, WAL with
//! `synchronous=FULL`, one bounded writer. An acknowledgement is returned only after its commit.
//! Data rows carry the workspace of the grant that sent them: a reconnect into another workspace never shows
//! earlier data there (C1).

use rusqlite::{Connection, ErrorCode, OptionalExtension, Transaction, TransactionBehavior, params};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Store format; a newer file refuses to open instead of losing grants or revocations (E9).
const VERSION: i64 = 1;
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS members(workspace TEXT NOT NULL, subject TEXT NOT NULL, role TEXT NOT NULL CHECK (role IN ('admin', 'member')), PRIMARY KEY (workspace, subject));
CREATE TABLE IF NOT EXISTS installations(id TEXT PRIMARY KEY, product TEXT NOT NULL, runtime TEXT NOT NULL, label TEXT NOT NULL, created INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS grants(installation TEXT NOT NULL REFERENCES installations(id), generation INTEGER NOT NULL, verifier BLOB NOT NULL UNIQUE, workspace TEXT NOT NULL, account TEXT NOT NULL, scopes TEXT NOT NULL, created INTEGER NOT NULL, PRIMARY KEY (installation, generation));
CREATE TABLE IF NOT EXISTS revocations(installation TEXT NOT NULL, generation INTEGER NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (installation, generation));
CREATE TABLE IF NOT EXISTS receipts(installation TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, schema TEXT NOT NULL, content TEXT NOT NULL, outcome TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (installation, generation, event_id));
CREATE TABLE IF NOT EXISTS snapshots(installation TEXT NOT NULL, workspace TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, schema TEXT NOT NULL, observed INTEGER NOT NULL, clock_uncertain INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY (installation, workspace));
CREATE TABLE IF NOT EXISTS history(installation TEXT NOT NULL, workspace TEXT NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, clock_uncertain INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS history_by_time ON history(installation, observed);
CREATE TABLE IF NOT EXISTS outbox(id INTEGER PRIMARY KEY, installation TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, queued INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS export_gaps(installation TEXT NOT NULL, events INTEGER NOT NULL, first_observed INTEGER NOT NULL, last_observed INTEGER NOT NULL, at INTEGER NOT NULL, workspace TEXT NOT NULL DEFAULT '');
CREATE TABLE IF NOT EXISTS pairings(device BLOB PRIMARY KEY, user_code TEXT NOT NULL UNIQUE, product TEXT NOT NULL, installation TEXT NOT NULL, runtime TEXT NOT NULL, label TEXT NOT NULL, schemas TEXT NOT NULL, expires INTEGER NOT NULL, interval INTEGER NOT NULL, polled INTEGER NOT NULL, failures INTEGER NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending', 'approved', 'denied', 'invalid', 'acked')), workspace TEXT, generation INTEGER, sealed BLOB);
CREATE TABLE IF NOT EXISTS activations(installation TEXT NOT NULL, generation INTEGER NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (installation, generation));
CREATE TABLE IF NOT EXISTS pm_requesters(workspace TEXT NOT NULL, subject TEXT NOT NULL, PRIMARY KEY(workspace,subject));
CREATE TABLE IF NOT EXISTS pm_hosts(installation TEXT PRIMARY KEY, generation INTEGER NOT NULL, workspace TEXT NOT NULL, targets TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS pm_ops(actor TEXT NOT NULL, op TEXT NOT NULL, subject TEXT NOT NULL, workspace TEXT NOT NULL, installation TEXT NOT NULL, generation INTEGER NOT NULL, revision TEXT NOT NULL, digest BLOB NOT NULL, request TEXT, expires INTEGER NOT NULL, delivered INTEGER NOT NULL, receipt TEXT, PRIMARY KEY(actor,op));
CREATE TABLE IF NOT EXISTS controllers(workspace TEXT NOT NULL, subject TEXT NOT NULL, PRIMARY KEY (workspace, subject));
CREATE TABLE IF NOT EXISTS answerers(workspace TEXT NOT NULL, subject TEXT NOT NULL, PRIMARY KEY (workspace, subject));
CREATE TABLE IF NOT EXISTS answers(op_id TEXT PRIMARY KEY, subject TEXT NOT NULL, workspace TEXT NOT NULL, board TEXT NOT NULL, task INTEGER NOT NULL, revision INTEGER NOT NULL, payload BLOB NOT NULL, state TEXT NOT NULL CHECK (state IN ('pending', 'accepted', 'rejected', 'stale', 'unknown')), receipt TEXT, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS resource_grants(grant_id TEXT PRIMARY KEY, actor_id TEXT NOT NULL, subject TEXT NOT NULL, workspace TEXT NOT NULL, installation TEXT NOT NULL, generation INTEGER NOT NULL, projects TEXT NOT NULL, runtimes TEXT NOT NULL, host_policy INTEGER NOT NULL, expires_at INTEGER NOT NULL, revoked INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS resource_ops(op_id TEXT PRIMARY KEY, installation TEXT NOT NULL, generation INTEGER NOT NULL, actor_id TEXT NOT NULL, grant_id TEXT NOT NULL, submit TEXT NOT NULL, expires_at INTEGER NOT NULL, delivered INTEGER NOT NULL, cancel INTEGER NOT NULL, state TEXT NOT NULL, receipt TEXT NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS operations(operation TEXT PRIMARY KEY, installation TEXT NOT NULL, generation INTEGER NOT NULL, capability TEXT NOT NULL, workspace TEXT NOT NULL, actor TEXT NOT NULL, apply INTEGER NOT NULL, digest TEXT NOT NULL, expires_at INTEGER NOT NULL, op TEXT NOT NULL, queued INTEGER NOT NULL, delivered INTEGER NOT NULL, result TEXT, result_at INTEGER);
CREATE INDEX IF NOT EXISTS operations_by_installation ON operations(installation, generation);
CREATE TABLE IF NOT EXISTS telemetry(id INTEGER PRIMARY KEY, installation TEXT NOT NULL, workspace TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, received INTEGER NOT NULL, status TEXT, payload TEXT NOT NULL, first_seq INTEGER, last_seq INTEGER, rows INTEGER);
CREATE INDEX IF NOT EXISTS telemetry_by_installation ON telemetry(installation, workspace, observed);
CREATE TABLE IF NOT EXISTS telemetry_pruned(installation TEXT NOT NULL, workspace TEXT NOT NULL, cause TEXT NOT NULL, blocks INTEGER NOT NULL, rows INTEGER NOT NULL, through_seq INTEGER, at INTEGER NOT NULL, PRIMARY KEY (installation, workspace, cause));
CREATE TABLE IF NOT EXISTS resources(installation TEXT NOT NULL, workspace TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, payload TEXT NOT NULL, PRIMARY KEY (installation, workspace));
";

/// C5 bounds; tests shrink them.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Store ceiling including WAL and outbox.
    pub store_bytes: u64,
    /// Kept free for grants, revocations and membership after telemetry is refused.
    pub reserve_bytes: u64,
    pub outbox_bytes: u64,
    pub history_secs: i64,
    /// `taskq-telemetry/1` blocks (D11 #161): kept this long after receipt, and at most this many payload bytes per
    /// installation and workspace (#181).
    pub telemetry_secs: i64,
    pub telemetry_bytes: u64,
    pub receipt_secs: i64,
    pub outbox_secs: i64,
    /// ponytail: 300 s skew marks a sample clock-uncertain; qualify with real hosts.
    pub future_secs: i64,
    pub records: usize,
    /// Callers waiting for the single writer before new ones get `Busy`.
    pub writers: usize,
}

pub const LIMITS: Limits = Limits {
    store_bytes: 4 << 30,
    reserve_bytes: 64 << 20,
    outbox_bytes: 1 << 30,
    history_secs: 7 * 86400,
    telemetry_secs: 7 * 86400,
    // #181 A4: a measured TaskQ host sends 10.6 MiB of blocks a day, 75 MiB in 7 days; twice that per installation
    telemetry_bytes: 160 << 20,
    receipt_secs: 8 * 86400,
    outbox_secs: 7 * 86400,
    future_secs: 300,
    records: 100,
    writers: 64,
};

#[derive(Debug, PartialEq)]
pub enum Error {
    /// Writer queue full; retry later.
    Busy,
    /// Store ceiling reached; nothing in this call was acknowledged.
    Full,
    /// Integrity check failed; the file is left as evidence, never reset.
    Corrupt,
    /// Written by a newer format.
    Newer(i64),
    /// Unknown or revoked credential.
    Denied,
    /// Installation exists with another product or runtime.
    Conflict,
    TooMany,
    Exists,
    Io(String),
}

impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        match e.sqlite_error_code() {
            Some(ErrorCode::DiskFull) => Error::Full,
            Some(ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase) => Error::Corrupt,
            _ => Error::Io(e.to_string()),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Grant {
    pub installation: String,
    pub generation: i64,
    pub workspace: String,
    pub account: String,
    pub scopes: String,
    /// Approved board scope `{"name", "value"}` as JSON; `None` when the request carried none.
    pub board: Option<String>,
    /// JSON array of the operation capabilities the approver granted; `None` when the request asked for none.
    pub capabilities: Option<String>,
}

pub struct Installation<'a> {
    pub id: &'a str,
    pub product: &'a str,
    pub runtime: &'a str,
    pub label: &'a str,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    /// Installation snapshot: current value plus history sample.
    Snapshot,
    /// Telemetry for the export outbox.
    Export,
    /// A `taskq-telemetry/1` block: stored once, bounded by `Limits::telemetry_*`, never a snapshot.
    Telemetry,
    /// A `taskq-resources/1` projection (D11 #160): the newest one per installation and workspace, never a snapshot.
    Resources,
}

pub struct Event<'a> {
    pub id: &'a str,
    pub schema: &'a str,
    pub observed: i64,
    pub kind: Kind,
    pub payload: &'a str,
}

/// Receipt: rejected, durably accepted, or accepted and read through the dashboard projection (`project`).
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Projected,
    Accepted,
    /// Wire reason: `scope`, `replay_mismatch`, `expired`, `store_full` or `outbox_full`.
    Rejected(&'static str),
}

/// Whose installations a projection shows (C3): the host administrator sees all; a workspace administrator its
/// workspace; a member the installations it approved.
#[derive(Clone, Copy, Debug)]
pub enum Scope<'a> {
    All,
    Subject(&'a str),
}

/// One expected installation as the dashboard projects it; `snapshot` is `None` until one arrives.
#[derive(Debug, PartialEq)]
pub struct Row {
    pub installation: String,
    pub product: String,
    pub runtime: String,
    pub label: String,
    pub workspace: String,
    pub generation: i64,
    /// `active`, `pending` (approved, not acknowledged) or `revoked`.
    pub connection: &'static str,
    pub snapshot: Option<Snapshot>,
    /// Retained history samples of this installation and workspace with a trusted clock.
    pub samples: i64,
    /// Export events of this installation and workspace removed unsent (C5 export gap).
    pub export_gap: i64,
    pub received_at: Option<i64>,
    pub scopes: String,
    pub board: Option<String>,
    pub capabilities: Option<String>,
    /// Stored `taskq-telemetry/1` blocks of this installation and workspace and their coverage.
    pub telemetry: Telemetry,
    /// Payload of the newest `taskq-resources/1` record of this installation and workspace and its generation.
    pub resources: Option<(i64, String)>,
}

/// Retained `taskq-telemetry/1` blocks of one installation and workspace (#181 A3): counts, the seq range from the
/// oldest to the newest block in receipt order, receipt times, the newest block's sender `status` JSON (`None`: not
/// reported), DashQ's own removals and the seq ranges no stored block covers.
#[derive(Debug, Default, PartialEq)]
pub struct Telemetry {
    pub blocks: i64,
    pub rows: i64,
    pub bytes: i64,
    pub first_seq: Option<i64>,
    pub last_seq: Option<i64>,
    pub oldest: Option<i64>,
    pub newest: Option<i64>,
    pub status: Option<String>,
    /// `(cause, blocks, rows, highest seq)` removed by DashQ: `capacity`, `retention` or `allowlist` (lines dropped at ingest).
    pub pruned: Vec<(String, i64, i64, Option<i64>)>,
    /// Inclusive seq ranges between stored blocks, at most [`HOLES`]; `holes_total` and `hole_rows` count all.
    pub holes: Vec<(i64, i64)>,
    pub holes_total: usize,
    pub hole_rows: i64,
    /// Blocks whose seq went back: the sender's store was replaced and counts again from 1.
    pub restarts: i64,
}

/// One stored block read back by event ID (#181 A6).
#[derive(Debug, PartialEq)]
pub struct Block {
    pub installation: String,
    pub workspace: String,
    pub generation: i64,
    pub event_id: String,
    pub observed: i64,
    pub received: i64,
    /// Receipt outcome `accepted`/`projected`; `None` once the receipt was pruned.
    pub receipt: Option<String>,
    pub payload: String,
    pub coverage: Telemetry,
}

/// A pairing session as the approval page shows it.
#[derive(Debug, PartialEq)]
pub struct Pairing {
    pub product: String,
    pub installation: String,
    pub runtime: String,
    pub label: String,
    /// JSON array of the requested schemas.
    pub schemas: String,
    /// Requested board scope `{"name", "value"}` as JSON, if any.
    pub board: Option<String>,
    /// JSON array of the requested operation capabilities, if any.
    pub capabilities: Option<String>,
    /// The installation has a live grant that approval replaces.
    pub replaces: bool,
}

/// `/machine/connect/token` answer (RFC 8628 § 3.5).
#[derive(Debug, PartialEq)]
pub enum Poll {
    Pending,
    SlowDown,
    Denied,
    Expired,
    Invalid,
    /// Approved: the sealed credential and its grant, repeated until acknowledgement or expiry.
    Token { sealed: Vec<u8>, grant: Grant, product: String, schemas: String },
}

/// Browser decision on a user code.
#[derive(Debug, PartialEq)]
pub enum Decision {
    Approved(Grant),
    Already,
    Denied,
    /// Unknown, expired, used or invalidated code.
    Invalid,
    /// The account is not a member of that workspace; counts as a failed approval.
    NotMember,
}

/// A credential with its revocation and acknowledgement state.
#[derive(Debug, PartialEq)]
pub struct Credential {
    pub grant: Grant,
    pub revoked: bool,
    pub active: bool,
}

/// One owner answer operation (answers.md): verified actor, scoped target, immutable payload digest and the
/// sanitized TaskQ receipt once known. Never pruned: an unresolved operation stays for reconciliation.
#[derive(Clone, Debug, PartialEq)]
pub struct AnswerOp {
    pub op_id: String,
    pub subject: String,
    pub workspace: String,
    pub board: String,
    pub task: i64,
    pub revision: i64,
    pub payload: Vec<u8>,
    /// `pending` (dispatched, no receipt yet), `accepted`, `rejected`, `stale` or `unknown`.
    pub state: String,
    pub receipt: Option<String>,
}

/// `queue` outcome: stored, the identical operation already stored, or refused with a safe code.
#[derive(Debug, PartialEq)]
pub enum Queued {
    Queued,
    Same,
    Refused(&'static str),
}

/// One queued operation (D16) as stored: the immutable wire object `op` plus delivery and result state.
#[derive(Debug, PartialEq)]
pub struct OpRecord {
    pub op: serde_json::Value,
    pub workspace: String,
    pub actor: String,
    pub delivered: bool,
    pub revoked: bool,
    pub result: Option<serde_json::Value>,
    pub result_at: Option<i64>,
}

/// D16 operation lifetime: AgentQ refuses a window over 300 s.
pub const OPERATION_SECS: i64 = 300;
/// Unresolved operations per installation, and resolved ones kept per installation after prune.
const MAX_UNRESOLVED: i64 = 64;
const KEEP_RESOLVED: i64 = 64;

/// Failed approvals per code before it is invalidated (C3).
const MAX_FAILURES: i64 = 5;

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    /// Connection generation that sent it.
    pub generation: i64,
    pub event_id: String,
    pub schema: String,
    pub observed: i64,
    pub clock_uncertain: bool,
    pub payload: String,
}

/// One outbox row waiting for forwarding.
#[derive(Debug, PartialEq)]
pub struct Pending {
    pub id: i64,
    pub installation: String,
    pub generation: i64,
    pub event_id: String,
    pub observed: i64,
    pub payload: String,
}

pub struct Store {
    conn: Mutex<Connection>,
    waiting: AtomicUsize,
    pub(crate) limits: Limits,
}

fn create_private(path: &Path) -> Result<(), Error> {
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600);
    o.open(path)?;
    Ok(())
}

fn check(c: &Connection) -> Result<(), Error> {
    if c.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))? != "ok" {
        return Err(Error::Corrupt);
    }
    match c.query_row("PRAGMA user_version", [], |r| r.get(0))? {
        v if v > VERSION => Err(Error::Newer(v)),
        _ => Ok(()),
    }
}

/// Live bytes: used pages plus the WAL file.
pub(crate) fn used(c: &Connection, path: &Path) -> Result<u64, Error> {
    let pages: i64 = c.query_row("SELECT (SELECT page_count FROM pragma_page_count) - (SELECT freelist_count FROM pragma_freelist_count)", [], |r| r.get(0))?;
    let page: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let wal = std::fs::metadata(format!("{}-wal", path.display())).map(|m| m.len()).unwrap_or(0);
    Ok(pages as u64 * page as u64 + wal)
}

pub(crate) fn authorize(c: &Connection, verifier: &[u8]) -> Result<Option<Grant>, Error> {
    Ok(c.query_row(
        "SELECT installation, generation, workspace, account, scopes, board, capabilities FROM grants g WHERE verifier = ?1
         AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)",
        [verifier],
        |r| Ok(Grant { installation: r.get(0)?, generation: r.get(1)?, workspace: r.get(2)?, account: r.get(3)?, scopes: r.get(4)?, board: r.get(5)?, capabilities: r.get(6)? }),
    )
    .optional()?)
}

/// New connection generation; revokes the previous one, keeps installation identity and history (C1).
#[allow(clippy::too_many_arguments)]
fn grant(tx: &Transaction, inst: &Installation, workspace: &str, account: &str, scopes: &str, board: Option<&str>, capabilities: Option<&str>, verifier: &[u8], now: i64) -> Result<Grant, Error> {
    let known: Option<(String, String)> = tx.query_row("SELECT product, runtime FROM installations WHERE id = ?1", [inst.id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    match known {
        Some((p, r)) if p != inst.product || r != inst.runtime => return Err(Error::Conflict),
        Some(_) => {
            // a live grant elsewhere is never moved silently; after revoke a new approval may pick another workspace,
            // and earlier data stays with its own workspace
            // ponytail: no live switch (C1 old → new) until its explicit flow exists
            let live: Option<String> = tx
                .query_row(
                    "SELECT workspace FROM grants g WHERE installation = ?1
                     AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation) ORDER BY generation DESC LIMIT 1",
                    [inst.id],
                    |r| r.get(0),
                )
                .optional()?;
            if live.is_some_and(|w| w != workspace) {
                return Err(Error::Conflict);
            }
        }
        None => {
            tx.execute("INSERT INTO installations VALUES (?1, ?2, ?3, ?4, ?5)", params![inst.id, inst.product, inst.runtime, inst.label, now])?;
        }
    }
    tx.execute(
        "INSERT OR IGNORE INTO revocations SELECT installation, generation, ?2 FROM grants WHERE installation = ?1",
        params![inst.id, now],
    )?;
    let generation: i64 = tx.query_row("SELECT COALESCE(MAX(generation), 0) + 1 FROM grants WHERE installation = ?1", [inst.id], |r| r.get(0))?;
    tx.execute(
        "INSERT INTO grants (installation, generation, verifier, workspace, account, scopes, created, board, capabilities) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![inst.id, generation, verifier, workspace, account, scopes, now, board, capabilities],
    )?;
    Ok(Grant { installation: inst.id.into(), generation, workspace: workspace.into(), account: account.into(), scopes: scopes.into(), board: board.map(Into::into), capabilities: capabilities.map(Into::into) })
}

/// Pairing session lifetime (C3).
pub const PAIRING_SECS: i64 = 600;

/// Erases sealed token responses at expiry and drops sessions an hour after it.
// ponytail: runs on the next pairing request, not on a timer; add one to the prune pass if pairing goes idle with secrets
fn expire(tx: &Transaction, now: i64) -> Result<(), Error> {
    tx.execute("UPDATE pairings SET sealed = NULL WHERE expires <= ?1 AND sealed IS NOT NULL", [now])?;
    tx.execute("DELETE FROM pairings WHERE expires <= ?1", [now - 3600])?;
    Ok(())
}

/// Removes the telemetry blocks `ids` and adds them to the known loss of their installation and workspace by
/// `cause` (`capacity` or `retention`), which outlives the blocks (#181 A4).
fn drop_blocks(tx: &Transaction, ids: &[i64], cause: &str, now: i64) -> Result<(), Error> {
    if ids.is_empty() {
        return Ok(());
    }
    let ids = serde_json::to_string(ids).unwrap_or_default();
    tx.execute(
        "INSERT INTO telemetry_pruned SELECT installation, workspace, ?2, COUNT(*), COALESCE(SUM(rows), 0), MAX(last_seq), ?3 FROM telemetry
         WHERE id IN (SELECT value FROM json_each(?1)) GROUP BY installation, workspace
         ON CONFLICT (installation, workspace, cause) DO UPDATE SET blocks = blocks + excluded.blocks, rows = rows + excluded.rows,
           through_seq = MAX(COALESCE(through_seq, excluded.through_seq), COALESCE(excluded.through_seq, through_seq)), at = excluded.at",
        params![ids, cause, now],
    )?;
    tx.execute("DELETE FROM telemetry WHERE id IN (SELECT value FROM json_each(?1))", [ids])?;
    Ok(())
}

/// Seq ranges listed per installation; the rest are counted.
const HOLES: usize = 32;

/// What DashQ holds of one installation's telemetry and what it knows is missing (#181 A3).
fn coverage(c: &Connection, installation: &str, workspace: &str) -> Result<Telemetry, Error> {
    let mut t = Telemetry::default();
    let mut q = c.prepare_cached("SELECT first_seq, last_seq, rows, length(payload), received, status FROM telemetry WHERE installation = ?1 AND workspace = ?2 ORDER BY id")?;
    let mut rows = q.query(params![installation, workspace])?;
    let mut prev: Option<i64> = None;
    while let Some(r) = rows.next()? {
        let (first, last): (Option<i64>, Option<i64>) = (r.get(0)?, r.get(1)?);
        t.blocks += 1;
        t.rows += r.get::<_, Option<i64>>(2)?.unwrap_or(0);
        t.bytes += r.get::<_, i64>(3)?;
        t.oldest.get_or_insert(r.get(4)?);
        t.newest = r.get(4)?;
        t.status = r.get(5)?;
        t.first_seq = t.first_seq.or(first);
        t.last_seq = last.or(t.last_seq);
        // receipt order: a seq that does not continue the previous block is a hole, one that goes back a restart
        match (prev, first) {
            (Some(p), Some(f)) if f > p + 1 => {
                t.hole_rows += f - p - 1;
                t.holes.push((p + 1, f - 1));
            }
            (Some(p), Some(f)) if f <= p => t.restarts += 1,
            _ => {}
        }
        prev = last.or(prev);
    }
    t.holes_total = t.holes.len();
    t.holes.truncate(HOLES);
    let mut q = c.prepare_cached("SELECT cause, blocks, rows, through_seq FROM telemetry_pruned WHERE installation = ?1 AND workspace = ?2 ORDER BY cause")?;
    t.pruned = q.query_map(params![installation, workspace], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
    Ok(t)
}

fn credential(c: &Connection, verifier: &[u8]) -> Result<Option<Credential>, Error> {
    Ok(c.query_row(
        "SELECT installation, generation, workspace, account, scopes, board, capabilities,
           EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation),
           EXISTS (SELECT 1 FROM activations a WHERE a.installation = g.installation AND a.generation = g.generation)
         FROM grants g WHERE verifier = ?1",
        [verifier],
        |r| Ok(Credential { grant: Grant { installation: r.get(0)?, generation: r.get(1)?, workspace: r.get(2)?, account: r.get(3)?, scopes: r.get(4)?, board: r.get(5)?, capabilities: r.get(6)? }, revoked: r.get(7)?, active: r.get(8)? }),
    )
    .optional()?)
}

fn outcome(s: &str) -> Outcome {
    if s == "projected" { Outcome::Projected } else { Outcome::Accepted }
}

impl Store {
    pub fn open(path: &Path, limits: Limits) -> Result<Store, Error> {
        if !path.exists() {
            create_private(path)?;
        }
        let c = Connection::open(path)?;
        c.busy_timeout(std::time::Duration::from_secs(10))?;
        check(&c)?;
        if c.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))? != "wal" {
            return Err(Error::Io("WAL unavailable".into()));
        }
        let page: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
        c.execute_batch(&format!(
            "PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON; PRAGMA journal_size_limit = {}; PRAGMA max_page_count = {};",
            limits.reserve_bytes,
            limits.store_bytes / page as u64
        ))?;
        c.execute_batch(&format!("BEGIN IMMEDIATE; {SCHEMA} PRAGMA user_version = {VERSION}; COMMIT;"))?;
        // additive: gaps recorded before the column existed keep workspace '' and show in no scoped view
        if c.query_row("SELECT COUNT(*) FROM pragma_table_info('export_gaps') WHERE name = 'workspace'", [], |r| r.get::<_, i64>(0))? == 0 {
            c.execute_batch("ALTER TABLE export_gaps ADD COLUMN workspace TEXT NOT NULL DEFAULT ''")?;
        }
        // additive: requests and grants from before board scopes or capabilities carry none
        for t in ["pairings", "grants"] {
            for col in ["board", "capabilities"] {
                if c.query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('{t}') WHERE name = '{col}'"), [], |r| r.get::<_, i64>(0))? == 0 {
                    c.execute_batch(&format!("ALTER TABLE {t} ADD COLUMN {col} TEXT"))?;
                }
            }
        }
        // additive: resource grants from before #159 carry no project-limits scope
        if c.query_row("SELECT COUNT(*) FROM pragma_table_info('resource_grants') WHERE name = 'project_limits'", [], |r| r.get::<_, i64>(0))? == 0 {
            c.execute_batch("ALTER TABLE resource_grants ADD COLUMN project_limits INTEGER NOT NULL DEFAULT 0")?;
        }
        // additive (#181): block coverage columns; blocks stored before them, or by a rolled-back binary that never
        // writes them, are filled from their own payload, so every stored block stays readable
        for col in ["first_seq", "last_seq", "rows"] {
            if c.query_row(&format!("SELECT COUNT(*) FROM pragma_table_info('telemetry') WHERE name = '{col}'"), [], |r| r.get::<_, i64>(0))? == 0 {
                c.execute_batch(&format!("ALTER TABLE telemetry ADD COLUMN {col} INTEGER"))?;
            }
        }
        c.execute_batch(
            "UPDATE telemetry SET first_seq = COALESCE(json_extract(payload, '$.first.seq'), json_extract(payload, '$.first')),
               last_seq = COALESCE(json_extract(payload, '$.last.seq'), json_extract(payload, '$.last')), rows = json_extract(payload, '$.rows')
             WHERE rows IS NULL AND json_valid(payload)",
        )?;
        Ok(Store { conn: Mutex::new(c), waiting: AtomicUsize::new(0), limits })
    }

    pub(crate) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, Error> {
        if self.waiting.fetch_add(1, Ordering::SeqCst) >= self.limits.writers {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            return Err(Error::Busy);
        }
        // a panic mid-transaction rolled it back on drop; the connection stays usable
        let c = self.conn.lock().unwrap_or_else(|p| p.into_inner());
        self.waiting.fetch_sub(1, Ordering::SeqCst);
        Ok(c)
    }

    /// One transaction under the single writer; dropped without commit on any error.
    pub(crate) fn write<T>(&self, f: impl FnOnce(&Transaction) -> Result<T, Error>) -> Result<T, Error> {
        let mut c = self.lock()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let r = f(&tx)?;
        tx.commit()?;
        Ok(r)
    }

    pub(crate) fn path(c: &Connection) -> std::path::PathBuf {
        c.path().unwrap_or_default().into()
    }

    pub fn add_member(&self, workspace: &str, subject: &str, role: &str) -> Result<(), Error> {
        self.write(|tx| Ok(tx.execute("INSERT OR REPLACE INTO members VALUES (?1, ?2, ?3)", params![workspace, subject, role]).map(|_| ())?))
    }

    pub fn role(&self, workspace: &str, subject: &str) -> Result<Option<String>, Error> {
        Ok(self.lock()?.query_row("SELECT role FROM members WHERE workspace = ?1 AND subject = ?2", [workspace, subject], |r| r.get(0)).optional()?)
    }

    /// New connection generation; revokes the previous one, keeps installation identity and history (C1).
    pub fn grant(&self, inst: &Installation, workspace: &str, account: &str, scopes: &str, verifier: &[u8], now: i64) -> Result<Grant, Error> {
        self.write(|tx| grant(tx, inst, workspace, account, scopes, None, None, verifier, now))
    }

    /// Revokes the live generation; true when one was live. Uses the control reserve.
    pub fn revoke(&self, installation: &str, now: i64) -> Result<bool, Error> {
        self.write(|tx| {
            Ok(tx.execute("INSERT OR IGNORE INTO revocations SELECT installation, generation, ?2 FROM grants WHERE installation = ?1", params![installation, now])? > 0)
        })
    }

    /// Browser revoke (C1): revokes the live grant of `installation` in `workspace` when `subject` administers that
    /// workspace or approved the grant; false when nothing live is visible to it.
    pub fn revoke_as(&self, installation: &str, workspace: &str, subject: &str, now: i64) -> Result<bool, Error> {
        self.write(|tx| {
            Ok(tx.execute(
                "INSERT OR IGNORE INTO revocations SELECT g.installation, g.generation, ?4 FROM grants g WHERE g.installation = ?1 AND g.workspace = ?2
                   AND EXISTS (SELECT 1 FROM members m WHERE m.workspace = g.workspace AND m.subject = ?3 AND (m.role = 'admin' OR g.account = ?3))",
                params![installation, workspace, subject, now],
            )? > 0)
        })
    }

    /// Checked on every request: no cache can extend a revoked grant.
    pub fn authorize(&self, verifier: &[u8]) -> Result<Option<Grant>, Error> {
        authorize(&*self.lock()?, verifier)
    }

    /// Grants or removes the explicit answer capability (answers.md); separate from membership and every grant.
    pub fn set_answerer(&self, workspace: &str, subject: &str, allow: bool) -> Result<(), Error> {
        self.write(|tx| {
            if allow {
                tx.execute("INSERT OR IGNORE INTO answerers VALUES (?1, ?2)", [workspace, subject])?;
            } else {
                tx.execute("DELETE FROM answerers WHERE workspace = ?1 AND subject = ?2", [workspace, subject])?;
            }
            Ok(())
        })
    }

    /// True when `subject` administers `workspace` and holds its answer capability; read on every request.
    pub fn can_answer(&self, workspace: &str, subject: &str) -> Result<bool, Error> {
        Ok(self.lock()?.query_row(
            "SELECT EXISTS (SELECT 1 FROM members m JOIN answerers a ON a.workspace = m.workspace AND a.subject = m.subject WHERE m.workspace = ?1 AND m.subject = ?2 AND m.role = 'admin')",
            [workspace, subject],
            |r| r.get(0),
        )?)
    }

    /// Task controls require a separate explicit grant; answer/enrollment authority never implies it.
    pub fn set_controller(&self, workspace: &str, subject: &str, allow: bool) -> Result<(), Error> {
        self.write(|tx| {
            if allow { tx.execute("INSERT OR IGNORE INTO controllers VALUES (?1, ?2)", [workspace, subject])?; }
            else { tx.execute("DELETE FROM controllers WHERE workspace = ?1 AND subject = ?2", [workspace, subject])?; }
            Ok(())
        })
    }

    pub fn can_control(&self, workspace: &str, subject: &str) -> Result<bool, Error> {
        Ok(self.lock()?.query_row(
            "SELECT EXISTS (SELECT 1 FROM members m JOIN controllers c ON c.workspace = m.workspace AND c.subject = m.subject WHERE m.workspace = ?1 AND m.subject = ?2 AND m.role = 'admin')",
            [workspace, subject], |r| r.get(0),
        )?)
    }

    /// A control must pass its canonical row check before it acquires a pending dispatch identity.
    pub fn has_answer(&self, op_id: &str) -> Result<bool, Error> {
        Ok(self.lock()?.query_row("SELECT EXISTS (SELECT 1 FROM answers WHERE op_id = ?1)", [op_id], |r| r.get(0))?)
    }

    /// Records `op` as `pending` unless its op ID exists; returns the existing record instead.
    pub fn begin_answer(&self, op: &AnswerOp, now: i64) -> Result<Option<AnswerOp>, Error> {
        self.write(|tx| {
            let seen = tx
                .query_row("SELECT op_id, subject, workspace, board, task, revision, payload, state, receipt FROM answers WHERE op_id = ?1", [&op.op_id], |r| {
                    Ok(AnswerOp { op_id: r.get(0)?, subject: r.get(1)?, workspace: r.get(2)?, board: r.get(3)?, task: r.get(4)?, revision: r.get(5)?, payload: r.get(6)?, state: r.get(7)?, receipt: r.get(8)? })
                })
                .optional()?;
            if seen.is_none() {
                tx.execute(
                    "INSERT INTO answers VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', NULL, ?8)",
                    params![op.op_id, op.subject, op.workspace, op.board, op.task, op.revision, op.payload, now],
                )?;
            }
            Ok(seen)
        })
    }

    /// Stores the outcome and sanitized receipt of an operation.
    pub fn finish_answer(&self, op_id: &str, state: &str, receipt: &str, now: i64) -> Result<(), Error> {
        self.write(|tx| Ok(tx.execute("UPDATE answers SET state = ?2, receipt = ?3, at = ?4 WHERE op_id = ?1", params![op_id, state, receipt, now]).map(|_| ())?))
    }

    /// Queues one operation (D16, coreq `/machine/operations`) for its installation and generation under the server's own
    /// authority: `subject` is the verified account, `op.authority.actor` must be its actor ID, `op.authority.workspace`
    /// the live grant's, and the subject that workspace's administrator or the grant's approving member. The payload
    /// is never read for authority. One unresolved apply per installation; full storage refuses.
    pub fn queue(&self, subject: &str, op: &serde_json::Value, now: i64) -> Result<Queued, Error> {
        use coreq::enroll::{MAX_OPERATIONS, MAX_PAYLOAD, digest};
        let l = self.limits;
        let (a, s) = (&op["authority"], |k: &str| op[k].as_str().unwrap_or_default().to_string());
        let (id, inst, cap, dig) = (s("operation"), s("installation"), s("capability"), s("digest"));
        let (Some(generation), Some(expires)) = (op["generation"].as_i64(), op["expires_at"].as_i64()) else { return Ok(Queued::Refused("invalid")) };
        if id.len() != 32 || !id.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) || !op["payload"].is_object() || !a.is_object() {
            return Ok(Queued::Refused("invalid"));
        }
        if op["payload"].to_string().len() > MAX_PAYLOAD {
            return Ok(Queued::Refused("payload_too_large"));
        }
        if dig != digest(&op["payload"]) {
            return Ok(Queued::Refused("digest_mismatch"));
        }
        let profiles = a["profiles"].as_array().map(Vec::as_slice).unwrap_or_default();
        let pid = |p: &serde_json::Value| p.as_str().is_some_and(|p| (1..=64).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric() || b"._:-".contains(&b)));
        if a["actor"] != crate::enroll::actor_id(subject).as_str() || !(1..=64).contains(&profiles.len()) || !profiles.iter().all(pid) || profiles.iter().enumerate().any(|(i, p)| profiles[..i].contains(p)) {
            return Ok(Queued::Refused("authority"));
        }
        if expires <= now || expires - now > OPERATION_SECS {
            return Ok(Queued::Refused("invalid_expiry"));
        }
        let text = op.to_string();
        let apply = op["payload"]["kind"] == "apply";
        self.write(|tx| {
            if let Some(stored) = tx.query_row("SELECT op FROM operations WHERE operation = ?1", [&id], |r| r.get::<_, String>(0)).optional()? {
                return Ok(if stored == text { Queued::Same } else { Queued::Refused("replay_mismatch") });
            }
            let live: Option<(String, String, Option<String>)> = tx
                .query_row(
                    "SELECT workspace, account, capabilities FROM grants g WHERE installation = ?1 AND generation = ?2
                       AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)
                       AND EXISTS (SELECT 1 FROM activations x WHERE x.installation = g.installation AND x.generation = g.generation)",
                    params![inst, generation],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((workspace, account, caps)) = live else { return Ok(Queued::Refused("no_grant")) };
            if !serde_json::from_str::<Vec<String>>(caps.as_deref().unwrap_or("[]")).unwrap_or_default().contains(&cap) {
                return Ok(Queued::Refused("scope"));
            }
            let role: Option<String> = tx.query_row("SELECT role FROM members WHERE workspace = ?1 AND subject = ?2", [&workspace, subject], |r| r.get(0)).optional()?;
            if a["workspace"] != workspace.as_str() || !(role.as_deref() == Some("admin") || (role.is_some() && account == subject)) {
                return Ok(Queued::Refused("authority"));
            }
            // unresolved, in any generation: no result yet, generation not revoked, and either pulled or still
            // deliverable; a revoke resolves a delivered operation as rejected (#147): no result can arrive after it
            let (pending, unresolved, applying): (i64, i64, i64) = tx.query_row(
                "SELECT COALESCE(SUM(generation = ?2 AND expires_at > ?3), 0), COUNT(*), COALESCE(SUM(apply = 1), 0)
                 FROM operations o WHERE installation = ?1 AND result IS NULL AND (delivered = 1 OR expires_at > ?3)
                   AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = o.installation AND r.generation = o.generation)",
                params![inst, generation, now],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )?;
            if pending >= MAX_OPERATIONS as i64 || unresolved >= MAX_UNRESOLVED || (apply && applying > 0) {
                return Ok(Queued::Refused("busy"));
            }
            if used(tx, &Self::path(tx))? + l.reserve_bytes >= l.store_bytes {
                return Ok(Queued::Refused("store_full"));
            }
            tx.execute(
                "INSERT INTO operations (operation, installation, generation, capability, workspace, actor, apply, digest, expires_at, op, queued, delivered) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 0)",
                params![id, inst, generation, cap, workspace, a["actor"].as_str(), apply, dig, expires, text, now],
            )?;
            Ok(Queued::Queued)
        })
    }

    /// The live, acknowledged grant of `installation` and the current snapshot payload of its workspace.
    pub fn live(&self, installation: &str) -> Result<Option<(Grant, Option<String>)>, Error> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT g.generation, g.workspace, g.account, g.scopes, g.board, g.capabilities, s.payload FROM grants g
                 LEFT JOIN snapshots s ON s.installation = g.installation AND s.workspace = g.workspace
                 WHERE g.installation = ?1 AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)
                   AND EXISTS (SELECT 1 FROM activations x WHERE x.installation = g.installation AND x.generation = g.generation)
                 ORDER BY g.generation DESC LIMIT 1",
                [installation],
                |r| {
                    let g = Grant { installation: installation.into(), generation: r.get(0)?, workspace: r.get(1)?, account: r.get(2)?, scopes: r.get(3)?, board: r.get(4)?, capabilities: r.get(5)? };
                    Ok((g, r.get(6)?))
                },
            )
            .optional()?)
    }

    /// `/machine/operations`: pending operations of `capability` for the credential's own installation and generation (no
    /// result, not expired), at most 16, marked delivered in the same commit. `None`: capability outside the grant.
    pub fn pull(&self, verifier: &[u8], capability: &str, now: i64) -> Result<Option<Vec<serde_json::Value>>, Error> {
        self.write(|tx| {
            let g = authorize(tx, verifier)?.ok_or(Error::Denied)?;
            if !serde_json::from_str::<Vec<String>>(g.capabilities.as_deref().unwrap_or("[]")).unwrap_or_default().iter().any(|c| c == capability) {
                return Ok(None);
            }
            let mut q = tx.prepare(
                "SELECT operation, op FROM operations WHERE installation = ?1 AND generation = ?2 AND capability = ?3 AND result IS NULL AND expires_at > ?4
                 ORDER BY queued, operation LIMIT ?5",
            )?;
            let rows: Vec<(String, String)> = q.query_map(params![g.installation, g.generation, capability, now, coreq::enroll::MAX_OPERATIONS as i64], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
            drop(q);
            for (id, _) in &rows {
                tx.execute("UPDATE operations SET delivered = 1 WHERE operation = ?1", [id])?;
            }
            Ok(Some(rows.into_iter().map(|(_, op)| serde_json::from_str(&op).unwrap_or_default()).collect()))
        })
    }

    /// `/machine/operations/result`: stores the first result of the credential's own operation and binds its digest. `None`
    /// is `accepted` (committed before the reply; the same result again repeats it), else the rejection reason. A
    /// result uses the control reserve, so a full store still resolves operations.
    pub fn post_result(&self, verifier: &[u8], operation: &str, digest: &str, result: &str, now: i64) -> Result<Option<&'static str>, Error> {
        self.write(|tx| {
            let g = authorize(tx, verifier)?.ok_or(Error::Denied)?;
            let row: Option<(String, Option<String>)> = tx
                .query_row("SELECT digest, result FROM operations WHERE operation = ?1 AND installation = ?2 AND generation = ?3", params![operation, g.installation, g.generation], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            Ok(match row {
                None => Some("unknown_operation"),
                Some((d, _)) if d != digest => Some("digest_mismatch"),
                Some((_, Some(r))) => (r != result).then_some("replay_mismatch"),
                Some((_, None)) => {
                    tx.execute("UPDATE operations SET result = ?2, result_at = ?3 WHERE operation = ?1", params![operation, result, now])?;
                    None
                }
            })
        })
    }

    /// One stored operation with its delivery, grant and result state.
    pub fn operation(&self, id: &str) -> Result<Option<OpRecord>, Error> {
        Ok(self
            .lock()?
            .query_row(
                "SELECT op, workspace, actor, delivered, EXISTS (SELECT 1 FROM revocations r WHERE r.installation = o.installation AND r.generation = o.generation), result, result_at
                 FROM operations o WHERE operation = ?1",
                [id],
                |r| {
                    let (op, result): (String, Option<String>) = (r.get(0)?, r.get(5)?);
                    Ok(OpRecord {
                        op: serde_json::from_str(&op).unwrap_or_default(),
                        workspace: r.get(1)?,
                        actor: r.get(2)?,
                        delivered: r.get(3)?,
                        revoked: r.get(4)?,
                        result: result.and_then(|x| serde_json::from_str(&x).ok()),
                        result_at: r.get(6)?,
                    })
                },
            )
            .optional()?)
    }

    /// IDs of the newest `limit` operations queued for `installation`, newest first.
    pub fn operation_ids(&self, installation: &str, limit: i64) -> Result<Vec<String>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT operation FROM operations WHERE installation = ?1 ORDER BY queued DESC, rowid DESC LIMIT ?2")?;
        let rows = q.query_map(params![installation, limit], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Workspaces where `subject` is a member, with its role.
    pub fn workspaces(&self, subject: &str) -> Result<Vec<(String, String)>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT workspace, role FROM members WHERE subject = ?1 ORDER BY workspace")?;
        let rows = q.query_map([subject], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// New pairing session (C3). `TooMany` at `max_pending` live sessions, `Exists` on a user code collision.
    #[allow(clippy::too_many_arguments)]
    pub fn pair(&self, device: &[u8], user_code: &str, inst: &Installation, schemas: &str, board: Option<&str>, capabilities: Option<&str>, now: i64, max_pending: i64) -> Result<(), Error> {
        self.write(|tx| {
            expire(tx, now)?;
            if tx.query_row("SELECT COUNT(*) FROM pairings WHERE state = 'pending' AND expires > ?1", [now], |r| r.get::<_, i64>(0))? >= max_pending {
                return Err(Error::TooMany);
            }
            if tx.query_row("SELECT 1 FROM pairings WHERE user_code = ?1 OR device = ?2", params![user_code, device], |_| Ok(())).optional()?.is_some() {
                return Err(Error::Exists);
            }
            tx.execute(
                "INSERT INTO pairings (device, user_code, product, installation, runtime, label, schemas, expires, interval, polled, failures, state, board, capabilities) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 5, ?9, 0, 'pending', ?10, ?11)",
                params![device, user_code, inst.product, inst.id, inst.runtime, inst.label, schemas, now + PAIRING_SECS, now - 5, board, capabilities],
            )?;
            Ok(())
        })
    }

    /// The live pending session of a user code, for the approval page.
    pub fn pending(&self, user_code: &str, now: i64) -> Result<Option<Pairing>, Error> {
        Ok(self
            .lock()?
            .query_row("SELECT product, installation, runtime, label, schemas, EXISTS (SELECT 1 FROM grants g WHERE g.installation = p.installation
               AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)), p.board, p.capabilities
             FROM pairings p WHERE user_code = ?1 AND state = 'pending' AND expires > ?2", params![user_code, now], |r| {
                Ok(Pairing { product: r.get(0)?, installation: r.get(1)?, runtime: r.get(2)?, label: r.get(3)?, schemas: r.get(4)?, replaces: r.get(5)?, board: r.get(6)?, capabilities: r.get(7)? })
            })
            .optional()?)
    }

    /// Approves or denies a user code as `subject` in `workspace`. Approval grants and stores the credential sealed
    /// by `seal(device)` in the same transaction, so a crash leaves no grant without its recoverable token response.
    /// The grant keeps only the requested capabilities the approver also ticked in `granted` (D16): never more.
    #[allow(clippy::too_many_arguments)]
    pub fn decide(&self, user_code: &str, approve: bool, workspace: &str, subject: &str, granted: &[&str], verifier: &[u8], seal: &dyn Fn(&[u8]) -> Vec<u8>, now: i64) -> Result<Decision, Error> {
        self.write(|tx| {
            expire(tx, now)?;
            type Row = (Vec<u8>, String, String, String, String, String, String, Option<String>, Option<String>);
            let row: Option<Row> = tx
                .query_row("SELECT device, state, product, installation, runtime, label, schemas, board, capabilities FROM pairings WHERE user_code = ?1 AND expires > ?2", params![user_code, now], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
                })
                .optional()?;
            let Some((device, state, product, id, runtime, label, schemas, board, requested)) = row else { return Ok(Decision::Invalid) };
            match state.as_str() {
                "approved" | "acked" if approve => return Ok(Decision::Already),
                "pending" => {}
                _ => return Ok(Decision::Invalid),
            }
            if tx.query_row("SELECT 1 FROM members WHERE workspace = ?1 AND subject = ?2", [workspace, subject], |_| Ok(())).optional()?.is_none() {
                tx.execute(
                    "UPDATE pairings SET failures = failures + 1, state = CASE WHEN failures + 1 >= ?2 THEN 'invalid' ELSE state END WHERE device = ?1",
                    params![device, MAX_FAILURES],
                )?;
                return Ok(Decision::NotMember);
            }
            if !approve {
                tx.execute("UPDATE pairings SET state = 'denied' WHERE device = ?1", [&device])?;
                return Ok(Decision::Denied);
            }
            let inst = Installation { id: &id, product: &product, runtime: &runtime, label: &label };
            let caps = requested.map(|r| {
                let r: Vec<String> = serde_json::from_str(&r).unwrap_or_default();
                serde_json::to_string(&r.into_iter().filter(|c| granted.contains(&c.as_str())).collect::<Vec<_>>()).expect("strings")
            });
            let g = grant(tx, &inst, workspace, subject, &schemas, board.as_deref(), caps.as_deref(), verifier, now)?;
            tx.execute("UPDATE pairings SET state = 'approved', workspace = ?2, generation = ?3, sealed = ?4 WHERE device = ?1", params![device, workspace, g.generation, seal(&device)])?;
            Ok(Decision::Approved(g))
        })
    }

    /// `/machine/connect/token`: RFC 8628 polling with slow-down; the approved token response repeats until ack or expiry.
    pub fn poll(&self, device: &[u8], now: i64) -> Result<Poll, Error> {
        self.write(|tx| {
            expire(tx, now)?;
            let row: Option<(String, i64, i64, i64, String, String, Option<Vec<u8>>, Option<i64>, Option<String>)> = tx
                .query_row("SELECT state, expires, interval, polled, product, installation, sealed, generation, schemas FROM pairings WHERE device = ?1", [device], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?))
                })
                .optional()?;
            let Some((state, expires, interval, polled, product, id, sealed, generation, schemas)) = row else { return Ok(Poll::Invalid) };
            Ok(match state.as_str() {
                "acked" => Poll::Invalid,
                _ if now >= expires => Poll::Expired,
                "denied" => Poll::Denied,
                "invalid" => Poll::Invalid,
                "approved" => {
                    let g = tx.query_row("SELECT workspace, account, scopes, board, capabilities FROM grants WHERE installation = ?1 AND generation = ?2", params![id, generation], |r| {
                        Ok(Grant { installation: id.clone(), generation: generation.unwrap_or(0), workspace: r.get(0)?, account: r.get(1)?, scopes: r.get(2)?, board: r.get(3)?, capabilities: r.get(4)? })
                    })?;
                    Poll::Token { sealed: sealed.unwrap_or_default(), grant: g, product, schemas: schemas.unwrap_or_default() }
                }
                _ if now - polled < interval => {
                    tx.execute("UPDATE pairings SET interval = interval + 5, polled = ?2 WHERE device = ?1", params![device, now])?;
                    Poll::SlowDown
                }
                _ => {
                    tx.execute("UPDATE pairings SET polled = ?2 WHERE device = ?1", params![device, now])?;
                    Poll::Pending
                }
            })
        })
    }

    /// A credential whether live or revoked; `None` when unknown.
    pub fn credential(&self, verifier: &[u8]) -> Result<Option<Credential>, Error> {
        credential(&*self.lock()?, verifier)
    }

    /// `/machine/connect/ack`: activates the generation and erases its sealed token response. Repeats are the same.
    pub fn ack(&self, verifier: &[u8], now: i64) -> Result<Option<Credential>, Error> {
        self.write(|tx| {
            let Some(c) = credential(tx, verifier)? else { return Ok(None) };
            if !c.revoked {
                let g = &c.grant;
                tx.execute("INSERT OR IGNORE INTO activations VALUES (?1, ?2, ?3)", params![g.installation, g.generation, now])?;
                tx.execute("UPDATE pairings SET state = 'acked', sealed = NULL WHERE installation = ?1 AND generation = ?2", params![g.installation, g.generation])?;
                tx.execute("INSERT OR IGNORE INTO revocations SELECT installation, generation, ?3 FROM grants WHERE installation = ?1 AND generation < ?2", params![g.installation, g.generation, now])?;
            }
            credential(tx, verifier)
        })
    }

    /// `/machine/connection/revoke`: revokes this credential's generation only, never a newer one.
    pub fn revoke_credential(&self, verifier: &[u8], now: i64) -> Result<Option<Credential>, Error> {
        self.write(|tx| {
            let Some(c) = credential(tx, verifier)? else { return Ok(None) };
            tx.execute("INSERT OR IGNORE INTO revocations VALUES (?1, ?2, ?3)", params![c.grant.installation, c.grant.generation, now])?;
            tx.execute("UPDATE pairings SET sealed = NULL WHERE installation = ?1 AND generation = ?2", params![c.grant.installation, c.grant.generation])?;
            credential(tx, verifier)
        })
    }

    /// Stores a batch atomically. Ownership comes from the credential's grant; a schema outside the grant is
    /// rejected. Replays of `(generation, event id)` return the first outcome; changed content is rejected.
    pub fn accept(&self, verifier: &[u8], events: &[Event], now: i64) -> Result<Vec<Outcome>, Error> {
        if events.len() > self.limits.records {
            return Err(Error::TooMany);
        }
        let l = self.limits;
        self.write(|tx| {
            let g = authorize(tx, verifier)?.ok_or(Error::Denied)?;
            let scopes: Vec<String> = serde_json::from_str(&g.scopes).unwrap_or_default();
            let full = used(tx, &Self::path(tx))? + l.reserve_bytes >= l.store_bytes;
            let mut outbox = tx.query_row("SELECT COALESCE(SUM(length(payload)), 0) FROM outbox", [], |r| r.get::<_, i64>(0))? as u64;
            let mut out = Vec::with_capacity(events.len());
            for e in events {
                let kind = match e.kind { Kind::Snapshot => "snapshot", Kind::Export => "export", Kind::Telemetry => "telemetry", Kind::Resources => "resources" };
                let content = format!("{kind}\n{}\n{}\n{}", e.schema, e.observed, e.payload);
                let seen: Option<(String, String)> = tx
                    .query_row(
                        "SELECT content, outcome FROM receipts WHERE installation = ?1 AND generation = ?2 AND event_id = ?3",
                        params![g.installation, g.generation, e.id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                let o = match seen {
                    Some((c, o)) if c == content => outcome(&o),
                    Some(_) => Outcome::Rejected("replay_mismatch"),
                    None if !scopes.iter().any(|s| s == e.schema) => Outcome::Rejected("scope"),
                    // a dedup record may be pruned already: refuse instead of duplicating
                    None if e.observed < now - l.receipt_secs => Outcome::Rejected("expired"),
                    // an export Langfuse cannot time (after 2554) is refused before acknowledgement
                    None if e.kind == Kind::Export && crate::export::nanos(e.observed).is_none() => Outcome::Rejected("payload"),
                    None if full => Outcome::Rejected("store_full"),
                    None if e.kind == Kind::Export && outbox + e.payload.len() as u64 > l.outbox_bytes => Outcome::Rejected("outbox_full"),
                    None if e.kind == Kind::Telemetry && e.payload.len() as u64 > l.telemetry_bytes => Outcome::Rejected("payload"),
                    None => {
                        if e.kind == Kind::Telemetry {
                            let p = serde_json::from_str::<serde_json::Value>(e.payload).unwrap_or_default();
                            let seq = |k: &str| p[k]["seq"].as_i64().or(p[k].as_i64());
                            tx.execute("INSERT INTO telemetry (installation, workspace, generation, event_id, observed, received, status, payload, first_seq, last_seq, rows) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                                params![g.installation, g.workspace, g.generation, e.id, e.observed, now, p.get("status").map(|x| x.to_string()), e.payload, seq("first"), seq("last"), p["rows"].as_i64()])?;
                            // lines dropped at ingest (outside the allowlist) are known loss like pruned ones (#181)
                            if let Some(n) = p["dropped"].as_i64().filter(|n| *n > 0) {
                                tx.execute(
                                    "INSERT INTO telemetry_pruned VALUES (?1, ?2, 'allowlist', 1, ?3, ?4, ?5)
                                     ON CONFLICT (installation, workspace, cause) DO UPDATE SET blocks = blocks + 1, rows = rows + excluded.rows,
                                       through_seq = MAX(COALESCE(through_seq, excluded.through_seq), COALESCE(excluded.through_seq, through_seq)), at = excluded.at",
                                    params![g.installation, g.workspace, n, seq("last"), now],
                                )?;
                            }
                            // one budget per installation and workspace: a busy host never evicts another's blocks;
                            // the oldest leave first and their loss stays counted
                            let over: Vec<i64> = tx
                                .prepare("SELECT id FROM (SELECT id, SUM(length(payload)) OVER (ORDER BY id DESC) AS total FROM telemetry WHERE installation = ?1 AND workspace = ?2) WHERE total > ?3")?
                                .query_map(params![g.installation, g.workspace, l.telemetry_bytes as i64], |r| r.get(0))?
                                .collect::<Result<_, _>>()?;
                            drop_blocks(tx, &over, "capacity", now)?;
                        } else if e.kind == Kind::Resources {
                            // the newest projection replaces the last one whole: a project taskq stops listing leaves;
                            // a new generation's first record always replaces the old generation's
                            tx.execute("INSERT INTO resources VALUES (?1, ?2, ?3, ?4, ?5, ?6) ON CONFLICT (installation, workspace) DO UPDATE SET generation = excluded.generation, event_id = excluded.event_id, observed = excluded.observed, payload = excluded.payload WHERE excluded.observed >= resources.observed OR excluded.generation > resources.generation",
                                params![g.installation, g.workspace, g.generation, e.id, e.observed, e.payload])?;
                        } else if e.kind == Kind::Export {
                            outbox += e.payload.len() as u64;
                            tx.execute("INSERT INTO outbox (installation, generation, event_id, observed, queued, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![g.installation, g.generation, e.id, e.observed, now, e.payload])?;
                        } else {
                            let uncertain = e.observed > now + l.future_secs;
                            tx.execute("INSERT INTO history VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![g.installation, g.workspace, e.id, e.observed, uncertain, e.payload])?;
                            // late samples join history only; an uncertain current value yields to any sample
                            let current: Option<(i64, bool)> = tx
                                .query_row("SELECT observed, clock_uncertain FROM snapshots WHERE installation = ?1 AND workspace = ?2", [&g.installation, &g.workspace], |r| Ok((r.get(0)?, r.get(1)?)))
                                .optional()?;
                            if current.is_none_or(|(t, u)| u || e.observed >= t) {
                                tx.execute(
                                    "INSERT OR REPLACE INTO snapshots VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                    params![g.installation, g.workspace, g.generation, e.id, e.schema, e.observed, uncertain, e.payload],
                                )?;
                            }
                        }
                        tx.execute("INSERT INTO receipts VALUES (?1, ?2, ?3, ?4, ?5, 'accepted', ?6)", params![g.installation, g.generation, e.id, e.schema, content, now])?;
                        Outcome::Accepted
                    }
                };
                out.push(o);
            }
            Ok(out)
        })
    }

    /// A device reads only its own generation's receipts: `(schema, outcome)`.
    pub fn receipt(&self, verifier: &[u8], event_id: &str) -> Result<Option<(String, Outcome)>, Error> {
        let c = self.lock()?;
        let g = authorize(&c, verifier)?.ok_or(Error::Denied)?;
        let o: Option<(String, String)> = c
            .query_row(
                "SELECT schema, outcome FROM receipts WHERE installation = ?1 AND generation = ?2 AND event_id = ?3",
                params![g.installation, g.generation, event_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        Ok(o.map(|(s, o)| (s, outcome(&o))))
    }

    /// Stored telemetry blocks named `event_id` among the `(installation, workspace)` pairs the caller may read, and
    /// the receipt times of accepted ones that are no longer stored (#181 A6).
    pub fn telemetry_block(&self, event_id: &str, pairs: &[(String, String)]) -> Result<(Vec<Block>, Vec<(String, String, i64)>), Error> {
        let c = self.lock()?;
        let mut q = c.prepare(
            "SELECT t.installation, t.workspace, t.generation, t.event_id, t.observed, t.received, r.outcome, t.payload FROM telemetry t
             LEFT JOIN receipts r ON r.installation = t.installation AND r.generation = t.generation AND r.event_id = t.event_id
             WHERE t.event_id = ?1 ORDER BY t.id",
        )?;
        let mut blocks: Vec<Block> = q
            .query_map([event_id], |r| {
                Ok(Block { installation: r.get(0)?, workspace: r.get(1)?, generation: r.get(2)?, event_id: r.get(3)?, observed: r.get(4)?, received: r.get(5)?, receipt: r.get(6)?, payload: r.get(7)?, coverage: Telemetry::default() })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        blocks.retain(|b| pairs.iter().any(|(i, w)| *i == b.installation && *w == b.workspace));
        for b in &mut blocks {
            b.coverage = coverage(&c, &b.installation, &b.workspace)?;
        }
        let mut q = c.prepare(
            "SELECT r.installation, g.workspace, r.at FROM receipts r JOIN grants g ON g.installation = r.installation AND g.generation = r.generation
             WHERE r.event_id = ?1 AND r.schema = 'taskq-telemetry/1'
               AND NOT EXISTS (SELECT 1 FROM telemetry t WHERE t.installation = r.installation AND t.generation = r.generation AND t.event_id = r.event_id)",
        )?;
        let gone = q
            .query_map([event_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?
            .filter(|x| x.as_ref().map_or(true, |(i, w, _)| pairs.iter().any(|p| p.0 == *i && p.1 == *w)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok((blocks, gone))
    }

    /// Stored telemetry blocks of one installation and workspace received at or after `since`, oldest first, as
    /// `(id, payload)`; the payload only where `need(id)`, since blocks never change and callers decode each once (#182).
    pub fn telemetry_since(&self, installation: &str, workspace: &str, since: i64, need: impl Fn(i64) -> bool) -> Result<Vec<(i64, Option<String>)>, Error> {
        let c = self.lock()?;
        let ids: Vec<i64> = c
            .prepare_cached("SELECT id FROM telemetry WHERE installation = ?1 AND workspace = ?2 AND received >= ?3 ORDER BY id")?
            .query_map(params![installation, workspace, since], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let mut q = c.prepare_cached("SELECT payload FROM telemetry WHERE id = ?1")?;
        ids.into_iter().map(|id| Ok((id, if need(id) { Some(q.query_row([id], |r| r.get(0))?) } else { None }))).collect()
    }

    /// The newest current snapshot of an installation in any workspace.
    pub fn snapshot(&self, installation: &str) -> Result<Option<Snapshot>, Error> {
        Ok(self
            .lock()?
            .query_row("SELECT event_id, schema, observed, clock_uncertain, payload, generation FROM snapshots WHERE installation = ?1 ORDER BY observed DESC LIMIT 1", [installation], |r| {
                Ok(Snapshot { event_id: r.get(0)?, schema: r.get(1)?, observed: r.get(2)?, clock_uncertain: r.get(3)?, payload: r.get(4)?, generation: r.get(5)? })
            })
            .optional()?)
    }

    /// The dashboard projection (D11): every expected installation in `scope`, one row per installation and
    /// workspace from its newest grant there, with that workspace's current snapshot or none. Receipts of the
    /// snapshots read here that a view `renders` (product, snapshot) become `projected` in the same transaction, so
    /// `projected` means this read showed it; a snapshot no view renders stays `accepted`.
    pub fn project(&self, scope: Scope, renders: impl Fn(&str, &Snapshot) -> bool) -> Result<Vec<Row>, Error> {
        let subject = match scope {
            Scope::All => None,
            Scope::Subject(s) => Some(s),
        };
        self.write(|tx| {
            let mut q = tx.prepare(
                "SELECT i.id, i.product, i.runtime, i.label, g.workspace, g.generation,
                   EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation),
                   EXISTS (SELECT 1 FROM activations a WHERE a.installation = g.installation AND a.generation = g.generation),
                   s.event_id, s.schema, s.observed, s.clock_uncertain, s.payload, s.generation,
                   (SELECT COUNT(*) FROM history h WHERE h.installation = g.installation AND h.workspace = g.workspace AND NOT h.clock_uncertain),
                   (SELECT COALESCE(SUM(events), 0) FROM export_gaps x WHERE x.installation = g.installation AND x.workspace = g.workspace),
                   (SELECT MAX(at) FROM receipts r WHERE r.installation = g.installation AND r.generation = g.generation), g.scopes, g.board, g.capabilities,
                   (SELECT generation FROM resources x WHERE x.installation = g.installation AND x.workspace = g.workspace),
                   (SELECT payload FROM resources x WHERE x.installation = g.installation AND x.workspace = g.workspace)
                 FROM grants g JOIN installations i ON i.id = g.installation
                 LEFT JOIN snapshots s ON s.installation = g.installation AND s.workspace = g.workspace
                 WHERE g.generation = (SELECT MAX(generation) FROM grants x WHERE x.installation = g.installation AND x.workspace = g.workspace)
                   AND (?1 IS NULL OR EXISTS (SELECT 1 FROM members m WHERE m.workspace = g.workspace AND m.subject = ?1 AND (m.role = 'admin' OR g.account = ?1)))
                 ORDER BY i.label, i.id, g.workspace",
            )?;
            let mut rows: Vec<Row> = q
                .query_map([subject], |r| {
                    let (revoked, active): (bool, bool) = (r.get(6)?, r.get(7)?);
                    let snapshot = match r.get::<_, Option<String>>(8)? {
                        Some(event_id) => Some(Snapshot { event_id, schema: r.get(9)?, observed: r.get(10)?, clock_uncertain: r.get(11)?, payload: r.get(12)?, generation: r.get(13)? }),
                        None => None,
                    };
                    let row = Row {
                        installation: r.get(0)?,
                        product: r.get(1)?,
                        runtime: r.get(2)?,
                        label: r.get(3)?,
                        workspace: r.get(4)?,
                        generation: r.get(5)?,
                        connection: if revoked { "revoked" } else if active { "active" } else { "pending" },
                        snapshot,
                        samples: r.get(14)?,
                        export_gap: r.get(15)?,
                        received_at: r.get(16)?,
                        scopes: r.get(17)?,
                        board: r.get(18)?,
                        capabilities: r.get(19)?,
                        telemetry: Telemetry::default(),
                        resources: r.get::<_, Option<i64>>(20)?.zip(r.get::<_, Option<String>>(21)?),
                    };
                    Ok(row)
                })?
                .collect::<Result<_, _>>()?;
            drop(q);
            for row in &mut rows {
                row.telemetry = coverage(tx, &row.installation, &row.workspace)?;
            }
            for row in &rows {
                if let Some(s) = row.snapshot.as_ref().filter(|s| renders(&row.product, s)) {
                    tx.execute(
                        "UPDATE receipts SET outcome = 'projected' WHERE installation = ?1 AND generation = ?2 AND event_id = ?3",
                        params![row.installation, s.generation, s.event_id],
                    )?;
                }
            }
            Ok(rows)
        })
    }

    /// `(observed, payload)` samples in time order.
    pub fn history(&self, installation: &str) -> Result<Vec<(i64, String)>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT observed, payload FROM history WHERE installation = ?1 ORDER BY observed")?;
        let rows = q.query_map([installation], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// `(observed, event_id, payload)` of one installation and workspace with a trusted clock since `since`, newest
    /// `cap` in time order, and whether older samples in the window were left out.
    pub fn samples(&self, installation: &str, workspace: &str, since: i64, cap: usize) -> Result<(Vec<(i64, String, String)>, bool), Error> {
        let c = self.lock()?;
        let mut q = c.prepare(
            "SELECT observed, event_id, payload FROM history WHERE installation = ?1 AND workspace = ?2 AND observed >= ?3 AND NOT clock_uncertain
             ORDER BY observed DESC LIMIT ?4",
        )?;
        let mut rows: Vec<(i64, String, String)> = q.query_map(params![installation, workspace, since, cap as i64 + 1], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
        let capped = rows.len() > cap;
        rows.truncate(cap);
        rows.reverse();
        Ok((rows, capped))
    }

    /// The oldest outbox rows, at most `n`, in arrival order.
    pub fn outbox(&self, n: usize) -> Result<Vec<Pending>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT id, installation, generation, event_id, observed, payload FROM outbox ORDER BY id LIMIT ?1")?;
        let rows = q
            .query_map([n as i64], |r| Ok(Pending { id: r.get(0)?, installation: r.get(1)?, generation: r.get(2)?, event_id: r.get(3)?, observed: r.get(4)?, payload: r.get(5)? }))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Removes forwarded rows; rows the destination refused for good are recorded as export gaps first (C5).
    pub fn exported(&self, sent: &[i64], refused: &[i64], now: i64) -> Result<(), Error> {
        self.write(|tx| {
            for id in refused {
                // a gap belongs to the workspace of the grant that sent the event, as in `prune`
                tx.execute(
                    "INSERT INTO export_gaps (installation, events, first_observed, last_observed, at, workspace)
                     SELECT o.installation, 1, o.observed, o.observed, ?2, g.workspace FROM outbox o
                     JOIN grants g ON g.installation = o.installation AND g.generation = o.generation WHERE o.id = ?1",
                    params![id, now],
                )?;
            }
            for id in sent.iter().chain(refused) {
                tx.execute("DELETE FROM outbox WHERE id = ?1", [id])?;
            }
            Ok(())
        })
    }

    /// Removes rows of one grant that the destination accepted with `rejected` spans it did not name (OTLP
    /// `partialSuccess`): one export gap of `rejected` events over the rows' observation window, never per row.
    pub fn partially_exported(&self, rows: &[Pending], rejected: i64, now: i64) -> Result<(), Error> {
        let Some(first) = rows.first() else { return Ok(()) };
        let (lo, hi) = rows.iter().fold((i64::MAX, i64::MIN), |(lo, hi), r| (lo.min(r.observed), hi.max(r.observed)));
        self.write(|tx| {
            tx.execute(
                "INSERT INTO export_gaps (installation, events, first_observed, last_observed, at, workspace)
                 SELECT installation, ?3, ?4, ?5, ?6, workspace FROM grants WHERE installation = ?1 AND generation = ?2",
                params![first.installation, first.generation, rejected.min(rows.len() as i64), lo, hi, now],
            )?;
            for r in rows {
                tx.execute("DELETE FROM outbox WHERE id = ?1", [r.id])?;
            }
            Ok(())
        })
    }

    /// `(pending rows, export gap events)` for the export source.
    pub fn export_counts(&self) -> Result<(i64, i64), Error> {
        Ok(self.lock()?.query_row("SELECT (SELECT COUNT(*) FROM outbox), (SELECT COALESCE(SUM(events), 0) FROM export_gaps)", [], |r| Ok((r.get(0)?, r.get(1)?)))?)
    }

    /// `(installation, events)` recorded as export gaps.
    pub fn export_gaps(&self) -> Result<Vec<(String, i64)>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT installation, SUM(events) FROM export_gaps GROUP BY installation ORDER BY installation")?;
        let rows = q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn used(&self) -> Result<u64, Error> {
        let c = self.lock()?;
        used(&c, &Self::path(&c))
    }

    /// Retention: records expired exports as gaps before removing them, prunes history and
    /// receipts, then truncates the WAL. Grants and revocations are never pruned.
    pub fn prune(&self, now: i64) -> Result<(), Error> {
        let l = self.limits;
        self.write(|tx| {
            tx.execute("UPDATE pm_ops SET request=NULL WHERE expires<=?1", [now])?;
            let cut = now - l.outbox_secs;
            // a gap belongs to the workspace of the grant that sent its events
            tx.execute(
                "INSERT INTO export_gaps (installation, events, first_observed, last_observed, at, workspace)
                 SELECT o.installation, COUNT(*), MIN(o.observed), MAX(o.observed), ?2, g.workspace FROM outbox o
                 JOIN grants g ON g.installation = o.installation AND g.generation = o.generation WHERE o.queued < ?1 GROUP BY o.installation, g.workspace",
                params![cut, now],
            )?;
            tx.execute("DELETE FROM outbox WHERE queued < ?1", [cut])?;
            tx.execute("DELETE FROM export_gaps WHERE at < ?1", [now - l.history_secs])?;
            tx.execute("DELETE FROM history WHERE observed < ?1", [now - l.history_secs])?;
            let old: Vec<i64> = tx.prepare("SELECT id FROM telemetry WHERE received < ?1")?.query_map([now - l.telemetry_secs], |r| r.get(0))?.collect::<Result<_, _>>()?;
            drop_blocks(tx, &old, "retention", now)?;
            tx.execute("DELETE FROM receipts WHERE at < ?1", [now - l.receipt_secs])?;
            // resolved operations (a result, revoked generation, or expired never delivered) keep the newest per installation; unresolved stay
            tx.execute(
                "DELETE FROM operations WHERE (result IS NOT NULL OR (delivered = 0 AND expires_at <= ?1) OR EXISTS
                   (SELECT 1 FROM revocations r WHERE r.installation = operations.installation AND r.generation = operations.generation)) AND operation NOT IN
                   (SELECT k.operation FROM operations k WHERE k.installation = operations.installation
                      AND (k.result IS NOT NULL OR (k.delivered = 0 AND k.expires_at <= ?1) OR EXISTS
                        (SELECT 1 FROM revocations r WHERE r.installation = k.installation AND r.generation = k.generation)) ORDER BY k.queued DESC, k.operation DESC LIMIT ?2)",
                params![now, KEEP_RESOLVED],
            )?;
            Ok(())
        })?;
        self.lock()?.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        Ok(())
    }

    /// Consistent private copy; `dest` must not exist. Back it up with the deployment key (C4).
    pub fn backup(&self, dest: &Path) -> Result<(), Error> {
        // VACUUM INTO accepts an empty file, which keeps its private mode
        create_private(dest)?;
        let r = self.lock()?.execute("VACUUM INTO ?1", [dest.to_string_lossy()]);
        r.map(|_| ()).map_err(Error::from)
    }

    /// Verifies `backup` and copies it to a new private `path`; never overwrites a live store.
    pub fn restore(backup: &Path, path: &Path, limits: Limits) -> Result<Store, Error> {
        if path.exists() || Path::new(&format!("{}-wal", path.display())).exists() {
            return Err(Error::Exists);
        }
        check(&Connection::open_with_flags(backup, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?)?;
        create_private(path)?;
        std::fs::write(path, std::fs::read(backup)?)?;
        Store::open(path, limits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const NOW: i64 = 1_800_000_000;
    const SCOPES: &str = r#"["agentq.snapshot/1", "agentq.run/1"]"#;
    const AGENT: Installation = Installation { id: "inst-1", product: "agentq", runtime: "native", label: "fra" };

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dashq-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn snap<'a>(id: &'a str, observed: i64, payload: &'a str) -> Event<'a> {
        Event { id, schema: "agentq.snapshot/1", observed, kind: Kind::Snapshot, payload }
    }

    fn export<'a>(id: &'a str, payload: &'a str) -> Event<'a> {
        Event { id, schema: "agentq.run/1", observed: NOW, kind: Kind::Export, payload }
    }

    #[test]
    fn acknowledged_data_survives_crash_and_reopen() {
        let p = dir("crash").join("dashq.db");
        let s = Store::open(&p, LIMITS).unwrap();
        s.add_member("w", "github:1", "admin").unwrap();
        s.grant(&AGENT, "w", "github:1", SCOPES, b"v1", NOW).unwrap();
        s.grant(&Installation { id: "inst-2", ..AGENT }, "w", "github:1", SCOPES, b"v2", NOW).unwrap();
        assert!(s.revoke("inst-2", NOW).unwrap());
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW, "{}"), export("x1", "{}")], NOW).unwrap(), [Outcome::Accepted, Outcome::Accepted]);
        // durable, not yet read through the projection; the projection read makes it `projected`
        assert_eq!(s.receipt(b"v1", "e1").unwrap(), Some(("agentq.snapshot/1".into(), Outcome::Accepted)));
        assert_eq!(s.project(Scope::All, |_, _| true).unwrap().len(), 2);
        // crash: no close, no checkpoint; committed WAL frames must be enough
        std::mem::forget(s);
        let s = Store::open(&p, LIMITS).unwrap();
        assert_eq!(s.role("w", "github:1").unwrap().as_deref(), Some("admin"));
        assert_eq!(s.authorize(b"v1").unwrap().unwrap().generation, 1);
        assert_eq!(s.authorize(b"v2").unwrap(), None);
        assert_eq!(s.accept(b"v2", &[snap("e1", NOW, "{}")], NOW), Err(Error::Denied));
        assert_eq!(s.receipt(b"v1", "e1").unwrap(), Some(("agentq.snapshot/1".into(), Outcome::Projected)));
        assert_eq!(s.snapshot("inst-1").unwrap().unwrap().event_id, "e1");
        // the replay after a lost response returns the same outcome, no duplicate
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW, "{}")], NOW).unwrap(), [Outcome::Projected]);
        assert_eq!(s.history("inst-1").unwrap().len(), 1);
        // a schema outside the grant is rejected and not stored
        let other = Event { schema: "serverq-report/1", ..snap("e9", NOW, "{}") };
        assert_eq!(s.accept(b"v1", &[other], NOW).unwrap(), [Outcome::Rejected("scope")]);
        assert_eq!(s.receipt(b"v1", "e9").unwrap(), None);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&p).unwrap().permissions()) & 0o777, 0o600);
    }

    #[test]
    fn dedup_mismatch_late_and_future_samples() {
        let s = Store::open(&dir("dedup").join("dashq.db"), LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        assert_eq!(s.accept(b"v1", &[snap("e2", NOW, "new")], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.accept(b"v1", &[snap("e2", NOW, "changed")], NOW).unwrap(), [Outcome::Rejected("replay_mismatch")]);
        // late sample: history only, current value unchanged
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW - 60, "old")], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.snapshot("inst-1").unwrap().unwrap().payload, "new");
        // bounded window read: newest `cap` samples in time order, capped flagged, other workspaces never mixed in
        assert_eq!(s.samples("inst-1", "w", NOW - 3600, 1).unwrap(), (vec![(NOW, "e2".into(), "new".into())], true));
        assert_eq!(s.samples("inst-1", "w", NOW - 3600, 5).unwrap().0.len(), 2);
        assert_eq!(s.samples("inst-1", "w", NOW - 30, 5).unwrap(), (vec![(NOW, "e2".into(), "new".into())], false));
        assert!(s.samples("inst-1", "other", 0, 5).unwrap().0.is_empty());
        assert_eq!(s.history("inst-1").unwrap(), [(NOW - 60, "old".into()), (NOW, "new".into())]);
        assert_eq!(s.accept(b"v1", &[snap("e0", NOW - 9 * 86400, "x")], NOW).unwrap(), [Outcome::Rejected("expired")]);
        // future time: clock-uncertain, and it never blocks the next real sample
        s.accept(b"v1", &[snap("e3", NOW + 3600, "future")], NOW).unwrap();
        assert!(s.snapshot("inst-1").unwrap().unwrap().clock_uncertain);
        assert_eq!(s.accept(b"v1", &[snap("e4", NOW + 60, "real")], NOW + 60).unwrap(), [Outcome::Accepted]);
        assert!(!s.snapshot("inst-1").unwrap().unwrap().clock_uncertain);
        // re-enrollment: new generation, same identity and history; the old credential is dead
        let g = s.grant(&AGENT, "w", "a", SCOPES, b"v1b", NOW).unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(s.accept(b"v1", &[snap("e5", NOW, "x")], NOW), Err(Error::Denied));
        assert_eq!(s.history("inst-1").unwrap().len(), 4);
        assert_eq!(s.grant(&Installation { runtime: "wsl", ..AGENT }, "w", "a", SCOPES, b"v9", NOW), Err(Error::Conflict));
        assert_eq!(s.grant(&AGENT, "other", "a", SCOPES, b"v9", NOW), Err(Error::Conflict));
        assert!(s.authorize(b"v1b").unwrap().is_some());
        let many: Vec<_> = (0..101).map(|_| snap("e", NOW, "")).collect();
        assert_eq!(s.accept(b"v1b", &many, NOW), Err(Error::TooMany));
        // the writer queue is bounded: past it callers get Busy, not an unbounded wait
        let busy = Store::open(&dir("busy").join("dashq.db"), Limits { writers: 0, ..LIMITS }).unwrap();
        assert_eq!(busy.revoke("inst-1", NOW), Err(Error::Busy));
    }

    #[test]
    fn full_store_refuses_telemetry_keeps_control_reserve() {
        let l = Limits { store_bytes: 2 << 20, reserve_bytes: 1 << 20, ..LIMITS };
        let s = Store::open(&dir("full").join("dashq.db"), l).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        let big = "x".repeat(32 << 10);
        let ids: Vec<String> = (0..200).map(|i| format!("e{i}")).collect();
        let mut acked = vec![];
        for id in &ids {
            match s.accept(b"v1", &[snap(id, NOW, &big)], NOW).unwrap()[0] {
                Outcome::Rejected(r) => {
                    assert_eq!(r, "store_full");
                    break;
                }
                _ => acked.push(id),
            }
        }
        assert!(!acked.is_empty() && acked.len() < ids.len());
        // control records still fit in the reserve
        s.add_member("w", "b", "member").unwrap();
        assert!(s.revoke("inst-1", NOW).unwrap());
        assert_eq!(s.authorize(b"v1").unwrap(), None);
        // nothing acknowledged was lost
        assert_eq!(s.history("inst-1").unwrap().len(), acked.len());
        // past the hard ceiling SQLite itself refuses; the error is explicit, never success
        let s = Store::open(&dir("hard").join("dashq.db"), Limits { reserve_bytes: 0, ..l }).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        let huge = "x".repeat(4 << 20);
        assert_eq!(s.accept(b"v1", &[snap("h", NOW, &huge)], NOW), Err(Error::Full));
        assert_eq!(s.receipt(b"v1", "h").unwrap(), None);
    }

    #[test]
    fn older_export_gaps_gain_a_workspace_column() {
        let p = dir("migrate").join("dashq.db");
        Connection::open(&p).unwrap().execute_batch("CREATE TABLE export_gaps(installation TEXT NOT NULL, events INTEGER NOT NULL, first_observed INTEGER NOT NULL, last_observed INTEGER NOT NULL, at INTEGER NOT NULL); INSERT INTO export_gaps VALUES ('inst-1', 4, 0, 0, 0);").unwrap();
        let s = Store::open(&p, LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        // an old gap has no workspace and shows in no scoped row
        assert_eq!(s.project(Scope::All, |_, _| true).unwrap()[0].export_gap, 0);
        assert_eq!(s.export_gaps().unwrap(), [("inst-1".to_string(), 4)]);
    }

    #[test]
    fn corrupt_or_newer_store_refuses_and_is_kept() {
        let p = dir("corrupt").join("dashq.db");
        std::fs::write(&p, vec![0x5a; 8192]).unwrap();
        assert!(matches!(Store::open(&p, LIMITS), Err(Error::Corrupt)));
        assert_eq!(std::fs::read(&p).unwrap(), vec![0x5a; 8192]);
        let p = dir("newer").join("dashq.db");
        Store::open(&p, LIMITS).unwrap();
        Connection::open(&p).unwrap().execute_batch("PRAGMA user_version = 2").unwrap();
        assert!(matches!(Store::open(&p, LIMITS), Err(Error::Newer(2))));
    }

    #[test]
    fn retention_and_outbox_ceiling() {
        let p = dir("retain").join("dashq.db");
        let s = Store::open(&p, Limits { outbox_bytes: 10, ..LIMITS }).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        assert_eq!(s.accept(b"v1", &[export("x1", "123456"), export("x2", "123456"), snap("e1", NOW, "s")], NOW).unwrap(), [Outcome::Accepted, Outcome::Rejected("outbox_full"), Outcome::Accepted]);
        // a backpressured export was not acknowledged: its retry is evaluated again
        assert_eq!(s.receipt(b"v1", "x2").unwrap(), None);
        let later = NOW + 8 * 86400 + 1;
        s.accept(b"v1", &[snap("e2", later, "s2")], later).unwrap();
        s.prune(later).unwrap();
        assert_eq!(std::fs::metadata(format!("{}-wal", p.display())).unwrap().len(), 0);
        assert_eq!(s.export_gaps().unwrap(), [("inst-1".to_string(), 1)]);
        assert_eq!(s.accept(b"v1", &[Event { observed: later, ..export("x3", "123456") }], later).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.history("inst-1").unwrap(), [(later, "s2".into())]);
        assert_eq!(s.receipt(b"v1", "e1").unwrap(), None);
        s.project(Scope::All, |_, _| true).unwrap();
        assert_eq!(s.receipt(b"v1", "e2").unwrap(), Some(("agentq.snapshot/1".into(), Outcome::Projected)));
        assert!(s.authorize(b"v1").unwrap().is_some());
    }

    /// #161 A3: stored telemetry blocks stay under the configured byte cap (oldest leave first; a block over the
    /// cap alone is refused) and leave after the configured retention.
    #[test]
    fn telemetry_blocks_stay_bounded() {
        let s = Store::open(&dir("telemetry").join("dashq.db"), Limits { telemetry_bytes: 25, telemetry_secs: 86400, ..LIMITS }).unwrap();
        let tq = Installation { id: "tq-1", product: "taskq", runtime: "native", label: "mac-1" };
        s.grant(&tq, "w", "a", r#"["taskq-telemetry/1"]"#, b"v1", NOW).unwrap();
        let ev = |id, payload| Event { id, schema: "taskq-telemetry/1", observed: NOW, kind: Kind::Telemetry, payload };
        let ids = |s: &Store| s.write(|tx| Ok(tx.prepare("SELECT event_id FROM telemetry ORDER BY id")?.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?)).unwrap();
        let (b10, big) = (r#"{"n":1234}"#, "x".repeat(26));
        assert_eq!(s.accept(b"v1", &[ev("t1", b10), ev("t2", b10), ev("t3", b10), ev("big", &big)], NOW).unwrap(), [Outcome::Accepted, Outcome::Accepted, Outcome::Accepted, Outcome::Rejected("payload")]);
        assert_eq!(ids(&s), ["t2", "t3"]);
        s.accept(b"v1", &[ev("t4", b10)], NOW + 86400).unwrap();
        s.prune(NOW + 86400).unwrap();
        assert_eq!(ids(&s), ["t3", "t4"], "at the retention edge nothing older leaves early");
        s.prune(NOW + 86401).unwrap();
        assert_eq!(ids(&s), ["t4"]);
    }

    /// A store-level block over seqs `first..=last` (the store reads only the cursors and `rows`).
    fn tblock(first: i64, last: i64) -> String {
        format!(r#"{{"schema":1,"first":{{"seq":{first},"id":"a{first}"}},"last":{{"seq":{last},"id":"a{last}"}},"rows":{},"bytes":1,"gzip":"{}"}}"#, last - first + 1, "x".repeat(40))
    }

    fn tel(id: &str, payload: &str) -> Event<'static> {
        Event { id: Box::leak(id.to_string().into_boxed_str()), schema: "taskq-telemetry/1", observed: NOW, kind: Kind::Telemetry, payload: Box::leak(payload.to_string().into_boxed_str()) }
    }

    fn cover(s: &Store, inst: &str) -> Telemetry {
        s.project(Scope::All, |_, _| true).unwrap().into_iter().find(|r| r.installation == inst).unwrap().telemetry
    }

    // Chain: two hosts send -> one exceeds its budget -> eviction -> retention prune -> projection.
    // Risk (#181 A4): a busy host evicting another's blocks, or an eviction that leaves no trace of the lost rows.
    #[test]
    fn telemetry_budget_is_per_installation_and_loss_stays_known() {
        let one = tblock(1, 1).len() as u64;
        let s = Store::open(&dir("tbudget").join("dashq.db"), Limits { telemetry_bytes: 2 * one + 10, telemetry_secs: 86400, ..LIMITS }).unwrap();
        let (a, b) = (Installation { id: "tq-a", product: "taskq", runtime: "native", label: "a" }, Installation { id: "tq-b", product: "taskq", runtime: "native", label: "b" });
        s.grant(&a, "w", "x", r#"["taskq-telemetry/1"]"#, b"va", NOW).unwrap();
        s.grant(&b, "w", "x", r#"["taskq-telemetry/1"]"#, b"vb", NOW).unwrap();
        s.accept(b"vb", &[tel("b1", &tblock(1, 4))], NOW).unwrap();
        for (i, (f, l)) in [(1, 3), (4, 6), (7, 9), (12, 12)].iter().enumerate() {
            assert_eq!(s.accept(b"va", &[tel(&format!("a{i}"), &tblock(*f, *l))], NOW).unwrap(), [Outcome::Accepted]);
        }
        let t = cover(&s, "tq-a");
        assert_eq!((t.blocks, t.rows, t.first_seq, t.last_seq), (2, 4, Some(7), Some(12)), "the two newest blocks fit the budget");
        assert_eq!(t.pruned, [("capacity".to_string(), 2, 6, Some(6))], "evicted rows stay counted with their highest seq");
        assert_eq!((t.holes.clone(), t.hole_rows, t.holes_total), (vec![(10, 11)], 2, 1), "rows 10–11 never arrived");
        assert_eq!((cover(&s, "tq-b").blocks, cover(&s, "tq-b").pruned.len()), (1, 0), "the other host keeps its block");
        s.prune(NOW + 86401).unwrap();
        let t = cover(&s, "tq-a");
        assert_eq!((t.blocks, t.pruned.clone()), (0, vec![("capacity".to_string(), 2, 6, Some(6)), ("retention".to_string(), 2, 4, Some(12))]));
        // a full store acknowledges nothing: the sender keeps its backlog, so no loss is invented
        let full = Store::open(&dir("tfull").join("dashq.db"), Limits { store_bytes: 1 << 20, reserve_bytes: 1 << 20, ..LIMITS }).unwrap();
        full.grant(&a, "w", "x", r#"["taskq-telemetry/1"]"#, b"va", NOW).unwrap();
        assert_eq!(full.accept(b"va", &[tel("f1", &tblock(1, 1))], NOW).unwrap(), [Outcome::Rejected("store_full")]);
        assert_eq!(full.receipt(b"va", "f1").unwrap(), None);
        assert_eq!(cover(&full, "tq-a"), Telemetry::default());
    }

    // Chain: blocks stored by an older format or a rolled-back binary -> reopen; backup -> more blocks -> restore.
    // Risk (#181 A5): a migration or rollback leaving blocks unreadable, or a restore hiding the rows it lost.
    #[test]
    fn telemetry_survives_migration_rollback_and_restore() {
        let d = dir("tmigrate");
        let p = d.join("dashq.db");
        {
            let s = Store::open(&p, LIMITS).unwrap();
            let tq = Installation { id: "tq-1", product: "taskq", runtime: "native", label: "mac" };
            s.grant(&tq, "w", "x", r#"["taskq-telemetry/1"]"#, b"v1", NOW).unwrap();
        }
        // the #161 table without coverage columns, holding a numeric-cursor block
        let c = Connection::open(&p).unwrap();
        c.execute_batch(&format!(
            "DROP TABLE telemetry; CREATE TABLE telemetry(id INTEGER PRIMARY KEY, installation TEXT NOT NULL, workspace TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, received INTEGER NOT NULL, status TEXT, payload TEXT NOT NULL);
             INSERT INTO telemetry (installation, workspace, generation, event_id, observed, received, payload) VALUES ('tq-1', 'w', 1, 'old', {NOW}, {NOW}, '{{\"schema\":1,\"first\":1,\"last\":2,\"rows\":2}}');"
        ))
        .unwrap();
        drop(c);
        let s = Store::open(&p, LIMITS).unwrap();
        s.accept(b"v1", &[tel("t2", &tblock(3, 5))], NOW).unwrap();
        drop(s);
        // a rolled-back binary writes the #161 columns only; the next open fills the rest from the payload
        Connection::open(&p).unwrap().execute(&format!("INSERT INTO telemetry (installation, workspace, generation, event_id, observed, received, status, payload) VALUES ('tq-1', 'w', 1, 'rb', {NOW}, {NOW}, NULL, ?1)"), [tblock(6, 6)]).unwrap();
        let s = Store::open(&p, LIMITS).unwrap();
        let t = cover(&s, "tq-1");
        assert_eq!((t.blocks, t.rows, t.first_seq, t.last_seq, t.hole_rows), (3, 6, Some(1), Some(6), 0), "{t:?}");
        let pairs = [("tq-1".to_string(), "w".to_string())];
        assert_eq!(s.telemetry_block("old", &pairs).unwrap().0.len(), 1);
        // restore from a backup taken before t3: the sender's cursor is past it, so its rows read as not received
        s.backup(&d.join("backup.db")).unwrap();
        s.accept(b"v1", &[tel("t3", &tblock(7, 9))], NOW).unwrap();
        drop(s);
        let r = Store::restore(&d.join("backup.db"), &d.join("restored.db"), LIMITS).unwrap();
        r.accept(b"v1", &[tel("t4", &tblock(10, 10))], NOW).unwrap();
        let t = cover(&r, "tq-1");
        assert_eq!((t.holes.clone(), t.hole_rows, t.last_seq), (vec![(7, 9)], 3, Some(10)));
        assert!(r.telemetry_block("t3", &pairs).unwrap().0.is_empty());
        // a replaced sender store counts from 1 again: a restart, not a hole
        r.accept(b"v1", &[tel("t5", &tblock(1, 2))], NOW).unwrap();
        assert_eq!((cover(&r, "tq-1").restarts, cover(&r, "tq-1").hole_rows), (1, 3));
    }

    #[test]
    fn forwarded_rows_leave_the_outbox_refused_ones_become_gaps() {
        let s = Store::open(&dir("forward").join("dashq.db"), LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        s.accept(b"v1", &[export("x1", "1"), export("x2", "2"), export("x3", "3")], NOW).unwrap();
        let p = s.outbox(2).unwrap();
        assert_eq!(p.iter().map(|r| r.event_id.as_str()).collect::<Vec<_>>(), ["x1", "x2"]);
        assert_eq!((p[0].installation.as_str(), p[0].generation, p[0].observed), ("inst-1", 1, NOW));
        s.exported(&[p[0].id], &[p[1].id], NOW).unwrap();
        assert_eq!(s.export_counts().unwrap(), (1, 1));
        assert_eq!(s.outbox(9).unwrap()[0].event_id, "x3");
        // a replay of a forwarded event is the same receipt, never a second export
        assert_eq!(s.accept(b"v1", &[export("x1", "1")], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.export_counts().unwrap(), (1, 1));
        // gaps leave with the history window
        s.prune(NOW + 7 * 86400 + 1).unwrap();
        assert_eq!(s.export_counts().unwrap(), (0, 1));
        s.prune(NOW + 14 * 86400 + 2).unwrap();
        assert_eq!(s.export_counts().unwrap(), (0, 0));
    }

    #[test]
    fn backup_restore_keeps_grants_and_revocations() {
        let d = dir("backup");
        let s = Store::open(&d.join("dashq.db"), LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", SCOPES, b"v1", NOW).unwrap();
        s.grant(&Installation { id: "inst-2", ..AGENT }, "w", "a", SCOPES, b"v2", NOW).unwrap();
        s.revoke("inst-2", NOW).unwrap();
        s.accept(b"v1", &[snap("e1", NOW, "{}")], NOW).unwrap();
        s.project(Scope::All, |_, _| true).unwrap();
        s.backup(&d.join("backup.db")).unwrap();
        assert!(matches!(s.backup(&d.join("backup.db")), Err(Error::Io(_))));
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(d.join("backup.db")).unwrap().permissions()) & 0o777, 0o600);
        assert!(matches!(Store::restore(&d.join("backup.db"), &d.join("dashq.db"), LIMITS), Err(Error::Exists)));
        let r = Store::restore(&d.join("backup.db"), &d.join("restored.db"), LIMITS).unwrap();
        assert!(r.authorize(b"v1").unwrap().is_some());
        assert_eq!(r.authorize(b"v2").unwrap(), None);
        assert_eq!(r.receipt(b"v1", "e1").unwrap(), Some(("agentq.snapshot/1".into(), Outcome::Projected)));
    }

    // chain: D16 A2/A3 bounds, a full store refuses new operations but still commits results and keeps unresolved
    // ones across restart; prune keeps every unresolved operation and the newest resolved ones per installation
    #[test]
    fn operations_survive_full_storage_restart_and_prune() {
        let op = |n: u32, kind: &str, now: i64| {
            let payload = serde_json::json!({"schema": 1, "kind": kind, "n": n});
            serde_json::json!({"operation": format!("{n:032x}"), "capability": "agentq.configure", "installation": "inst-1", "generation": 1, "expires_at": now + OPERATION_SECS,
                "digest": coreq::enroll::digest(&payload), "payload": payload, "authority": {"actor": "a", "workspace": "w", "profiles": ["p1"]}})
        };
        let open = |name: &str, l: Limits| {
            let s = Store::open(&dir(name).join("dashq.db"), l).unwrap();
            s.write(|tx| grant(tx, &AGENT, "w", "a", SCOPES, None, Some(r#"["agentq.configure"]"#), b"v1", NOW)).unwrap();
            s.ack(b"v1", NOW).unwrap();
            s.add_member("w", "a", "admin").unwrap();
            s
        };
        let l = Limits { store_bytes: 2 << 20, reserve_bytes: 1 << 20, ..LIMITS };
        let s = open("ops-full", l);
        // the queuing subject must be the authority's actor
        assert_eq!(s.queue("b", &op(1, "apply", NOW), NOW).unwrap(), Queued::Refused("authority"));
        assert_eq!((s.queue("a", &op(1, "apply", NOW), NOW).unwrap(), s.queue("a", &op(2, "plan", NOW), NOW).unwrap()), (Queued::Queued, Queued::Queued));
        assert_eq!(s.pull(b"v1", "agentq.configure", NOW).unwrap().unwrap().len(), 2);
        let big = "x".repeat(32 << 10);
        for i in 0..200 {
            if s.accept(b"v1", &[snap(&format!("e{i}"), NOW, &big)], NOW).unwrap()[0] == Outcome::Rejected("store_full") {
                break;
            }
        }
        assert_eq!(s.queue("a", &op(3, "plan", NOW), NOW).unwrap(), Queued::Refused("store_full"));
        assert_eq!(s.post_result(b"v1", &format!("{:032x}", 1), op(1, "apply", NOW)["digest"].as_str().unwrap(), "{\"outcome\":\"applied\"}", NOW).unwrap(), None);
        let path = Store::path(&s.lock().unwrap());
        drop(s);
        let s = Store::open(&path, l).unwrap();
        let (a, b) = (s.operation(&format!("{:032x}", 1)).unwrap().unwrap(), s.operation(&format!("{:032x}", 2)).unwrap().unwrap());
        assert!(a.result.is_some() && b.delivered && b.result.is_none());
        // 70 plans expire undelivered one after another; prune keeps 64 of them and the unresolved delivered plan
        let s = open("ops-prune", LIMITS);
        assert_eq!(s.queue("a", &op(1000, "plan", NOW), NOW).unwrap(), Queued::Queued);
        s.pull(b"v1", "agentq.configure", NOW).unwrap();
        for n in 0..70 {
            let now = NOW + n as i64 * (OPERATION_SECS + 1);
            assert_eq!(s.queue("a", &op(n, "plan", now), now).unwrap(), Queued::Queued, "{n}");
        }
        s.prune(NOW + 70 * (OPERATION_SECS + 1)).unwrap();
        let left: i64 = s.lock().unwrap().query_row("SELECT COUNT(*) FROM operations", [], |r| r.get(0)).unwrap();
        assert_eq!(left, 65);
        assert!(s.operation(&format!("{:032x}", 1000)).unwrap().is_some() && s.operation(&format!("{:032x}", 0)).unwrap().is_none());
        // Revoke resolves the delivered plan too; it now shares the 64-row resolved bound.
        s.revoke_credential(b"v1", NOW + 70 * (OPERATION_SECS + 1)).unwrap();
        s.prune(NOW + 70 * (OPERATION_SECS + 1)).unwrap();
        let left: i64 = s.lock().unwrap().query_row("SELECT COUNT(*) FROM operations", [], |r| r.get(0)).unwrap();
        assert_eq!(left, KEEP_RESOLVED);
        assert!(s.operation(&format!("{:032x}", 1000)).unwrap().is_none());
    }
}
