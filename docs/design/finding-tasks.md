# ServerQ finding to TaskQ task

Accepted under D15. [TaskQ F1–F4](https://github.com/taskqorg/taskq/blob/main/docs/design/finding-tasks.md) owns task creation, UI, operation identity and receipts. [Enrollment](enrollment.md) remains observation-only. ServerQ remains read-only; its rule catalog and projection own facts and recommendations.

## H1. Placement and ownership

Attention opens the existing finding detail. **Create task** opens one shared TaskQ composer beside that context; Boards and the finding later open the same linked task/card, not duplicate editors. Preselection comes from the explicit owner installation-to-project mapping, shown as such; allow a project override. A producer named ServerQ does not automatically imply the ServerQ project owns the repair.

Review title, scope, numeric evidence, source time, catalog reference and acceptance before the TaskQ create/optional-start operation. Link by workspace, stable ServerQ installation, rule and episode (`first_seen`), plus canonical board/issue identity. Do not join by hostname or guess an AgentQ filesystem from the ServerQ projection. Same-episode links survive refresh and reconnect; lost installation identity is a different source.

The accepted reference is [finding-task-reference.html](finding-task-reference.html), [standalone preview](finding-task-preview.html), and [source manifest](finding-task-reference.sources.json). Its numeric data, project mapping, issue #2401, times, repair and price are synthetic. The parent functions are reused with documented fixture adapters; creation/recovery are simulations, not production assets.

Observation boundary (#40 owner answer): Hosts H4/H7 grouping stays unchanged. Findings and episodes stay separate by installation/workspace, with no inferred resource join. Task creation and project mapping remain the #53 handoff.

## H2. Independent readback

| TaskQ observation | ServerQ observation | Dashboard |
|---|---|---|
| Held/doing/ask | Open incident | Task card plus active problem; ordinary questions use the shared answer card |
| Closed | No later check | Waiting for verification; retain Attention |
| Closed | Fresh warn | Still warning; retain evidence and Attention |
| Any | Unknown or stale | Current condition unknown; do not resolve the open incident |
| Any | Fresh matching passing check, producer episode verified | Verified by ServerQ; remove this episode from active Attention and retain history |
| Previous task closed | New warning after verified episode | New incident; prior task stays historical, no automatic reopen or duplicate |

Verification requires producer `verified_at` and a fresh passing observation for the same installation/rule/episode, after the relevant repair observation. In the closed-task path the new check must postdate task completion; an older cached passing result is insufficient. Keep task completion, source observation and receipt times separate; uncertain clocks cannot establish ordering. A matching catalog is required to show a threshold/recommendation; missing or mismatched catalog stays explicit. Never replace a newer observation with a delayed one.

A passing check proves this condition passed at that time, not that the task caused recovery or that all manual checks passed. Recovery can precede task closure; the task remains governed by TaskQ. No data is not zero and an unknown probe does not resolve an incident. Retain history when recurrence returns to Attention.

## H3. Authorized adapter

Use the existing guarded browser identity, scoped store and CSRF/Origin boundary, plus separately granted task-create/start capabilities. Resolve board and executable server-side. Dispatch through canonical TaskQ APIs, never a shell or custom forge writer; do not extend `/answer` or machine credentials into creation authority. Persist actor, target, source reference and immutable operation ID through the existing store. TaskQ's board readback is authoritative.

Show accepted, rejected or unknown creation. Unknown disables another create and reconciles the same request; a recovered task is linked without duplication. Existing links open the task. Incomplete source/board data or missing authorization prevents a mutation with a precise reason. Existing host work and checks continue regardless of this action path.

## H4. Qualification

- Reuse TaskQ F4 checks, including real create/optional-start, denied/cross-workspace actors, lost response, restart and concurrent duplicate attempts.
- Exercise every H2 row, old/cached and out-of-order samples, reused host labels, identity loss and reconnect. Verify task closure alone never resolves a finding.
- Preserve first-screen charts, task row identity, right-aligned counters, focus/drafts and single-editor navigation. Check 16/20 px and both themes at desktop/narrow widths.
- Keep observation adapter work independent of write release. Publish exact TaskQ design/asset and DashQ versions with evidence; simulations are not rollout authorization.

Delivery: TaskQ #948 canonical create/recovery, #949 shared component (after #948/#938); DashQ #40 observation/episode projection, #53 handoff (after #40/#39/#34 and the TaskQ assets). Keep write release held until cross-repository prerequisites and live qualification pass. Reference browser checks passed at 736 px/16 px light and 360 px/20 px dark: project override/edit, held creation, answer, each observation state, unknown receipt and denied access.

## H5. Continuous recovery journey

Attention → server → check → task → verified recovery is one contextual route. Preserve installation/rule/episode, selected project, task identity, draft, answer receipt, disclosures and return position through navigation and automatic refresh. The server view retains resource bars/trends and check states; manual checks remain unverified. Join resource and finding identities only through the explicit mapping in H1.

Attention and Boards open the same TaskQ task/card instance. Task creation reuses the H1 composer; no duplicate answer form, copied task state machine or inferred completion. Breadcrumbs return to the same context, including an unfinished task draft. After a matching fresh passing observation, remove only the verified episode from active Attention; keep its check, task and evidence reachable in history. Recurrence creates a new episode; the old task stays closed and historical. H2 defines every recovery outcome.

The accepted [journey reference](attention-recovery-reference.html), [preview](attention-recovery-preview.html) and [manifest](attention-recovery-reference.sources.json) add navigation to the H1 reference. The established first-screen layout remains authoritative; the one-host fixture is a focused scenario, not a reduced fleet requirement. All measurements, task identity, price, times and actions are synthetic. Demo snapshot controls are not product controls. The fixture does not qualify consequential answers, live transport, joining or background refresh; retain the canonical action authority and release gates.

Acceptance: trace one episode through every route at 1024px/16px light and 360px/20px dark. Verify one task/card, draft and answer retention, ordinary answer semantics, closed-without-check, fresh warning, unknown/stale check, matching recovery and recurrence. Refresh each route without losing focus, scroll or draft, including delayed observations and changing resource capacity. Record the exact TaskQ parent/assets and browser-to-producer evidence. Delivery remains #53 with its existing dependencies.
