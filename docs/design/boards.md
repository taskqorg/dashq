# Project boards

Candidate #169 places boards in on-demand Tasks under [host-first composition](first-screen.md); owner visual approval pending, running page unchanged. TaskQ owns interiors; historical dashboard previews do not override them.

Accepted block under [D10](../../dashq.md#d10-dashboard-redesign-target); apply [specq W13–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md#project-board-summaries). [Editable reference](boards-reference.html) contains explicitly synthetic tasks and counts.

Historical interaction fixture, not the current TaskQ visual authority. Reference SHA-256: `99c9b50c62e2bc059cfa08a1e65732b1648fb9e96f1055e964bdd3015bc32816`.

- Place boards in Tasks (/projects), directly or from a host count; retain host scope, board identity and explicit unassigned work. Aggregate/project counters keep TaskQ order/coverage. Actionable questions/failures in Attention reach the same task/card instance.
- Project summary, provider Board link, task rows and counters are compact children of [TaskQ shared blocks B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md). Reuse the parent's assets, tokens, state order and counter interactions. The earlier `panels-top-left`, five-counter order and visible zero slots are superseded.
- Keep the configured board URL beside project identity; never invent a GitHub Projects URL. Use TaskQ's canonical counters, including separate Doing/Review, nonzero Later and quiet Closed; unknown remains explicit. The parent owns aggregate coverage and accessible state names.
- A small composition bar is a DashQ supplement, not a replacement counter strip: reflect canonical open states in the same order on one labelled common scale. Closed is a separate quiet total, not part of the open-queue bar. Incomplete input stays marked; do not guess missing states. It is composition, never completion progress.
- A nonzero count reveals only that project's matching tasks below its row; keep other project summaries available. Question detail shows the actual question, stable choice numbers and recommendation; failure detail names the reason. Task state, number and title form a linked block pointing to the real issue.
- Accepted D13 adds ordinary answers through the [shared TaskQ card](answers.md); retrying or moving tasks remains outside this capability. Real board and issue links open their named sources; synthetic reference tasks have no live issue links or real submission controls. Loading, source failure, no tasks and a zero matching count remain distinct.
- Preserve the selected project/filter and expansion through refresh; keep row order stable. Reflow the counters on narrow screens and retain readable 16–20 px main text. Each board shows its source age; aggregate counts exclude unknown sources and state coverage.

## Acceptance

Match the combined composition and current TaskQ parent blocks; use the historical reference only for in-place disclosure behavior. Verify board links reach the configured Issues pages, icon links have keyboard focus and accessible names, and real issue links preserve identity. Cover doing/review, unanswered questions, failed tasks, dependency blocking, zero counts, unknown/stale sources and long titles in both themes and narrow/large-text layouts. Fetch real question text/options from the authoritative task log; do not infer choices from labels or manufacture missing details.

Opening an example question is demonstrative, not a requirement on refresh. #169 changes placement only; TaskQ tokens, spacing and controls remain inherited.

## Release and adapter gaps

Consume [TaskQ v4.6.27 B1–B4](https://github.com/taskqorg/taskq/blob/v4.6.27/docs/design/shared-blocks.md), [project-blocks API](https://github.com/taskqorg/taskq/blob/v4.6.27/docs/design/project-blocks.md), [Menu M3/M12](https://github.com/taskqorg/taskq/blob/v4.6.27/docs/design/menu.md) and answer/resource contracts through pinned releases. Current project/answer blocks: v4.4.0 (assets/taskq/provenance.json); resource/project-limit components: v4.6.25 (assets/taskq/sources.json). #169 neither repins nor edits vendor files. Reading a newer contract does not qualify adoption.

Published project-blocks supplies summary/aggregate/task/handle with navigation callbacks, not a complete tray workspace or general task-control dispatcher. The target is the same released TaskQ detail/menus/controls as the tray. TaskQ must supply versioned browser API, capabilities, operations/receipts, fixtures and published hashes. Until then use released rows/answers, expose issue links and name each unavailable control. No imitation Start/Stop/Later/Close/Retry menu, remote shell or second editor. Existing answers/policies keep D13/D14 gates; observation grants add none. Consequential actions retain explicit review.

Host counts need TaskQ's authoritative assignment/completeness contract. Heartbeats show attempts, not all pending allocations; global Ready is not every host's queue. Unassigned work stays separate; unsupported assignment is `?` with a reason. Claude/Codex sign-in needs an explicit host/runtime probe, time and validity (Menu M3 names local status.json probes, not an enrolled remote-login schema). Activity, routing and board freshness are no substitute. Online likewise needs qualified probe evidence; absence is unknown. These producer/adoption gaps require approved follow-up, not invented fields or new authority.
