# Approved DashQ redesign handoff

Status: owner accepted the consolidated design on 2026-10-22 and authorized the PM planning task. Implementation and deployment are not authorized. Owner decisions dated 2026-10-22 are recorded in the [review](review/README.md#owner-decisions--2026-10-22).

## Problem and intended behavior

An operator needs to understand infrastructure, agents and TaskQ work at a glance, inspect why work waits or a service needs attention, and reach the permitted action or evidence with few steps. Current data exists across products, but some is not consumed by DashQ, some telemetry contracts disagree, and several attractive prototype explanations had no real producer.

Preserve the selected minimal host-first hierarchy, compact icons, readable adjustable text, right-hand trends with an explicit period and stable list/detail/return context. Unknown is not healthy, idle, zero cost or a confirmed failure. Actionable Attention contains explicit producer requests requiring a person, not ordinary waits or automatic retries.

The exact owner-accepted mock is retained in the private input packet. The [public synthetic derivative](overview-candidate.html) starts at Overview. Its host/attempt scenarios are example states, not the fleet inventory or a live report. Public Telemetry and source detail use synthetic coverage; live audit counts and financial aggregates are withheld. The report retains an example interval reconstruction while honestly showing the missing TaskQ cost connection. Runtime action controls are disabled; PM only previews a request.

## Owner choices and specification amendments

- Money: actual payments/API charges and API-equivalent estimates are separate. Subscription and infrastructure expense stay separate, with a declared period/source/allocation rule. Unknown upstream cost basis is not silently reclassified as either.
- Diagnostic scope: events and metrics only. No log excerpts, arbitrary commands, raw conversations or tool contents added to telemetry. The existing `session.line` worker-text value is excluded from the redesign target: keep stage/update time and structured error classes, not line content. Retire its projection/ingestion through a reviewed compatible migration and normal retention; no retrospective deletion is authorized here.
- PM: request from DashQ, continue in the native session. TaskQ owns request/session dispatch, state and destination, and must publish the shared request composer/receipt component. The DashQ-authored form in the mock is an interaction study to move upstream, not reusable implementation. No DashQ conversation store or browser chat backend.
- Update the current DashQ D8/D10 design target for overview trends and preserved compact host facts. Update the D13/board consumer target for shared task controls already requested by the owner; keep actor grants, current revision, operation ID, receipt/readback and existing Close-on-board behavior. These amendments do not enable production effects.

Reconcile current canonical contracts against audit baseline DashQ 0db7eca before implementing. Retain current runtime rules until the corresponding reviewed implementation and qualification gates are met.

## Capability preservation ledger

| Capability | Final destination / owning UI | Required handling |
|---|---|---|
| Hosts, resource values/history and source freshness | Overview → host/resource detail | Existing history/sampler, stable IDs, resets/gaps, per-host missing source; no second collector |
| TaskQ presence/login/assigned queue | Host details / queue explanation | Consume scoped existing observations with producer expiry; service presence distinct from physical host state |
| Versions, selected/running/latest packages and updater state | System → Versions & updates | Preserve current detail and readback; no inferred running version |
| Services | System → Services; direct host links | Preserve scoped service evidence and its own time |
| Checks, manual rules, findings, incidents and linked work | System → Checks & findings; direct Attention | Separate current, gap and history; manual != passed; no inferred resource/finding join |
| Projects, queue, tasks, answers and menus | Tasks and direct task links | Full released TaskQ surfaces; no competing DashQ task editor; extract any missing reusable surface upstream |
| Start/Stop/Later/Retry | Same TaskQ detail/menu | v4.6.30 shared task-controls; allowed states from the upstream fixture, never guessed. Read-only when actor/revision/adapter is unqualified |
| Close task | Board link owned by TaskQ | Consequential board path; not a new DashQ control |
| Project resource controls | Existing shared TaskQ resource surface | Preserve review/dispatch/receipt/cancel and grants; complete view, not a homegrown substitute |
| AgentQ configuration | System → Resources & configuration | Preserve existing review/apply and effective-state readback; availability from its own capability |
| Connections, enrollment, consent, revocation | System → Integrations & enrollment | Preserve independent product lifecycle and scope; no new auth or silent opt-in |
| Agent work | Agents → attempt details | TaskQ attempt identity, host grouping, client activity separate; log age is activity, not useful progress |
| Stage aggregates | System → Optimization & stage totals / reports link | Preserve D21 meaning; feed current stage source, not starved legacy text reader |
| Optimization/accounting | Same separate evidence destination | Preserve D17 paired cohorts/coverage; generic cost/time reports do not replace it |
| Reports | Reports → scoped time/source evidence | Known/of, unknown intervals, price basis, currency, retained range; no inferred waste/savings |
| Telemetry and source health | Telemetry; direct object gaps | Producer→local queue→receipt→projection/query→export boundaries, each honestly known or unknown |
| PM | PM request → TaskQ native session | Typed request with operation ID/receipt/session reference; no simulated accepted request |

System is a preservation/navigation study in this candidate. Its existing detailed views are retained by contract and are not recreated or visually qualified by the placeholder descriptions. The complete shared TaskQ workspace/detail/menu adoption also needs upstream surface inventory; importing only project blocks is not completion. Shared task-control internals stay unmodified; offers come from the upstream fixture/row and an outer disabled adapter gate is used in this study. Complete shared view/style adoption and its fidelity remain an upstream extraction/qualification gate.

## Cross-service work and dependencies

| Owner | Smallest coherent deliverable | Observable proof |
|---|---|---|
| DashQ + TaskQ | Reconcile enrolled task revisions and scoped observations with configured board identity, label/title reads, completeness and source authority; retain label-only fallback as read-only | A real stored observation is rendered with expiry; revision-bearing task opens the same shared view; conflicting/stale/unknown sources never grant an operation |
| TaskQ + DashQ | Conform `taskq-telemetry/1` envelope/schema/cursors and status/backlog/gap fields using a shared fixture; preserve IDs, dedup, receipts and consent | Sender bytes accepted by receiver; duplicate/replay, restart and rejection cases; event readback by the same ID |
| DashQ | Bounded semantic projections/readback for attempts, why-not-running, telemetry route and receipts; same `/data` + `dashq report` meaning | One real waiting task explained from assignment/deps/holds/resources with source times; an unavailable admission decision stays unavailable; UI and agent query agree |
| CoreQ/TaskQ | Reuse universal SQLite/export; genuine interval semantics, attempt/session correlation and retained watermarks/gaps | Complete and partial/open attempt replay; no nested/parallel double count; unknown remains visible; old D21 text reader replaced or adapted without a second store |
| AgentQ/runtime adapters | Qualify Claude, Codex and Hermes model accounting separately; minimal product-owned model/call/usage/price provenance extensions only where absent | Native runtime event through durable receipt to a bounded report; per-field known/of, unknown price, dedup and correction cases |
| DashQ reporting + owner-supplied billing input | Separate expense records from invoices/provider exports or owner records: stable billing ID, period, currency, amount and category (API/subscription/infrastructure); allocations explicit or unallocated. No billing credentials accessed during design; no estimate added to an actual-expense subtotal | Reconcile one owner-supplied period to its source total, deduplicate corrections, preserve unknown/unallocated and mixed currencies |
| DashQ/store owners | Measured per-installation retention/budget and pruning-gap records; query cap/pagination and rebuild/restore boundaries | Representative compressed volumes; slow/offline sender, eviction and full-store cases; retained range and known loss remain queryable |
| TaskQ | Publish a shared request composer/receipt component, typed PM request and native session handoff with shared context/status/error handling; DashQ imports it unchanged | One DashQ request reaches native PM, result/session visible in the owning surface; denial/timeout/uncertain delivery never duplicate the request |
| Existing service owners | Structured resource/check evidence and permitted diagnostic/action paths | Correct stable scope and independent readback; no raw command/log channel or inferred service-impact claim |
| DashQ + TaskQ UI owners | Adopt complete shared views/components, preserve routes and stable in-place refresh/focus/drafts | Fidelity to pinned assets; task/state/menu/answer journeys, narrow/large text, keyboard and refresh recovery; existing routes remain reachable |

Delivery order: existing-data integration → telemetry compatibility/trust → attempt/report projections and missing producer contracts → native PM/shared operation qualification → final cross-service acceptance. The full requested target stays in scope; unavailable states are delivery gates, not permanent deletion of the requirements.

Do not duplicate completed TaskQ [host observations](https://github.com/taskqorg/taskq/pull/1509), [task controls](https://github.com/taskqorg/taskq/pull/1512) or [operator UI](https://github.com/taskqorg/taskq/issues/1578). Re-read current board/ref/release state before decomposition. Session-progress source was unreleased at this audit and must be rechecked.

## PM planning task

[Planning issue #177](https://github.com/taskqorg/dashq/issues/177).

**Title:** Plan the reviewed DashQ operator redesign and telemetry integration across TaskQ, AgentQ, CoreQ and ServerQ

Use this package and the final owner-accepted mock to reconcile existing work, review the owning contracts and produce implementation tasks with dependencies, bounded delivery slices, migration/rollback and concrete native/end-to-end evidence. Preserve complete shared TaskQ views, existing DashQ capabilities and board authority. Separate existing-data consumption, missing producer fields, transport compatibility, storage lifecycle, query semantics and action qualification. Keep actual cost, estimates and unknown basis distinct. PM runs through the existing native session; telemetry remains events/metrics only.

Return a reviewed coverage table mapping every DQ-01–DQ-10 / T1–T9 requirement and preservation row to an existing task or a coherent new task, owner and acceptance proof. Resolve engineering choices with volume/source evidence. Do not file one task per field or invent a new generic database, collector, shell or conversation backend. Report remaining blockers and the implementation order. Planning alone does not authorize enrollment/configuration changes, cutover or deployment.

**Entry gate met:** owner accepted the consolidated visual and scope; accepted mock hash is recorded in approval.json; the exact private input packet is read on the registered primary checkout on host mac. **Exit gate:** owner can inspect the complete reviewed plan and actual task links before implementation starts.
