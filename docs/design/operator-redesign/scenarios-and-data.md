# Approved scenarios and data requirements

Scope: planning input accepted on 2026-10-22. Current source gaps and review dispositions: [review](review/README.md). Live evidence remains owner-private.

## Scenario coverage

| ID | Initial state and intent | Observable outcome / recovery | Primary owner |
|---|---|---|---|
| DQ-01 | Start the day; understand the whole system | Hosts, useful current evidence, work and direct human actions are readable together. Unknown, stale, partial and historical data remain distinct. | DashQ composition; each producer's health evidence |
| DQ-02 | Something changed during absence | Compare a named window/checkpoint; show new, ongoing and independently resolved conditions with source times and coverage. No baseline means comparison unavailable. | DashQ history/reporting; producers' episodes |
| DQ-03 | Server/resource/service problem | Open affected host/resource, timeline, check evidence and relevant work; propose a permitted next step; verify the source after action. Closing a task alone is not recovery. | ServerQ checks; AgentQ resource probes; service lifecycle owner |
| DQ-04 | Agent silent, failing, unauthenticated or unexpectedly slow | Separate runtime, login, request, task attempt, tool, integration and exporter evidence. Show observed last progress and missing probes; silence alone is not a diagnosis. | TaskQ/AgentQ/native runtime |
| DQ-05 | Ready work exists but nothing appears to start | Explain assignment, admission, dependencies, scope, capacity, policy and failures with their source revisions. Distinguish idle, held, running without recent progress and unobserved. | TaskQ scheduler/board/host authority |
| DQ-06 | Answer/manage a task from DashQ | Use the complete released TaskQ view and controls; preserve exact choice IDs, context, revision, operation ID and receipt. Retry reconciles uncertain delivery. | TaskQ; DashQ scoped adapter |
| DQ-07 | Ask PM to diagnose, plan or create work | Same TaskQ conversation workspace and project context; inspect plan/effects and per-operation readback; preserve draft across navigation/outage. Worker attempts stay separate. | TaskQ PM/operator contract |
| DQ-08 | Find where time, tokens and cost go | Drill from project/host/runtime/attempt to measured stages/calls. Separate wall time, summed overlapping execution, queue waits and missing coverage; no invented cost/savings. | Producer accounting; DashQ reports |
| DQ-09 | Determine whether telemetry itself is trustworthy | Follow emitted → locally queued → durably accepted → projected → exported → queryable, retaining gaps and reasons at each qualified stage. | Each sender; DashQ store/outbox; destination |
| DQ-10 | Source/network/store unavailable, then recovers | Other products continue; keep last-known state and drafts; reconcile delivery without duplicates. Scoped direct links and agent queries expose the same meaning. | All affected products |

Every flow needs list → detail → permitted action → observed result → return, including no data, long content, denial, stale revision, timeout, partial success and cancellation where applicable. Exact click counts will be measured on the candidate; no global “one click” promise applies to consequential effects.

## Telemetry requirements

Universal means a common envelope, correlation and query semantics with versioned product-owned payloads. It does not mean flattening all products into one event type or duplicating task authority. Langfuse's [observation/trace/session model](https://langfuse.com/docs/observability/data-model) is a useful destination mapping; it does not supply queue assignment, durable sender coverage or host-health evidence.

| ID | Requirement to resolve and verify | Operator / agent query it enables |
|---|---|---|
| T1 Identity | Stable event ID and payload/schema version; authenticated workspace/installation/generation; verified host/runtime mapping; project/board/task/attempt and session/trace/span links only when genuinely known. Preserve unlinked events. | Follow one issue/attempt across services without hostname guesses or double counting. |
| T2 Time | Distinguish occurrence/observation, send/accept/project/export times; stage start/end and duration basis; clock uncertainty, restarts and late/out-of-order records. | Explain whether delay was work, waiting, delivery or query visibility. |
| T3 Coverage | State queried scope/window, expected versus reporting producers, retained interval, known loss, source watermark and cap/truncation. Define complete intervals from producer evidence; silence alone proves none. | Tell empty activity from missing data and display honest totals/rates. |
| T4 Delivery | Durable receipt, idempotent replay/mismatch, sender backlog/oldest age, dropped/gap records, rejection reason and export/readback boundary. Accepted, projected and queryable remain separate. | Diagnose why a local event is absent centrally and recover without duplicates. |
| T5 Lifecycle | Versioned stage/state vocabulary owned by TaskQ; reason-coded blockers; progress evidence, terminal outcome and operation/revision/receipt links. Define expected work before labeling a stall. | Explain why ready work is not running and what a person can usefully do. |
| T6 Accounting | Per-model usage fields with known/of denominators, reported versus estimated cost and currency/pricing basis; monotonic duration where available; measured/estimated/unknown attribution; reset semantics. | Rank bottlenecks and spend without counting nested spans or cached tokens twice. |
| T7 Query | Same scoped semantic projection for UI and agents, stable IDs, bounded pagination/export, explicit period/filter/timezone, coverage and drill-down evidence. Agree actual queries before selecting indexes or rollups. | Reproduce a report and continue diagnosis from an agent without scraping the UI. |
| T8 Storage lifecycle | Preserve board authority; define retained events, query projections and rebuildability, aggregates, receipts and audit retention separately. Name pruning gaps, storage limits, restore and schema-migration/rollback behavior. Assess current SQLite/store first. | Compare periods and restore/rebuild reports without silently changing their meaning. |
| T9 Privacy/authority | Retain existing metadata allowlists and server-side scope. PM conversation content/attachments use TaskQ's separately governed channel, not the generic telemetry payload or public report. Action audit does not grant execution. | Let agents inspect permitted evidence without exposing unrelated conversation content or secrets. |

Exact field names, collection cadence, retention periods, budgets, loss policy and stalled-work thresholds remain to be reconciled with existing producer contracts and volume evidence. Do not copy enrollment's current resource retention into every telemetry/report class.


Each PM output maps DQ-01–DQ-10, T1–T9 and preservation rows to an owner, existing/new task, dependency and observable check.
