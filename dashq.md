# dashq
dashq is one guarded operator page for the taskqorg tools: hosts and versions, TaskQ boards, agent activity and AgentQ events from Langfuse, and ServerQ checks. It reads and shows metadata; it is not an editor, scheduler, shell or second auth system, and it changes state only through the narrow authorities that R3 lists.

This file holds every decision and no history, at most 200 lines (declared cap below); changes follow specq [spec first](https://github.com/taskqorg/specq/blob/main/engineering/spec-first.md). Lower-priority detail lives in the files under [Linked detail](#linked-detail).
<!-- contract-budget: bytes=61440 lines=200 -->
## Rules
Mandatory for every change. Product sections refine these rules and never weaken them.
- **R1 Metadata only.** Never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces. `/data` holds aggregates and evidence trace IDs, never prompts, commands, tool results, tokens or keys.
- **R2 Unknown is never healthy.** Every value carries the state of its source. Missing data never becomes zero or healthy (D6).
- **R3 Read-only by default.** dashq never writes to Langfuse, GitHub, ServerQ or any host, and has no state-changing button, except these narrow authorities: D11 enrollment, ingestion and connection management; D13 TaskQ answers, task controls and PM requests; D14 resource controls; D15 finding tasks; D16 AgentQ configuration; D25 blind host link relay, transport only; D27 public limits push.
  - D11 replaces the no-write, no-push and no-database restrictions only for enrollment, ingestion and connection management.
- **R4 Grants stay separate.** Enrollment and observation grants carry no answer, control or host-command authority; AgentQ configuration needs a separately ticked D16 capability. Answer grants carry no control authority. D11 authorizes no host commands.
- **R5 No new auth system.** The page sits behind the existing Langfuse UI forward-auth guard (D3). Browser data, including `/data`, always remains guarded. D11 machine routes use their own narrow authorization instead of browser forward-auth; no wildcard exemption.
- **R6 Loopback only.** `dashq serve` binds `127.0.0.1:18882`; the port is not configurable to a non-loopback address. No public port.
- **R7 Secrets stay server side.** Secrets never reach the browser, `/data`, logs, URLs or `report` output.
- **R8 Pull by default.** dashq pulls the D4 sources. Hosts push only through the R3 machine routes; any other host push is a later task (#4).
- **R9 No other database.** Caches are in memory and a restart rebuilds them from the sources. The D11 SQLite store is the only database.
## D2. Process
- One Rust binary `dashq` on the shared `taskqorg/coreq` crate, pinned in `Cargo.toml` and updated by Dependabot (release, update, launcher, service). Installation prepares and activates the release before reloading the service; a refusal leaves the running unit alone. `dashq serve` serves the page (R6).
- `dashq report [--period=S] [--host=H] [--agent=A] [--session=ID] [--cursor=N] [--block=ID]` prints the `/data` JSON once (D7) for agents on the host without a browser login. `dashq admin WORKSPACE SUBJECT` binds a workspace administrator for enrollment (D11). `dashq limits [--check]` prints the D27 payload, or compares it with the live page. `dashq export` and `dashq parity` read the D28 store.
- `dashq selftest` loads config, renders the fixture snapshot and reads each source once; no writes. Without any config file it skips the source reads and says so.
- `dashq service install|uninstall|run` and `dashq update [--check]` are the coreq commands; `service run` is `serve` and marks the release healthy after 60 s. `profile`, `doctor`, `--snapshot`, update, rollback and release: [process detail](docs/contract/process.md).
- Baseline routes: `GET /` (page), `GET /app.js`, `GET /style.css`, `GET /data`. D11, D13, D14, D16, D22 and D25 add their explicit routes. Any other path is 404; any other method is 405.
- Every response sends `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and CSP `default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`. No third-party assets.
- Requests whose `Host` is neither the loopback address nor the configured public name get 421 (DNS rebinding).
- Config is a private JSON file named by env `DASHQ_CONFIG`, else `<install>/config.json` (coreq service units pass only `PATH`), mode 0600. Invalid config: the service refuses to start and names the bad key, never the value. Keys: [process detail](docs/contract/process.md#config).
## D3. Deployment
- Runs on the production host pds18.ufobe.com, where Langfuse and the ServerQ report live, under the `dash.` host name of the Langfuse UI domain.
- The reverse proxy site for that name runs the same `forward_auth` guard as the Langfuse UI first, then proxies to `127.0.0.1:18882`. No firewall change, no listener on a public address, no route under `/api/public`.
- Rollback: remove the `dash.` site block and stop the dashq service. Nothing else changes. dashq supersedes the AgentQ ServiceQ panel (`/serviceq`); its removal is AgentQ's task, not dashq's.
## D12. Security
- Follows the [specq security baseline](https://github.com/taskqorg/specq/blob/main/engineering/security.md); reports go to the route in `SECURITY.md`. The repository is public and the shared baseline applies. Workflow permissions: [process detail](docs/contract/process.md#release).
- Recorded exception: the C3 ingestion credential (D11) does not expire. It is revocable, per installation and observation-only; all other credential classes follow the shared baseline.
## D6. Unknown and stale
- Source states: `ok` (fresh read), `stale` (read succeeded but data is older than its limit), `unconfirmed` (this read failed; the previous value is shown with its time), `unknown` (no value yet, unsupported schema or not configured), `incomplete` (capped query). Each non-ok state has a one-line reason, for example `Langfuse: read failed (timeout), showing 14:02`.
- Freshness limits: Langfuse read 600 s; TaskQ heartbeat its `ttl` (300 s default); ServerQ report 7200 s; GitHub read 180 s.
- Display: green only for `ok` and a healthy value. `stale`, `unconfirmed` and `unknown` are gray or amber with the reason and age, never green. A missing number shows `—` with `unknown: <reason>`, never `0`. Capped counts show `≥N (capped)`.
- Time follows [specq Time](https://github.com/taskqorg/specq/blob/71fcb39ca9aafa868a57a6f6f5a13845cccec747/caveman/rules/indicators.md#time).
- Counters that reset with a process restart say so; a reset is not shown as a drop. Historical incidents are deduplicated and never shown as current failures.
- A Source status summary lists every source that is not `ok`, with its reason; D10 sets its placement. `/data` sections carry `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`, never `0`.
## D4. Sources
Each source has its own cache, single flight (one read in progress, callers share it) and a 10 s timeout per HTTP call.
A browser request never waits for a source read: Fleet, Langfuse, board, release and TaskQ snapshot reads refresh in the background. Pages and `/data` show the last data with its age and D6 state, or unknown until the first read completes.
### D4.1 Langfuse API
- Public API with the project keys, server side only. Model activity is every `GENERATION` observation (a Langfuse type, not an event name), whatever its producer. AgentQ event spans and attributes: [views detail](docs/contract/views.md#langfuse-reads).
- Periods: 30 min, 2 h, 24 h (1800, 7200, 86400 s; default 1800). Health and incident history always read 24 h.
- Cursor pagination, at most 10,000 rows per event name and period; hitting the cap marks that source `incomplete`. Cache 30 s per period: page loads inside 30 s share one read.
### D4.2 GitHub boards and TaskQ heartbeats
- Projects are an explicit list in config (`taskqorg/taskq`, `taskqorg/agentq`, `taskqorg/serverq`, `taskqorg/coreq`, `taskqorg/specq`, `taskqorg/dashq`). Repositories are never discovered.
- Boards: open issues labelled `taskq`, one page of 100 per repo; more than 100 marks the board `incomplete`. Heartbeat age = response `Date` minus issue `updated_at`, never the local clock. Labels, releases and caches: [views detail](docs/contract/views.md#github-reads).
- Snapshot adapter ([S3 R1](docs/design/adapters.md#s3-taskq-snapshots-and-questions)): with config `taskq.binary`, dashq runs `taskq status --json` per project in an isolated scratch run whose totals, questions and failure reasons replace the label counts. A failed, incomplete or unknown-schema run is a failed read with its reason. Detail: [views detail](docs/contract/views.md#snapshot-adapter).
- TaskQ vocabularies: states, labels and telemetry codes come from the snapshot's `vocabulary`, else the pinned snapshot fixture; DashQ keeps no TaskQ table.
### D4.3 ServerQ report
- The local `report.json` written by `serverq check` (path in optional `serverq` config) is read on every refresh. On failure the last-good report stays in memory with its original producer time, unconfirmed until a successful read.
- Absent config is unconfigured, distinct from a missing report. Another schema or invalid JSON is unknown with the reason. Stale after 7200 s. Schema, paths and enrolled reports: [views detail](docs/contract/views.md#serverq-report).
### D4.4 Resource samples
- Producer contract: agentq.md § 6 Resource samples (`agentq.resource`, one per 300 s bucket). Enrolled history (D11) is read first, Langfuse for the rest; each row names its `provenance`. Raw samples only, no aggregation.
- A missing, invalid or inconsistent value is `null` with the producer's or dashq's reason; nothing is derived from other fields or the legacy disk-headroom summary.
- Current = newest sample; stale after 900 s. A row that is not `ok` joins Attention gaps. Window, cap, gaps, resets and warmup: [views detail](docs/contract/views.md#resource-samples).
### D4.5 Agent runtimes
- Applies [specq agent runtimes](https://github.com/taskqorg/specq/blob/main/engineering/agent-runtimes.md). The runtime table `src/runtimes.json` holds the AgentQ runtime descriptors (name, label, brand, family, capabilities) at a pinned AgentQ revision, each runtime's TaskQ codes, login field, model accounting and D27 limits card, and the AgentQ clients with their producer names.
- Code never branches on a runtime or client name; a test fails on such a literal outside the table and tests.
- Session links stay opaque: a receipt keeps an https link only for a runtime whose descriptor has `session-url`; no host or path pattern is parsed. Host sign-in lists each runtime with a login field by label and brand. Detail: [views detail](docs/contract/views.md#agent-runtimes).
## D7. /data JSON
- `GET /data?period=1800|7200|86400&host=H&agent=A&session=ID&block=ID&cursor=N` returns the document the page renders. Filters are optional; `block` (D11 telemetry) adds `telemetry_block` and filters nothing; `cursor` pages capped lists (D23).
- Invalid parameters: 400. All sources failed: 503 with `{"error"}`. Partial failure is 200 with source states. Session filters apply to model activity only; host health and resources keep host scope.
- Schema changes are additive within `schema` 2; a breaking change raises it. Top-level keys and the declared exceptions: [views detail](docs/contract/views.md#data-document).
## D5. Refresh
- The page fetches `/data` on load, every 60 s and on the Refresh button; no live push. The page shows `Updated <time>` and each source's age.
- A failed fetch keeps the last page and shows the error banner; it never blanks values. Rendering and target: [views detail](docs/contract/views.md#refresh).
## D8. Page
The [operator redesign](docs/design/operator-redesign/README.md) is the target for D8 and D10 over the host-first baseline: hosts and their state first, tasks through shared TaskQ UI, other information on demand. The page is English. R1–R9 and D2–D7 stay binding; D10 amends only the source-summary placement. Production qualification remains pending. Item detail: [views detail](docs/contract/views.md#page).
1. **Hosts**: `/hosts` starts with one read-only server table (D30), with no agent inside DashQ; one machine/kind per visual group; each value keeps its source and time. Silence means unknown, not offline. A heartbeat never proves client login or service health. No host state is inferred from activity or totals. Shared queues remain unassigned, never multiplied across hosts.
2. **Attention**: only a condition requiring a person and an identified action; each item names host/task, reason, age and action. Observation gaps, revoked connections and incident history are detail, not current failures; automatic retries and ordinary waits are not requests for a person. One exception: an installation silent for over 1 h (D11) is a request to reconnect it.
3. **Tasks**: selecting a host's count opens its observed tasks in the TaskQ view; a direct Tasks entry includes all configured projects and unassigned work. A missing full-workspace or control release stays an explicit gap, not a DashQ editor.
4. **Agents**: `/agents` starts with TaskQ attempts and a selected inspector. Client activity, D21 stage totals, D17 optimization and D24 model use keep separate linked destinations; totals are never merged. Cost is null unless reported; login is a separate probe.
5. **Server checks**: host detail or Checks retains per-rule detail, incidents, linked work, numeric evidence and manual rules marked not verified. A closed task never establishes recovery.
## D10. Dashboard redesign target
The [operator redesign package](docs/design/operator-redesign/README.md) owns the next target and its preservation ledger. Existing runtime authority and qualification gates remain. The target enables no collection or action and grants no deployment, sign-in, host command, restart/update button or mutation authority.
- Expected hosts stay visible when observations are stale. Attention is exactly the D23 request list, immediately after the hosts. Unconfigured products create no empty panels. Layout, ownership and acceptance: [views detail](docs/contract/views.md#redesign-target).
- D6's Source status is collapsed, with its non-ok count and all reasons; affected values retain state and age. Hiding stale, superseded or revoked installation records never hides a current host warning.
- Production thresholds remain producer/policy-owned; no demo threshold becomes policy. Prototype fixtures prove layout only; real producer and action qualification stays open.
- Refresh every 60 s. Preserve stable rows, selection, focus, drafts, disclosures, scroll and return position; never reorder hosts because a value changed. Freshness is producer or probe time; a page fetch is not a heartbeat.
- TaskQ interiors come only from a published release pinned by version and hashes, never an unreleased source SHA; existing action gates stay separate. Every pin set names one release, and a scheduled check fails while the latest TaskQ release has a newer asset (taskq#1981). Pinned sets: [views detail](docs/contract/views.md#pinned-taskq-assets).
## D9. ServiceQ views
- The [preservation ledger](docs/design/first-screen.md#preservation-ledger) maps every current section and prior ServiceQ capability to its first-screen or detail destination. The host-first page deletes none of them; already retired Node-monitor features remain retired. D7 report/JSON, D4 periods/cache/caps and all source evidence remain available independently of page placement.
## D22. Shareable views
- Applies [specq § F7](https://github.com/taskqorg/specq/blob/478f9223d5e57f49fd59198ca607273bc94a3fe5/design/implementation-fidelity.md#f7-stable-view-urls). `/` is the D8 overview; the other guarded GET page paths are in [views detail](docs/contract/views.md#shareable-views). Unknown paths return 404 with an overview recovery link; unavailable views say so and never silently select another view.
- Shareable state is the path and the D7 period/host/agent/session query; secrets never enter URLs. Local drafts and selection stay local to the page session; reload restores the view and filters, not unsent drafts.
## D19. First launch and partial availability
[Startup and outages U1–U4](docs/design/startup-outages.md) compose D6, D10–D14 and enrollment without new authority. Detail: [views detail](docs/contract/views.md#first-launch-and-outages).
- Unconfigured products create no empty panels. Unavailable sources preserve last-known values and drafts, with per-source action gating. A failed page refresh marks board observations unconfirmed; recovery clears the banner without a write or incident resolution.
## D11. Independent product connections
[Enrollment C1–C7](docs/design/enrollment.md), including E1–E10, owns the end-to-end connection contract; [connection interface L1–L4](docs/design/connections.md) owns its presentation. Implementation and release qualification remain pending.
- One process and one SQLite store (WAL/FULL) own grants, membership, revocations, receipts, projections, history and the bounded export outbox. No extra daemon or database server.
- C3 owns browser/device routes and authority, C4 durable acceptance and replay, C5 limits. Adjust CSP only for reviewed same-origin approval forms. Keep the loopback listener; never expose read APIs through machine-route exceptions.
- Optional Langfuse/GitHub sources cannot block enrollment or hide committed snapshots; their failure never turns `/data` into 503. Legacy sources keep their provenance and never create duplicate identities by hostname.
- D6/D7 apply the authenticated actor's scope server-side, including HTML, history, evidence, exports and local report. Loopback requests and `dashq report` see all; requests to the public name see their verified subject's workspaces; none without a subject.
- `dashq report` reads the store only with the 0600 config and key, so local report stays a protected host-administrator capability, never a remote identity bypass.
- The browser subject comes only from `X-Dashq-Subject`, which the proxy strips from clients and copies from the guard's verified answer ([fixture/Caddyfile](fixture/Caddyfile)).
- Freshness uses producer observation time; connection and export health stay separate. No receipt means contact unknown; silence never proves the host stopped. Langfuse forwarding stays off by default.
- Silence: an active or pending installation with no report (newest durable receipt or trusted producer time; none: since its grant) for over 3600 s reads `silent`, an Attention request to reconnect it. Over 30 days it reads `retired` and leaves hosts, checks, versions and Attention; its row, grant and history stay. Revoked is never silent.
- A receipt keeps only a SHA-256 digest of the record it acknowledged, for the 8-day receipt window; the hourly prune vacuums the file when most of it is free.
- A receipt becomes `projected` only when the shared projection reads its snapshot as current and a view renders it. Registry, approval, ingestion, projection, telemetry and resources: [connections detail](docs/contract/connections.md).
## D28. Global telemetry store
Applies the [specq telemetry contract](https://github.com/taskqorg/specq/blob/d4f6ce09971242fbaa9a78f09c213323fddc73da/engineering/profiling.md#telemetry-contract): D11 is the only global store and the only reader of CoreQ observations. No new authority. Detail: [connections detail](docs/contract/connections.md#global-observation-store).
- Ingestion: `/machine/ingest` accepts `coreq-observation/1`, the CoreQ shipper's wire shape, for a grant naming that schema, under the same enrollment credential, per-device budget and metadata allowlist (R4, R5).
- Inventory: a record of kind `inventory` holds one typed `inventory` object: CoreQ's entity, identity, measures and per-field reasons, nothing else. Only `mount`, `project_root` and `folder` identities may be paths. `dashq export` carries it; the scoped D30 server projection in `/data` and `dashq report` may carry these typed identities and measures (R1, D11).
  - The rest of the record and every other kind stay under the metadata allowlist: no file contents, environment values, credentials, raw output, or paths outside that typed field.
- Retention: observations have no fixed retention. No prune removes them, the receipt window does not expire them and the C5 ceiling does not count them. `observations.store` reports the measured volume; pruning starts only when that measurement requires it.
- Readers: `/data.observations`, the Telemetry view, `dashq report`, `dashq export` and `dashq parity` read this table at read time; D21 and D26 read it next to the old TaskQ blocks. No derived copy is a source of truth; a volume counter only measures.
- Cost: the D24 price table computes the `API-equivalent estimate` at read time. A record holding a price or cost field is rejected `payload`. `agent.turn` and `agent.call` totals describe the same tokens: they stay separate and are never summed.
- Errors: one query over outcome and error class, `observations.errors`, lists failures, unstable operations and attempts without a terminal record. `refused` is counted apart and never listed.
- Export: `dashq export [--window=S] [--to=UNIX] [--format=jsonl|csv] [--product=P]` writes one schema version per file, with a header stating schema, version and window.
- Parity: `dashq parity [--window=S] [--to=UNIX]` compares the old and new paths per product for one window. Langfuse stays until that report reads `parity`; missing or partial evidence is `unknown`, never parity (R2).
## D25. Host link relay
DashQ is a blind relay: connected TaskQ installations exchange end-to-end encrypted messages through it. TaskQ owns keys, encryption and message schema. DashQ never decodes a body, holds no key, runs no host command and adds no authority.
- Only TaskQ installations with an acknowledged, unrevoked grant in the same workspace exchange messages, through the `/machine/link/v1/` routes authenticated by the enrollment credential. Routes, errors and limits: [connections detail](docs/contract/connections.md#host-link-relay).
- Memory only: nothing about a message reaches SQLite, logs, `/data` or `report`; only D20 counts its requests. A restart drops pending messages; the sender asks again.
## D13. TaskQ answers, task controls and PM requests
TaskQ owns the components, renderer, commands and receipts; DashQ pins released assets and supplies scoped browser adapters. Each route stays off until configured; detail: [views detail](docs/contract/views.md#gated-taskq-actions).
- **Answer card** ([shared-card integration](docs/design/answers.md)): guarded `POST /answer` uses TaskQ's canonical dispatcher, expected revision, operation ID and verified receipt; a choice TaskQ marks `consequential` (close, publication, approval), or one without that flag, is refused. Cards stay read-only wherever `answer` is not configured.
- Worker line text never reaches DashQ views: ingestion strips `session.line` before storing a snapshot, and older stored lines are never projected. The full log never leaves its host.
- **Task controls** ([adapter](docs/design/task-controls.md)): guarded `GET|POST /task/control`. Every request requires the verified subject to be the workspace's administrator with the separate `dashq controller` grant. `/data` task rows add the canonical `controls` list; no offer is inferred.
- Actor, a fresh complete row/revision and a qualified adapter gate every dispatch. Unknown delivery never becomes accepted or a fresh operation.
- **PM requests** ([adapter](docs/design/task-workspace.md)): guarded `GET|POST /pm/request` and TaskQ-only `POST /machine/pm/v1/exchange` use `taskq.pm/1` under a separate `dashq pm-requester` grant. No conversation, reply bridge, task admission or host-command authority.
- Receipts and `/data`/`report` never contain request text; board-created tasks return through normal board observations.
## D14. TaskQ resource controls
[Resource controls](docs/design/resource-controls.md) reuse TaskQ-owned node/runtime, capacity and Work mode components as a separately authorized typed host-policy capability; it does not extend `/answer` or observation credentials.
- TaskQ's host authority owns limits, Auto admission and effective readback. No remote shell or second scheduler. A deployment sets `resources` only after the target-host adapter ships and live qualification passes; route and project limits: [views detail](docs/contract/views.md#resource-controls).
- Resource operations accept `taskq.admission/1` and `taskq.project-limits/1`; retired `taskq.capacity/1` is unsupported.
## D15. ServerQ finding handoff
[Finding tasks H1–H5](docs/design/finding-tasks.md): an explicitly authorized TaskQ create/optional-start adapter and the shared TaskQ composer. Observation credentials and `/answer` gain no authority. Task creation and live qualification remain pending.
- TaskQ owns board mutation and readback; ServerQ owns incident verification. A closed task alone never removes a problem from Attention. Identity, episodes and catalog: [views detail](docs/contract/views.md#serverq-findings).
## D16. AgentQ integration settings
[Client integration G1–G4](docs/design/agentq-settings.md): a separately authorized typed AgentQ configuration adapter. AgentQ owns discovery, plans, revision checks, journaled apply, readback and native-client verification; DashQ supplies scoped presentation and delivery.
- Config `agentq_configure` (default off) enables guarded `/agentq/configure`; it stays off until AgentQ A5 qualification. Observation credentials and `/answer` gain no authority.
- Approving a connection alone grants no capability; the grant keeps only the ticked subset. Queued operations take `authority` from the server only; payload claims never add authority. Detail: [AgentQ operations](docs/contract/connections.md#agentq-operations).
- The Agent integration view is read-only: a missing, older or unsupported projection is `unknown` with its reason; a stale snapshot is never green; configured never reads as verified.
## D21. Agent work
- Source: TaskQ attempt stage records from D28 observations and stored `taskq-telemetry/1` blocks (D11), else a configured local `taskq.log`. Only valid stage, runtime, outcome, error and unknown codes count; runtime and error codes are the pinned snapshot vocabulary's; other lines never leave the reader. Detail: [views detail](docs/contract/views.md#agent-work).
- Elapsed and execution times exist only for `complete` coverage, else null with a state and reason. Intervals may overlap; open, adopted, partial or untimed evidence never becomes a complete duration or an idle interval.
- A period without rows reads `No attempt stages` only when the block is `ok`, else unknown with the reason. Missing, old or partial evidence is never zero or ok; incomplete counts are a lower bound.
## D24. Model use and estimates
Read-only: no billing source, credential or new authority. Actual expenses are a separate task (#185). Price table and shape: [views detail](docs/contract/views.md#model-use).
- Every estimate is labelled `API-equivalent estimate`, never spending. An estimate needs the model, all four token counts and a price for every non-zero count; else that call is unknown with its reason. Currencies stay separate subtotals; no FX.
## D17. AgentQ optimization results
- [Optimization results P1–P3](docs/design/optimization-results.md): read-only AgentQ accounting with honest coverage; no configuration authority, optimizer or experiment runner. Empty reads are unknown and observed counts partial; whole-task effect is unknown without a qualified experiment source. Detail: [views detail](docs/contract/views.md#optimization-results).
- Compression accounting reads `agentq.compress` only; `agentq.run` remains RTK usage evidence, never compression evidence. The schema-2 `coverage.legacy_run` field stays zero. Host identity uses producer machine/kind; no unused legacy host ID is derived from an AgentQ installation.
## D23. Waiting work, changes and route
Read-only, no new authority. One projection serves `/data`, the page and `dashq report`. Detail: [views detail](docs/contract/views.md#waiting-work-changes-and-route).
- DashQ never infers a stall or another reason; an admission code outside TaskQ's pinned vocabulary reads `unknown`. Changes classify conditions only from producer times and never say "nothing changed"; an unknown route stage is never passed.
- `attention.requests` lists only producer requests needing a person and D11 silent installations. The Attention list and count render exactly these.
## D18. Service versions and diagnostics
- [Service versions V1–V4](docs/design/service-versions.md): distinguish available, selected, running and verified health per installation. No remote update, restart or rollback authority. `update` is `available|current` only from a fresh verified comparison. Detail: [views detail](docs/contract/views.md#service-versions).
## D26. Fleet
- `/fleet` is one read-only page for every observed or expected host: token counts by project, task attempt and account over fixed 24 h and 7 d windows from CoreQ universal `agent.turn` observations in the D28 store and in D11 TaskQ telemetry. Counts are metadata, never credentials (R1/R7); stage token summaries are not added to turn counts.
- Each account shows its newest reported limit used percent, reset time and window, or null with the producer reason. Host cadence is observed task completions per hour over each window; workers started counts successful worker spawns, separately. Deduplicate stable observation identities; missing history, loss, capped reads and missing fields retain explicit coverage and reasons (R2/D6).
- Host Claude and Codex CLI versions come from TaskQ heartbeat `cli` lines. Compare with fresh upstream latest stable releases (`anthropics/claude-code`, `openai/codex`); missing, unsupported or stale versions/comparisons are unknown. These explicit release sources do not discover boards (D4.2). No update, restart or health authority is added (D18).
## D27. Public limits push
Config `limits_push` (default off) makes DashQ the data source of the public limits page, a Cloudflare Worker DashQ does not own: DashQ POSTs a snapshot to the Worker's existing `/push`. Outbound only: no public DashQ route (R5, R6), and the token stays server side (R7). Detail: [process detail](docs/contract/process.md#public-limits-push).
- Source: TaskQ `limit` heartbeat rows on the configured boards, for every runtime with a table `limits` entry (D4.5). Rows of one runtime and account `id` merge across hosts, newest per window; a row without an `id` merges with nothing.
- The payload keeps the Worker's shape and holds only short labels, percentages and resets: never an email, masked email, plan or org name, account id, host name, token or unknown reason.
- `lastFetch` is the producer's time, so the page marks an old account stale as it always did (R2). A failing account reads `unknown`. A snapshot with no window is not pushed: the Worker keeps its last payload, which ages on the page.
- Cutover: `dashq limits --check` compares the snapshot with the live page without writing. Only a passing check allows adding `limits_push.token` and stopping the previous publisher; two publishers overwrite each other.
## D29. AssetQ summary
[AssetQ summary M1–M5](docs/design/assetq-summary.md): one guarded, read-only `/assets` view of AssetQ jobs, methods, comparisons, recommendations, storage and retained records, with links to the AssetQ site. AssetQ owns all state; DashQ keeps only its in-memory read cache and adds no R3 authority, grant or write route. Detail: [views detail](docs/contract/views.md#assetq).
- **Read.** With config `assetq`, dashq reads AssetQ's versioned API per configured project, GET only and in the background (D4), with a server-side Bearer read credential and a Cloudflare Access service token on every read (R7, R8).
- **Scope.** `/data.assets` and `dashq report` carry the same projection, scoped by workspace (D11). A project whose credential names another project, or an unsupported shape, reads unknown with the reason.
- **Metadata only (R1).** Job request text, dataset cases, attachment contents, method specifications and free-form evidence never reach `/data`, `report` or logs. No preview, download proxy, provider URL or storage path.
- **Unknown (R2/D6).** Freshness limit 180 s. Capped reads are `incomplete`, missing usage and judgments are null with reasons, deferred backup and restore stay unknown, provider health stays separate from qualification and recommendations. DashQ names no winner.
- **Actions stay on the AssetQ site.** Submitting and cancelling jobs, editing methods, promotion, asset browsing, previews, uploads, benchmarks, datasets, evaluators and acceptance happen on the AssetQ site that `assetq.base_url` names; DashQ links there, and the site authenticates the viewer. Host remediation stays with ServerQ. AssetQ rows join Attention only through D23's rule.
## D30. Server table
- `/hosts` shows every observed or expected server, one machine/kind per workspace, with status, producer age, CPU, RAM, mounts and largest observed disk consumer. A row opens projects, services/containers and largest folders; missing fields are unknown, stale values keep their age and state (R2/D6). Overview and existing detail destinations remain available.
- Discovered sites, processes, images, security findings and typed links need no project manifest; D28 export preserves their metadata and source.
- D28 inventory is the source, read in the background from unrevoked, non-retired D11 installations in the actor's scope; legacy hosts stay separate unless an installation identity joins them. No collection, host command, mutation or AI is added (R1–R9).
- Deterministic trends use the last 7 days: at least 3 distinct producer times spanning 24 h, with no gap over 7200 s, missing/partial sample or capacity change.
- CPU and RAM show observed change per day; disk exhaustion is the latest free space divided by positive fitted usage growth, added to producer time. Short, stale, inconsistent or non-growing history gives no date and states why; estimates are not policy or guarantees.
- Each row has a PilotQ handoff selector and, with `pilotq_url` configured, a discussion link. The selector carries only workspace and machine/kind, never paths or authority. The link contract is in [views detail](docs/contract/views.md#pilotq-link-contract); live chat selection is qualified by PilotQ, separately from this table.
- Telegram deep link: `pilotq_url` `https://t.me/BOT` gives each row `https://t.me/BOT?start=server-DIGEST`. DIGEST is FNV-1a 64-bit of the exact `pilotq_selector`, in 16 lowercase hexadecimal digits. Only this link drops the `chat selection pending` label; any other target keeps it, and no target reads `PilotQ unavailable: not configured`.
## D20. Profiling
- Applies [built-in profiling](https://github.com/taskqorg/specq/blob/main/engineering/profiling.md). Counters are always on in `serve`, `service run`, `report` and `update`; records hold no secrets, content, URLs or paths. A failed write never fails the operation. Detail: [process detail](docs/contract/process.md#profiling).
## Linked detail
- [docs/contract/process.md](docs/contract/process.md): config keys, profile command, host requirements, snapshot command, update and rollback, release, profiling.
- [docs/contract/connections.md](docs/contract/connections.md): registry, pairing, ingestion and allowlist, forwarding, projection, lifecycle UI, enrolled ServerQ, TaskQ telemetry and resources, AgentQ operations, link relay.
- [docs/contract/views.md](docs/contract/views.md): `/data` document, source reads, refresh, page items, redesign target, shareable views, gated TaskQ actions, resource controls, findings, agent work, reports, waiting work, model use, optimization, service versions and AssetQ.
