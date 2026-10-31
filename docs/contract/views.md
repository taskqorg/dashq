# Views detail

Linked detail of [dashq.md](../../dashq.md): the `/data` document, the source reads of D4 and the view rules of D5, D8–D10, D13–D15, D17–D19 and D21–D24. The contract rules R1–R9 apply; this page adds lower-priority detail and never weakens them.

## Data document

- Top level: `schema` (2), `generated_at`, `period`, `filters`, `sources`, `hosts`, `attention`, `boards`, `resources`, `agents`, `checks`, `versions` (D18), `optimization` (D17: AgentQ accounting, event-time buckets and source freshness), `agent_work` (D21), `model_use` (D24).
- `resources` (D4.4): `window_s`, `from`, `to`, `bucket_s`, `aggregation`, `cap`, `rows` of `{installation, host, runtime, workspace, provenance, source, current, history}`.
- D11 adds `installations`, `sources.enrollment`, `sources.export` and `telemetry_block`; D23 adds `changes` and `query`; their sections state when.
- Declared exceptions within `schema` 2, not additive changes:
  - removal of `boards[].tasks[].session.line` and of the `line` of `taskq-board/1` snapshot payloads (D13);
  - `agent_work.rows[].time_ms` means the sum of complete attempts' elapsed time on the row; it had summed timed stages, counting nested stages twice.
- Agents read `/data` with `dashq report` on the host, or through the guard with the owner's session.

## Langfuse reads

- Agents sources are the AgentQ observations (agentq.md § 6), one span per event, span name = event name: `agentq.compress` (core optimization accounting, D17), `agentq.run` (RTK runs), `agentq.retrieval` (MCP tool calls), `agentq.installation` (hourly heartbeat), `agentq.incident` and `agentq.model` (one Claude model call, D24).
- Each event carries `agentq.installation` (fleet key: one installation, one row), `agentq.version`, `agentq.runtime`, `host.name` and `agentq.stage`. The Agents view takes fleet, versions, last seen and health from them, usage from `agentq.run`/`agentq.retrieval` and incidents from `agentq.incident`.
- Last seen is the newest event time per installation. An installation without an `agentq.installation` heartbeat in 7200 s is stale.
- Client use (D8.4) also reads `agentq.compress` and `agentq.retrieval` over 24 h; the 24 h period reuses these reads.
- The `agentq.installation` read takes the `io` field group, since its `agentq.routing` and `agentq.hooks` JSON strings exceed Langfuse's 200-character metadata cut.

## GitHub reads

D4.2 detail.

- Boards: state from the projected `q-<state>` label, priority from `p0|p1|p2`. dashq does not fold comments: a label can lag the log by one service pass.
- Heartbeats: open issues labelled `taskq-host` (`taskq host <name>`) with `version <v>` and `#<N> <attempt>` lines. Age = response `Date` minus issue `updated_at`, never the local clock.
- Latest releases: `releases/latest` per project repo, cached 10 min. Other reads cache 60 s per repo.
- A `403`/`429` rate limit keeps the last value as unconfirmed until reset.

## Snapshot adapter

D4.2 detail ([S3 R1](../design/adapters.md#s3-taskq-snapshots-and-questions)).

- With config `taskq.binary` (absolute path of the pinned taskq), dashq runs `taskq status --json` per project in an isolated scratch run ([process detail](process.md#snapshot-adapter-run)); single flight, 10 s timeout, cache 60 s. Absent key: labels only.
- Snapshot totals, questions (S3 shape, read-only) and failure reasons replace the label counts; heartbeats stay label reads.
- Counters are one per state of the snapshot's `vocabulary.states`, Held included; a snapshot without it, and the label adapter, use the [pinned fixture](#pinned-taskq-assets)'s ids and labels.
- A failed, incomplete or unknown-schema run is a failed read with its reason; `sources.taskq` lists it.

## ServerQ report

D4.3 detail.

- Absolute config paths and their acceptance fixtures use the target platform's path rules.
- Accepted: `schema` 1 with `at` (epoch seconds), `results` (`ok|warn|unknown` per check with `detail`), `incidents`, `manual_rules`. Stale after 7200 s (hourly timer plus margin).
- Manual rules are listed as not verified, never as passed.
- Enrolled `serverq-report/1` snapshots follow the same rules per installation and workspace: [enrolled ServerQ reports](connections.md#enrolled-serverq-reports).

## Resource samples

D4.4 detail.

- Langfuse `agentq.resource` observations (with the `io` field group, else Langfuse cuts `agentq.resource.disks`) serve installations without enrolled samples.
- Window = the period; at most 2016 samples per installation (7 days of buckets), newest kept; a cut marks the row `incomplete`. Langfuse reads keep D4.1 pagination and cap.
- History shows `gaps` (over two buckets without a sample, or a stale tail), `resets` (`warmup` or `counter reset` CPU baselines) and `warmup` below three samples.
- Per sample (D4.4): CPU percent; RAM used, available and total bytes; per filesystem `id`, role, fstype, used, free and total bytes.

## Refresh

D5 detail.

- The server renders the page shell; D13 boards use the pinned TaskQ browser renderer.
- `GET /` renders the sections from the same document as `/data`; after a good `/data` fetch the page reloads its sections from `/` with the same query.
- Target: [shared dashboard rules W4–W6](https://github.com/taskqorg/specq/blob/main/design/dashboards.md) and D10. This mechanism is the baseline, not proof of stable in-place updates.

## Page

D8 item detail.

- Hosts: the [minimal Overview](../design/operator-redesign/overview-candidate.html) shows one machine/kind per visual group: identity, then scoped state and age, then resource facts with right-hand trends, then direct detail/action. Missing resources form one readable unavailable state; partial metrics retain individual reasons.
- Versions/update, login probes, queue counts and TaskQ receipts stay in expandable host detail and their D22 views; each value keeps its source and time.
- Attention mappings: expired login → that host's sign-in instructions; failed unit → its diagnostics; verified low disk → that filesystem's finding; task question → the single shared answer card.
- Attention detail items keep their non-ok meaning beside the affected host values. A source outage requiring intervention may appear with its recovery action.
- Tasks reuse the tray's shared rows, counters, answers and released controls under D13/D14. Retain project/board identity, state age, runtime, issue/session links, questions, failures, ready, blocked, later and Closed coverage.
- Agents: identity, host/runtime, coverage state/reason and last stage observation age precede detail. Selection, focus and return survive refresh, Back and narrow reflow.
- Per-model field sample counts stay separate. [Client observation rules](../design/optimization-results.md#client-observations-and-coverage) apply.

## Host observations

D8.1 detail.

- Runtime `hosts[].taskq.observed_running` is nullable fresh attempt evidence. `boards[].attempts` adds canonical `host_id` and `current`, not pending assignment.
- TaskQ v4.6.30 `taskq-board/1` observations supply online, per-runtime login and assigned queue.
- `hosts[].taskq.observations` preserves board/workspace/installation, producer, host, `observed`, `valid_until`, completeness and reason.
- The latest observation per board/workspace/host/kind wins; equal-time conflicts are unknown.
- Missing, expired, future, invalid or partial evidence reads unknown (counts null), retaining TaskQ's reason.
- Heartbeat machine identity ([H2](../design/hosts.md)) joins labels; absent identity keeps its own row.

## Redesign target

D10 detail.

- Ownership: the [host-first package](../design/first-screen.md) owns composition; [overview](../design/overview.md) owns resources, [boards](../design/boards.md) TaskQ adoption, [connections](../design/connections.md) lifecycle presentation. Apply [W1–W15](https://github.com/taskqorg/specq/blob/fea6e540efa6971c5739453b198183cbb3fb010d/design/dashboards.md).
- Overview: approved compact host cards across the main width, one column at narrow widths without smaller type; no side-by-side project boards or permanent detail panels.
- Attention keeps its header shortcut and count.
- Stale, superseded and revoked installation records are hidden by default in Connections, available through explicit filters with history intact. D11/D16 automatic overview sections move to detail.
- Refresh shows a brief local 1.3 s changed-value tint (none under reduced motion).
- Acceptance: A1–A5 of #169 and the [review package](../design/first-screen.md#review-and-acceptance).

## Overview resources

D10 detail.

- Overview resources are current CPU percent, RAM and per-filesystem used/total bars when capacity is known, beside small right-hand trends labelled last 30 minutes, 0–100%.
- Only D4.4 samples within that window draw history; three valid samples per metric are required.
- Missing values, gaps and CPU resets break lines; warmup/unavailable, stale and incomplete coverage remain explicit. Detailed history stays in host detail.
- The browser fidelity check pins approval.json's public derivative hash and compares its host geometry and hierarchy at 1280/16, 360/20 and large text in both themes. Fixture evidence never qualifies production.

## Pinned TaskQ assets

- Project/answer blocks and task controls: TaskQ v4.6.30, commit 09cb0d7db35d0de51bef83d9fa3ec803eafad42d, [shared blocks](https://github.com/taskqorg/taskq/blob/v4.6.30/docs/design/shared-blocks.md), `assets/taskq/provenance.json`.
- Resources: TaskQ v4.6.25, `assets/taskq/sources.json`.
- Status snapshot fixture and schema (states, choices, telemetry codes): `assets/taskq/snapshot-provenance.json`, SHA-256 checked by `selftest`; a field DashQ reads failing in a repinned fixture fails its contract test.
- Full workspace, detail, menus and PM composer: TaskQ v4.6.34, `assets/taskq/workspace-provenance.json`; unchanged released components through the [adapter](../design/task-workspace.md).

## Redesign target decisions

- Actual expenses and model-use estimates stay separate.
- Telemetry is events and metrics only; worker-line content retires compatibly, with no retrospective deletion.
- PM requests go through DashQ with native-session continuation through TaskQ-owned shared UI.
- Versions/update, client sign-in and task counts remain in expandable detail.

## Shareable views

D22 detail.

- Guarded GET page paths: `/projects` (Tasks, every configured project and source coverage), `/hosts` (servers, host/resource detail), `/agents` (activity, work and optimization results), `/services` (versions), `/checks` (ServerQ), `/integrations` (connections and AgentQ profiles), `/telemetry` (per-sender delivery and coverage), `/reports`.
- Existing controls retain their authority and components.
- The shareable query keeps the D7 defaults and validation; navigation preserves these filters.
- Local drafts, disclosures, component selection, focus and reading positions stay local to the page session and survive tab changes, history navigation and automatic refresh. Reload or shared entry restores the view and encoded filters, not unsent local drafts.
- `/reports` reads only D21 `agent_work`, D23 `changes`/`query` and D24 `model_use` ([Reports](#reports)).

## Gated TaskQ actions

D13 detail.

### Answer card

- TaskQ owns one component; DashQ pins its assets and supplies the scoped browser adapter through guarded `POST /answer`.
- The route stays off until a deployment configures `answer` ([answers § Route](../design/answers.md#route-41)). The browser takes its answer capability from `GET /answer` per board.
- Production writes wait for live qualification ([#44](https://github.com/taskqorg/dashq/issues/44)). FRA configures `answer` for the `taskqorg/dashq` board only, for that live board run; other boards stay read-only.
- Tasks owns one card per board/task; Attention navigates to it. Source rows preserve canonical question fields. The card uses the published D10 assets.
- `line` is removed from task rows. Snapshots stored with one keep it only until normal retention ages them out, never projected; nothing is deleted.

### Task controls

- [Adapter and qualification](../design/task-controls.md): `/task/control` is off unless `task_controls: {token: "<absolute credential path>", boards: {"<owner/repo>": "<workspace>"}}` and `taskq.binary` are configured. Each board must be a configured project. Production FRA configures it for the six configured boards.
- The host administrator grants `dashq controller WORKSPACE SUBJECT on|off`.
- TaskQ v4.6.30 owns the unmodified renderer, fixed command table, revision/op-id fencing and receipts for start/stop/later/retry. Close remains a board link.
- Origin/CSRF, the durable operation journal and same-ID retry follow the answer transport.

### PM requests

- `POST /machine/pm/v1/exchange` is authenticated. Config `pm_requests: true` is off by default; production FRA has it on. `dashq pm-requester WORKSPACE SUBJECT on|off` grants a workspace administrator separately.
- The actor is the SHA-256 of the verified subject. TaskQ must advertise its matching unexpired host PM grant and project revision. Origin/CSRF and per-actor rate limits gate submits.
- One bounded temporary request outbox in the existing SQLite store keeps exact input until a terminal receipt or expiry. Permanent digest tombstones prohibit changed-input or op-id reuse.
- Receipts correlate actor, op-id and revision and name the native host/runtime/session when observed.
- Drafts and uncertain requests remain in the browser page session through refresh and navigation; only the same request may reconcile.

## Task rows

D13 detail.

- Session progress (taskq R6 3): a doing or review task row adds `session` `{host, runtime, stage, age_s}` from the newest enrolled `taskq-board/1` task `session` of that board, so an off-host Codex session shows its host, runtime, stage and update age. TaskQ links its rows here, never `codex://` off host.
- Static/no-script snapshots retain issue links and an explicit component-unavailable reason; browser evidence qualifies the shared blocks.

## Resource controls

D14 detail.

- DashQ's operations follow TaskQ's pinned wire fixtures and stay off ([resource controls § Route](../design/resource-controls.md#route-47)). The target-host adapter is [taskq#931](https://github.com/taskqorg/taskq/issues/931).
- Project limits ([§ Project limits](../design/resource-controls.md#project-limits-159)): DashQ reads the enrolled `taskq-resources/1` projection and offers the `taskq.project-limits/1` write under a `project-limits` grant, on the pinned TaskQ v4.6.25 release assets. Live qualification is #158.

## ServerQ findings

D15 detail.

- Findings use workspace/installation/rule/first_seen identity, never an inferred resource join; Hosts H4/H7 grouping stays unchanged.
- Retained producer samples preserve episodes. Only a complete, fresh, matching passing rule with ordered `verified_at` resolves an open episode. Missing or capped history is explicit.
- The vendored ServerQ catalog is used only on an exact SHA-256 match; otherwise thresholds and recommendations are unavailable.

## Agent work

D21 detail.

### Reading

- Source: TaskQ attempt stage records (taskq.md § 4 Profiling). Primary: the `stage` observations inside stored `taskq-telemetry/1` blocks (D11) of each TaskQ installation in the caller's scope. Fallback: `taskq-stage` JSON lines of a configured local `taskq.log` (`taskq_logs`). A host with neither source has no row.
- Telemetry: blocks received from one day before the window are read from the same store; each block is decoded once per process. Host = the record's `host`, else the installation label.
- Local log: read without board access every 60 s; the last value is kept on failure (D6).
- A host with both sources is read from both during the cutover. Records equal in task, attempt, stage, runtime, end time and open flag count once, so no gap or duplicate; the host's state is the telemetry's.

### Rows

- Period: the D7 period by record end time (`at_ms`); the host filter applies.
- `/data.agent_work` `{source, window_s, from, to, hosts, rows, attempts, attempts_total}`; `hosts` per host `{host, source, provenance, records, retained_from, coverage}`.
- `rows` per host and runtime (`claude|codex|codex-app|other`), slowest stage p95 first: `attempts` (distinct attempt ids), `tasks`, `outcomes` (build/review hand-ins), `time_ms` (sum of complete attempts' elapsed time on this row; null when none), `elapsed` `{known, of, reasons}`, `untimed`, `tokens` and `calls` (build/review).
- `stages` slowest p95 first, each `{stage, count, timed, p50_ms, p95_ms, max_ms, total_ms, outcomes, errors, calls, tokens}`.
- Usage is `{sum, known, of, reasons}` with taskq's codes; `sum` comes only from reported values. TaskQ context `tokens` include cache and are never added to another count.

### Attempts

- One row per TaskQ attempt (project, task, attempt id) with a record ending in the period, newest first, at most 200 (`attempts_total` counts all).
- Fields: `{project, task, attempt, link, hosts, runtimes, stages, outcome, state, reason, elapsed_ms, execution_ms, queue_wait_s, queue_wait_reason, tokens, calls, output_tokens, models, estimate}`.
- `link` is the issue only when the project is configured (D4.2); a record without a project stays unlinked. An attempt observed by several hosts is one row naming each.
- Stage interval = [end − duration, end]. `elapsed_ms` (wall time) is the union of the attempt's stage intervals, so overlapping or nested stages count once; `execution_ms` is the sum of its stage durations (overlaps added).
- Both need `complete` coverage: a closed build/review record, every record timed, start evidence (a `claim` record or an `open` lifetime marker) and no telemetry seq hole inside the attempt's rows.
- Else null with `state` `open` (`open-stage`, `no terminal record`), `adopted` (a stage adopted across a service restart), `untimed` or `partial` (`rows not received`, `attempt start not retained`). Open markers never count as stage samples.
- `queue_wait_s` is the `previous_seconds` of the TaskQ `transition` observation from `ready` whose entry is the attempt id (taskq.md § 2: the worker spawn). Else null with `ready transition not retained`, or `reviewer wait not recorded` (a reviewer spawns inside `review`).
- Rows and attempts add `output_tokens` and `models` (`ids` with counts, `known`/`of`, reasons) from taskq v4.6.32 stage `model`/`output_tokens`; `estimate` stays null (`context tokens not split by cache`).

### Unknown and incomplete

- A missing or unreadable log, or one without stage records (no profiling taskq, no attempt yet), is `unknown` with `records` null.
- Telemetry without a stored block is `unknown`; a newest block older than 300 s is `stale`; seq holes, or loss before a window it retains only in part, are `incomplete`.
- Skipped records, or a log retaining records only after the window start, make the host `incomplete` (`incomplete: true`, counts a lower bound); a log retaining records only after the window start says earlier unknown.
- Any incomplete host makes the block `incomplete: true`; the worst host names the block state.
- A stage adopted across a service restart has no duration. Unreported tokens or calls are null with reasons; `no-model` stages read `none (no model)`.
- Missing, old or partial evidence reads `unknown`, `stale` or `incomplete` with its reason. Budgets are unknown.

### Page and inspector

- The Agent work page shows attempts first, only when configured (a log or a TaskQ telemetry grant).
- Selecting an attempt shows its stage timeline, queue wait, coverage, last completed stage and the existing TaskQ workspace/issue links.
- Additive attempt fields: `last_stage_at_ms`, `last_completed_stage` (stage/end/outcome or null), `timeline` (stage, host/runtime, start/end/duration or null, open flag and reason), `timeline_total` and `timeline_incomplete`.
- The timeline keeps the newest 200 stage records per attempt; measurements still use all retained records. Intervals may overlap.
- Meaningful progress is unknown without producer evidence; neither client activity nor log/read age proves it.
- D23 admission is labelled current task evidence, never a historical attempt wait. Stage totals remain a separate on-demand destination; existing shared TaskQ controls retain their gates.

## Reports

D22 `/reports` detail.

- Additive `agent_work.totals` contains `elapsed_ms`, `execution_ms` and `queue_wait_s`, each `{sum, known, of, reasons}`, across all retained attempts selected by end time before pagination. No known values means null, including an empty read.
- Wall time unions stages within each complete attempt, then sums attempts (parallel attempts add); execution sums their stage durations; ready-queue wait stays separate.
- Totals are not clipped to the window and are never fleet clock time, useful work or inferred savings. The page and `dashq report` use these same values.
- Time is host-scoped (D21); agent/session filters apply to model use only.
- Show source state, known/of, reasons and each host's retained start (end watermark unknown). Paged attempt evidence links to `/agents` and its stage inspector, preserving filters and cursor. Changes retain D23's comparison gaps.
- Model use (D24) shows the existing estimate and per-model subtotals with known/of and reasons, separate currencies, configured price source/effective date/rates and source state.
- The model-source retained range is unknown (not reported), distinct from the requested query window. Model use remains a separate Agents destination.

## Waiting work, changes and route

D23 detail.

### Admission

- Vocabulary: taskq#1616, v4.6.32 [fixture/taskq/admission-codes.json](../../fixture/taskq/admission-codes.json), SHA-256 pinned.
- Each ready or waiting `boards[].tasks[]` row adds `admission` `{code, meaning, observed, age_s, host, reason, source}` from the TaskQ snapshot adapter, else the newest enrolled `taskq-board/1` task admission of that board.
- A code outside the pinned vocabulary, an `observed` over 60 s after the document time, no record (`admission not reported by taskq`) or the labels adapter alone read `unknown` with the reason.
- The Tasks view lists waiting work per board: task, meaning, TaskQ reason, host and observed age.
- Failed rows keep TaskQ's failure reason.

### Changes

- `/data.changes` `{window_s, from, to, timezone, comparison, coverage, rows, rows_total, unclassified}` over the period.
- Conditions are Attention current and gap items, retained incident events and enrolled ServerQ episodes.
- A producer start `since` at or after `from` is `new`; earlier and unresolved is `ongoing`; an episode whose `verified_at` lies in the window is `resolved`. An item without `since` counts in `unclassified` per source.
- `coverage` names per source whether it retains the window's history: enrolled ServerQ episodes (not when capped) and the Langfuse 24 h incident read do; TaskQ boards, Langfuse model errors and the local ServerQ report keep no start/end history (`no baseline`).
- `comparison.state` is `available` when every listed source has a baseline, `partial` when some, `unavailable` when none.
- A source without history reads `no baseline`.

### Route

- Each `installations.rows[]` adds `route` `{emitted, queued, accepted, projected, exported, queryable}`, each `{state: known|unknown|not_applicable, value, at, reason}`.
- Emitted and queued come only from the sender's own counters (TaskQ telemetry `status`; none sent: unknown). Accepted is the last durable receipt; projected the matching-generation readback; exported the D11 export state (not applicable except AgentQ); queryable whether `/data` serves its snapshot or `block=ID` its telemetry.
- Connections retains one Route line. Telemetry renders six labelled steps per installation/workspace with each stage's state, value, evidence time and reason.
- Scope and evidence are the same `/data`/`dashq report` projection; no collector, store or payload content is added.
- Known false is not passed.

### Requests

- Other current problems, gaps and history stay Observations detail.
- Each `attention.requests` item is `{kind, source, host, project, task, rule, installation, reason, since, age_s, action, target}`.
- Kinds: TaskQ questions (`/projects#board-<repo>-q-<n>`), failed TaskQ tasks (the issue) and ServerQ `warn` rules (the exact open episode `/checks#finding-<id>`, else the rule row `/checks#check-<installation|report>-<rule>`).

### Query

- `/data.query` `{period, from, to, timezone: "UTC", cursor, next_cursor, cap: 200}`.
- `cursor=N` (offset, default 0) pages `changes.rows` and `agent_work.attempts` by `cap`; `next_cursor` is null on the last page.
- Times are epoch seconds; the page prints UTC.

## Model use

D24 detail.

- The `Model use` section (Agents view) states the `API-equivalent estimate` label. `dashq report` prints the same `model_use`.
- Config `prices` is a non-empty array of `{model, source (https URL), effective (YYYY-MM-DD), currency (ISO 4217), input, output, cache_read?, cache_write?}`, rates per million tokens, one entry per model and day. Invalid: `config: bad key prices.<i>.<key>`.
- A call takes the price entry of its model effective on or before its day.
- TaskQ stage tokens include cache, so their price field is never known. No actual-expense figure or TaskQ-token price is inferred.
- `/data.model_use` `{source, label, prices {state, reason, entries}, runtimes, rows, estimate}`.
- `rows` per host and model from Langfuse `agentq.model` spans in the period (D7 host/agent/session filters; one event id counts once), with `calls`, per-field `tokens` `{sum, known, of, reasons}`, `estimate` and `reported` (client-reported cost only) `{amounts {currency: n}, known, of, reasons}`.
- Unknown estimate reasons: `no price table configured`, `no price for <model>`, `no <rate> price for <model>`, `<field> not reported`.
- Source state is Langfuse's; only a capped `agentq.model` read makes it `incomplete`.
- `runtimes` lists `claude`, `codex` and `hermes`, each with `status` and per-source per-field `known`/`of`: `agentq.model` (Claude, qualified by agentq#161) and `taskq stages` (D21 rows).
- Codex reads `unknown` (`model accounting not qualified (no model id)`); Hermes reads `unknown` (`no model accounting adapter`).

## Optimization results

D17 detail.

- [Optimization results P1–P3](../design/optimization-results.md) refine D8.4 with AgentQ-owned accounting definitions and a separate paired-experiment view. Metadata/privacy and source-failure rules apply.
- Exported events carry no interval coverage; event-time buckets never invent missing samples.
- `/data.optimization` consumes `agentq.compress` and identity-matched legacy run observations through `agentq.accounting/1`, pinned with its 29 conformance cases.
- Results inherit period/agent/host/session filters; disclosures, focus and selection survive refresh.
- Only the macOS run adapter is live-qualified ([evidence](../design/evidence/optimization-results-live.md)); other adapters and runtimes remain unqualified.

## Service versions

D18 detail.

- [Service versions V1–V4](../design/service-versions.md) refine D8.2/D9. coreq and each consumer retain lifecycle ownership. Read-only diagnostics use allowlisted evidence.
- Unconfirmed observations never support an `update` comparison. A stale status or a service that is not `running` keeps versions `last_known`.
- Versions are `MAJOR.MINOR.PATCH[-pre]`; other forms read unknown.
- `/data` `versions` is `{rows, mixed}`: one row per installation and workspace.
- Sources: enrolled `agentq-snapshot/1`, `serverq-report/1` (version only) and `taskq-board/1` (observation time only; the board observation reports no version, so `running` is a producer gap); Langfuse `agentq.installation` (installations not enrolled); legacy updater snapshots (by device); taskq heartbeats (by `service` id and project; project is `workspace`).
- Fields: `running`, `selected` (`last_known` when the observation is not current), `release` (version, check time, state), `reported_available`, `update` (`error` from the producer's error flag), `health`, `lifecycle`, `libraries` (engines; coreq unreported), `queue_age_s`, `coverage` and `gaps`.
- `lifecycle` comes from enrolled `agentq-snapshot/1` `release` (agentq v0.10.0): stage `waiting_for_activation|starting|healthy|pending_disable` with its reason code; `healthy` only beside a verified receipt.
- `health` is `verified` only for scope `status_written` on the running version, with the receipt at or before release time.
- A non-ok observation, a release time missing, future or over 180 s old, producer `fresh` false or any other value reads unknown with the reason. Other producers report neither lifecycle nor health.
- Version observations retain producer timestamps (enrolled: the older of snapshot time and `status_at`; Langfuse: the event that reported the version), add elapsed cache time to board age and propagate collection failures.
- The host filter applies to every row before `mixed`.
- Producer gaps: selected `current` and coreq version (AgentQ `release` carries both; not read yet); health receipt and lifecycle stage in the taskq and serverq contracts. Cases: [fixture/versions.json](../../fixture/versions.json).
- The Services view renders this projection with the pinned V4 reference: one selected installation/workspace detail, separate release and host times, and explicit producer gaps. Selection, disclosures, focus and scroll survive refresh; no lifecycle action or demo selector. The page fixture includes this section.
- Other enrolled fields and the Langfuse adapters await production-source authorization.

## First launch and outages

D19 detail.

- [Prototype coverage](../design/prototype-coverage.md) separates accepted examples from remaining design and live-qualification gaps.
- TaskQ-only has no empty resource, AgentQ or ServerQ panels.
- A failed page refresh preserves the page and shared component instances and shows advancing ages.
- Shared answer and policy adapters remain read-only until real action and recovery qualification.
- One first-launch entry continues to the existing `/connect` L2 flow; source context lists optional unconfigured inputs once.
- Failed boards retain historical counts and rows beside unknown current counters and explicit coverage.
- ServerQ retains last-good evidence. Langfuse resource reads cannot stay fresh after a collection failure. Trends need three real samples under U1.
- Real action and recovery qualification ([#119](https://github.com/taskqorg/dashq/issues/119)) follows #44 and TaskQ #931.
- Config validation fixtures use native absolute paths without relaxing validation.
