# Views detail

Linked detail of [dashq.md](../../dashq.md): the `/data` document and the view rules of D4, D8, D10, D13, D15, D17–D19 and D21–D24. The contract rules R1–R9 apply; this page adds lower-priority detail and never weakens them.

## Data document

- Top level: `schema` (2), `generated_at`, `period`, `filters`, `sources`, `hosts`, `attention`, `boards`, `resources`, `agents`, `checks`, `versions` (D18), `optimization` (D17: AgentQ accounting, event-time buckets and source freshness), `agent_work` (D21), `model_use` (D24).
- `resources` (D4.4): `window_s`, `from`, `to`, `bucket_s`, `aggregation`, `cap`, `rows` of `{installation, host, runtime, workspace, provenance, source, current, history}`.
- D11 adds `installations`, `sources.enrollment`, `sources.export` and `telemetry_block`; D23 adds `changes` and `query`; their sections state when.
- Declared exceptions within `schema` 2, not additive changes:
  - removal of `boards[].tasks[].session.line` and of the `line` of `taskq-board/1` snapshot payloads (D13);
  - `agent_work.rows[].time_ms` means the sum of complete attempts' elapsed time on the row; it had summed timed stages, counting nested stages twice.

## Langfuse reads

- Client use (D8.4) also reads `agentq.compress` and `agentq.retrieval` over 24 h; the 24 h period reuses these reads.
- The `agentq.installation` read takes the `io` field group, since its `agentq.routing` and `agentq.hooks` JSON strings exceed Langfuse's 200-character metadata cut.

## Resource samples

- Per sample (D4.4): CPU percent; RAM used, available and total bytes; per filesystem `id`, role, fstype, used, free and total bytes.

## Host observations

D8.1 detail.

- Runtime `hosts[].taskq.observed_running` is nullable fresh attempt evidence. `boards[].attempts` adds canonical `host_id` and `current`, not pending assignment.
- TaskQ v4.6.30 `taskq-board/1` observations supply online, per-runtime login and assigned queue.
- `hosts[].taskq.observations` preserves board/workspace/installation, producer, host, `observed`, `valid_until`, completeness and reason.
- The latest observation per board/workspace/host/kind wins; equal-time conflicts are unknown.
- Missing, expired, future, invalid or partial evidence reads unknown (counts null), retaining TaskQ's reason.
- Heartbeat machine identity ([H2](../design/hosts.md)) joins labels; absent identity keeps its own row.

## Overview resources

D10 detail.

- Overview resources are current CPU percent, RAM and per-filesystem used/total bars when capacity is known, beside small right-hand trends labelled last 30 minutes, 0–100%.
- Only D4.4 samples within that window draw history; three valid samples per metric are required.
- Missing values, gaps and CPU resets break lines; warmup/unavailable, stale and incomplete coverage remain explicit. Detailed history stays in host detail.
- The browser fidelity check pins approval.json's public derivative hash and compares its host geometry and hierarchy at 1280/16, 360/20 and large text in both themes. Fixture evidence never qualifies production.

## Pinned TaskQ assets

- Project/answer blocks and task controls: TaskQ v4.6.30, commit 09cb0d7db35d0de51bef83d9fa3ec803eafad42d, [shared blocks](https://github.com/taskqorg/taskq/blob/v4.6.30/docs/design/shared-blocks.md), `assets/taskq/provenance.json`.
- Resources: TaskQ v4.6.25, `assets/taskq/sources.json`.
- Full workspace, detail, menus and PM composer: TaskQ v4.6.34, `assets/taskq/workspace-provenance.json`; unchanged released components through the [adapter](../design/task-workspace.md).

## Redesign target decisions

- Actual expenses and model-use estimates stay separate.
- Telemetry is events and metrics only; worker-line content retires compatibly, with no retrospective deletion.
- PM requests go through DashQ with native-session continuation through TaskQ-owned shared UI.
- Versions/update, client sign-in and task counts remain in expandable detail.

## Task rows

D13 detail.

- Session progress (taskq R6 3): a doing or review task row adds `session` `{host, runtime, stage, age_s}` from the newest enrolled `taskq-board/1` task `session` of that board, so an off-host Codex session shows its host, runtime, stage and update age. TaskQ links its rows here, never `codex://` off host.
- Static/no-script snapshots retain issue links and an explicit component-unavailable reason; browser evidence qualifies the shared blocks.

## ServerQ findings

D15 detail.

- Retained producer samples preserve episodes. Only a complete, fresh, matching passing rule with ordered `verified_at` resolves an open episode. Missing or capped history is explicit.
- The vendored ServerQ catalog is used only on an exact SHA-256 match; otherwise thresholds and recommendations are unavailable.

## Agent work

D21 detail.

### Reading

- Telemetry: blocks received from one day before the window are read from the same store; each block is decoded once per process. Host = the record's `host`, else the installation label.
- Local log: read without board access every 60 s; the last value is kept on failure (D6).
- A host with both sources is read from both during the cutover. Records equal in task, attempt, stage, runtime, end time and open flag count once, so no gap or duplicate; the host's state is the telemetry's.

### Rows

- Period: the D7 period by record end time (`at_ms`); the host filter applies.
- `/data.agent_work` `{source, window_s, from, to, hosts, rows, attempts, attempts_total}`; `hosts` per host `{host, source, provenance, records, retained_from, coverage}`.
- `rows` per host and runtime (`claude|codex|codex-app|other`), slowest stage p95 first: `attempts` (distinct attempt ids), `tasks`, `outcomes` (build/review hand-ins), `time_ms` (sum of complete attempts' elapsed time on this row; null when none), `elapsed` `{known, of, reasons}`, `untimed`, `tokens` and `calls` (build/review).
- `stages` slowest p95 first, each `{stage, count, timed, p50_ms, p95_ms, max_ms, total_ms, outcomes, errors, calls, tokens}`.
- Usage is `{sum, known, of, reasons}` with taskq's codes; `sum` comes only from reported values.

### Attempts

- One row per TaskQ attempt (project, task, attempt id) with a record ending in the period, newest first, at most 200 (`attempts_total` counts all).
- Fields: `{project, task, attempt, link, hosts, runtimes, stages, outcome, state, reason, elapsed_ms, execution_ms, queue_wait_s, queue_wait_reason, tokens, calls, output_tokens, models, estimate}`.
- `link` is the issue only when the project is configured (D4.2); a record without a project stays unlinked. An attempt observed by several hosts is one row naming each.
- Stage interval = [end − duration, end]. `elapsed_ms` (wall time) is the union of the attempt's stage intervals, so overlapping or nested stages count once; `execution_ms` is the sum of its stage durations (overlaps added).
- Both need `complete` coverage: a closed build/review record, every record timed, start evidence (a `claim` record or an `open` lifetime marker) and no telemetry seq hole inside the attempt's rows.
- Else null with `state` `open` (`open-stage`, `no terminal record`), `adopted` (a stage adopted across a service restart), `untimed` or `partial` (`rows not received`, `attempt start not retained`).
- `queue_wait_s` is the `previous_seconds` of the TaskQ `transition` observation from `ready` whose entry is the attempt id (taskq.md § 2: the worker spawn). Else null with `ready transition not retained`, or `reviewer wait not recorded` (a reviewer spawns inside `review`).
- Rows and attempts add `output_tokens` and `models` (`ids` with counts, `known`/`of`, reasons) from taskq v4.6.32 stage `model`/`output_tokens`; `estimate` stays null (`context tokens not split by cache`).

### Unknown and incomplete

- A missing or unreadable log, or one without stage records (no profiling taskq, no attempt yet), is `unknown` with `records` null.
- Telemetry without a stored block is `unknown`; a newest block older than 300 s is `stale`; seq holes, or loss before a window it retains only in part, are `incomplete`.
- Skipped records, or a log retaining records only after the window start, make the host `incomplete` (`incomplete: true`, counts a lower bound); a log retaining records only after the window start says earlier unknown.
- Any incomplete host makes the block `incomplete: true`; the worst host names the block state.
- A stage adopted across a service restart has no duration. Unreported tokens or calls are null with reasons; `no-model` stages read `none (no model)`.

### Page and inspector

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

### Changes

- `/data.changes` `{window_s, from, to, timezone, comparison, coverage, rows, rows_total, unclassified}` over the period.
- Conditions are Attention current and gap items, retained incident events and enrolled ServerQ episodes.
- A producer start `since` at or after `from` is `new`; earlier and unresolved is `ongoing`; an episode whose `verified_at` lies in the window is `resolved`. An item without `since` counts in `unclassified` per source.
- `coverage` names per source whether it retains the window's history: enrolled ServerQ episodes (not when capped) and the Langfuse 24 h incident read do; TaskQ boards, Langfuse model errors and the local ServerQ report keep no start/end history (`no baseline`).
- `comparison.state` is `available` when every listed source has a baseline, `partial` when some, `unavailable` when none.

### Route

- Each `installations.rows[]` adds `route` `{emitted, queued, accepted, projected, exported, queryable}`, each `{state: known|unknown|not_applicable, value, at, reason}`.
- Emitted and queued come only from the sender's own counters (TaskQ telemetry `status`; none sent: unknown). Accepted is the last durable receipt; projected the matching-generation readback; exported the D11 export state (not applicable except AgentQ); queryable whether `/data` serves its snapshot or `block=ID` its telemetry.
- Connections retains one Route line. Telemetry renders six labelled steps per installation/workspace with each stage's state, value, evidence time and reason.
- Scope and evidence are the same `/data`/`dashq report` projection; no collector, store or payload content is added.

### Requests

- Each `attention.requests` item is `{kind, source, host, project, task, rule, installation, reason, since, age_s, action, target}`.
- Kinds: TaskQ questions (`/projects#board-<repo>-q-<n>`), failed TaskQ tasks (the issue) and ServerQ `warn` rules (the exact open episode `/checks#finding-<id>`, else the rule row `/checks#check-<installation|report>-<rule>`).

### Query

- `/data.query` `{period, from, to, timezone: "UTC", cursor, next_cursor, cap: 200}`.
- `cursor=N` (offset, default 0) pages `changes.rows` and `agent_work.attempts` by `cap`; `next_cursor` is null on the last page.
- Times are epoch seconds; the page prints UTC.

## Model use

D24 detail.

- `/data.model_use` `{source, label, prices {state, reason, entries}, runtimes, rows, estimate}`.
- `rows` per host and model from Langfuse `agentq.model` spans in the period (D7 host/agent/session filters; one event id counts once), with `calls`, per-field `tokens` `{sum, known, of, reasons}`, `estimate` and `reported` (client-reported cost only) `{amounts {currency: n}, known, of, reasons}`.
- Unknown estimate reasons: `no price table configured`, `no price for <model>`, `no <rate> price for <model>`, `<field> not reported`.
- Source state is Langfuse's; only a capped `agentq.model` read makes it `incomplete`.
- `runtimes` lists `claude`, `codex` and `hermes`, each with `status` and per-source per-field `known`/`of`: `agentq.model` (Claude, qualified by agentq#161) and `taskq stages` (D21 rows).
- Codex reads `unknown` (`model accounting not qualified (no model id)`); Hermes reads `unknown` (`no model accounting adapter`).

## Optimization results

D17 detail.

- `/data.optimization` consumes `agentq.compress` and identity-matched legacy run observations through `agentq.accounting/1`, pinned with its 29 conformance cases.
- Results inherit period/agent/host/session filters; disclosures, focus and selection survive refresh.
- Only the macOS run adapter is live-qualified ([evidence](../design/evidence/optimization-results-live.md)); other adapters and runtimes remain unqualified.

## Service versions

D18 detail.

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

- One first-launch entry continues to the existing `/connect` L2 flow; source context lists optional unconfigured inputs once.
- Failed boards retain historical counts and rows beside unknown current counters and explicit coverage.
- ServerQ retains last-good evidence. Langfuse resource reads cannot stay fresh after a collection failure. Trends need three real samples under U1.
- Real action and recovery qualification ([#119](https://github.com/taskqorg/dashq/issues/119)) follows #44 and TaskQ #931.
- Config validation fixtures use native absolute paths without relaxing validation.
