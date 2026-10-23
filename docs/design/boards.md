# Project boards

Approved #169 design places boards in on-demand Tasks under [host-first composition and owner acceptance](first-screen.md); runtime status and qualification: [#170](host-first-runtime.md). TaskQ owns interiors; historical dashboard previews do not override them.

Accepted block under [D10](../../dashq.md#d10-dashboard-redesign-target); apply [specq W13–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md#project-board-summaries). [Editable reference](boards-reference.html) contains explicitly synthetic tasks and counts.

Historical interaction fixture, not the current TaskQ visual authority. Reference SHA-256: `99c9b50c62e2bc059cfa08a1e65732b1648fb9e96f1055e964bdd3015bc32816`.

- Place boards in Tasks (/projects), directly or from a host count; retain host scope, board identity and explicit unassigned work. Aggregate/project counters keep TaskQ order/coverage. Actionable questions/failures in Attention reach the same task/card instance.
- Project summary, provider Board link, task rows and counters are compact children of [TaskQ shared blocks B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md). Reuse the parent's assets, tokens, state order and counter interactions. The earlier `panels-top-left`, five-counter order and visible zero slots are superseded.
- Keep the configured board URL beside project identity; never invent a GitHub Projects URL. Use TaskQ's canonical counters, including separate Doing/Review and nonzero Later; Closed coverage stays a board link; unknown remains explicit. The parent owns aggregate coverage and accessible state names.
- A small composition bar is a DashQ supplement, not a replacement counter strip: reflect canonical open states in the same order on one labelled common scale. Closed stays a separate board link, not part of the open-queue bar. Incomplete input stays marked; do not guess missing states. It is composition, never completion progress.
- A nonzero count reveals only that project's matching tasks below its row; keep other project summaries available. Question detail shows the actual question, stable choice numbers and recommendation; failure detail names the reason. Task state, number and title form a linked block pointing to the real issue.
- Accepted D13 adds ordinary answers through the [shared TaskQ card](answers.md); retrying or moving tasks remains outside this capability. Real board and issue links open their named sources; synthetic reference tasks have no live issue links or real submission controls. Loading, source failure, no tasks and a zero matching count remain distinct.
- Preserve the selected project/filter and expansion through refresh; keep row order stable. Reflow the counters on narrow screens and retain readable 16–20 px main text. Each board shows its source age; aggregate counts exclude unknown sources and state coverage.

## Acceptance

Match the combined composition and current TaskQ parent blocks; use the historical reference only for in-place disclosure behavior. Verify board links reach the configured Issues pages, icon links have keyboard focus and accessible names, and real issue links preserve identity. Cover doing/review, unanswered questions, failed tasks, dependency blocking, zero counts, unknown/stale sources and long titles in both themes and narrow/large-text layouts. Fetch real question text/options from the authoritative task log; do not infer choices from labels or manufacture missing details.

Opening an example question is demonstrative, not a requirement on refresh. #169 changes placement only; TaskQ tokens, spacing and controls remain inherited.

## Release and adapter gaps

Consume [TaskQ v4.6.30 B1–B4](https://github.com/taskqorg/taskq/blob/v4.6.30/docs/design/shared-blocks.md), [project-blocks API](https://github.com/taskqorg/taskq/blob/v4.6.30/docs/design/project-blocks.md), [Menu M3/M12](https://github.com/taskqorg/taskq/blob/v4.6.30/docs/design/menu.md) and answer/resource contracts through pinned releases. Current project/answer/task-control blocks: v4.6.30 (assets/taskq/provenance.json); resource/project-limit components: v4.6.25 (assets/taskq/sources.json). #169 neither repins nor edits vendor files. Reading a newer contract does not qualify adoption.

Published project-blocks supplies summary/aggregate/task/handle with navigation callbacks, not a complete tray workspace or general task-control dispatcher. The target is the same released TaskQ detail/menus/controls as the tray. TaskQ must supply versioned browser API, capabilities, operations/receipts, fixtures and published hashes. The released task-control component and fixed command table are adopted behind the separate [control gate](task-controls.md); the full workspace remains unavailable. No imitation workspace, Close operation, remote shell or second editor. Existing answers/policies keep D13/D14 gates; observation grants add none. Consequential actions retain explicit review.

Release readback for #186 (#203, [TaskQ v4.6.33](https://github.com/taskqorg/taskq/releases/tag/v4.6.33), 2026-10-23): the PM composer from taskq#1615 is published with its fixtures and source manifest, each listed in `SHA256SUMS` (SHA-256):

- `taskq-4.6.33-pm-composer.js` `8718133305abe624efa57aebac2b4d734ec60fd45d833a7f4df5ec368d4682f8`
- `taskq-4.6.33-pm-composer.css` `10e0263ce874eb0a5f9570e1aa8e99323a52cb2d28d2054d25631d357f0320fb`
- `taskq-4.6.33-pm-composer-fixtures.json` `0f9b25c1a6abbd7aa7b0fc5e8f1551933fc9f7e73c41720510116fd4740d8e78`
- `taskq-4.6.33-pm-composer-sources.json` `98a7ceb388e0c1da4be285d93541ce82a5d1fbeb92f157d6d36e231969db0ca5` (component `pm-composer` version 1, schema `taskq.pm/1`)

v4.6.33 publishes no workspace, detail or menu browser asset; its browser components are project-blocks, answer card, task controls, resource controls, finding composer and PM composer. #186 A2 therefore stays an explicit adoption gap until TaskQ releases them. Nothing here is live-board qualified (#195).

Host counts need TaskQ's authoritative assignment/completeness contract. Heartbeats show attempts, not all pending allocations; global Ready is not every host's queue. Unassigned work stays separate; unsupported assignment is `?` with a reason. Claude/Codex sign-in needs an explicit host/runtime probe, time and validity (Menu M3 names local status.json probes, not an enrolled remote-login schema). Activity, routing and board freshness are no substitute. Online likewise needs qualified probe evidence; absence is unknown. These producer/adoption gaps require approved follow-up, not invented fields or new authority.

## Task controls (#179)

[Task controls](task-controls.md) pins the released v4.6.30 project blocks, answer card and control renderer unchanged. It replaces the earlier control-release gap with a separately gated adapter; production writes still wait for owner-run #195. Closed coverage stays an explicit board link because this released renderer omits its counter. A reusable full tray workspace remains a separate adoption gap.
