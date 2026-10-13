# TaskQ resource controls in DashQ

Accepted under [D14](../../dashq.md#d14-taskq-resource-controls). The [TaskQ shared-resource contract](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-resource-controls.md) owns component behavior, policy semantics and target-host adapter. Reuse the versioned static assets from the shared-card delivery mechanism; no independently maintained renderer or copied Auto rules.

Composition follows [first-screen F-A1–F-A5](first-screen.md); compact blocks inherit [TaskQ B1–B4](https://github.com/taskqorg/taskq/blob/main/docs/design/shared-blocks.md). The dashboard shell may place or disclose the shared component, never fork its internal layout/icons or remove required controls.

## Placement and scope

- Connected node rows expose runtime usage/effective limits, Auto/Fixed or the legacy guard, current blockers and policy/sample age. Open the same Work mode editor beside the selected node. Preserve the first-screen resource charts and board layout; detailed settings appear on demand.
- Named host, selected registered projects/runtimes, optional ceilings and advanced fields follow TaskQ. A view is not a grant to change all projects. Unselected or unknown values stay untouched; mixed values remain Mixed.
- Add/connect node follows existing enrollment. Unsupported installation, provider login or project creation stays outside this resource editor with its actual supported setup route, never a pretend working button.
- Keep AgentQ resource history distinct from TaskQ admission measurements. Missing capability is read-only with a reason and a link; a stale/offline host cannot be shown as applied or idle.

## Authorized operations

Separate resource-policy authorization from `/answer`, observation enrollment and read access. Resolve the authenticated actor, workspace, enrolled installation and explicit host-operator resource grant server-side. Enforce session/CSRF/origin protections and target scope; never accept arbitrary paths, shell commands, credentials or permission changes.

Use the existing DashQ store and TaskQ enrolled service for bounded typed changes/receipts, through the canonical target-host adapter. DashQ never edits a local taskq.json and calls that remote success. The adapter contract and shared conformance fixtures must be published before implementing its transport; no new supervisor, general remote-execution channel or task queue.

Preview exact host/project/runtime old→new changes. Confirmation binds that diff and expected policy revision; new edits invalidate the confirmation. Record one scoped immutable operation ID, reconcile ambiguous delivery with it, and reject stale/changed/revoked/expired operations. Reserve bounded persistence before accepting a mutation; a full store refuses rather than losing a pending change.

Display saved/pending/applied/rejected/unknown per target. Applied requires fresh matching effective readback from TaskQ's actual host authority, not HTTP success, a saved config or a receipt from the dashboard store. Offline changes must expire according to the adapter contract, never unexpectedly apply after indefinite absence. Cancellation/revocation races must have a definite tested outcome. Running tasks continue when limits shrink.

## Gates

- TaskQ #896 owns Auto and its compatibility floor; #913 owns independent enrollment. Legacy controls remain version-aware; this work never enables Auto on existing hosts silently.
- Shared resources build on the answer-card asset mechanism, not the answer HTTP route. TaskQ native adoption and DashQ rendering run the same fixtures.
- Release observation and ordinary answers independently. Enable resource writes only after installed target/guard/grant tests and exact-version evidence: duplicate/lost response, wrong node/workspace, stale policy, offline expiry, revoked grant, restart, partial results and restored baseline.
- Existing render fidelity applies: 16/20 px, both themes, narrow widths, keyboard/touch, charts and drafts preserved. Auto is not a spending or provider-quota guarantee.
