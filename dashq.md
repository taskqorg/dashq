# dashq

dashq is one operator page for the taskqorg tools: hosts and versions, taskq boards, agent activity and compression from Langfuse, and serverq checks. This file holds every decision; changes follow specq [spec first](https://github.com/taskqorg/specq/blob/main/engineering/spec-first.md). No history here: decisions only, at most 200 lines.

## D1. Scope
The pull-only rules below describe the current baseline. Accepted D11 narrowly replaces the no-write, no-push and no-database restrictions for enrollment, ingestion and connection management; it does not authorize host commands.
- Read-only: dashq never writes to Langfuse, GitHub, serverq or any host. No buttons that change state.
- No new auth system: the page sits behind the existing Langfuse UI forward-auth guard (D3).
- No push channel from hosts: dashq only pulls the sources in D4. Host push is a later task (#4).
- No database: in-memory caches only; a restart rebuilds everything from the sources.
- Metadata only: never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces.

## D2. Process
- One Rust binary `dashq` on the shared `taskqorg/coreq` crate (release, update, launcher, service).
- `dashq serve` binds `127.0.0.1:18882` only; the port is not configurable to a non-loopback address.
- `dashq report [--period=S] [--host=H] [--agent=A] [--session=ID]` prints the `/data` JSON once (D7) for agents on the host without a browser login.
- `dashq admin WORKSPACE SUBJECT` binds a workspace administrator for enrollment (D11).
- `dashq selftest` loads config, renders the fixture snapshot and reads each source once; no writes.
- `dashq --snapshot DIR` writes the fixture page as `DIR/page.html` (stylesheet inlined, no script) and `DIR/page.png`, so a reviewer sees the rendering without a browser login. PNG renderer: headless Chrome (`--headless --screenshot`, 1280 px wide), the binary from env `DASHQ_CHROME`, else `google-chrome`, `chromium`, `chromium-browser` on `PATH`, else the macOS Google Chrome app. No Chrome: `page.html` is written and the command fails naming the missing renderer. No config, no source reads.
- Baseline routes (D11 adds its explicit authenticated routes): `GET /` (page), `GET /app.js`, `GET /style.css`, `GET /data`. Any other path 404; any other method 405.
- Headers on every response: `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, CSP `default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`. No third-party assets.
- Config: a private JSON file named by env `DASHQ_CONFIG`, else `<install>/config.json` (coreq service units pass only `PATH`), mode 0600: Langfuse base URL, project id and keys, a read-only GitHub token, the project list (D4.2), the serverq report path and the host id its row joins, the public `dash.` host name. Requests whose `Host` is not the loopback address or that name get 421 (DNS rebinding). Secrets never reach the browser, `/data`, logs or `report` output. Invalid config: the service refuses to start and names the bad key, never the value. `selftest` without any config file skips the source reads and says so.
- `dashq service install|uninstall|run` and `dashq update [--check]` are the coreq commands; `service run` is `serve` and marks the release healthy after 60 s.
- Release: CI (`.github/workflows/release.yml`) on a `v<ver>` tag equal to the `Cargo.toml` version builds static `dashq-<ver>-linux-x86_64` and `dashq-<ver>-linux-aarch64`, `SHA256SUMS` and `latest.json` into a GitHub release of `taskqorg/dashq`. Each artifact passes `selftest` with `DASHQ_SELFTEST_OFFLINE=1` before publishing; a failure publishes nothing. Only owners push tags. Hosts take it with `dashq update`; rollback is coreq `previous`.

## D3. Deployment
- Runs on the production host (FRA), where Langfuse and the serverq report live.
- Served under the `dash.` host name of the Langfuse UI domain. The reverse proxy site for that name runs the same `forward_auth` guard as the Langfuse UI first, then proxies to `127.0.0.1:18882`.
- No public port: no firewall change, no listener on a public address, no route under `/api/public`, browser data including `/data` always remains guarded. D11 machine routes use their own narrow authorization instead of browser forward-auth; no wildcard exemption.
- Rollback: remove the `dash.` site block, stop the dashq service. Nothing else changes.
- dashq supersedes the agentq ServiceQ panel (`/serviceq`); its removal is agentq's task, not dashq's.

## D4. Sources
Each source has its own cache, single flight (one read in progress, callers share it) and a 10 s timeout per HTTP call.

### D4.1 Langfuse API
- Public API with the project keys, server side only. Reads the same event names as ServiceQ: monitor snapshots, activity and compression traces, `agent-context.native-tool` observations.
- Agents sources: the agentq observations (agentq.md § 6), one span per event, span name = event name: `agentq.run` (RTK runs), `agentq.retrieval` (MCP tool calls), `agentq.installation` (hourly heartbeat) and `agentq.incident`. Each carries `agentq.installation` (fleet key: one installation, one row), `agentq.version`, `agentq.runtime`, `host.name` and `agentq.stage`; the Agents view takes fleet, versions, last seen and health from them, usage from `agentq.run`/`agentq.retrieval`, incidents from `agentq.incident`. Last seen is the newest event time per installation; an installation without an `agentq.installation` heartbeat in 7200 s is stale (D6). The legacy Node names above stay until the last Node host is gone (agentq#50).
- Periods: 30 min, 2 h, 24 h (1800, 7200, 86400 s; default 1800). Health and incident history always read 24 h.
- Cursor pagination, at most 10,000 rows per event name and period. Hitting the cap marks that source `incomplete` (D6).
- Cache 30 s per period: page loads inside 30 s share one read.

### D4.2 GitHub: boards and taskq heartbeats
- Projects: an explicit list in config (`taskqorg/taskq`, `taskqorg/agentq`, `taskqorg/serverq`, `taskqorg/coreq`, `taskqorg/specq`, `taskqorg/dashq`). Repositories are never discovered.
- Boards: open issues labelled `taskq`, one page of 100 per repo; state from the projected `q-<state>` label, priority from `p0|p1|p2`. More than 100 marks the board `incomplete`. dashq does not fold comments: a label can lag the log by one service pass.
- Heartbeats: open issues labelled `taskq-host` (`taskq host <name>`): `version <v>`, `#<N> <attempt>` lines. Age = response `Date` minus issue `updated_at`, never the local clock.
- Latest releases: `releases/latest` per project repo, cached 10 min.
- Cache 60 s per repo. A `403`/`429` rate limit keeps the last value as unconfirmed (D6) until reset.
- Snapshot adapter ([S3 R1](docs/design/adapters.md#s3-taskq-snapshots-and-questions)): with config `taskq.binary` (absolute path of the pinned taskq), dashq runs `taskq status --json` per project in a fresh scratch folder holding only `taskq.json` and a scratch install whose `token` is the read-only token (`TASKQ_INSTALL`; taskq reads no token variable), no HOME, incident reports off, folder removed after the run; single flight, 10 s timeout, cache 60 s. Its totals, questions (S3 shape, read-only) and failure reasons replace the label counts; heartbeats stay label reads. A failed, incomplete or unknown-schema run is a failed read with its reason (D6); `sources.taskq` lists it. Absent key: labels only.

### D4.3 serverq report
- The local `report.json` written by `serverq check` (path in config). Read on each refresh after a `stat`; no cache beyond the file.
- Accepted: `schema` 1 with `at` (epoch seconds), `results` (`ok|warn|unknown` per check with `detail`), `incidents`, `manual_rules`. Another schema or invalid JSON: unknown with the reason.
- Stale after 7200 s (hourly timer plus margin). Manual rules are listed as not verified, never as passed.

## D5. Refresh
- Redesign target: [shared dashboard rules W4–W6](https://github.com/taskqorg/specq/blob/main/design/dashboards.md) and D10. The mechanism below is the current baseline, not proof of stable in-place updates.
- The page fetches `/data` on load, every 60 s and on the Refresh button. No live push.
- One renderer, on the server: `GET /` renders the sections from the same document as `/data`; after a good `/data` fetch the page reloads its sections from `/` with the same query.
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
English. The existing one-column layout below remains the implementation baseline; D10 is the approved redesign direction, pending implementation and verification. Header links: Langfuse traces, Langfuse dashboards.

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

## D10. Dashboard redesign target

Integration research: [independent products](docs/design/integration.md), [service adapters and owner answers](docs/design/adapters.md). Historical exploratory findings: [layout review](docs/design/overview-review.md). The accepted [combined first screen](docs/design/first-screen.md) now owns composition; accepted enrollment in D11 amends D1–D8 without replacing the layout references below.

Approved design; not implemented by this specification change. Apply [shared dashboard and widget rules W1–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md). The [combined first screen](docs/design/first-screen.md) owns composition; [overview](docs/design/overview.md) owns resource visuals and update parameters. TaskQ block interiors inherit [shared blocks](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md), not the dashboard demo styling. D8 documents the existing section inventory; preserve its information while changing hierarchy.

- First viewport: compact Attention and server overview together. Answer which hosts need attention, their current CPU utilization, RAM used/total and disk used/total per filesystem, and how those values are changing. Keep warnings visible alongside the affected host. Board questions and failures join Attention; board totals must not displace the server overview.
- Beside servers on the first viewport: [project board summaries](docs/design/boards.md), inline answers and node-policy entries; reflow without shrinking. Next content: detailed project boards, agent activity and compression, host versions and individual server checks. Last: incident history, process counters and diagnostic evidence. Do not create a third screen merely to fill it; preserve access to every existing view.
- Server trends require timestamped samples and visible time windows. D4 does not yet guarantee CPU/RAM/disk history: qualify existing sources before selecting a collector or retention mechanism. Missing metrics or history remain explicitly unavailable; never fabricate charts from fixtures or a single reading.
- Automatic refresh is required. Retain the current 60 s cadence until separately changed; manual Refresh is optional recovery, never the normal workflow. Apply new values without losing open details or reading position. Freshness describes source observations, not just the last page fetch.
- Use readable moderate density, aligned groups and concise semantic Unicode/icons. Select exact glyphs and verify rendering in the design iteration. Group repeated Attention observations by source or host without claiming an unproven common cause; keep raw records and linked tasks accessible.
- Compact refinement: remove repeated missing-value prose and excess cell padding before reducing text size. Use a visible shared missing-data legend and accessible per-cell reasons; retain host freshness and warning meaning in the overview. Animation demonstrations belong in the design preview, not the production first viewport.
- Changes to CPU, RAM or disk usage do not automatically imply failure or recovery. Persistent status follows explicit thresholds; a transient change cue cannot override unknown/stale state or hide a warning.

- Layout: compact Attention above side-by-side servers and Boards under first-screen F-A1–F-A5; shared TaskQ Work mode opens on demand. The server column uses a shared host grid. Each host has identity/freshness, CPU percentage and trend, RAM used/total with utilization bar and trend, then one used/total bar per filesystem. Keep the host rows aligned and put warnings beside affected resources. History and diagnostics follow below.
- Reference defaults: 30 min trends on a shared 0–100% scale, 16 px main text, brief local value tint and direction/delta where useful. Production remains automatic at 60 s; the preview's sample button is not a product control. Source retention/cadence must qualify the chosen window.

Still unresolved: production warning thresholds, collection/retention qualification and Attention grouping depth for multiple real incidents. Demo host capacities, filesystem names, freshness and 85/90% thresholds are invented examples, not approved inventory or policy. Do not fabricate data while these remain unavailable.

Acceptance: review the first viewport and deeper content with representative live data, missing history, warnings and long labels; verify readable large text, both themes, reduced motion and multiple automatic refreshes. Record which data and UI requirements are implemented, unverified or blocked by a source gap.

## D12. Security
- Follows the [specq security baseline](https://github.com/taskqorg/specq/blob/main/engineering/security.md); reports go to the route in `SECURITY.md`.
- Every workflow declares top-level `permissions: contents: read`; a publishing job adds only what it writes (`release.yml` `publish`: `contents: write`).
- Accepted exception (owner @alexkirs): the repository is private on the GitHub free plan, so branch protection, secret scanning with push protection, Dependabot alerts and private vulnerability reporting are unavailable. Reason: the plan does not offer them for private repositories. Compensation: owner-only pushes and tags, reviewed PRs, committed lockfile. Revisit when the repository goes public or the plan changes.

## D11. Independent product connections

Accepted target: [enrollment C1–C7](docs/design/enrollment.md), including E1–E10, owns the end-to-end connection contract; [connection interface L1–L4](docs/design/connections.md) owns its accepted presentation. Implementation and release qualification remain pending.

- One process and one SQLite store (WAL/FULL) own grants, membership, revocations, receipts, projections, history and bounded export outbox. No extra daemon or database server.
- C3 owns browser/device routes and authority; C4 owns durable acceptance and replay; C5 owns limits. Adjust CSP only for reviewed same-origin approval forms. Keep the loopback listener; never expose read APIs through machine-route exceptions.
- D4 gains authenticated producer snapshots. Enrollment registry supplies expected installations even without recent events; optional Langfuse/GitHub sources cannot block independent product enrollment or hide committed snapshots. Legacy sources keep their provenance and never create duplicate identities by hostname.
- D6/D7 apply the authenticated actor's scope server-side, including HTML, history, evidence, exports and local report. Local report is a protected host-administrator capability, never a remote identity bypass. Freshness uses producer observation time; connection and export health remain separate.
- D8 adds approval, scoped installation overview and explicit revoke. First completion requires the same projection as the browser; resource history needs real samples. Observation grants provide no task-answer or host-command authority.
- Pairing (#34): `POST /connect/device|token|ack` and `/connection/revoke` per coreq wire `coreq-enrollment/1`; guarded `GET|POST /connect` approves or denies a code. The browser subject comes only from `X-Dashq-Subject`, which the proxy strips from clients and copies from the guard's verified answer ([fixture/Caddyfile](fixture/Caddyfile); source header qualified in #4). Loopback callers are host-local and trusted like `dashq report`; a proxy-only listener or shared secret is a #4 qualification item if the host has untrusted local users. Re-pairing a connected installation warns that approval replaces it. Approval needs workspace membership, the `https://<public_host>` Origin and a CSRF token bound to account and code. `dashq admin WORKSPACE SUBJECT`, run by the host administrator, creates `dashq.db` and `dashq.key` (0600) next to the config and binds the first administrator; until then enrollment routes answer 503. The token response is sealed with `dashq.key` (AES-256-GCM) and erased at ack or expiry; the store keeps only credential and device-code SHA-256 verifiers.
- Board scope (#58): `/connect/device` may carry `scope` `{name: "<id>/<version>", value: <single line ≤128>}` (coreq#36); malformed is `invalid_request`. The approval page names it under Board scope; none renders as before. The grant stores it; token, ack/revoke status and receipt replies name it as `scope`. A later token, ack, ingest or receipt request naming another scope gets 403 `scope`; revoke always succeeds. Evidence: [light](docs/design/evidence/approval-scope-light.png), [dark](docs/design/evidence/approval-scope-dark.png).
- Ingestion (#35): `POST /ingest` and `/connection/receipt` per `coreq-enrollment/1`; DashQ runs coreq's conformance traces ([fixture/enrollment](fixture/enrollment), copied unchanged from the pinned tag). The grant supplies installation and workspace; a schema outside the grant is rejected `scope`, a payload outside the metadata allowlist (short identifier keys, numbers, booleans, short single-line strings; no content, command, output, path or secret fields or path-like values) `payload`, a reused event ID with other content `replay_mismatch`, a record older than the receipt window `expired`; a full store answers 503 so nothing is acknowledged. Per device 60 requests/min, burst 10. Every record is a snapshot until export schemas exist. Data rows keep the workspace of the grant that sent them; after revoke a new approval may pick another workspace, which starts empty.
- Projection: one function serves `/data`, `dashq report` and a 2 s service pass; a receipt becomes `projected` only when that projection read its snapshot as current. `/data` adds `installations` `{source, rows}` and `sources.enrollment` (only once set up): one row per installation and workspace from its newest grant, `connection` `active|pending|revoked`, `observation` from producer time (stale after 180 s; future time unknown, `clock uncertain`; none: `no snapshot yet`) and `snapshot` `{event_id, schema, payload}`. Scope: loopback requests and `dashq report` see all; requests to the public name see their verified subject's workspaces (administrator: all installations there; member: those it approved), none without a subject. Langfuse and GitHub config are optional; their failure never hides snapshots or turns `/data` into 503.
- Lifecycle UI (#36): the page's Installations section, after Attention, shows per row connection (`active`, `pending`, `revoked` with history kept, never an active failure), data age by producer time, history warmup until 3 samples, export gaps of that workspace and the current snapshot's event ID as evidence. Approval refreshes `/connect?wait=` every 2 s for at most 60 s and redirects to the row (`/#inst-<installation>-<workspace>`) once the scoped projection reads that generation's snapshot, which makes its receipt `projected`. `Revoke…` opens a guarded confirmation that names installation and effect; its POST needs the Origin and a CSRF token bound to account, installation and workspace, and only the workspace administrator or the approving member revokes. Refresh keeps open disclosures, focus and scroll; charts belong to D10 (#23). `dashq report` reads the store only with the 0600 config and key, so it stays a host-administrator capability. Visual evidence: [docs/design/evidence](docs/design/evidence).
- Security exception: C3's owner-accepted non-expiring ingestion credential is revocable, per installation and observation-only; all other credential classes follow the shared security baseline.

## D13. TaskQ answer card

Accepted target: [shared-card integration](docs/design/answers.md). TaskQ owns one component; DashQ pins its assets and supplies the scoped browser adapter. This narrowly amends the no-state-changing-buttons restriction in D1 and adds guarded `POST /answer` to D2/D7; observation enrollment retains no action authority. Ordinary answers use TaskQ’s canonical dispatcher, expected revision, operation ID and verified receipt. Consequential operations are excluded. Transport and authority are implemented ([#41](https://github.com/taskqorg/dashq/issues/41), [answers § Route](docs/design/answers.md#route-41)); the route stays off until a deployment configures `answer`, and production writes wait for live qualification ([#44](https://github.com/taskqorg/dashq/issues/44)). The card embedding is [#43](https://github.com/taskqorg/dashq/issues/43).

## D14. TaskQ resource controls

Accepted target: [resource controls](docs/design/resource-controls.md) reuses TaskQ-owned node/runtime, capacity and Work mode components. This separately authorized typed host-policy capability narrowly extends D1; it does not extend `/answer` or observation credentials. TaskQ's host authority owns limits, Auto admission and effective readback. No remote shell or second scheduler. Implementation, concrete adapter contract and live qualification remain gated as specified.

## D15. ServerQ finding handoff

Accepted target: [finding tasks H1–H5](docs/design/finding-tasks.md) narrowly extends D1 with an explicitly authorized TaskQ create/optional-start adapter and shared TaskQ composer. Observation credentials and `/answer` gain no authority. TaskQ owns board mutation/readback; ServerQ owns incident verification. A closed task alone never removes a problem from Attention. Implementation and live qualification remain pending.

## D16. AgentQ integration settings

Accepted target: [client integration G1–G4](docs/design/agentq-settings.md) adds a separately authorized typed AgentQ configuration adapter to D1/D2. AgentQ owns discovery, plans, revision checks, journaled apply, readback and native-client verification; DashQ supplies scoped presentation and delivery. Observation credentials and `/answer` gain no authority. Release remains gated by real host and consumer qualification.

## D17. AgentQ optimization results

Accepted target: [optimization results P1–P3](docs/design/optimization-results.md) refines D8.4 with AgentQ-owned accounting definitions, honest coverage and a separate paired-experiment view. Read-only; it adds no configuration authority, optimizer or experiment runner. Existing metadata/privacy and source-failure rules remain.

## D18. Service versions and diagnostics

Accepted target: [service versions V1–V4](docs/design/service-versions.md) refines D8.2/D9. Distinguish available, selected, running and verified health per installation; coreq and each consumer retain lifecycle ownership. Read-only diagnostics use allowlisted evidence. No remote update, restart or rollback authority is added.
