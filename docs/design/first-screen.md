# Combined first screen

Accepted composition under [D10](../../dashq.md#d10-dashboard-redesign-target): Attention above servers and project boards side by side, with TaskQ resource controls reached from each node. Approval covers placement; the demo's approximated TaskQ styling and simplified controls are not a replacement design.

## Published reference

Use [the accepted fragment](first-screen-reference.html) or [the standalone preview](first-screen-preview.html). [Source manifest](first-screen-reference.sources.json) pins TaskQ design commit `75757d5d843159726a2ea44ca1615f9cf7fd18fd`, component source paths and SHA-256 hashes. This is a design revision, not a claim that shared production assets are released.

| Block | Exact parent at the pinned revision |
|---|---|
| Aggregate/project counters | `docs/design/menu-next-variants.html`: `counts`, parent counter CSS; K-A8 right alignment, including wrapped lines |
| Work mode | Same source: `resourceEditor`, `work`, `scopeReview`; full review content, not the old two-field form |
| Answer card | `docs/design/plans-and-decisions.html`: `question` and its CSS; source-numbered choices, Other answer and Checked & context |
| Board identity and appearance | `docs/design/assets/github.svg`, `visual-style.md`; icon names supplied by parent functions |

The five parent render functions are included unchanged. The manifest names the fixture/placement adapters. All counters align right; capsule gaps and source order stay unchanged. Supersedes prior combined chat demos and the old board/answer mocks as the visual integration reference. Earlier resource files remain only chart/data examples. Implementers must use the pinned reference and qualify later parent revisions explicitly.

The preview keeps simulated actions and parent-fixture limitations visible: fixed selected projects, example scope interpretation and receipts do not establish working production controls. Preserve B1-B4 and D13/D14 live-action gates. A missing capability is not permission to copy the simulation into production.

Verification: browser rendering and right-edge checks at 1024 px/16 px light and 360 px/20 px dark, plus TaskQ parent counter navigation/zero/unknown checks. Natural-size images inspected. Native tray and real source/action integration are not qualified by this publication.

## Layout

```text
Header and source freshness
Attention: current problems and observation gaps
Servers                             Boards
  identity + TaskQ policy summary     TaskQ aggregate/project summaries
  CPU trend | RAM bar/trend | disks   TaskQ task rows and inline answer
  one row per installation/runtime    question and failure details
  shared Work mode detail on demand
Supporting views, history and diagnostics
```

- At the 1024 px reference width, use roughly three fifths for servers and two fifths for boards. Tune available width from the parent component's fit; do not force fixed columns through large text or long names.
- Keep all four fixture nodes, six project identities, CPU/RAM trends and every filesystem visible in the overview. No duplicate permanent node list for limits. Policy summary stays with its stable installation/runtime identity and opens that target's shared editor.
- Attention stays above both columns. Its task links reveal the existing card in Boards; no second answer form. Unknown sources remain explicit and never contribute invented zeros to totals.
- Default: one representative question expanded for review; Work mode closed until requested. Preserve the user's actual disclosures on refresh. A full editor/review can expand into the available page width, with the target and return route intact; it need not fit inside the narrow server column.
- Default main text is 16 px, with 20 px supported. A first-screen target is not a fixed height or a reason to shrink text, charts or controls. Reflow to one column when needed; allow normal page scrolling for large text, more nodes or expanded details. Measure the usable browser viewport including chrome, not a full-document screenshot.
- Keep resource window/scale, units, source age, warnings and missing-data reasons legible. [Overview](overview.md) owns the chart/update requirements. Deeper board, agent, version, check, history and diagnostic routes remain available.

## TaskQ children

Use [TaskQ shared-blocks B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md) for project summaries, counters, task/answer rows and node/Work mode controls. DashQ owns placement, available width and adapters; TaskQ owns block interiors and compact variants. Pin and verify the parent design revision plus released component/assets and fixtures.

| Demo discrepancy | Required inherited result |
|---|---|
| Generic board-shaped icon used as Board | TaskQ provider Board asset and configured link; `panels-top-left` remains detach |
| Work total, fixed five-state strip and zero slots | Canonical TaskQ state capsules/order, separate Doing/Review, hidden zero, explicit unknown, Later and quiet Closed |
| Plain colored numbers and approximate theme values | Parent capsule treatment, exact theme/type/spacing tokens and assets |
| Simplified question body and synthetic nonlinks | Parent linked task flow, original Done/Problem/evidence, exact choices and receipt states |
| Independent Auto/Fixed + maximum form | Compact entry to the complete shared Work mode editor, explicit scopes, Mixed, overrides, review and readback |
| Demo reply immediately subtracts a count | Receipt stays local; counters change only with the canonical source snapshot |
| Missing freshness and reduced unit labels | Producer source age and units remain visible; admission measurements and resource history stay distinct |

This supersedes conflicting placement/icon/counter instructions in the earlier board and answer mock references. Those fixtures remain historical examples of charts or interaction, not pixel authorities for TaskQ children. The source parent is selected by TaskQ's current package index; do not edit generated workspace output or redraw its icons.

## Acceptance

- F-A1: the normal four-node/six-board fixture at 1024 px/16 px includes both columns, one open answer, every resource chart/filesystem and usable node-policy entries; report actual viewport dimensions and any overflow.
- F-A2: each embedded block records its TaskQ parent/variant/version; compare identical fixtures with the parent. No unapproved icon, token, order, target or state differences.
- F-A3: 20 px, both themes, narrow width, long identities/titles and unknown sources retain readable data and reachable actions. Expanded Work mode/review and final controls remain reachable.
- F-A4: automatic refresh retains row identity, scroll, disclosure, selection and typed drafts. Attention and source rows agree; answers/limits use their existing capability and readback gates.
- F-A5: fixture data/actions are clearly marked. Live views contain no demo fallback, invented threshold or simulated receipt. Rendering evidence does not prove real writes or deployment.

Implementation: #23 owns composition, #43 answers/board children, #46 node controls. #44 and #48 retain actual action qualification. Observation-only layout does not wait for write authorization; unsupported controls keep their explicit reason and source route.

ServerQ detail and task handoff follow [H1–H4](finding-tasks.md); Attention, finding and Boards share one task editor. The first-screen layout is unchanged.

Connection detail uses [L1–L4](connections.md); source freshness and enrollment state remain distinct in the overview.

AgentQ integration detail follows [G1–G4](agentq-settings.md); configuration, host readback and client verification stay distinct. TaskQ Work mode remains the separate task/admission owner.

Supporting AgentQ activity view: [optimization results P1–P3](optimization-results.md); it does not displace first-screen resources, Attention or Boards.

Service versions and diagnostics are a supporting view under [D18 / V1–V4](service-versions.md), preserving the first-screen operational layout.
