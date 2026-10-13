# Project boards

Current combined visual authority: [published first-screen reference and pinned TaskQ sources](first-screen.md#published-reference). Older previews do not override it.

Accepted block under [D10](../../dashq.md#d10-dashboard-redesign-target); apply [specq W13–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md#project-board-summaries). [Editable reference](boards-reference.html) contains explicitly synthetic tasks and counts.

Historical interaction fixture, not the current TaskQ visual authority. Reference SHA-256: `99c9b50c62e2bc059cfa08a1e65732b1648fb9e96f1055e964bdd3015bc32816`.

- Place the block beside the server overview under [combined first screen](first-screen.md); stack on narrow views. Its aggregate strip inherits the same TaskQ state order and coverage rules as project strips. Important questions and failures also feed top-level Attention without repeating full task lists there.
- Project summary, provider Board link, task rows and counters are compact children of [TaskQ shared blocks B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md). Reuse the parent's assets, tokens, state order and counter interactions. The earlier `panels-top-left`, five-counter order and visible zero slots are superseded.
- Keep the configured board URL beside project identity; never invent a GitHub Projects URL. Use TaskQ's canonical counters, including separate Doing/Review, nonzero Later and quiet Closed; unknown remains explicit. The parent owns aggregate coverage and accessible state names.
- A small composition bar is a DashQ supplement, not a replacement counter strip: reflect canonical open states in the same order on one labelled common scale. Closed is a separate quiet total, not part of the open-queue bar. Incomplete input stays marked; do not guess missing states. It is composition, never completion progress.
- A nonzero count reveals only that project's matching tasks below its row; keep other project summaries available. Question detail shows the actual question, stable choice numbers and recommendation; failure detail names the reason. Task state, number and title form a linked block pointing to the real issue.
- Accepted D13 adds ordinary answers through the [shared TaskQ card](answers.md); retrying or moving tasks remains outside this capability. Real board and issue links open their named sources; synthetic reference tasks have no live issue links or real submission controls. Loading, source failure, no tasks and a zero matching count remain distinct.
- Preserve the selected project/filter and expansion through refresh; keep row order stable. Reflow the counters on narrow screens and retain readable 16–20 px main text. Each board shows its source age; aggregate counts exclude unknown sources and state coverage.

## Acceptance

Match the combined composition and current TaskQ parent blocks; use the historical reference only for in-place disclosure behavior. Verify board links reach the configured Issues pages, icon links have keyboard focus and accessible names, and real issue links preserve identity. Cover doing/review, unanswered questions, failed tasks, dependency blocking, zero counts, unknown/stale sources and long titles in both themes and narrow/large-text layouts. Fetch real question text/options from the authoritative task log; do not infer choices from labels or manufacture missing details.

Reference opening one example question is demonstrative, not a requirement to reopen it on every refresh. Combined placement is accepted in first-screen.md; this does not approve shrinking server charts or changing TaskQ block interiors to fit a viewport.
