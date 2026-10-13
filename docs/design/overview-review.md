# Overview review

Review of the side-by-side exploration from the design conversation. Status: changes required; not a replacement for the accepted [overview](overview.md) or [boards](boards.md). Apply [operator review](https://github.com/taskqorg/specq/blob/main/design/operator-ui.md#review-each-change).

| Finding | Rule | Required correction |
|---|---|---|
| Attention repeats question/failure counts in its heading, contents and board totals | D4, W2 | One attention summary; detail beside the affected entity. Keep necessary warning context, not repeated prose |
| CPU/RAM and sample wording repeat for every host | D2, D4, W10 | Shared metric headers at wide sizes; row-local short labels only after reflow makes scope ambiguous |
| Footer explains ordinary interaction and state meanings at length | D5, W7 | Keep unfamiliar legend once; move diagnostics/explanations into accessible detail |
| Symbols duplicate text without replacing any meaning | D10, D11 | Stable semantic counter symbols and one legend; preserve accessible names and exact values |
| About 855 px content height was reported without accounting for application chrome | F2, W1 | Measure against the actual usable viewport; do not claim first-screen fit from content height alone |
| An open answer is inserted into the board list and can shift other projects | F3, W4 | Review expanded state, stable focus, draft retention and refresh, not only the closed overview |
| Preview receipts and recommendations look plausible but have no producer | D8, F1 | Keep explicit demo provenance; production uses source choices and verified receipts |

Next reference must preserve the same synthetic values, all four hosts, every filesystem bar, CPU/RAM trends, six boards and readable 16/20 px text. A single demo marker covers the whole preview; ordinary data must retain units and observation age.

Review the full composition in both themes, narrow reflow, long names, unknown/stale sources, open questions and large text. Pair every removed label with the structural cue that replaces it. Reject an icon-only replacement when meaning requires guessing. No new screenshot or runtime qualification is claimed by this document.
