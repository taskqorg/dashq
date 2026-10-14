# Prototype coverage review

Review: 2026-10-14. Scope: requested dashboard scenarios and D8–D19 preservation requirements. This index links existing authorities; it is not a second contract or proof of deployed behavior. “Shown” means an accepted synthetic reference exists. “Partial” means a material route/state is still absent or simplified.

| Scenario | Authority and accepted example | Coverage / remaining work |
|---|---|---|
| First-screen Attention, server CPU/RAM/filesystems, boards, right-aligned TaskQ counters | [First screen](first-screen.md), first-screen-reference | Shown; reference retains exact pinned TaskQ parents. Actual usable viewport and production assets still require F-A1–F-A5. |
| Bars, CPU/RAM trends, visible number updates, reduced motion | [Overview](overview.md), overview-reference | Shown in resource reference; not yet combined with all interactive routes in one prototype. |
| Board questions, numbered choices/recommendation, custom answer, single card | [Answers](answers.md), first-screen-reference | Normal flow shown. Changed/rejected/unknown receipts are parent fixtures, not all reachable in the combined screen. |
| Node limits, Auto/Fixed, scope and Work mode review | [Resource controls](resource-controls.md), first-screen-reference | Normal editor/review shown. Mixed/legacy values, pending/partial/rejected/unknown host readback and offline expiry lack a complete dashboard walkthrough. |
| ServerQ finding → project → task → check → recovery/recurrence | [H1–H5](finding-tasks.md), finding-task-reference and attention-recovery-reference | Shown, including task closure without verification, unknown checks and recurrence. Full arbitrary check inventory/manual rules remains a supporting-view gap. |
| Independent product enrollment, no contact, old data, revoke, offline revoke, reinstall | [L1–L4](connections.md), connections-reference | Shown. Full expiry/denial/cancellation and race traces remain E1–E10 qualification, not all visual states. |
| First launch, approval waiting, TaskQ-only, warmup, partial/all outage, recovery | [U1–U4](startup-outages.md), startup-outages-reference | Shown. Fresh empty board and first-ever failed source require explicit fixture/qualification; scenarios use a demo switch, not integrated navigation. |
| AgentQ clients/profiles, routing, MCP/hooks, optimization configuration | [G1–G4](agentq-settings.md), agentq-settings-reference | Main review/configured/verified distinction and access/stale/conflict shown. Full multi-target late/partial outcomes remain qualification. |
| Optimization calls, passes, errors, paired bytes and actual-effect evidence | [P1–P3](optimization-results.md), optimization-results-reference | Shown, including missing/empty/partial and separate paired evidence. Does not replace model activity/usage. |
| Service versions, pending activation, checksum failure, verified rollback, safe diagnostics | [V1–V4](service-versions.md), service-versions-reference | Shown; observation-only. Actual update execution is not a dashboard capability. |
| Model/runtime activity, reported usage/cost, host/agent/session filters | D8.4/D9 | Partial: optimization view does not cover this retained ServiceQ information. Needs a dedicated demo with missing usage fields and trace links. |
| Per-filesystem history and deeper server-check/history browsing | D10, [overview](overview.md), H1–H5 | Partial: current disk bars and one incident journey do not cover a history explorer or complete manual/unknown check list. |
| One application route, context and return across all supporting views | [F-A4](first-screen.md), H5, L3, G4 | Partial: independent references exist; only the Attention-to-recovery journey connects its full route. A single integrated navigation/focus/draft review remains. |

## Next prototype work

1. Recommended: one server-detail route with filesystem history, full check list, manual/unknown checks and return to the selected row. Reuse H5's incident/task card.
2. Model activity and reported usage with host/agent/session filters; preserve D8/D9 and keep optimization accounting separate.
3. TaskQ-owned failure/readback variants for answers and Work mode, then one integrated navigation and refresh walkthrough using those same components.

These are existing scope gaps, not approved new layouts. Design each before accepting its reference. Implementation owners retain their existing delivery tasks; no live qualification gate is removed by prototype approval. No assertion that all planned scenarios are complete.
