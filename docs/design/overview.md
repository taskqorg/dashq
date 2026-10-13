# Accepted dashboard overview

Owner-approved layout under [D10](../../dashq.md#d10-dashboard-redesign-target). Shared visual behavior lives in [specq W1–W15](https://github.com/taskqorg/specq/blob/main/design/dashboards.md); do not duplicate it here.

## Reference

- Editable [reference fragment](overview-reference.html): accepted dashboard-bars-demo revision from this design session. Every measurement, trend, host capacity, filesystem name, warning and freshness value is synthetic and visibly labelled.
- Reference SHA-256: `fae9772c872d5dfc3128886b598328b298f7d0f8b5873509f3a2465c1a0d41ae`.
- Render with the Visualize fragment runtime. Its optional design controls are preview tools, not product dependencies. The sample button demonstrates one update; production updates automatically under D5/D10.
- Reference width: 1024 px, main text: 16 px; support 20 px text. The four illustrated host rows are a fixture, not a limit. Preserve readable reflow on narrower screens rather than fitting every host by shrinking text.

## Composition

1. Compact header, followed by Attention: short warning summaries, affected host/resource and relevant work or evidence links when available. Do not retain the demo's invented warning count.
2. One shared grid: host identity and sample age; CPU percentage and line chart; RAM used/total, percentage, capacity bar and line chart; used/total and capacity bar for each filesystem. Keep row and column alignment across hosts.
3. CPU and RAM plots cover 30 minutes on the same 0–100% scale. Label time endpoints and share scale/window labels where unambiguous. Disk bars show current utilization; detailed disk history remains available below the overview.
4. Keep resource warnings beside the affected value and in Attention. Use the same calm fill for ordinary capacity and line marks. Threshold marks and warning colors require approved production thresholds; 85/90% in the preview are illustrative only.
5. Immediately after the host overview: the accepted [project board block](boards.md), followed by the remaining D10 supporting views, history and diagnostics. No production demo banner, simulation button or animation playground.

## Update reference

Show the final number immediately; tint its background locally for about 1.3 s. A bar may settle over about 0.6 s. These are review defaults, not a reason to delay data. Percentage deltas use percentage points (`pp`). CPU increase stays neutral unless a defined status changes. Update Attention and the row consistently from one sample. Reduced motion removes transitions while retaining the new value and status.

The preview replaces its fixture rows for convenience; production must preserve row identity, focus, scroll and expanded state under W4. Do not copy that rendering shortcut as product behavior.

## Acceptance

Compare the implementation against this reference with the same synthetic fixture and with live resource events. Include long host/filesystem names, several disks, missing values/history, stale readings, rapid consecutive refreshes, digit-width changes and threshold transitions. Verify both themes, 16/20 px text, narrow reflow and reduced motion. A missing value stays explicit; no synthetic chart may appear in the live view.

Design approval is not implementation or deployment proof. Producer contract/readback: [agentq #84](https://github.com/taskqorg/agentq/issues/84). Consumer implementation: [dashq #23](https://github.com/taskqorg/dashq/issues/23).
