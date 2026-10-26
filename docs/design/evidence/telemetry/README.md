# Telemetry view (#222)

Runtime fixture evidence for [D11/D22/D23](../../../../dashq.md), [DQ-09/DQ-10 and T3/T4/T7/T8/T9](../../operator-redesign/scenarios-and-data.md). Synthetic metadata only; no production collection, configuration, credentials or payload content changed. [manifest.json](manifest.json) pins normalized source/asset and rendered fixture hashes, browser, platform and matrix. The enclosing commit pins this package. [delivery.png](delivery.png) is the Windows Chrome output, 1280 × 1000 viewport, 16 px system text, light theme, reduced motion; the full-page image includes three sender scenarios.

## Navigation and preserved behavior

| Requirement | Behavior / evidence |
|---|---|
| A1, D23 | `/telemetry` renders Emitted, Queued, Accepted, Projected, Exported and Queryable in order for each scoped installation/workspace. Each has its own state, value, evidence time and reason. Unknown uses a dashed outline; known false says No; no stage implies an all-event pass. Route projection and Connections evidence stay reachable. |
| A2, D11 | Separate coverage groups show receipt-ordered retained endpoints/times, sender counters, unreceived holes (listed/total), restarts and capacity/retention/allowlist loss. Replaced sender stores do not become continuous seq intervals. |
| A2, D11 | A disclosure lists the newest 200 retained block descriptors with total count. Every listed block links to `/data?block=ID` and its installation row. Existing metadata allowlists, replay, pruning, restore and block readback remain unchanged. |
| A3, D22 | More → Telemetry, direct entry, reload, Back and Connections links retain period/host/agent/session. Refresh retains disclosure, focus and reading position. Empty, unavailable and all-pruned observations remain explicit. |
| A3, D7/D23 | One scoped projection supplies the renderer, `/data` and `dashq report`. The store adds bounded metadata descriptors only; no new collector, table or content rendering. Existing role/workspace scope remains authoritative. |

## Checks and before/after

- R12/R13, Testing § Run policy: before implementation, `cargo test --locked telemetry_view` failed `#222: missing dedicated Telemetry view`. After implementation the same check passes.
- The intended D22 navigation and empty Telemetry markup changed the pinned page. `fixture_rendering_is_pinned` failed before regeneration and passes after updating `fixture/page.html`; its exact-render assertion and `selftest` comparison remain in place (Testing § Before adding or changing a test, step 4).
- Focused development gate: `cargo test --locked telemetry_` passes. Retained risks include full-store non-acknowledgement, per-installation budgets, migration/rollback/restore and allowlist rejection. Added protection covers descriptor cap/order/scope, payload absence, unknown versus false stages, and page/data/report agreement without browser authority.
- Browser defect before: the large-text matrix failed `steps reflow without smaller type`, observing six columns instead of reflow. After: content-sized columns reflow with text size; the same large-text case passes without relaxing text-size or overflow protection.
- `node tests/telemetry-browser.cjs` passes six cells: 1280/16, 360/20, 1280/24 × light/dark. Each checks stage values/reasons, known zero versus unknown, coverage/loss, cap disclosure, evidence URLs/filters, direct entry/reload/Back, stale installation reveal, automatic refresh preservation, outage/recovery, empty/pruned states, no horizontal overflow, no JavaScript errors and no writes.
- The existing GitHub tests workflow generates the fixture during its Rust gate and runs the browser chain using its pinned Playwright installation on PR and merge-group candidates.

## Independent review

Claude Opus, high effort, reviewed task fit and implementation quality with read-only tools, including the desktop picture. Its blocking finding was the stale pinned fixture; regeneration and the unchanged exact-render check resolve it. Its non-blocking observations led to explicit unknown holes/restarts without retained blocks, a neutral newest-block time label, documented nested coverage metadata and a real More → Telemetry browser journey using the complete production asset bundle. The empty-coverage regression failed `#222: absent retained sequence coverage cannot claim zero gaps` before the fix and passes after it, retaining the store's known zero counts while preventing a misleading UI claim. No required protection was removed.

The focused follow-up review reports task fit met, implementation quality good and no remaining binding finding or regression in the requested fixes. Claude did not execute checks; automated evidence is separate. The existing `startup-browser.cjs` and `workspace-browser.cjs` checks also pass, retaining outage/recovery, navigation, keyboard and local draft protection. The reviewer noted that an older general page fixture omits telemetry on one TaskQ row; real enrolled rows always carry its explicit state. This fixture-only omission is not production evidence.

Reproduce with `DASHQ_TELEMETRY_EVIDENCE` set to a scratch directory: run `cargo test --locked telemetry_view`, then `node tests/telemetry-browser.cjs`. `DASHQ_CHROME` optionally selects Chrome; CI uses Playwright Chromium. `--capture` refreshes the one review picture and manifest.

Production telemetry, deployment and owner visual acceptance remain unqualified. CI acceptance follows R12 and TaskQ § 7; local checks alone do not establish merge-group acceptance.
