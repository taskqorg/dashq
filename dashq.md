# dashq
dashq is one guarded operator page for the taskqorg tools: hosts and versions, TaskQ boards, agent activity and AgentQ events from Langfuse, and ServerQ checks. It reads and shows metadata; it is not an editor, scheduler, shell or second auth system, and it changes state only through the narrow authorities that R3 lists.

This file holds every decision and no history, at most 200 lines (declared cap below); changes follow specq [spec first](https://github.com/taskqorg/specq/blob/main/engineering/spec-first.md). Lower-priority detail lives in the files under [Linked detail](#linked-detail).
<!-- contract-budget: bytes=61440 lines=200 -->
## Rules
Mandatory for every change. Product sections refine these rules and never weaken them.
- **R1 Metadata only.** Never conversation text, prompts, tool input/output, raw commands, stderr/stdout or stack traces. `/data` holds aggregates and evidence trace IDs, never prompts, commands, tool results, tokens or keys.
- **R2 Unknown is never healthy.** Every value carries the state of its source. Missing data never becomes zero or healthy (D6).
- **R3 Read-only by default.** dashq never writes to Langfuse, GitHub, ServerQ or any host, and has no state-changing button, except these narrow authorities: D11 enrollment, ingestion and connection management; D13 TaskQ answers, task controls and PM requests; D14 resource controls; D15 finding tasks; D16 AgentQ configuration; D25 blind host link relay, transport only.
  - D11 replaces the no-write, no-push and no-database restrictions only for enrollment, ingestion and connection management.
- **R4 Grants stay separate.** Enrollment and observation grants carry no answer, control or host-command authority; AgentQ configuration needs a separately ticked D16 capability. Answer grants carry no control authority. D11 authorizes no host commands.
- **R5 No new auth system.** The page sits behind the existing Langfuse UI forward-auth guard (D3). Browser data, including `/data`, always remains guarded. D11 machine routes use their own narrow authorization instead of browser forward-auth; no wildcard exemption.
- **R6 Loopback only.** `dashq serve` binds `127.0.0.1:18882`; the port is not configurable to a non-loopback address. No public port.
- **R7 Secrets stay server side.** Secrets never reach the browser, `/data`, logs, URLs or `report` output.
- **R8 Pull by default.** dashq pulls the D4 sources. Hosts push only through the R3 machine routes; any other host push is a later task (#4).
- **R9 No other database.** Caches are in memory and a restart rebuilds them from the sources. The D11 SQLite store is the only database.
## D2. Process
- One Rust binary `dashq` on the shared `taskqorg/coreq` crate (release, update, launcher, service). `dashq serve` serves the page (R6).
- `dashq report [--period=S] [--host=H] [--agent=A] [--session=ID] [--cursor=N] [--block=ID]` prints the `/data` JSON once (D7) for agents on the host without a browser login. `dashq admin WORKSPACE SUBJECT` binds a workspace administrator for enrollment (D11).
- `dashq selftest` loads config, renders the fixture snapshot and reads each source once; no writes. Without any config file it skips the source reads and says so.
- `dashq service install|uninstall|run` and `dashq update [--check]` are the coreq commands; `service run` is `serve` and marks the release healthy after 60 s. `profile`, `doctor`, `--snapshot`, update, rollback and release: [process detail](docs/contract/process.md).
- Baseline routes: `GET /` (page), `GET /app.js`, `GET /style.css`, `GET /data`. D11, D13, D14, D16, D22 and D25 add their explicit routes. Any other path is 404; any other method is 405.
- Every response sends `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin` and CSP `default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'`. No third-party assets.
- Requests whose `Host` is neither the loopback address nor the configured public name get 421 (DNS rebinding).
- Config is a private JSON file named by env `DASHQ_CONFIG`, else `<install>/config.json` (coreq service units pass only `PATH`), mode 0600. Invalid config: the service refuses to start and names the bad key, never the value. Keys: [process detail](docs/contract/process.md#config).
## D3. Deployment
- Runs on the production host (FRA), where Langfuse and the ServerQ report live, under the `dash.` host name of the Langfuse UI domain.
- The reverse proxy site for that name runs the same `forward_auth` guard as the Langfuse UI first, then proxies to `127.0.0.1:18882`. No firewall change, no listener on a public address, no route under `/api/public`.
- Rollback: remove the `dash.` site block and stop the dashq service. Nothing else changes. dashq supersedes the AgentQ ServiceQ panel (`/serviceq`); its removal is AgentQ's task, not dashq's.
## D12. Security
- Follows the [specq security baseline](https://github.com/taskqorg/specq/blob/main/engineering/security.md); reports go to the route in `SECURITY.md`. The repository is public and the shared baseline applies. Workflow permissions: [process detail](docs/contract/process.md#release).
- Recorded exception: the C3 ingestion credential (D11) does not expire. It is revocable, per installation and observation-only; all other credential classes follow the shared baseline.
## D6. Unknown and stale
- Source states: `ok` (fresh read), `stale` (read succeeded but data is older than its limit), `unconfirmed` (this read failed; the previous value is shown with its time), `unknown` (no value yet, unsupported schema or not configured), `incomplete` (capped query). Each non-ok state has a one-line reason, for example `Langfuse: read failed (timeout), showing 14:02`.
- Freshness limits: Langfuse read 600 s; TaskQ heartbeat its `ttl` (300 s default); ServerQ report 7200 s; GitHub read 180 s.
- Display: green only for `ok` and a healthy value. `stale`, `unconfirmed` and `unknown` are gray or amber with the reason and age, never green. A missing number shows `—` with `unknown: <reason>`, never `0`. Capped counts show `≥N (capped)`.
- Counters that reset with a process restart say so; a reset is not shown as a drop. Historical incidents are deduplicated and never shown as current failures.
- A Source status summary lists every source that is not `ok`, with its reason; D10 sets its placement. `/data` sections carry `source: {state, at, age_s, reason, incomplete}`; unknown numbers are `null`, never `0`.
## D4. Sources
Each source has its own cache, single flight (one read in progress, callers share it) and a 10 s timeout per HTTP call.
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
## D7. /data JSON
- `GET /data?period=1800|7200|86400&host=H&agent=A&session=ID&block=ID&cursor=N` returns the document the page renders. Filters are optional; `block` (D11 telemetry) adds `telemetry_block` and filters nothing; `cursor` pages capped lists (D23).
- Invalid parameters: 400. All sources failed: 503 with `{"error"}`. Partial failure is 200 with source states. Session filters apply to model activity only; host health and resources keep host scope.
- Schema changes are additive within `schema` 2; a breaking change raises it. Top-level keys and the declared exceptions: [views detail](docs/contract/views.md#data-document).
## D5. Refresh
- The page fetches `/data` on load, every 60 s and on the Refresh button; no live push. The page shows `Updated <time>` and each source's age.
- A failed fetch keeps the last page and shows the error banner; it never blanks values. Rendering and target: [views detail](docs/contract/views.md#refresh).
## D8. Page
The [operator redesign](docs/design/operator-redesign/README.md) is the target for D8 and D10 over the host-first baseline: hosts and their state first, tasks through shared TaskQ UI, other information on demand. The page is English. R1–R9 and D2–D7 stay binding; D10 amends only the source-summary placement. Production qualification remains pending. Item detail: [views detail](docs/contract/views.md#page).
1. **Hosts**: one machine/kind per visual group; each value keeps its source and time. Silence means unknown, not offline. A heartbeat never proves client login or service health. No host state is inferred from activity or totals. Shared queues remain unassigned, never multiplied across hosts.
2. **Attention**: only a condition requiring a person and an identified action; each item names host/task, reason, age and action. Observation gaps, revoked connections and incident history are detail, not current failures; automatic retries and ordinary waits are not requests for a person.
3. **Tasks**: selecting a host's count opens its observed tasks in the TaskQ view; a direct Tasks entry includes all configured projects and unassigned work. A missing full-workspace or control release stays an explicit gap, not a DashQ editor.
4. **Agents**: `/agents` starts with TaskQ attempts and a selected inspector. Client activity, D21 stage totals, D17 optimization and D24 model use keep separate linked destinations; totals are never merged. Cost is null unless reported; login is a separate probe.
5. **Server checks**: host detail or Checks retains per-rule detail, incidents, linked work, numeric evidence and manual rules marked not verified. A closed task never establishes recovery.
## D10. Dashboard redesign target
The [operator redesign package](docs/design/operator-redesign/README.md) owns the next target and its preservation ledger. Existing runtime authority and qualification gates remain. The target enables no collection or action and grants no deployment, sign-in, host command, restart/update button or mutation authority.
- Expected hosts stay visible when observations are stale. Attention is exactly the D23 request list, immediately after the hosts. Unconfigured products create no empty panels. Layout, ownership and acceptance: [views detail](docs/contract/views.md#redesign-target).
- D6's Source status is collapsed, with its non-ok count and all reasons; affected values retain state and age. Hiding stale, superseded or revoked installation records never hides a current host warning.
- Production thresholds remain producer/policy-owned; no demo threshold becomes policy. Prototype fixtures prove layout only; real producer and action qualification stays open.
- Refresh every 60 s. Preserve stable rows, selection, focus, drafts, disclosures, scroll and return position; never reorder hosts because a value changed. Freshness is producer or probe time; a page fetch is not a heartbeat.
- TaskQ interiors come only from a published release pinned by version and hashes, never an unreleased source SHA; existing action gates stay separate. Pinned sets: [views detail](docs/contract/views.md#pinned-taskq-assets).
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
- A receipt becomes `projected` only when the shared projection reads its snapshot as current and a view renders it. Registry, approval, ingestion, projection, telemetry and resources: [connections detail](docs/contract/connections.md).
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
## D15. ServerQ finding handoff
[Finding tasks H1–H5](docs/design/finding-tasks.md): an explicitly authorized TaskQ create/optional-start adapter and the shared TaskQ composer. Observation credentials and `/answer` gain no authority. Task creation and live qualification remain pending.
- TaskQ owns board mutation and readback; ServerQ owns incident verification. A closed task alone never removes a problem from Attention. Identity, episodes and catalog: [views detail](docs/contract/views.md#serverq-findings).
## D16. AgentQ integration settings
[Client integration G1–G4](docs/design/agentq-settings.md): a separately authorized typed AgentQ configuration adapter. AgentQ owns discovery, plans, revision checks, journaled apply, readback and native-client verification; DashQ supplies scoped presentation and delivery.
- Config `agentq_configure` (default off) enables guarded `/agentq/configure`; it stays off until AgentQ A5 qualification. Observation credentials and `/answer` gain no authority.
- Approving a connection alone grants no capability; the grant keeps only the ticked subset. Queued operations take `authority` from the server only; payload claims never add authority. Detail: [AgentQ operations](docs/contract/connections.md#agentq-operations).
- The Agent integration view is read-only: a missing, older or unsupported projection is `unknown` with its reason; a stale snapshot is never green; configured never reads as verified.
## D21. Agent work
- Source: TaskQ attempt stage records from stored `taskq-telemetry/1` blocks (D11), else a configured local `taskq.log`. Only valid stage, runtime, outcome, error and unknown codes count; runtime and error codes are the pinned snapshot vocabulary's; other lines never leave the reader. Detail: [views detail](docs/contract/views.md#agent-work).
- Elapsed and execution times exist only for `complete` coverage, else null with a state and reason. Intervals may overlap; open, adopted, partial or untimed evidence never becomes a complete duration or an idle interval.
- A period without rows reads `No attempt stages` only when the block is `ok`, else unknown with the reason. Missing, old or partial evidence is never zero or ok; incomplete counts are a lower bound.
## D24. Model use and estimates
Read-only: no billing source, credential or new authority. Actual expenses are a separate task (#185). Price table and shape: [views detail](docs/contract/views.md#model-use).
- Every estimate is labelled `API-equivalent estimate`, never spending. An estimate needs the model, all four token counts and a price for every non-zero count; else that call is unknown with its reason. Currencies stay separate subtotals; no FX.
## D17. AgentQ optimization results
- [Optimization results P1–P3](docs/design/optimization-results.md): read-only AgentQ accounting with honest coverage; no configuration authority, optimizer or experiment runner. Empty reads are unknown and observed counts partial; whole-task effect is unknown without a qualified experiment source. Detail: [views detail](docs/contract/views.md#optimization-results).
## D23. Waiting work, changes and route
Read-only, no new authority. One projection serves `/data`, the page and `dashq report`. Detail: [views detail](docs/contract/views.md#waiting-work-changes-and-route).
- DashQ never infers a stall or another reason; an admission code outside TaskQ's pinned vocabulary reads `unknown`. Changes classify conditions only from producer times and never say "nothing changed"; an unknown route stage is never passed.
- `attention.requests` lists only producer requests needing a person. The Attention list and count render exactly these.
## D18. Service versions and diagnostics
- [Service versions V1–V4](docs/design/service-versions.md): distinguish available, selected, running and verified health per installation. No remote update, restart or rollback authority. `update` is `available|current` only from a fresh verified comparison. Detail: [views detail](docs/contract/views.md#service-versions).
## D20. Profiling
- Applies [built-in profiling](https://github.com/taskqorg/specq/blob/main/engineering/profiling.md). Counters are always on in `serve`, `service run`, `report` and `update`; records hold no secrets, content, URLs or paths. A failed write never fails the operation. Detail: [process detail](docs/contract/process.md#profiling).
## Linked detail
- [docs/contract/process.md](docs/contract/process.md): config keys, profile command, host requirements, snapshot command, update and rollback, release, profiling.
- [docs/contract/connections.md](docs/contract/connections.md): registry, pairing, ingestion and allowlist, forwarding, projection, lifecycle UI, enrolled ServerQ, TaskQ telemetry and resources, AgentQ operations, link relay.
- [docs/contract/views.md](docs/contract/views.md): `/data` document, source reads, refresh, page items, redesign target, shareable views, gated TaskQ actions, resource controls, findings, agent work, reports, waiting work, model use, optimization and service versions.
