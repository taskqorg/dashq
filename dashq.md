# dashq

dashq is one guarded operator page for the taskqorg tools: hosts and versions, TaskQ boards, agent activity and AgentQ events from Langfuse, and ServerQ checks. It reads and shows metadata; it is not an editor, scheduler, shell or second auth system, and it changes state only through the narrow authorities that R3 lists.

This file holds every decision; changes follow specq [spec first](https://github.com/taskqorg/specq/blob/main/engineering/spec-first.md). Lower-priority detail lives in the files under [Linked detail](#linked-detail).

## Rules
Mandatory for every change. Product sections refine these rules and never weaken them.
- **R1 Metadata only.** Never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces. `/data` holds aggregates and evidence trace IDs, never prompts, commands, tool results, tokens or keys.
- **R2 Unknown is never healthy.** Every value carries the state of its source. Missing data never becomes zero or healthy (D6).
- **R3 Read-only by default.** dashq never writes to Langfuse, GitHub, ServerQ or any host, and has no state-changing button, except these narrow authorities: D11 enrollment, ingestion and connection management; D13 TaskQ answers, task controls and PM requests; D14 resource controls; D15 finding tasks; D16 AgentQ configuration; D25 blind host link relay, transport only.
  - D11 replaces the no-write, no-push and no-database restrictions only for enrollment, ingestion and connection management.
- **R4 Grants stay separate.** Enrollment and observation grants carry no answer, control, configuration or host-command authority. Answer grants carry no control authority. D11 authorizes no host commands.
- **R5 No new auth system.** The page sits behind the existing Langfuse UI forward-auth guard (D3). Browser data, including `/data`, always remains guarded. D11 machine routes use their own narrow authorization instead of browser forward-auth; no wildcard exemption.
- **R6 Loopback only.** `dashq serve` binds `127.0.0.1:18882`; the port is not configurable to a non-loopback address. No public port.
- **R7 Secrets stay server side.** Secrets never reach the browser, `/data`, logs, URLs or `report` output.
- **R8 Pull by default.** dashq pulls the D4 sources. Hosts push only through the R3 machine routes; any other host push is a later task (#4).
- **R9 No other database.** Caches are in memory and a restart rebuilds them from the sources. The D11 SQLite store is the only database.
## Runtime and access
### D2. Process
- One Rust binary `dashq` on the shared `taskqorg/coreq` crate (release, update, launcher, service).
- `dashq serve` serves the page (R6). `dashq report [--period=S] [--host=H] [--agent=A] [--session=ID] [--cursor=N] [--block=ID]` prints the `/data` JSON once (D7) for agents on the host without a browser login.
- `dashq admin WORKSPACE SUBJECT` binds a workspace administrator for enrollment (D11). `dashq profile [--window=S]` prints the profile report (D20); the default window is 86400 s.
- `dashq selftest` loads config, renders the fixture snapshot and reads each source once; no writes. Without any config file it skips the source reads and says so.
- `dashq doctor` checks host requirements and `dashq --snapshot DIR` writes the fixture page for review; both are in [process detail](docs/contract/process.md).
- `dashq service install|uninstall|run` and `dashq update [--check]` are the coreq commands; `service run` is `serve` and marks the release healthy after 60 s. Update, restart and rollback: [process detail](docs/contract/process.md#update-and-rollback).
- Baseline routes: `GET /` (page), `GET /app.js`, `GET /style.css`, `GET /data`. D11, D13–D16, D22 and D25 add their explicit routes. Any other path is 404; any other method is 405.
- Every response sends `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and CSP `default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`. No third-party assets.
- Requests whose `Host` is neither the loopback address nor the configured public name get 421 (DNS rebinding).
- Config is a private JSON file named by env `DASHQ_CONFIG`, else `<install>/config.json` (coreq service units pass only `PATH`), mode 0600. Invalid config: the service refuses to start and names the bad key, never the value. Keys: [process detail](docs/contract/process.md#config).
- Release: a `v<ver>` tag equal to the `Cargo.toml` version publishes static Linux builds that passed an offline selftest; a failure publishes nothing. Only owners push tags. Detail: [process detail](docs/contract/process.md#release).
### D3. Deployment
- Runs on the production host (FRA), where Langfuse and the ServerQ report live.
- Served under the `dash.` host name of the Langfuse UI domain. The reverse proxy site for that name runs the same `forward_auth` guard as the Langfuse UI first, then proxies to `127.0.0.1:18882`. No firewall change, no listener on a public address, no route under `/api/public`.
- Rollback: remove the `dash.` site block and stop the dashq service. Nothing else changes.
- dashq supersedes the AgentQ ServiceQ panel (`/serviceq`); its removal is AgentQ's task, not dashq's.
### D12. Security
- Follows the [specq security baseline](https://github.com/taskqorg/specq/blob/main/engineering/security.md); reports go to the route in `SECURITY.md`. The repository is public and the shared baseline applies.
- Every workflow declares top-level `permissions: contents: read`; a publishing job adds only what it writes (`release.yml` `publish`: `contents: write`).
- Recorded exception: the C3 ingestion credential (D11) does not expire. It is revocable, per installation and observation-only; all other credential classes follow the shared baseline.
### D20. Profiling
- Applies [built-in profiling](https://github.com/taskqorg/specq/blob/main/engineering/profiling.md). Counters are always on in `serve`, `service run`, `report` and `update`: one record per finished operation `{at, op, ms, outcome, error}`. `selftest` and tests write nothing.
- Records hold no secrets, content, URLs or paths. A failed write never fails the operation. The report never shows a missing or partial measurement as zero or ok. Budgets, store, traces and report: [process detail](docs/contract/process.md#profiling).
## Data and sources
### D6. Unknown and stale
- Source states: `ok` (fresh read), `stale` (read succeeded but data is older than its limit), `unconfirmed` (this read failed; the previous value is shown with its time), `unknown` (no value yet, unsupported schema or not configured), `incomplete` (capped query). Each non-ok state has a one-line reason, for example `Langfuse: read failed (timeout), showing 14:02`.
- Freshness limits: Langfuse read 600 s; TaskQ heartbeat its `ttl` (300 s default); ServerQ report 7200 s; GitHub read 180 s.
- Display: green only for `ok` and a healthy value. `stale`, `unconfirmed` and `unknown` are gray or amber with the reason and age, never green. A missing number shows `—` with `unknown: <reason>`, never `0`. Capped counts show `≥N (capped)`.
- Counters that reset with a process restart say so; a reset is not shown as a drop. Historical incidents are deduplicated and never shown as current failures.
- A Source status summary lists every source that is not `ok`, with its reason; D10 sets its placement.
- `/data` uses the same states: each section has `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`, never `0`.
### D4. Sources
Each source has its own cache, single flight (one read in progress, callers share it) and a 10 s timeout per HTTP call.
#### D4.1 Langfuse API
- Public API with the project keys, server side only. Model activity is every `GENERATION` observation (a Langfuse type, not an event name), whatever its producer.
- Agents sources are the AgentQ observations (agentq.md § 6), one span per event, span name = event name: `agentq.compress` (core optimization accounting, D17), `agentq.run` (RTK runs), `agentq.retrieval` (MCP tool calls), `agentq.installation` (hourly heartbeat), `agentq.incident` and `agentq.model` (one Claude model call, D24).
- Each event carries `agentq.installation` (fleet key: one installation, one row), `agentq.version`, `agentq.runtime`, `host.name` and `agentq.stage`. The Agents view takes fleet, versions, last seen and health from them, usage from `agentq.run`/`agentq.retrieval` and incidents from `agentq.incident`.
- Last seen is the newest event time per installation. An installation without an `agentq.installation` heartbeat in 7200 s is stale.
- Periods: 30 min, 2 h, 24 h (1800, 7200, 86400 s; default 1800). Health and incident history always read 24 h. Client use reads: [views detail](docs/contract/views.md#langfuse-reads).
- Cursor pagination, at most 10,000 rows per event name and period; hitting the cap marks that source `incomplete`. Cache 30 s per period: page loads inside 30 s share one read.
#### D4.2 GitHub boards and TaskQ heartbeats
- Projects are an explicit list in config (`taskqorg/taskq`, `taskqorg/agentq`, `taskqorg/serverq`, `taskqorg/coreq`, `taskqorg/specq`, `taskqorg/dashq`). Repositories are never discovered.
- Boards: open issues labelled `taskq`, one page of 100 per repo; state from the projected `q-<state>` label, priority from `p0|p1|p2`. More than 100 marks the board `incomplete`. dashq does not fold comments: a label can lag the log by one service pass.
- Heartbeats: open issues labelled `taskq-host` (`taskq host <name>`) with `version <v>` and `#<N> <attempt>` lines. Age = response `Date` minus issue `updated_at`, never the local clock.
- Latest releases: `releases/latest` per project repo, cached 10 min. Other reads cache 60 s per repo. A `403`/`429` rate limit keeps the last value as unconfirmed until reset.
- Snapshot adapter ([S3 R1](docs/design/adapters.md#s3-taskq-snapshots-and-questions)): with config `taskq.binary` (absolute path of the pinned taskq), dashq runs `taskq status --json` per project in an isolated scratch run ([process detail](docs/contract/process.md#snapshot-adapter-run)); single flight, 10 s timeout, cache 60 s. Absent key: labels only.
- Snapshot totals, questions (S3 shape, read-only) and failure reasons replace the label counts; heartbeats stay label reads. A failed, incomplete or unknown-schema run is a failed read with its reason; `sources.taskq` lists it.
#### D4.3 ServerQ report
- The local `report.json` written by `serverq check` (path in optional `serverq` config) is read on every refresh. On failure the last-good report stays in memory with its original producer time, unconfirmed until a successful read.
- Absent config is unconfigured, distinct from a missing report. Absolute config paths and their acceptance fixtures use the target platform's path rules.
- Accepted: `schema` 1 with `at` (epoch seconds), `results` (`ok|warn|unknown` per check with `detail`), `incidents`, `manual_rules`. Another schema or invalid JSON is unknown with the reason. Stale after 7200 s (hourly timer plus margin).
- Manual rules are listed as not verified, never as passed. Enrolled `serverq-report/1` snapshots follow the same rules per installation and workspace: [enrolled ServerQ reports](docs/contract/connections.md#enrolled-serverq-reports).
#### D4.4 Resource samples
- Producer contract: agentq.md § 6 Resource samples (`agentq.resource`, one per 300 s bucket). Enrolled history (D11) is read first; Langfuse `agentq.resource` observations (with the `io` field group, else Langfuse cuts `agentq.resource.disks`) serve installations without enrolled samples. Each row names its `provenance`.
- Window = the period; raw samples, no aggregation; at most 2016 per installation (7 days of buckets), newest kept; a cut marks the row `incomplete`. Langfuse reads keep D4.1 pagination and cap.
- A missing, invalid or inconsistent value is `null` with the producer's or dashq's reason; nothing is derived from other fields or the legacy disk-headroom summary. Sample fields: [views detail](docs/contract/views.md#resource-samples).
- Current = newest sample; stale after 900 s. History shows `gaps` (over two buckets without a sample, or a stale tail), `resets` (`warmup` or `counter reset` CPU baselines) and `warmup` below three samples. A row that is not `ok` joins Attention gaps.
### D7. /data JSON
- `GET /data?period=1800|7200|86400&host=H&agent=A&session=ID&block=ID&cursor=N` returns the document the page renders. Filters are optional; `block` (D11 telemetry) adds `telemetry_block` and filters nothing; `cursor` pages capped lists (D23).
- Invalid parameters: 400. All sources failed: 503 with `{"error"}`. Partial failure is 200 with source states.
- Session filters apply to model activity only; host health and resources keep host scope.
- Schema changes are additive within `schema` 2; a breaking change raises it. Top-level keys and the declared exceptions: [views detail](docs/contract/views.md#data-document).
- Agents read `/data` with `dashq report` on the host, or through the guard with the owner's session.
### D5. Refresh
- The page fetches `/data` on load, every 60 s and on the Refresh button; no live push. The server renders the page shell; D13 boards use the pinned TaskQ browser renderer.
- `GET /` renders the sections from the same document as `/data`; after a good `/data` fetch the page reloads its sections from `/` with the same query.
- The page shows `Updated <time>` and each source's age. A failed fetch keeps the last page and shows the error banner; it never blanks values.
- Target: [shared dashboard rules W4–W6](https://github.com/taskqorg/specq/blob/main/design/dashboards.md) and D10. This mechanism is the baseline, not proof of stable in-place updates.
## Operator page
### D8. Page
The [operator redesign](docs/design/operator-redesign/README.md) is the target for D8 and D10 over the host-first baseline: hosts and their state first, tasks through shared TaskQ UI, other information on demand. The page is English. R1–R9 and D2–D7 stay binding; D10 amends only the source-summary placement. Production qualification remains pending.
1. **Hosts**: the [minimal Overview](docs/design/operator-redesign/overview-candidate.html) shows one machine/kind per visual group: identity, then scoped state and age, then resource facts with right-hand trends, then direct detail/action. Missing resources form one readable unavailable state; partial metrics retain individual reasons.
   - Versions/update, login probes, queue counts and TaskQ receipts stay in expandable host detail and their D22 views; each value keeps its source and time. Observations, joins and conflicts: [views detail](docs/contract/views.md#host-observations).
   - Silence means unknown, not offline. A heartbeat never proves client login or service health. No host state is inferred from activity or totals. Shared queues remain unassigned, never multiplied across hosts.
2. **Attention**: only a condition requiring a person and an identified action: expired login → that host's sign-in instructions; failed unit → its diagnostics; verified low disk → that filesystem's finding; task question → the single shared answer card. Each item names host/task, reason, age and action.
   - Observation gaps, revoked connections and incident history are detail, not current failures; automatic retries and ordinary waits are not requests for a person. Their non-ok meaning stays beside the affected host values. A source outage requiring intervention may appear with its recovery action.
3. **Tasks**: selecting a host's count opens its observed tasks in the TaskQ view; a direct Tasks entry includes all configured projects and unassigned work. Reuse the tray's shared rows, counters, answers and released controls under D13/D14.
   - Retain project/board identity, state age, runtime, issue/session links, questions, failures, ready, blocked, later and Closed coverage. A missing full-workspace or control release stays an explicit gap, not a DashQ editor.
4. **Agents**: `/agents` starts with TaskQ attempts and a selected inspector; identity, host/runtime, coverage state/reason and last stage observation age precede detail. Selection, focus and return survive refresh, Back and narrow reflow.
   - Client activity, D21 stage totals, D17 optimization and D24 model use keep separate linked destinations; totals are never merged. Per-model field sample counts stay separate; cost is null unless reported. [Client observation rules](docs/design/optimization-results.md#client-observations-and-coverage) apply; login is a separate probe.
5. **Server checks**: host detail or Checks retains per-rule detail, incidents, linked work, numeric evidence and manual rules marked not verified. A closed task never establishes recovery.
### D10. Dashboard redesign target
The [operator redesign package](docs/design/operator-redesign/README.md) owns the next target and its preservation ledger. Existing runtime authority and qualification gates remain. The target enables no collection or action and grants no deployment, sign-in, host command, restart/update button or mutation authority.
- Ownership: the [host-first package](docs/design/first-screen.md) owns composition; [overview](docs/design/overview.md) owns resources, [boards](docs/design/boards.md) TaskQ adoption, [connections](docs/design/connections.md) lifecycle presentation. Apply [W1–W15](https://github.com/taskqorg/specq/blob/fea6e540efa6971c5739453b198183cbb3fb010d/design/dashboards.md).
- Overview: approved compact host cards across the main width, one column at narrow widths without smaller type; no side-by-side project boards or permanent detail panels. Expected hosts stay visible when observations are stale. Attention is exactly the D23 request list, immediately after the hosts, with its header shortcut and count.
- D6's Source status is collapsed, with its non-ok count and all reasons; affected values retain state and age.
- Stale, superseded and revoked installation records are hidden by default in Connections, available through explicit filters with history intact; this never hides a current host warning. D11/D16 automatic overview sections move to detail. Unconfigured products create no empty panels.
- Production thresholds remain producer/policy-owned; no demo threshold becomes policy. Overview resources, trends and the fidelity check: [views detail](docs/contract/views.md#overview-resources).
- Refresh every 60 s with a brief local 1.3 s changed-value tint (none under reduced motion). Preserve stable rows, selection, focus, drafts, disclosures, scroll and return position; never reorder hosts because a value changed. Freshness is producer or probe time; a page fetch is not a heartbeat.
- TaskQ interiors come only from a published release pinned by version and hashes, never an unreleased source SHA; existing action gates stay separate. Pinned sets: [views detail](docs/contract/views.md#pinned-taskq-assets).
- Acceptance: A1–A5 of #169 and the [review package](docs/design/first-screen.md#review-and-acceptance). Prototype fixtures prove layout only; real producer and action qualification stays open. Target decisions: [views detail](docs/contract/views.md#redesign-target-decisions).
### D9. ServiceQ views
- The [preservation ledger](docs/design/first-screen.md#preservation-ledger) maps every current section and prior ServiceQ capability to its first-screen or detail destination. Nothing is deleted; already retired Node-monitor features remain retired. D7 report/JSON, D4 periods/cache/caps and all source evidence remain available independently of page placement.
### D22. Shareable views
- Applies [specq § F7](https://github.com/taskqorg/specq/blob/478f9223d5e57f49fd59198ca607273bc94a3fe5/design/implementation-fidelity.md#f7-stable-view-urls). `/` is the D8 overview. Guarded GET page paths: `/projects` (Tasks, every configured project and source coverage), `/hosts` (servers, host/resource detail), `/agents` (activity, work and optimization results), `/services` (versions), `/checks` (ServerQ), `/integrations` (connections and AgentQ profiles), `/telemetry` (per-sender delivery and coverage), `/reports`.
- Unknown paths return 404 with an overview recovery link. Unavailable views say so and never silently select another view. Existing controls retain their authority and components.
- Shareable state is the path and the D7 period/host/agent/session query (same defaults and validation); navigation preserves these filters. Secrets never enter URLs.
- Local drafts, disclosures, component selection, focus and reading positions stay local to the page session and survive tab changes, history navigation and automatic refresh. Reload or shared entry restores the view and encoded filters, not unsent local drafts.
- `/reports` reads only D21 `agent_work`, D23 `changes`/`query` and D24 `model_use`. Totals and display: [views detail](docs/contract/views.md#reports).
### D19. First launch and partial availability
[Startup and outages U1–U4](docs/design/startup-outages.md) compose D6, D10–D14 and enrollment without new authority. [Prototype coverage](docs/design/prototype-coverage.md) separates accepted examples from remaining design and live-qualification gaps.
- Unconfigured products create no empty panels; TaskQ-only has no empty resource, AgentQ or ServerQ panels. Unavailable sources preserve last-known values and drafts, with per-source action gating.
- A failed page refresh preserves the page and shared component instances, marks board observations unconfirmed and shows advancing ages. Recovery clears the banner without a write or incident resolution.
- Shared answer and policy adapters remain read-only until real action and recovery qualification ([#119](https://github.com/taskqorg/dashq/issues/119)). First launch and outage detail: [views detail](docs/contract/views.md#first-launch-and-outages).
## Product connections
### D11. Independent product connections
[Enrollment C1–C7](docs/design/enrollment.md), including E1–E10, owns the end-to-end connection contract; [connection interface L1–L4](docs/design/connections.md) owns its presentation. Implementation and release qualification remain pending.
- One process and one SQLite store (WAL/FULL) own grants, membership, revocations, receipts, projections, history and the bounded export outbox. No extra daemon or database server.
- C3 owns browser/device routes and authority, C4 durable acceptance and replay, C5 limits. Adjust CSP only for reviewed same-origin approval forms. Keep the loopback listener; never expose read APIs through machine-route exceptions.
- The enrollment registry supplies expected installations even without recent events. Optional Langfuse/GitHub sources cannot block enrollment or hide committed snapshots; their failure never turns `/data` into 503. Legacy sources keep their provenance and never create duplicate identities by hostname.
- D6/D7 apply the authenticated actor's scope server-side, including HTML, history, evidence, exports and local report. Local report is a protected host-administrator capability, never a remote identity bypass.
- Loopback requests and `dashq report` see all. Requests to the public name see their verified subject's workspaces (administrator: all installations there; member: those it approved); none without a subject.
- The browser subject comes only from `X-Dashq-Subject`, which the proxy strips from clients and copies from the guard's verified answer ([fixture/Caddyfile](fixture/Caddyfile)).
- Freshness uses producer observation time; connection and export health stay separate. No receipt means contact unknown; silence never proves the host stopped.
- One projection function serves `/data`, `dashq report` and a 2 s service pass. A receipt becomes `projected` only when that projection reads its snapshot as current and a view renders it. Approval, revoke, ingestion, forwarding, projection, lifecycle UI, telemetry and resources: [connections detail](docs/contract/connections.md).
- Data rows keep the workspace of the grant that sent them; after revoke a new approval may pick another workspace, which starts empty. Langfuse forwarding stays off by default.
### D25. Host link relay
DashQ is a blind relay: connected TaskQ installations exchange end-to-end encrypted messages through it. TaskQ owns keys, encryption and message schema. DashQ never decodes a body, holds no key, runs no host command and adds no authority.
- Routes: `POST /machine/link/v1/send` `{protocol 1, to, id, body}` and `POST /machine/link/v1/recv` `{protocol 1, wait?}` behind the existing `/machine/*` exemption, authenticated by the enrollment credential.
- Caller and receiver must be TaskQ installations with an acknowledged, unrevoked grant in the same workspace; else 401 `invalid_credential`/`revoked`, 403 `not_active` or `scope` (unknown, disconnected and foreign receivers answer the same). Per installation 5 requests/s, burst 20, apart from the C5 ingest budget.
- Memory only: nothing about a message reaches SQLite, logs, `/data` or `report`; only D20 counts its requests. A restart drops pending messages; the sender asks again. Limits: [connections detail](docs/contract/connections.md#host-link-relay).
## Gated actions
### D13. TaskQ answers, task controls and PM requests
#### TaskQ answer card
[Shared-card integration](docs/design/answers.md): TaskQ owns one component; DashQ pins its assets and supplies the scoped browser adapter through guarded `POST /answer`.
- Ordinary answers use TaskQ's canonical dispatcher, expected revision, operation ID and verified receipt. Consequential operations are excluded.
- The route stays off until a deployment configures `answer` ([answers § Route](docs/design/answers.md#route-41)). The browser takes its answer capability from `GET /answer` per board, so cards stay read-only wherever `answer` is not configured.
- Production writes wait for live qualification ([#44](https://github.com/taskqorg/dashq/issues/44)). FRA configures `answer` for the `taskqorg/dashq` board only, for that live board run; other boards stay read-only.
- Tasks owns one card per board/task; Attention navigates to it. Source rows preserve canonical question fields. The card uses the published D10 assets. Session progress and static snapshots: [views detail](docs/contract/views.md#task-rows).
- Worker line text never reaches DashQ views: `line` is removed from task rows and ingestion strips `session.line` before storing a snapshot. Snapshots stored with one keep it only until normal retention ages them out, never projected; nothing is deleted. The full log never leaves its host.
#### Task controls
[Adapter and qualification](docs/design/task-controls.md): guarded `GET|POST /task/control`, off unless `task_controls: {token: "<absolute credential path>", boards: {"<owner/repo>": "<workspace>"}}` and `taskq.binary` are configured. Each board must be a configured project. Production FRA configures it for the six configured boards.
- The host administrator grants `dashq controller WORKSPACE SUBJECT on|off`. Every request requires the verified subject to be that workspace's administrator with this separate grant.
- `/data` task rows add the canonical `controls` list; no offer is inferred. TaskQ v4.6.30 owns the unmodified renderer, fixed command table, revision/op-id fencing and receipts for start/stop/later/retry. Close remains a board link.
- Actor, a fresh complete row/revision and a qualified adapter gate every dispatch. Origin/CSRF, the durable operation journal and same-ID retry follow the answer transport. Unknown delivery never becomes accepted or a fresh operation.
#### PM requests
Guarded `GET|POST /pm/request` and authenticated TaskQ-only `POST /machine/pm/v1/exchange` use `taskq.pm/1` ([adapter](docs/design/task-workspace.md)).
- Config `pm_requests: true` is off by default; production FRA has it on. `dashq pm-requester WORKSPACE SUBJECT on|off` grants a workspace administrator separately.
- The actor is the SHA-256 of the verified subject. TaskQ must advertise its matching unexpired host PM grant and project revision. Origin/CSRF and per-actor rate limits gate submits.
- One bounded temporary request outbox in the existing SQLite store keeps exact input until a terminal receipt or expiry. Permanent digest tombstones prohibit changed-input or op-id reuse.
- No conversation, reply bridge, task admission or host-command authority. Receipts correlate actor, op-id and revision, name the native host/runtime/session when observed, and never contain request text.
- `/data` and `report` contain no PM text; board-created tasks return through normal board observations. Drafts and uncertain requests remain in the browser page session through refresh and navigation; only the same request may reconcile.
### D14. TaskQ resource controls
[Resource controls](docs/design/resource-controls.md) reuse TaskQ-owned node/runtime, capacity and Work mode components as a separately authorized typed host-policy capability; it does not extend `/answer` or observation credentials.
- TaskQ's host authority owns limits, Auto admission and effective readback. No remote shell or second scheduler.
- DashQ's operations follow TaskQ's pinned wire fixtures and stay off ([resource controls § Route](docs/design/resource-controls.md#route-47)). A deployment sets `resources` only after the target-host adapter ([taskq#931](https://github.com/taskqorg/taskq/issues/931)) ships and live qualification passes.
- Project limits ([§ Project limits](docs/design/resource-controls.md#project-limits-159)): DashQ reads the enrolled `taskq-resources/1` projection and offers the `taskq.project-limits/1` write under a `project-limits` grant, on the pinned TaskQ v4.6.25 release assets. Live qualification is #158.
### D15. ServerQ finding handoff
[Finding tasks H1–H5](docs/design/finding-tasks.md): an explicitly authorized TaskQ create/optional-start adapter and the shared TaskQ composer. Observation credentials and `/answer` gain no authority. Task creation and live qualification remain pending.
- TaskQ owns board mutation and readback; ServerQ owns incident verification. A closed task alone never removes a problem from Attention.
- Findings use workspace/installation/rule/first_seen identity, never an inferred resource join; Hosts H4/H7 grouping stays unchanged. Episodes and catalog: [views detail](docs/contract/views.md#serverq-findings).
### D16. AgentQ integration settings
[Client integration G1–G4](docs/design/agentq-settings.md): a separately authorized typed AgentQ configuration adapter. Observation credentials and `/answer` gain no authority. Release stays gated by real host and consumer qualification.
- AgentQ owns discovery, plans, revision checks, journaled apply, readback and native-client verification; DashQ supplies scoped presentation and delivery.
- Config `agentq_configure` (default off) enables guarded `/agentq/configure` ([browser adapter](docs/design/agentq-settings.md#browser-adapter-63)); it stays off until AgentQ A5 qualification.
- Approving a connection alone grants no capability; the grant keeps only the ticked subset. Queued operations take `authority` from the server only; payload claims never add authority.
- The Agent integration view is read-only: a missing, older or unsupported projection is `unknown` with its reason; a stale snapshot is never green; configured never reads as verified. Detail: [AgentQ operations](docs/contract/connections.md#agentq-operations).
## Agent and service views
### D21. Agent work
- Source: TaskQ attempt stage records (taskq.md § 4 Profiling). Primary: the `stage` observations inside stored `taskq-telemetry/1` blocks (D11) of each TaskQ installation in the caller's scope. Fallback: `taskq-stage` JSON lines of a configured local `taskq.log` (`taskq_logs`).
- Only valid stage, runtime, outcome, error and unknown codes count; other lines never leave the reader. A host with neither source has no row.
- Elapsed and execution times exist only for `complete` coverage, else null with a state and reason. Open markers never count as stage samples. TaskQ context `tokens` include cache and are never added to another count.
- Missing, old or partial evidence reads `unknown`, `stale` or `incomplete` with its reason; incomplete counts are a lower bound. A period without rows reads `No attempt stages` only when the block is `ok`. Never zero or ok. Budgets are unknown.
- Page: attempts first, only when configured (a log or a TaskQ telemetry grant). Overlapping, open, adopted, partial or untimed evidence never becomes a complete duration or an idle interval. Reading, fields, attempt rules and inspector: [views detail](docs/contract/views.md#agent-work).
### D24. Model use and estimates
Read-only: no billing source, credential or new authority. Actual expenses are a separate task (#185).
- Every estimate is labelled `API-equivalent estimate`, never spending; the `Model use` section (Agents view) states it. `dashq report` prints the same `model_use`.
- Config `prices` is a non-empty array of `{model, source (https URL), effective (YYYY-MM-DD), currency (ISO 4217), input, output, cache_read?, cache_write?}`, rates per million tokens, one entry per model and day. Invalid: `config: bad key prices.<i>.<key>`.
- A call takes the price entry of its model effective on or before its day. An estimate needs the model, all four token counts and a price for every non-zero count; else that call is unknown with its reason. Currencies stay separate subtotals; no FX.
- TaskQ stage tokens include cache, so their price field is never known. No actual-expense figure or TaskQ-token price is inferred. Shape, runtimes and Reports: [views detail](docs/contract/views.md#model-use).
### D17. AgentQ optimization results
[Optimization results P1–P3](docs/design/optimization-results.md) refine D8.4 with AgentQ-owned accounting definitions, honest coverage and a separate paired-experiment view.
- Read-only: no configuration authority, optimizer or experiment runner. Metadata/privacy and source-failure rules apply.
- Exported events carry no interval coverage: empty reads are unknown, observed counts are partial, and event-time buckets never invent missing samples. Whole-task effect is unknown without a separate qualified experiment source. Accounting source, filters and qualification: [views detail](docs/contract/views.md#optimization-results).
### D23. Waiting work, changes and route
Read-only, no new authority. One projection serves `/data`, the page and `dashq report`.
- Admission: ready or waiting task rows carry TaskQ's admission code from a pinned vocabulary; anything else reads `unknown` with the reason. DashQ never infers a stall or another reason; failed rows keep TaskQ's failure reason.
- Changes classify conditions as `new`, `ongoing` or `resolved` only from producer times. A source without history reads `no baseline`; the comparison never says "nothing changed".
- Route: each installation shows six delivery stages, each known, unknown or not applicable. Known false is not passed; unknown is never passed.
- `attention.requests` lists only producer requests needing a person. The Attention list and count render exactly these; other current problems, gaps and history stay Observations detail. Shapes, sources, the Tasks waiting list and paging: [views detail](docs/contract/views.md#waiting-work-changes-and-route).
### D18. Service versions and diagnostics
[Service versions V1–V4](docs/design/service-versions.md) refine D8.2/D9.
- Distinguish available, selected, running and verified health per installation. coreq and each consumer retain lifecycle ownership. Read-only diagnostics use allowlisted evidence. No remote update, restart or rollback authority.
- `update` is `available|current` only from a fresh verified comparison; unconfirmed observations never support one. A stale status or a service that is not `running` keeps versions `last_known`.
- Versions are `MAJOR.MINOR.PATCH[-pre]`; other forms read unknown. Projection and Services view: [views detail](docs/contract/views.md#service-versions).
## Linked detail
- [docs/contract/process.md](docs/contract/process.md): config keys, host requirements, snapshot command and adapter run, update and rollback, release, profiling.
- [docs/contract/connections.md](docs/contract/connections.md): enrolled ServerQ, pairing, ingestion and allowlist, forwarding, projection, lifecycle UI, TaskQ telemetry and resources, AgentQ operations, link relay limits.
- [docs/contract/views.md](docs/contract/views.md): `/data` document, Langfuse reads, host observations, resources, redesign target, task rows, findings, agent work, reports, waiting work, model use, optimization and service versions.
