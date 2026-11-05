# Dashboard latency (#260)

[Owner request and acceptance](https://github.com/taskqorg/dashq/issues/260):
"Dashboard page must answer within seconds."

## Result

| Item | State | Evidence |
|---|---|---|
| A1 root cause | named | [Root cause](#a1-root-cause) |
| A2 Fleet never holds the page | verified in tests and live | [A2](#a2-fleet) |
| A3 ten public tries under 5 s | passed on the rebased candidate | [Current candidate](#current-candidate-a285098) |

Source of the service for the original install receipts below:
`b3d7259bac8f3bc6a3fc47c29aa682058771bc09`, a development activation before merge.
These receipts describe that artifact, not a later rebased candidate.

## Merge conflict resolution

The main-moved requeue rebases this change onto
`57d1ece375c7b78372bf49d5a599591e759e84c2`, which includes #262's invalid-block fix.
The read-only Fleet query now returns block IDs with payloads so the background
reader retains valid rows and `known_loss` together (D4, D6, D11). Both existing
regression checks remain: invalid blocks after restart and browser routes during a
blocked Fleet read. No protecting assertion or authentication rule changes.

The conflict-resolution attempt did not activate another production binary. Its
receipts apply to the original artifact. The subsequent activation below covers
the rebased runtime, including main's invalid-block fix.

Rebase validation on Windows: `cargo test --locked` passed (160 passed, 2 existing
live checks ignored); `cargo run -- selftest` passed with no config, skipping live
source reads. `node scripts/taskq-assets.cjs`, `node tests/resources.cjs` and the
CI-pinned specq contract lint passed. The lint reports the existing line-headroom
warning. No production install, public probes or browser chain was repeated for
this merge-conflict resolution; those live receipts remain tied to their artifact.

## Current candidate a285098

[Machine-readable receipts](dashboard-latency-260-rebased.json) pair the source,
archive, built and running digests with all ten public probes and profile rows.

- Source: `a285098836adc3d2df5b02eb2c2e01b192a4f508`.
- The `git archive` transfer matched SHA-256
  `7de0a1b724b7d809423a087f8eff78212fdcfe84dc116f96f4835e3ad3204f56` at both ends.
- Built on pds18 as the service user with `cargo build --locked --release`.
  Built file and running `/proc/PID/exe` matched SHA-256
  `cb20feb739ed52a48fb1844b4acc3c1612908d254c3248a4faeebb422da2dadd`.
- Activated at 2026-11-05 12:42:27 UTC using the existing runtime drop-in
  `90-task260.conf`. The prior binary and drop-in were preserved for rollback.
  A readback at 12:44:43 UTC confirmed the same digest and an active service.
- From the macOS worker, 12:43:02–12:43:25 UTC: **10/10 authenticated public
  `GET /` requests returned HTTP 200 under 5 s**, with a 5 s client limit.
  Full-response times were 1.192–3.250 s; first-byte times were 0.866–1.935 s.
  No request was discarded or retried in this acceptance run. Authentication
  and the proxy configuration were unchanged (R5, R7).
- `/data` returned in 3.210 s. Cached Fleet streams retained producer ages of
  173 s and 8622 s with `unconfirmed: refresh in progress`; absent values retained
  null ages and `unknown` (D4, D6, D11). Two explicit `known_loss` entries were
  visible; the target restart regression also verified preservation of valid records.

The before profile was re-read from the retained pds18 snapshot: 39273 rows,
SHA-256 `e6eab569cc70e6f2172043c95ae79aeb9aabd02ba3453378b701496ab90fe88c`.
It retains both named request maxima. Its 287929 ms page window contains eleven
completed Fleet reads, including three over 120 s; its 88515 ms data window
contains four, including the three long serial reads named below. Profile rows
still have no request ID, so attribution remains by code path and interval (D20).

| Operation | Before p50 / p95 / max ms | During public probes and data read p50 / p95 / max ms | Rows after |
|---|---:|---:|---:|
| `http.page` | 5461 / 200501 / 759354 | 539 / 1570 / 1570 | 10 |
| `http.data` | 9536 / 88078 / 634887 | 1581 / 1581 / 1581 | 1 |
| `read.fleet` | 29327 / 71494 / 157263 | 23660 / 25821 / 25821 | 7 |
| `http.enroll` | 524 / 2667 / 27198 | 7 / 595 / 595 | 10 |

The profile also retains startup requests before the acceptance run: six
`http.page` rows exceeded 5 s, with a maximum of 6165 ms, before loopback readiness
succeeded. This proves the ten-request acceptance after readiness, not a sub-5 s
cold-start guarantee. No startup failure is excluded from the attached profile.
The holder of those startup delays was not attributed in this activation attempt.

Target checks on pds18 passed: `cargo test --locked fleet_` (10 tests),
`browser_routes_do_not_wait_for_a_hung_source_read`,
`projection_reads_times_and_counts_without_payload_pages` and
`revalidate_keeps_the_last_value_unmarked_during_one_read` (one each).
The installed artifact's `DASHQ_SELFTEST_OFFLINE=1 dashq selftest` passed fixture
and config validation; live source reads were skipped. Existing protecting
assertions were unchanged. Only evidence documents changed after this build;
the Windows full-suite and exact-head GitHub CI results remain applicable to the
unchanged runtime, tests and dependencies. Merge-result CI remains separate.

## A1 root cause

The profile on pds18 at 0.18.1 held 39164 rows, 2026-11-04 07:48 UTC to 2026-11-05
11:25 UTC. Both named maxima are retained: `http.page` 287929 ms finished at
1793873309 and `http.data` 88515 ms finished at 1793872631.

| Operation | Rows | p50 ms | p95 ms | Max ms | Over 5 s |
|---|---:|---:|---:|---:|---:|
| `http.page` | 310 | 5470 | 200501 | 759354 | 212 |
| `http.data` | 302 | 9562 | 88078 | 634887 | 301 |
| `read.fleet` | 643 | 29415 | 71494 | 157263 | 480 |
| `http.enroll` | 11021 | 524 | 2666 | 27198 | 68 |

Three causes combined. The first explains the size of the maxima; the second and
third set the floor under every page.

1. **Page requests read Fleet synchronously, one read after another.** `App::doc`
   called `fleet_records`, whose `Cache::get` held the slot lock during the read.
   At base, `get` set `next = now + ttl` from a `now` taken before the read. A read
   of 30 s or more had already expired when it finished, so each caller queued on
   the slot repeated the read. 299 of 643 reads took 30 s or more; 154 failed with
   `stored block invalid`.
   - `http.data` 88515 ms (1793872543 to 1793872631): three `read.fleet` of 6971,
     35534 and 33545 ms ran back to back from 1793872552 to 1793872628. They account
     for 76 s of the 88.5 s, and the request finished 3 s after the last one.
   - `http.page` 287929 ms (1793873021 to 1793873309): eleven `read.fleet` finished in
     the window, three of them 121593 to 134557 ms. A fourth of 133868 ms finished 2 s
     after the request.
   - Limit: a profile row has a completion time and no request ID. This correlation
     is by interval, supported by the code path and the blocked-read reproduction.
2. **Each projection walked 2.6 GB of old receipts under the store lock.**
   `Store::project` runs in a write transaction every 2 s and on every page. Its
   `MAX(at)` per grant reads the `at` column of `receipts`, which is stored after
   `content`. Receipts written before #242 hold the full record (135242 rows, 2.6 GB
   of the 3.4 GB file, average 19 KB), so SQLite walks each row's overflow pages.
   Read-only timing on the live file, warm cache:

   | Query, per grant | Time |
   |---|---:|
   | `SELECT MAX(at) FROM receipts` | 1.41 to 1.50 s |
   | `SELECT COUNT(*) FROM receipts` (primary key only) | 0.017 s |
   | `SELECT COUNT(*) FROM history` | 0.44 to 0.51 s |
   | Telemetry coverage with `length(payload)` | 0.43 to 0.45 s |
   | Telemetry coverage with `octet_length(payload)` | 0.18 to 0.22 s |

   The projection thread used 54% of uptime. `perf` on it over 20 s: 57% in `pread64`
   and 33% copying pages, against 5.8% in `lengthFunc`.
3. **Cold and expiring reads stayed on the page path.** The first page after a
   restart took 34459 ms while it decoded the two-day stage window behind the memo
   lock. A source read at cache expiry raised a 1.4 s page to 4.3 to 4.7 s.

The review's attribution of the projection to `length(payload)` is partly confirmed.
That call does read the overflow pages, but it is the smaller part: 0.2 s of the
pass, against 1.4 s for receipts. `http.enroll` shared the same lock and also summed
`length(payload)` on each telemetry accept.

## Change

- **A2:** browser Fleet reads run in one background flight per stream on a read-only
  connection (D4, D6, D11).
- **Projection:** covering indexes `receipts_by_time` and `history_by_scope` answer
  receipt time and sample count. Block size and the telemetry budget use
  `octet_length`, which reads no payload page and counts bytes. The indexes are
  additive, the store version is unchanged, and an older binary still opens the file.
- **Sources:** browser requests and the limits push start Langfuse, board, release and
  TaskQ reads in the background. The last value keeps its time, so D6 decides its
  state. `dashq report` and `dashq limits` still wait for their reads.
- **Restart:** the service decodes the stage window at start. Pages read `agent_work`
  as unknown with `refresh in progress` until it is done.

Authentication, grants and configuration are unchanged (R5, R7).

## Regression checks

All pass on the candidate. The second column gives the recorded failure before the fix.

| Check | Failure before | Protects |
|---|---|---|
| `fleet_refresh_does_not_hold_callers`, `fleet_pending_read_keeps_routes_and_scope_available` | `Fleet held the caller behind its read: Timeout` | A2: last data, age, single flight, actor scope |
| `projection_reads_times_and_counts_without_payload_pages` | Plan `SEARCH r USING INDEX sqlite_autoindex_receipts_1`; bytes `2`, expected `4` | The projection reads no payload to reach another column |
| `browser_routes_do_not_wait_for_a_hung_source_read` | `the source read held the routes: Timeout`, with the background read reverted | A page and `/data` answer during a hung read; a first read is not "all sources failed" |
| `revalidate_keeps_the_last_value_unmarked_during_one_read` | None: new method | Last value keeps its time, one read runs, the result replaces it |
| `taskq_stage_replay_through_ingest`, added case | None: new case | During the start decode a browser read reads unknown, then returns the same records |

## A2 Fleet

A2 regression checks are above. On the installed candidate, public `/data` showed
Fleet streams `unconfirmed`, `refresh in progress`, with source ages of 195 s and
5788 s while their reads ran. It showed `unknown`, `stored block invalid` for the
one host whose block does not decode (see [Open](#open)).

## A3 install and probes

Built on pds18 from the exact commit with `cargo build --locked --release`, and
activated through a runtime drop-in on the existing `dashq-dashq.service`:

- Source `b3d7259bac8f3bc6a3fc47c29aa682058771bc09`.
- Built and running SHA-256 match:
  `94e85b4faa6adb96114df693c11003eb9493ba18ac68defd0baa27139d2e1562`.
- Replaced release 0.18.1, SHA-256
  `d93df6bd34cfc36783e813290f4f4c73d6c2e24ba8bda6efd5f6678bb4bb4774`.
- Activated at 1793879146. The listener answered 4 s after the restart.

Public `GET /` with the agent site credentials, from the Windows worker:

| Run | Under 5 s, HTTP 200 | Slowest |
|---|---:|---:|
| Before, on 0.18.1 ([receipts](dashboard-latency-260-probes.json)) | 0/10 | 5 s client timeout |
| First run, from the cold restart | 9/10 | 9.39 s |
| 24 tries with first-byte timing | 24/24 | 3.78 s |
| **Acceptance run, 5 s client limit** ([receipts](dashboard-latency-260-after.json)) | **10/10** | 3.97 s |
| Soak, 30 tries | 30/30 | 4.44 s |

The first request after the restart took 4.26 s. The one miss, 9.39 s at 1793879185,
is outside DashQ: its `http.page` row is 1316 ms, finished at 1793879187. No later try
reproduced it, and its cause is unknown. A public `/style.css` took 1.2 to
1.5 s, which is the guard and proxy cost of a small request. `/data` took 2.9 to 3.3 s.

Profile since activation against the 0.18.1 table above:

| Operation | Rows | p50 ms | p95 ms | Max ms | Over 5 s |
|---|---:|---:|---:|---:|---:|
| `http.page` | 74 | 1498 | 1927 | 2854 | 0 |
| `http.data` | 3 | 1467 | 1863 | 1863 | 0 |
| `http.enroll` | 75 | 8 | 430 | 541 | 0 |
| `read.fleet`, off the page path | 59 | 5620 | 27350 | 28782 | 36 |

## Open

- **Normal auto-update stays off** until the drop-in `90-task260.conf` of
  `dashq-dashq.service` is removed and a release with this change is installed. A
  binary outside the release layout does not update itself. The drop-in is a runtime
  file, so a reboot also removes it and returns the service to 0.18.1.
- **Rollback:** remove that drop-in, reload the user units and restart the service.
  The indexes stay and are harmless.
- **Invalid stored block:** #262/#264 is included in the current activated candidate.
  Corrupt blocks retain explicit loss reasons while valid records remain visible;
  the historical installed artifact above predates that fix.
- **`read.fleet` still takes about 25 s per stream** in the background while pages are
  being requested. It no longer holds a request.
- **Merge-result CI** is separate required evidence.
- **Cold startup:** six page rows exceeded 5 s before readiness on the current
  activation. The ten public acceptance probes all passed after readiness.
