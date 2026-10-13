//! Enrollment store (docs/design/enrollment.md C4/C5, dashq.md D11): one SQLite file, WAL with
//! `synchronous=FULL`, one bounded writer. An acknowledgement is returned only after its commit.
//! Inactive capability: the pairing and ingestion routes that call it are separate tasks.

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
CREATE TABLE IF NOT EXISTS receipts(installation TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, content TEXT NOT NULL, outcome TEXT NOT NULL, at INTEGER NOT NULL, PRIMARY KEY (installation, generation, event_id));
CREATE TABLE IF NOT EXISTS snapshots(installation TEXT PRIMARY KEY, event_id TEXT NOT NULL, schema TEXT NOT NULL, observed INTEGER NOT NULL, clock_uncertain INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS history(installation TEXT NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, clock_uncertain INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS history_by_time ON history(installation, observed);
CREATE TABLE IF NOT EXISTS outbox(id INTEGER PRIMARY KEY, installation TEXT NOT NULL, generation INTEGER NOT NULL, event_id TEXT NOT NULL, observed INTEGER NOT NULL, queued INTEGER NOT NULL, payload TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS export_gaps(installation TEXT NOT NULL, events INTEGER NOT NULL, first_observed INTEGER NOT NULL, last_observed INTEGER NOT NULL, at INTEGER NOT NULL);
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
}

pub struct Event<'a> {
    pub id: &'a str,
    pub schema: &'a str,
    pub observed: i64,
    pub kind: Kind,
    pub payload: &'a str,
}

/// Receipt: rejected, durably accepted, or accepted and projected as the current value.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Projected,
    Accepted,
    Rejected(&'static str),
}

#[derive(Debug, PartialEq)]
pub struct Snapshot {
    pub event_id: String,
    pub schema: String,
    pub observed: i64,
    pub clock_uncertain: bool,
    pub payload: String,
}

pub struct Store {
    conn: Mutex<Connection>,
    waiting: AtomicUsize,
    limits: Limits,
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
fn used(c: &Connection, path: &Path) -> Result<u64, Error> {
    let pages: i64 = c.query_row("SELECT (SELECT page_count FROM pragma_page_count) - (SELECT freelist_count FROM pragma_freelist_count)", [], |r| r.get(0))?;
    let page: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    let wal = std::fs::metadata(format!("{}-wal", path.display())).map(|m| m.len()).unwrap_or(0);
    Ok(pages as u64 * page as u64 + wal)
}

fn authorize(c: &Connection, verifier: &[u8]) -> Result<Option<Grant>, Error> {
    Ok(c.query_row(
        "SELECT installation, generation, workspace, account, scopes FROM grants g WHERE verifier = ?1
         AND NOT EXISTS (SELECT 1 FROM revocations r WHERE r.installation = g.installation AND r.generation = g.generation)",
        [verifier],
        |r| Ok(Grant { installation: r.get(0)?, generation: r.get(1)?, workspace: r.get(2)?, account: r.get(3)?, scopes: r.get(4)? }),
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
        Ok(Store { conn: Mutex::new(c), waiting: AtomicUsize::new(0), limits })
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, Error> {
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
    fn write<T>(&self, f: impl FnOnce(&Transaction) -> Result<T, Error>) -> Result<T, Error> {
        let mut c = self.lock()?;
        let tx = c.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let r = f(&tx)?;
        tx.commit()?;
        Ok(r)
    }

    fn path(c: &Connection) -> std::path::PathBuf {
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
        self.write(|tx| {
            let known: Option<(String, String)> = tx.query_row("SELECT product, runtime FROM installations WHERE id = ?1", [inst.id], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
            match known {
                Some((p, r)) if p != inst.product || r != inst.runtime => return Err(Error::Conflict),
                Some(_) => {
                    // ponytail: a workspace switch (C1 old → new) is refused until its explicit flow exists
                    let last: Option<String> = tx.query_row("SELECT workspace FROM grants WHERE installation = ?1 ORDER BY generation DESC LIMIT 1", [inst.id], |r| r.get(0)).optional()?;
                    if last.is_some_and(|w| w != workspace) {
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
            tx.execute("INSERT INTO grants VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![inst.id, generation, verifier, workspace, account, scopes, now])?;
            Ok(Grant { installation: inst.id.into(), generation, workspace: workspace.into(), account: account.into(), scopes: scopes.into() })
        })
    }

    /// Revokes the live generation; true when one was live. Uses the control reserve.
    pub fn revoke(&self, installation: &str, now: i64) -> Result<bool, Error> {
        self.write(|tx| {
            Ok(tx.execute("INSERT OR IGNORE INTO revocations SELECT installation, generation, ?2 FROM grants WHERE installation = ?1", params![installation, now])? > 0)
        })
    }

    /// Checked on every request: no cache can extend a revoked grant.
    pub fn authorize(&self, verifier: &[u8]) -> Result<Option<Grant>, Error> {
        authorize(&*self.lock()?, verifier)
    }

    /// Stores a batch atomically. Replays of `(generation, event id)` return the first outcome.
    pub fn accept(&self, verifier: &[u8], events: &[Event], now: i64) -> Result<Vec<Outcome>, Error> {
        if events.len() > self.limits.records {
            return Err(Error::TooMany);
        }
        let l = self.limits;
        self.write(|tx| {
            let g = authorize(tx, verifier)?.ok_or(Error::Denied)?;
            let full = used(tx, &Self::path(tx))? + l.reserve_bytes >= l.store_bytes;
            let mut outbox = tx.query_row("SELECT COALESCE(SUM(length(payload)), 0) FROM outbox", [], |r| r.get::<_, i64>(0))? as u64;
            let mut out = Vec::with_capacity(events.len());
            for e in events {
                let kind = if e.kind == Kind::Snapshot { "snapshot" } else { "export" };
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
                    Some(_) => Outcome::Rejected("event id reused with different content"),
                    // a dedup record may be pruned already: refuse instead of duplicating
                    None if e.observed < now - l.receipt_secs => Outcome::Rejected("older than the retention window"),
                    None if full => Outcome::Rejected("store full"),
                    None if e.kind == Kind::Export && outbox + e.payload.len() as u64 > l.outbox_bytes => Outcome::Rejected("export outbox full"),
                    None => {
                        let o = if e.kind == Kind::Export {
                            outbox += e.payload.len() as u64;
                            tx.execute("INSERT INTO outbox (installation, generation, event_id, observed, queued, payload) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![g.installation, g.generation, e.id, e.observed, now, e.payload])?;
                            Outcome::Accepted
                        } else {
                            let uncertain = e.observed > now + l.future_secs;
                            tx.execute("INSERT INTO history VALUES (?1, ?2, ?3, ?4, ?5)", params![g.installation, e.id, e.observed, uncertain, e.payload])?;
                            // late samples join history only; an uncertain current value yields to any sample
                            let current: Option<(i64, bool)> = tx.query_row("SELECT observed, clock_uncertain FROM snapshots WHERE installation = ?1", [&g.installation], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
                            if current.is_none_or(|(t, u)| u || e.observed >= t) {
                                tx.execute("INSERT OR REPLACE INTO snapshots VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![g.installation, e.id, e.schema, e.observed, uncertain, e.payload])?;
                                Outcome::Projected
                            } else {
                                Outcome::Accepted
                            }
                        };
                        let s = if o == Outcome::Projected { "projected" } else { "accepted" };
                        tx.execute("INSERT INTO receipts VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![g.installation, g.generation, e.id, content, s, now])?;
                        o
                    }
                };
                out.push(o);
            }
            Ok(out)
        })
    }

    /// A device reads only its own receipts.
    pub fn receipt(&self, verifier: &[u8], event_id: &str) -> Result<Option<Outcome>, Error> {
        let c = self.lock()?;
        let g = authorize(&c, verifier)?.ok_or(Error::Denied)?;
        let o: Option<String> = c
            .query_row("SELECT outcome FROM receipts WHERE installation = ?1 AND generation = ?2 AND event_id = ?3", params![g.installation, g.generation, event_id], |r| r.get(0))
            .optional()?;
        Ok(o.map(|o| outcome(&o)))
    }

    pub fn snapshot(&self, installation: &str) -> Result<Option<Snapshot>, Error> {
        Ok(self
            .lock()?
            .query_row("SELECT event_id, schema, observed, clock_uncertain, payload FROM snapshots WHERE installation = ?1", [installation], |r| {
                Ok(Snapshot { event_id: r.get(0)?, schema: r.get(1)?, observed: r.get(2)?, clock_uncertain: r.get(3)?, payload: r.get(4)? })
            })
            .optional()?)
    }

    /// `(observed, payload)` samples in time order.
    pub fn history(&self, installation: &str) -> Result<Vec<(i64, String)>, Error> {
        let c = self.lock()?;
        let mut q = c.prepare("SELECT observed, payload FROM history WHERE installation = ?1 ORDER BY observed")?;
        let rows = q.query_map([installation], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        Ok(rows)
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
            let cut = now - l.outbox_secs;
            tx.execute(
                "INSERT INTO export_gaps SELECT installation, COUNT(*), MIN(observed), MAX(observed), ?2 FROM outbox WHERE queued < ?1 GROUP BY installation",
                params![cut, now],
            )?;
            tx.execute("DELETE FROM outbox WHERE queued < ?1", [cut])?;
            tx.execute("DELETE FROM history WHERE observed < ?1", [now - l.history_secs])?;
            tx.execute("DELETE FROM receipts WHERE at < ?1", [now - l.receipt_secs])?;
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
        s.grant(&AGENT, "w", "github:1", "snapshot", b"v1", NOW).unwrap();
        s.grant(&Installation { id: "inst-2", ..AGENT }, "w", "github:1", "snapshot", b"v2", NOW).unwrap();
        assert!(s.revoke("inst-2", NOW).unwrap());
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW, "{}"), export("x1", "{}")], NOW).unwrap(), [Outcome::Projected, Outcome::Accepted]);
        // crash: no close, no checkpoint; committed WAL frames must be enough
        std::mem::forget(s);
        let s = Store::open(&p, LIMITS).unwrap();
        assert_eq!(s.role("w", "github:1").unwrap().as_deref(), Some("admin"));
        assert_eq!(s.authorize(b"v1").unwrap().unwrap().generation, 1);
        assert_eq!(s.authorize(b"v2").unwrap(), None);
        assert_eq!(s.accept(b"v2", &[snap("e1", NOW, "{}")], NOW), Err(Error::Denied));
        assert_eq!(s.receipt(b"v1", "e1").unwrap(), Some(Outcome::Projected));
        assert_eq!(s.snapshot("inst-1").unwrap().unwrap().event_id, "e1");
        // the replay after a lost response returns the same outcome, no duplicate
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW, "{}")], NOW).unwrap(), [Outcome::Projected]);
        assert_eq!(s.history("inst-1").unwrap().len(), 1);
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&p).unwrap().permissions()) & 0o777, 0o600);
    }

    #[test]
    fn dedup_mismatch_late_and_future_samples() {
        let s = Store::open(&dir("dedup").join("dashq.db"), LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", "snapshot", b"v1", NOW).unwrap();
        assert_eq!(s.accept(b"v1", &[snap("e2", NOW, "new")], NOW).unwrap(), [Outcome::Projected]);
        assert_eq!(s.accept(b"v1", &[snap("e2", NOW, "changed")], NOW).unwrap(), [Outcome::Rejected("event id reused with different content")]);
        // late sample: history only, current value unchanged
        assert_eq!(s.accept(b"v1", &[snap("e1", NOW - 60, "old")], NOW).unwrap(), [Outcome::Accepted]);
        assert_eq!(s.snapshot("inst-1").unwrap().unwrap().payload, "new");
        assert_eq!(s.history("inst-1").unwrap(), [(NOW - 60, "old".into()), (NOW, "new".into())]);
        assert_eq!(s.accept(b"v1", &[snap("e0", NOW - 9 * 86400, "x")], NOW).unwrap(), [Outcome::Rejected("older than the retention window")]);
        // future time: clock-uncertain, and it never blocks the next real sample
        s.accept(b"v1", &[snap("e3", NOW + 3600, "future")], NOW).unwrap();
        assert!(s.snapshot("inst-1").unwrap().unwrap().clock_uncertain);
        assert_eq!(s.accept(b"v1", &[snap("e4", NOW + 60, "real")], NOW + 60).unwrap(), [Outcome::Projected]);
        assert!(!s.snapshot("inst-1").unwrap().unwrap().clock_uncertain);
        // re-enrollment: new generation, same identity and history; the old credential is dead
        let g = s.grant(&AGENT, "w", "a", "snapshot", b"v1b", NOW).unwrap();
        assert_eq!(g.generation, 2);
        assert_eq!(s.accept(b"v1", &[snap("e5", NOW, "x")], NOW), Err(Error::Denied));
        assert_eq!(s.history("inst-1").unwrap().len(), 4);
        assert_eq!(s.grant(&Installation { runtime: "wsl", ..AGENT }, "w", "a", "snapshot", b"v9", NOW), Err(Error::Conflict));
        assert_eq!(s.grant(&AGENT, "other", "a", "snapshot", b"v9", NOW), Err(Error::Conflict));
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
        s.grant(&AGENT, "w", "a", "snapshot", b"v1", NOW).unwrap();
        let big = "x".repeat(32 << 10);
        let ids: Vec<String> = (0..200).map(|i| format!("e{i}")).collect();
        let mut acked = vec![];
        for id in &ids {
            match s.accept(b"v1", &[snap(id, NOW, &big)], NOW).unwrap()[0] {
                Outcome::Rejected(r) => {
                    assert_eq!(r, "store full");
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
        s.grant(&AGENT, "w", "a", "snapshot", b"v1", NOW).unwrap();
        let huge = "x".repeat(4 << 20);
        assert_eq!(s.accept(b"v1", &[snap("h", NOW, &huge)], NOW), Err(Error::Full));
        assert_eq!(s.receipt(b"v1", "h").unwrap(), None);
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
        s.grant(&AGENT, "w", "a", "snapshot export", b"v1", NOW).unwrap();
        assert_eq!(s.accept(b"v1", &[export("x1", "123456"), export("x2", "123456"), snap("e1", NOW, "s")], NOW).unwrap(), [Outcome::Accepted, Outcome::Rejected("export outbox full"), Outcome::Projected]);
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
        assert_eq!(s.receipt(b"v1", "e2").unwrap(), Some(Outcome::Projected));
        assert!(s.authorize(b"v1").unwrap().is_some());
    }

    #[test]
    fn backup_restore_keeps_grants_and_revocations() {
        let d = dir("backup");
        let s = Store::open(&d.join("dashq.db"), LIMITS).unwrap();
        s.grant(&AGENT, "w", "a", "snapshot", b"v1", NOW).unwrap();
        s.grant(&Installation { id: "inst-2", ..AGENT }, "w", "a", "snapshot", b"v2", NOW).unwrap();
        s.revoke("inst-2", NOW).unwrap();
        s.accept(b"v1", &[snap("e1", NOW, "{}")], NOW).unwrap();
        s.backup(&d.join("backup.db")).unwrap();
        assert!(matches!(s.backup(&d.join("backup.db")), Err(Error::Io(_))));
        #[cfg(unix)]
        assert_eq!(std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(d.join("backup.db")).unwrap().permissions()) & 0o777, 0o600);
        assert!(matches!(Store::restore(&d.join("backup.db"), &d.join("dashq.db"), LIMITS), Err(Error::Exists)));
        let r = Store::restore(&d.join("backup.db"), &d.join("restored.db"), LIMITS).unwrap();
        assert!(r.authorize(b"v1").unwrap().is_some());
        assert_eq!(r.authorize(b"v2").unwrap(), None);
        assert_eq!(r.receipt(b"v1", "e1").unwrap(), Some(Outcome::Projected));
    }
}
