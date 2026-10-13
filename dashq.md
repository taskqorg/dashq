# dashq

dashq is one read-only page for the taskqorg tools: hosts and versions, taskq boards, agent activity and compression from Langfuse, and serverq checks. This file holds every decision. A change that alters a decision edits this file first (AGENTS.md); code, README and deployment follow. No history here: decisions only, at most 200 lines.

## D1. Scope
- Read-only: dashq never writes to Langfuse, GitHub, serverq or any host. No buttons that change state.
- No new auth system: the page sits behind the existing Langfuse UI forward-auth guard (D3).
- No push channel from hosts: dashq only pulls the sources in D4. Host push is a later task (#4).
- No database: in-memory caches only; a restart rebuilds everything from the sources.
- Metadata only: never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces.

## D2. Process
- One Rust binary `dashq` on the shared `taskqorg/coreq` crate (release, update, launcher, service).
- `dashq serve` binds `127.0.0.1:18882` only; the port is not configurable to a non-loopback address.
- `dashq report [--period=S] [--host=H] [--agent=A] [--session=ID]` prints the `/data` JSON once (D7) for agents on the host without a browser login.
- `dashq selftest` loads config, renders the fixture snapshot and reads each source once; no writes.
- Routes: `GET /` (page), `GET /app.js`, `GET /style.css`, `GET /data`. Any other path 404; any other method 405.
- Headers on every response: `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, CSP `default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`. No third-party assets.
- Config: a private JSON file named by env `DASHQ_CONFIG` (mode 0600): Langfuse base URL and keys, a read-only GitHub token, the project list (D4.2), the serverq report path. Secrets never reach the browser, `/data`, logs or `report` output. Invalid config: the service refuses to start and names the bad key, never the value.

## D3. Deployment
- Runs on the production host (FRA), where Langfuse and the serverq report live.
- Served under the `dash.` host name of the Langfuse UI domain. The reverse proxy site for that name runs the same `forward_auth` guard as the Langfuse UI first, then proxies to `127.0.0.1:18882`.
- No public port: no firewall change, no listener on a public address, no route under `/api/public`, no bypass of the guard for any path including `/data`.
- Rollback: remove the `dash.` site block, stop the dashq service. Nothing else changes.
- dashq supersedes the agentq ServiceQ panel (`/serviceq`); its removal is agentq's task, not dashq's.

## D4. Sources
Each source has its own cache, single flight (one read in progress, callers share it) and a 10 s timeout per HTTP call.

### D4.1 Langfuse API
- Public API with the project keys, server side only. Reads the same event names as ServiceQ: monitor snapshots, activity and compression traces, `agent-context.native-tool` observations.
- Periods: 30 min, 2 h, 24 h (1800, 7200, 86400 s; default 1800). Health and incident history always read 24 h.
- Cursor pagination, at most 10,000 rows per event name and period. Hitting the cap marks that source `incomplete` (D6).
- Cache 30 s per period: page loads inside 30 s share one read.

### D4.2 GitHub: boards and taskq heartbeats
- Projects: an explicit list in config (`taskqorg/taskq`, `taskqorg/agentq`, `taskqorg/serverq`, `taskqorg/coreq`, `taskqorg/specq`, `taskqorg/dashq`). Repositories are never discovered.
- Boards: open issues labelled `taskq`, one page of 100 per repo; state from the projected `q-<state>` label, priority from `p0|p1|p2`. More than 100 marks the board `incomplete`. dashq does not fold comments: a label can lag the log by one service pass.
- Heartbeats: open issues labelled `taskq-host` (`taskq host <name>`): `version <v>`, `#<N> <attempt>` lines. Age = response `Date` minus issue `updated_at`, never the local clock.
- Latest releases: `releases/latest` per project repo, cached 10 min.
- Cache 60 s per repo. A `403`/`429` rate limit keeps the last value as unconfirmed (D6) until reset.

### D4.3 serverq report
- The local `report.json` written by `serverq check` (path in config). Read on each refresh after a `stat`; no cache beyond the file.
- Accepted: `schema` 1 with `at` (epoch seconds), `results` (`ok|warn|unknown` per check with `detail`), `incidents`, `manual_rules`. Another schema or invalid JSON: unknown with the reason.
- Stale after 7200 s (hourly timer plus margin). Manual rules are listed as not verified, never as passed.

## D5. Refresh
- The page fetches `/data` on load, every 60 s and on the Refresh button. No live push.
- The page shows `Updated <time>` and each source's age. A failed fetch keeps the last page and shows the error banner (D6); it never blanks values.

## D6. Unknown and stale
Every value carries the state of its source. The page never turns missing data into zero or into healthy.

- Source states: `ok` (fresh read), `stale` (read succeeded but data older than its limit), `unconfirmed` (this read failed; the previous value is shown with its time), `unknown` (no value yet, unsupported schema or not configured), `incomplete` (capped query). Each non-ok state has a one-line reason, e.g. `Langfuse: read failed (timeout), showing 14:02`.
- Freshness limits: host snapshot in Langfuse 600 s; taskq heartbeat its `ttl` (300 s default); serverq report 7200 s; GitHub read 180 s.
- Display: green only for `ok` and a healthy value. `stale`, `unconfirmed`, `unknown` are gray or amber with the reason and age, never green. A missing number shows `—` with `unknown: <reason>`, never `0`. Capped counts show `≥N (capped)`.
- Counters that reset with a process restart say so; a reset is not shown as a drop.
- Historical incidents are deduplicated and never shown as current failures.
- A summary bar at the top lists every source that is not `ok`, with its reason.
- `/data` uses the same states: each section has `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`, never `0`.

## D7. /data JSON
- `GET /data?period=1800|7200|86400&host=H&agent=A&session=ID` returns the document the page renders; filters are optional. Invalid parameters: 400. All sources failed: 503 with `{"error"}`; partial failure is 200 with source states.
- Top level: `schema` (1), `generated_at`, `period`, `filters`, `sources`, `hosts`, `attention`, `boards`, `agents`, `checks`.
- Session filters apply to activity and compression only; host health keeps host scope.
- It holds aggregates and evidence trace IDs, never prompts, commands, tool results, tokens or keys.
- Schema changes are additive within `schema` 1; a breaking change raises it.
- Agents read it with `dashq report` on the host, or through the guard with the owner's session.

## D8. Page
English, one column, sections in this order. Header links: Langfuse traces, Langfuse dashboards.

1. **Attention**: current problems from all sources, then observation gaps (stale or unknown checks), then incident history (24 h, deduplicated). Each item: source, host, what, since when.
2. **Hosts**: one row per host seen in any source. Columns: freshness, taskq version (heartbeat), agentq monitor and proxy versions (running, selected, latest stable, updater state), serverq report age and status, services from the latest snapshot (proxy, gateway, Langfuse health). A running version older than the latest release says `behind`.
3. **Boards**: one row per project: `In work N · Waiting for answer N · Ready N` (taskq R6 names), questions (`ask`) and failed with issue links, live attempts per host from heartbeats.
4. **Agents** (period): model activity per runtime and host; model usage per observed model with sample counts for each input/cache/output/reasoning field, never summed into a total; cost only when reported, else null; compression outcomes (successes, normal skips, failures disjoint; character reduction only when every counted call has before/after); native RTK estimates with origin; process counters (Headroom `codex_ws_*`, telemetry queue) in a collapsed block with their reset semantics.
5. **Server checks**: serverq results per check with detail, open incidents, manual rules marked not verified.

## D9. ServiceQ views
Every view of agentq `docs/SERVICEQ.md` maps here:

| ServiceQ view | dashq |
|---|---|
| Hosts and services, 10 min freshness, newest device identity | D8.2 Hosts; D6 limit 600 s |
| Problems and observation gaps (Что проверить) | D8.1 Attention |
| Last native failure receipt (safe fields) | D8.1 Attention, same safe fields, nulls kept |
| Incident history, deduplicated | D8.1 Attention |
| Host updates (selected package, stable release, running versions, subscription, updater) | D8.2 Hosts version columns |
| Model activity and legacy compression | D8.4 Agents |
| Model usage per model | D8.4 Agents |
| Native RTK accounting | D8.4 Agents |
| Process counters and telemetry queue, native Headroom counters | D8.4 Agents, collapsed block |
| `/serviceq/data` JSON and `--report` with host/agent/session filters | D7 `/data` and `dashq report` |
| Period 30 min / 2 h / 24 h, page refresh 1 min, 30 s coalescing, 10,000-row cap | D4.1, D5 |
| ServerQ report through the monitor | D4.3: read directly, not through monitor snapshots |
| TaskQ status report (`checkedAt`, `state`) through the monitor | Dropped: never implemented; D4.2 reads heartbeats from the board, the source of truth |
| Langfuse dashboard `ServiceQ` (provisioned widgets) | Dropped: a write to Langfuse (D1); the header links to Langfuse dashboards |
| Retry/recovery durations at snapshot time | D8.1 Attention, labelled with the snapshot time, not a countdown |
