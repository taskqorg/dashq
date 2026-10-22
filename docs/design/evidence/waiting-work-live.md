# Waiting work live check

Task #183 checks D23 against one real TaskQ board. Read-only: no write, deployment, enrollment or production
access.

## Run

The ignored test `live_waiting_work_report_matches_data` ran on the owner's development Mac on 2026-10-23
(about epoch `1792705195`). It used a private 0600 config outside the repository with GitHub and the
`taskqorg/dashq` project only, the installed taskq `4.6.33-dev` binary as `taskq.binary`, and no Langfuse,
ServerQ or enrollment.

```sh
DASHQ_LIVE_CONFIG=<private config> cargo test --locked -- --ignored live_waiting --nocapture
```

## Result

- Waiting rows: all four waiting tasks carry TaskQ's own admission from the snapshot adapter. Each has code
  `dependency`, TaskQ's reason (for example `agentq#161 open` on #184 and
  `#183 open, #184 open, #185 open, #186 open` on #187) and producer time `1792705205`.
- Clock: TaskQ's `observed` was 10 s after the document time, because DashQ takes `now` before its reads.
  The first run therefore read every row `unknown` (`clock uncertain`). D23 now accepts records up to 60 s
  after the document time, like the other D4 readers, and the second run passed.
- Query: `{period: 86400, timezone: "UTC", cursor: 0, next_cursor: null, cap: 200}`.
- Changes: no rows. The comparison is `unavailable` (`no baseline`), because the only configured condition
  source is TaskQ, which reports no state change times.
- Requests: none. This board had no question, failure or ServerQ finding open.
- Agreement: `/data` through the HTTP route and `dashq report` built from the same parameters were equal
  after dropping times that move by the second (`generated_at`, ages and window bounds).

## Not covered

- No ready task was open, so no capacity, host, focus, load-gate or login code was seen live. The pinned
  v4.6.32 fixture covers these codes.
- Route stages and the enrolled `taskq-board/1` admission path were not checked live, because enrollment is
  not set up on this Mac. Unit tests cover them.
- Changes with a baseline (enrolled ServerQ episodes) were checked against fixtures only.
