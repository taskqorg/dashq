# Service adapters and owner answers

Design proposal for [dashq #24](https://github.com/taskqorg/dashq/issues/24), for owner review. It does not amend [D1–D7](../../dashq.md), add a write endpoint, change auth, tokens or deployment. Shared contract: [coreq dashboard contract](https://github.com/taskqorg/coreq/blob/056df1b466c21c7e30b974df83107a7eb3613595/docs/design/dashboard-contract.md) (coreq #22). Findings: [serverq dashboard findings](https://github.com/taskqorg/serverq/blob/74a7e97c3d695ce8eebf694143daf2e0ce4f66e4/docs/design/dashboard-findings.md) (serverq #21). Connection: [enrollment C1–C7](enrollment.md). Product boundaries: [integration](integration.md).

Reviewed revisions (contract and source reading, not deployment evidence): taskq [`21bf7e5`](https://github.com/taskqorg/taskq/tree/21bf7e52f88f85916bc195b78a4786fa7aff64a5), agentq [`17d6a06`](https://github.com/taskqorg/agentq/tree/17d6a06b93e8f784ab18c139938be5a510209c3a), serverq [`74a7e97`](https://github.com/taskqorg/serverq/tree/74a7e97c3d695ce8eebf694143daf2e0ce4f66e4), coreq [`056df1b`](https://github.com/taskqorg/coreq/tree/056df1b466c21c7e30b974df83107a7eb3613595), specq [`4a1d088`](https://github.com/taskqorg/specq/tree/4a1d088b1bcd955df43157e3f7f9e6e8fc19cd7b), dashq [`f9f8413`](https://github.com/taskqorg/dashq/tree/f9f84139a944d600d8f3b7713842d61f660b357d).

## S1. Field map

Status: **shipped** (dashq reads and renders it today), **stale** (rendered, but known to lag or fail in production), **missing** (no producer publishes it), **proposed** (in an unapproved design). Targets: Hosts, Attention, Boards, Detail (service detail below the overview).

| Producer field | Target | Status | Note |
|---|---|---|---|
| taskq `taskq-host` heartbeat `version`, `#N <attempt>`, `updated_at` | Hosts, Boards | stale | D4.2; live reads fail with HTTP 401 until [#21](https://github.com/taskqorg/dashq/issues/21) |
| taskq `q-<state>`, `p0-2` labels | Boards, Attention | stale | Shipped counts lag the log by one service pass; 401 as above |
| taskq snapshot `observed`, `complete`, `reason`, `totals` | Boards coverage | proposed | `totals` null when incomplete stays unknown, never zero |
| taskq snapshot task `card`, `revision`, `state` | Boards detail, Attention | proposed | Real question text; today dashq shows the title only |
| taskq snapshot task `failed` | Boards detail, Attention | proposed | Failure reason; today title only |
| taskq snapshot `hosts[]`, `capabilities` | Hosts, view selection | proposed | Same heartbeat data, already folded by taskq |
| taskq `.taskq/status.json` `capacity`, `agents`, `focus` | Detail | missing | Local file only; export needs the C2/C7 projection |
| taskq installation id | Hosts join | missing | coreq decision 1 (`install` key) |
| agentq `agentq.installation` heartbeat: installation, version, runtime, `host.name`, queue counters | Hosts, Detail | shipped | D4.1; stale after 7200 s |
| agentq `agentq.run`, `agentq.retrieval` | Detail (Agents) | shipped | Usage per period |
| agentq `agentq.incident` | Attention history | shipped | Deduplicated, 24 h |
| agentq `agentq.resource` CPU, RAM, filesystems (HMAC id), `*_unknown` | Hosts | proposed | [#23](https://github.com/taskqorg/dashq/issues/23), gated by [agentq #86](https://github.com/taskqorg/agentq/issues/86) |
| agentq legacy Node monitor snapshots | Hosts services | stale | Until the last Node host is gone (agentq #50) |
| agentq `status.json` `mcp`, `hooks`, engine versions | Detail | missing | Promised by agentq.md, not written |
| serverq `report.json` `results`, `incidents`, `manual_rules`, `at` | Attention, Detail (checks) | shipped | One configured host, joined by a configured host id |
| serverq `installation`, `rules_sha256`, `observed_at`, `reason: probe-failed` | Detail, Hosts join | proposed | serverq follow-up P1 |
| serverq `serverq-findings/1` projection | Attention, Detail | proposed | serverq follow-up P2; required for any remote host |
| serverq recommendation text | Detail | missing | Only in `rules.json` `instruction`; resolve by `{rule, rules_sha256, version}` |
| coreq `status.json` common keys | Hosts | missing | Local only; raw export forbidden (paths, error text) |
| GitHub `releases/latest` per product | Hosts `behind` | shipped | 10 min cache |

## S2. Adapters

Each adapter reads one existing producer interface and returns the coreq source frame (`service`, `installation`, `schema`, `observed`, `collected`, `fresh_s`, `state`, `complete`, `reason`, `capabilities`, `unknown[]`, `payload` verbatim). The frame is a dashq type; producers change nothing for it.

| Adapter | Reads | Capabilities | Status |
|---|---|---|---|
| `taskq-labels` | GitHub issues and heartbeats (D4.2) | `board-counts`, `attempts` | shipped; fallback while no snapshot |
| `taskq-snapshot` | `taskq status --json` per board (S3) | `board-counts`, `questions`, `failures`, `attempts` | proposed |
| `agentq-langfuse` | Langfuse events (D4.1) | `fleet`, `activity`, `incidents`, later `resources` | shipped, `resources` proposed |
| `agentq-ingest` | enrollment snapshot and samples (C2, C4) | same as above | proposed |
| `serverq-report` | local `report.json` (D4.3) | `findings`, `incidents` | shipped |
| `serverq-findings` | `serverq-findings/1` through enrollment | `findings`, `incidents` | proposed |

Composition (integration I1–I7, A7):

- **Independent producers.** Each view needs only its capability. TaskQ alone shows boards and questions; ServerQ alone shows checks; AgentQ alone shows fleet and resources. No placeholder panel for an absent product.
- **Optional connections.** Per source: not configured (hidden, listed once in the source legend), disconnected or revoked (last observation kept with age, not an active failure), unsupported capability (field in `unknown[]`), stale observation, failed read (D6 `unconfirmed`).
- **Verified joins.** Rows join only through an owner-made `installation → host` mapping (C3). Equal `host.name` values are display text. Native Windows and WSL are separate installations. Without a mapping, each installation keeps its own row; no resource is counted twice.
- **Mixed versions.** A missing additive key is an `unknown[]` entry; the view stays. An unknown schema major is `unsupported` with the last good frame. No fleet-wide update is required.
- **Disconnects.** Removing dashq or one source stops nothing on a host. Another source's views remain complete.
- **Freshness.** `observed` against the producer's own limit: taskq heartbeat `ttl`, agentq heartbeat 7200 s and samples 900 s, serverq `stale_after`. A working connection never refreshes an old observation.

## S3. TaskQ snapshots and questions

dashq never folds the taskq log (integration: do not rebuild the state machine). Two read paths deliver the taskq#886 snapshot:

- **R1 (recommended now).** dashq runs the pinned `taskq status --json` binary per configured board with its existing read-only GitHub token, under the same single flight, 10 s timeout and 60 s cache as D4.2. No host push, no new credential. Qualification: taskq reads a board without a project checkout and with a read-only token; otherwise R2 only.
- **R2 (later).** taskq services deliver the same snapshot through enrollment (C2 TaskQ row). Same frame, same question shape.

Question shape per coreq § 3: `{id: issue url, revision, text, choices[{k, text}], recommended, free_text}`.

- Choices come from the card with taskq's own rule (`K. text` lines, `★` = recommendation), or from parsed fields if taskq adopts coreq decision 2. A card that does not parse renders verbatim with no answer control.
- Keep the source `k`, order and text; never renumber, reorder, reword or merge choices. Free text stays free text, never mapped to a number.
- `review` cards (result decisions) use the same shape; their state remains `review`.
- `revision` is the task revision from the same snapshot; it is the expected revision of any answer (S4).

## S4. Accepted answer action

The owner accepted reuse of the TaskQ card and ordinary real answers. [D13](../../dashq.md#d13-taskq-answer-card) and [answers](answers.md) now own this contract; the former S4 proposal is superseded. Shared presentation belongs to TaskQ. Observation enrollment remains separate. Other proposals on this research page are not implicitly approved.

## S5. Consequential operations (A4)

- An `accepted` receipt proves one board entry. A worker resume shows only as a later observation (state `doing`, heartbeat attempt). A repair shows only as a later passing check.
- Close, requeue, later, failed, start, capacity, permissions, serverq remediation and agentq configuration each need their own reviewed contract with actor, effects and readback. This route does not grow into them.
- A serverq warning or manual rule is a recommendation. "Create task" now follows accepted D15 and [finding tasks](finding-tasks.md), using a separate TaskQ action capability.

## S6. Read paths and credentials (A5)

| Path | Credential | Carries | Fit |
|---|---|---|---|
| Today: Langfuse API, GitHub, local `report.json` | Langfuse keys, read-only GitHub token, file mode | agentq events, labels/heartbeats, one serverq host | Keep; all server-side (D2) |
| [#4](https://github.com/taskqorg/dashq/issues/4) per-host `status.json` push | per-host token | raw coreq status | Replaced by C7 projections: raw status leaks paths and error text |
| [agentq #60](https://github.com/taskqorg/agentq/issues/60) device-token OTLP forward | device credential | agentq telemetry | Part of enrollment C2 |
| Enrollment C1–C7 | per-installation credential verifier | snapshots, samples, findings | One path for #4 and #60 |
| Proposed answer route (S4) | guard identity + server-side write token | one answer entry | Separate from every machine credential |

Machine telemetry and owner authority stay apart: a device, telemetry or read credential never reaches `POST /answer`, and the answer route never accepts ingest credentials. Langfuse keys, GitHub tokens and device verifiers never reach the browser, `/data`, logs or `report`.

## S7. Display (A8)

Follow [overview review](overview-review.md) and [boards](boards.md):

- One question block per task under the board's shared headings: state, `#N` title link, question text. Choices as a numbered list with the source `k`; `★` with accessible name `recommended` replaces a "Recommended" word.
- Receipt state is one word with an icon: `✓ Accepted`, `↻ Changed, re-read`, `? Not confirmed`, `✕ Refused` plus the producer reason. No prose about resuming.
- Source age once per board in the shared header; one missing-data legend covers not configured, unsupported and stale. Units, ages and uncertainty stay; 16/20 px text and the accepted graphs are unchanged.
- Drafts, focus and open questions survive the 60 s refresh (W4).

## S8. Links and remaining work (A6)

- [#21](https://github.com/taskqorg/dashq/issues/21): live board reads fail (HTTP 401). Blocks verification of both taskq adapters.
- [#22](https://github.com/taskqorg/dashq/issues/22) (closed): deployed agentq fleet changes; the `agentq-langfuse` adapter is its shipped baseline.
- [#23](https://github.com/taskqorg/dashq/issues/23): resource overview and board details. Its question details use the S3 shape; resources come from `agentq-langfuse`.
- [agentq #80](https://github.com/taskqorg/agentq/issues/80) (closed): fleet telemetry verified behind stale snapshots. [#85](https://github.com/taskqorg/agentq/issues/85) (closed): Windows/WSL probes. [#86](https://github.com/taskqorg/agentq/issues/86): live history gate for #23.
- Covered elsewhere, no new task: enrollment batches (C7, #4, agentq #60), serverq P1/P2 (serverq #21 follow-ups), coreq `install` key (coreq decision 1).

Cross-repository approvals (coreq decisions 1, 2, 4; serverq follow-ups) gate the work below; they are not TaskQ dependencies. Proposed tasks, filed by the PM after owner approval:

1. dashq: `taskq-snapshot` adapter (R1), questions and failure reasons in `/data` and Boards, no answer control. Depends on #21.
2. dashq: `serverq-findings` adapter over the local projection and the owner mapping. After serverq P2.
3. dashq: answer route per S4, after D1–D4/D7 approval. Depends on task 1.

Remaining deployment input: the authorized forge account and concrete minimum supported scope. S4 is accepted through D13/answers.md; R1/R2 source qualification remains separate.
