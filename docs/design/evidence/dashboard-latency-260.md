# Dashboard latency (#260)

[Owner request and acceptance](https://github.com/taskqorg/dashq/issues/260):
"Dashboard page must answer within seconds."

## Result

| Item | State | Evidence |
|---|---|---|
| A1 root cause | named | [Root cause](#a1-root-cause) |
| A2 Fleet never holds the page | verified in tests and live | [A2](#a2-fleet) |
| A3 ten public tries under 5 s | passed on the installed candidate | [A3](#a3-install-and-probes) |

Source of the running service: `b3d7259bac8f3bc6a3fc47c29aa682058771bc09`, a
development activation before merge. Later commits on the branch change evidence only.

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
   - `http.page` 287929 ms (1793873021 to 1793873309): nine `read.fleet` finished in
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
- **Invalid stored block:** one stream's window contains a block that no longer
  decodes, so `read.fleet` fails for it. After a restart that stream has no last data
  and reads unknown until the block ages out. This is filed as a follow-up task.
- **`read.fleet` still takes about 25 s per stream** in the background while pages are
  being requested. It no longer holds a request.
- **Merge-result CI** is separate required evidence.
