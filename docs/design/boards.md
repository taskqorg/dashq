# Project boards

Accepted block under [D10](../../dashq.md#d10-dashboard-redesign-target); apply [specq W13–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md#project-board-summaries). [Editable reference](boards-reference.html) contains explicitly synthetic tasks and counts.

Reference SHA-256: `99c9b50c62e2bc059cfa08a1e65732b1648fb9e96f1055e964bdd3015bc32816`.

- Place the block immediately after the server overview. Its heading summarizes in-work, owner-answer and failed counts plus unavailable-source coverage. Important questions and failures also feed top-level Attention without repeating full task lists there.
- Use one aligned row per configured project. A board icon beside its name links to the configured board URL, currently that repository's GitHub Issues page; never invent a GitHub Projects URL. Use Lucide `panels-top-left` with an accessible name such as `Open taskq board`.
- Counter order: In work (`doing` + `review`), Your answer (`ask`), Failed (`failed`), Ready (`ready`), Blocked (`waiting`). Work uses restrained green, answers amber, failures red; remaining states are neutral. Review detail retains its own state and never implies success.
- The small segmented bar below the project name follows counter order on one scale based on the largest displayed sum. It is queue composition, not progress; later and closed are excluded and reachable through the board link. Label the scale. Show zero as zero; failed or stale reads retain D6 semantics.
- A nonzero count reveals only that project's matching tasks below its row; keep other project summaries available. Question detail shows the actual question, stable choice numbers and recommendation; failure detail names the reason. Task state, number and title form a linked block pointing to the real issue.
- No answering, retrying or moving tasks inside dashq (D1). Real board and issue links open their named sources; synthetic reference tasks have no issue links or submission controls. Loading, source failure, no tasks and a zero matching count remain distinct.
- Preserve the selected project/filter and expansion through refresh; keep row order stable. Reflow the counters on narrow screens and retain readable 16–20 px main text. Each board shows its source age; aggregate counts exclude unknown sources and state coverage.

## Acceptance

Match the reference's rows, counter alignment and in-place task disclosure. Verify board links reach the configured Issues pages, icon links have keyboard focus and accessible names, and real issue links preserve identity. Cover doing/review, unanswered questions, failed tasks, dependency blocking, zero counts, unknown/stale sources and long titles in both themes and narrow/large-text layouts. Fetch real question text/options from the authoritative task log; do not infer choices from labels or manufacture missing details.

Reference opening one example question is demonstrative, not a requirement to reopen it on every refresh. Combined page composition remains subject to owner review; this block's acceptance does not approve shrinking server charts to fit a viewport.
