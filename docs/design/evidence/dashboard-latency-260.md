# Dashboard latency (#260)

[Owner request and acceptance](https://github.com/taskqorg/dashq/issues/260):
"Dashboard page must answer within seconds."

## Failure and cause

The task reports `http.page` up to **287929 ms** and `http.data` up to
**88515 ms** in the production 0.18.1 `profile.jsonl`. These are task-supplied
observations; this worker has not read that production file.

At base `bfe39ebfdab6559d0d0922d6e8545c93bf883138`, both HTML and `/data`
call `App::doc`, which synchronously calls `enroll::fleet_records`.
`Cache::get` holds the Fleet slot mutex throughout the read. On expiry, Fleet
calls `Store::telemetry_since(..., |_| true)`: it lists every retained block ID
and fetches every payload separately while holding the connection used by
ingestion and projection. Fleet then parses and decompresses every block before
keeping the newest 10,000 relevant rows. The row cap does not cap that work.
Other page requests queue behind the same slot; their earlier projection can
also queue behind the shared database connection. Fleet's two CLI release reads
are also synchronous and can consume two sequential 10 s HTTP timeouts.

This establishes a blocking mechanism, not historical attribution of those two
maxima. Production profile/trace correlation for **A1 remains open**.

An authenticated `GET https://dash.ufobe.com/` from the Windows worker timed out
after **15.015078 s**, curl exit **28**, with no HTTP status. Credentials stayed
in curl stdin and were not recorded. SSH to the production name failed with
`Host key verification failed.` No host key or authentication setting changed
(R5, R7).

## Change and regression evidence

- **D4 / A2:** browser HTML and `/data` use a single background Fleet refresh per
  installation/workspace. Fleet CLI release reads use the same background cache
  mechanism. `dashq report` retains synchronous source reads.
- **D6 / R2:** pending/failed refreshes retain the prior value, incompleteness
  and time. A pending first read has null data/time and a reason. Existing Fleet
  rendering shows producer age and an unconfirmed/unknown state.
- **D11 / R9:** Fleet opens a read-only connection to the same WAL database; it
  neither holds the ingestion/projection connection nor creates another store.
  Its query retrieves payloads in one ordered scan rather than one query per ID.
- **D11:** actor-scoped rows are checked before each cache lookup. Tests retain
  denied, anonymous and revoked-cache protections. No raw telemetry is added to
  browser output (R1).

Before the implementation, `cargo test --locked fleet_refresh_does_not_hold_callers -- --nocapture`
failed: `Fleet held the caller behind its read: Timeout`. The reproduction used
the original synchronous cache through a forwarding `refresh` entry point.
After the implementation, `cargo test --locked fleet_ -- --nocapture` passed
**9 checks**. The retained checks cover blocked and failing reads, shared
single-flight reads, cold unknown-to-success, last-good time, the independent
SQLite reader, all ten HTML view routes and `/data`, producer age, and scope.
The route check releases its deliberately blocked reader only after receiving
all eleven responses. This is deterministic regression evidence, not production
performance qualification.

## Remaining acceptance and rollback

**A2 is locally verified. A1 production correlation and A3 are unverified.**
No candidate was installed on pds18 and no post-install ten-request result is
claimed. Other source reads remain synchronous; their contribution needs the
production profile. Windows checks do not establish Linux or proxy behavior.

Once trusted production access is available, inspect the same profile report
before and after installing the exact checked candidate. Record its SHA/artifact
digest and the running executable; run ten authenticated public `GET /` probes,
requiring HTTP 200 and less than 5 s each (A3). Retain source states and producer
age while Fleet refresh is slow or fails. Preserve the previous executable for
rollback. This change has no schema migration, configuration or grant change;
rollback restores the prior executable against the same database.
