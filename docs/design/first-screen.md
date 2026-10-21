# Host-first dashboard

Approved design: #169 A1–A5, replacing the previous Attention/servers/Boards composition under [D8–D10](../../dashq.md#d8-page). The owner [approved option 1](https://github.com/taskqorg/dashq/issues/169#issuecomment-6318252313): deliver the documentation PR and retain the listed implementation/qualification gaps. The reviewed specification and five screens are at commit `74ff807fcb0a0129624f0cc84312c318afa94f84`; this acceptance record changes no visuals. No Rust implementation, deployment or new action authority is part of this change.

## First screen

Header: Hosts, Tasks and More; Attention count links directly to the action list. Hosts is the initial view. More exposes existing supporting routes, Connect and Langfuse links. One full-width host grid comes first, followed by short Attention. No board totals, installation inventory, charts or diagnostic tables precede the hosts.

Each row shares these columns at 1280 px: host/kind and online evidence/age; TaskQ, AgentQ and ServerQ running versions with update state; Claude/Codex sign-in and probe age; CPU; RAM used/total; every disk used/total; running and host-assigned queued tasks. Selecting the name opens host detail; selecting a count opens Tasks filtered to that host. Queue includes only explicitly assigned pending work; unassigned board work stays separately visible in Tasks. Running means current attempt evidence, not responsive execution. Preserve identities under [H1–H7](hosts.md); never join by friendly label.

Online/offline needs an authoritative producer probe, scope, reason and validity window. Until supplied, show Unknown and newest observation age, including last seen for stale hosts; receipt time is not observation time. Each product/login retains its own time in accessible detail. Freshness of one product cannot make another healthy. Missing sources show `?` with accessible reasons and one visible legend. Revoked grants never supply current state.

Attention lists human actions, not a telemetry inventory. Each row names affected host/task, current reason, age and one direct action. Examples: expired login → host sign-in instructions; failed unit → diagnostics; producer-confirmed low disk → exact finding; question → existing TaskQ card. Inspecting grants no repair command. Unknown/stale values stay beside affected host values and in Source status, with all reasons; only an evidenced intervention enters Attention. An empty list says No actions requested, never All healthy.

At 360 px each host reflows into the same labelled groups, without smaller type or sideways scrolling. Default main type is 16 px, supports 20 px and system scaling; more hosts scroll normally. Never reorder on refresh. [Overview](overview.md) owns detail charts/updates.

## Scenarios and coverage

| ID | Initial state and intent | Observable result / exception |
|---|---|---|
| HF1 (A1) | Several hosts; find where work runs | Grid exposes products, logins, resources, running/assigned queue and per-field evidence. Missing probes stay unknown. |
| HF2 (A2) | Login expired, failed unit, low disk or question | Direct action identifies host/task. No automatic sign-in, repair or task mutation. Silence/revocation alone adds no failure. |
| HF3 (A3) | Open a count, then question | Tasks retains host scope, shared rows/card, one editor per revision. Unassigned/unknown assignment stays explicit; missing workspace release never permits a copy. |
| HF4 (D6/D19) | Source failure, recovery, missing history | Retain values/ages/reasons and drafts; no healthy/zero substitution, invented trend, cleared incident or stale action capability. |
| HF5 (D22) | Detail, Back, reload/shared URL | Path/filters identify the view; Back restores local position. Unknown route gives recovery. Prototype covers three routes only. |
| HF6 (D11) | Find inactive installation or add service | More → Connections, explicit inactive filter, history intact. Existing approval/readback/revoke protections retained. |

## Preservation ledger

Current section inventory checked against src/page.rs, resource.rs, clients.rs, agent_work.rs, optimization.rs and findings. No current capability is deleted.

| Current section / capability | Destination and retained behavior |
|---|---|
| First launch | No installations: one Connect entry, U1/L2; no invented hosts/empty product panels. |
| Hosts / server resources | First screen: identity, ages, resources, versions, tasks. Host detail: real 30 min CPU/RAM trends, disk history, gaps/resets/warmup and evidence. |
| Attention current problems | First screen only with person/action; coverage/routing/hook records remain host → Agent use until actionable intervention is identified. |
| Observation gaps / source summary | Collapsed Source status with non-ok count; affected host values retain unknown/stale meaning and age. |
| Incidents / findings | Host → Checks or /checks; deduplicated, separate from current problems, same episode identity/evidence. Linked tasks retained; creation D15-gated. |
| Boards / questions / failures | Tasks (/projects), shared TaskQ blocks, Board links, all states/coverage, single answer editor and D13 authority. Unassigned queue stays here. |
| Work mode / project limits | Host → TaskQ or project's policy entry; shared component, D14 grants/readback; no host commands. |
| Connections / approval / revoke | More → Connections (/integrations), current by default; stale/superseded/revoked filters retain history. Direct inactive/approval anchors reveal the record. Expected hosts never disappear solely due to staleness. |
| Agent integrations | Host detail or /integrations; aliases, overrides, conflicts, routing and existing plan/apply/readback gates (D16). |
| Agents / Clients | Host → Agent use or /agents; activity, per-model sample counts, reported costs, routing/hooks/MCP and #116/#131 observations. Login is separate. |
| Optimization | /agents; P1–P3 accounting, coverage, event-time buckets, separate unqualified whole-task experiments. |
| Agent work | /agents; D21 attempt stages, host/runtime, timing, usage coverage and unknowns; configured logs only. |
| Services / host updates | Grid summaries; /services or host detail for selected/available/running/health/lifecycle, libraries and source times. No lifecycle control. |
| Server checks | Host detail or /checks; per-rule details, manual rules not verified, incidents, catalog-qualified evidence. |
| Diagnostics / process counters / export / telemetry | Relevant host/installation detail, reset/coverage semantics, no raw commands/output/secrets. D20 profile retained; stored TaskQ telemetry analysis still unimplemented. |
| Header filters / Langfuse links / JSON / report | More and detail retain D7 filters/links; API/report, periods, cadence, cache and caps unchanged. |
| Previously retired ServiceQ | Node snapshots/services/failure receipts, legacy compression/RTK/Headroom counters, updater snapshot state and retry countdowns stay removed (agentq#50). ServerQ now direct D4.3; monitor TaskQ status never implemented; Langfuse dashboard provisioning excluded by D1. |

## Published reference

Approved editable [prototype](host-first-preview.html), [fixtures](host-first-fixture.json), [capture/check recipe](host-first-check.cjs) and [manifest](evidence/host-first/manifest.json). All hosts, versions, capacities, login/queue probes and findings are invented. Navigation covers Hosts, one detail and Tasks; auxiliary destinations explain the preview gap. No writes, live API or simulated success receipt.

TaskQ project/answer assets are consumed unchanged from [release provenance](../../assets/taskq/provenance.json), with [design-source hashes](../../assets/taskq/project-blocks.sources.json). Parent-to-child map: TaskqProjectBlocks.summary/task/handle → Tasks; TaskqAnswerCard.render → selected read-only question. Only outer placement/width differ. [Resource assets](../../assets/taskq/sources.json) remain pinned; no editor is redrawn. [Boards](boards.md#release-and-adapter-gaps) names missing full-workspace/control/source contracts. These captures do not prove B4 native/browser conformance.

Previous [fragment](first-screen-reference.html), [preview](first-screen-preview.html) and [manifest](first-screen-reference.sources.json) remain historical, not composition authority for this design. Chart semantics/provenance remain usable in detail; approximations are not a source for task controls.

## Review and acceptance

Five requested review captures (at most ten): Hosts 1280 light/dark, Hosts 360 dark, host detail, Tasks. The owner request explicitly includes theme/width variants. Matrix checks cover 16/20 px, both themes, 360/1280 widths, long labels, unknowns and reduced motion without publishing duplicate pictures. Inspect at natural size; geometry is not live-source evidence.

| Screen | Review question |
|---|---|
| [Hosts · 1280 light](evidence/host-first/hosts-1280-light.png) | Are host state, versions, sign-in, resources and counts readable together? |
| [Hosts · 1280 dark](evidence/host-first/hosts-1280-dark.png) | Are warning and unknown meanings legible in dark theme? |
| [Hosts · 360 dark](evidence/host-first/hosts-360-dark.png) | Does one host remain usable as labelled groups? |
| [Host detail](evidence/host-first/host-detail.png) | Are trends, actions and evidence available without crowding the overview? |
| [Tasks](evidence/host-first/tasks.png) | Are shared rows/answers and the full-workspace release gap explicit? |

Run `node docs/design/host-first-check.cjs` with Playwright on NODE_PATH and Chromium installed (or DASHQ_CHROME); add --capture to regenerate images. It checks routes/Back/reload, shared rendering, no writes, overflow and fixture meaning; manifest records hashes, environment and geometry. Documentation checks cover shrinkage/preservation/links/no Rust diff. No production gate changes.

A1–A5: specification, preservation mapping and five fixture screens **approved by the owner** in the linked answer under TaskQ § 7. Producer gaps stay unknown. Multiple actual refreshes, draft recovery, live online/login/assigned-queue sources, full shared controls, native tray and authorized actions are **not tested here**. Retain D11–D19 qualification; design approval does not establish implementation or production readiness.

## Runtime implementation

[#170 review package](host-first-runtime.md) compares the real renderer with these approved fixtures. The owner [approved the five runtime screens](https://github.com/taskqorg/dashq/issues/170#issuecomment-6319532742) at `eaabcc879ff51505d5ce29027e00c7558a80e034`; the PM owns the two named TaskQ handoffs. A1 live qualification and CI acceptance remain open; no production deployment is approved by this screen acceptance.
