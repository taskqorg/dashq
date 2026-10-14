# TaskQ resource controls in DashQ

Current combined visual authority: [published first-screen reference and pinned TaskQ sources](first-screen.md#published-reference). Older previews do not override it.

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

## Route (#47)

DashQ's half of TaskQ's `taskq.resources/1` wire, built before the target-host adapter ([taskq#931](https://github.com/taskqorg/taskq/issues/931)) by owner decision; writes stay disabled until that adapter ships and live qualification passes. Contract source: [fixture/taskq-resources/resource-policy-v1.json](../../fixture/taskq-resources/resource-policy-v1.json), copied unchanged from taskq `64b43c8` and replayed by `cargo test --locked policy`.

- Off unless config has `resources: true`; otherwise every route answers 503 and nothing is stored or delivered.
- Grants: `dashq resources grant WORKSPACE SUBJECT INSTALLATION HOURS [host] [project=ID].. [runtime=ID]..`, run by the host administrator, binds a workspace member to the installation's live enrollment generation and prints `grant_id` and the derived `actor_id`; `dashq resources revoke GRANT_ID` ends it at once. A grant is usable only while unrevoked, unexpired, its subject still a member and that generation still live. Answer capability, membership alone and device credentials grant nothing here.
- Browser routes (`X-Dashq-Subject` from the guard): `GET /api/taskq/resources/v1/grants` returns `{schema, csrf, grants}` for the subject. `POST .../operations` (Submit), `GET .../operations/{op_id}` and `POST .../operations/{op_id}/cancel` (Cancel) follow the TaskQ payloads exactly. Mutations need `Origin: https://<public_host>`, `Content-Type: application/json` and `X-Dashq-Csrf` (HMAC of subject, route and UTC day); bodies are at most 16 KiB, duplicate or unknown keys and non-integer integers are refused, and no path, command or config field exists. The submit's `actor_id` and `grant_id` must name the subject's own grant, whose project, runtime or `host_policy` scope and installation/generation cover the target.
- Store: one `resource_ops` row per op ID binding the canonical submit; the same object repeats the latest receipt, any change is `replay_mismatch`. Pending is capped at 32 per installation and 8 per actor; completed bindings are kept 24 h past expiry, at most 4096 per installation, and a full table refuses new IDs. Lifetime 1–300 s, issued at most 30 s ahead; an accepted submit is 202 with a pending receipt.
- Exchange: `POST /machine/resources/v1/exchange` with the installation's enrollment bearer credential (the machine prefix, which the proxy passes without the browser guard; #110). The body names that installation and generation only; at most 8 receipts in, 8 operations and 8 cancellations out, at least 5 s apart. Pending submits are resent until a receipt arrives; before delivery a cancel, the deadline or an inactive grant ends the operation without a write; a grant that stops being active after delivery sends a cancellation.
- Receipts: a host report moves a non-final operation; a completed write is never undone by a later cancel or expiry report. `applied` only when the reported effective revision equals the saved revision and its readback is at most 60 s old and not in the future; a stale or future match is `unknown`, another revision stays `saved`. A receipt for another target or generation is ignored.
- Not here: the read projection and its observation scope (taskq#931 A4), and the Work mode embedding.

- TaskQ #896 owns Auto and its compatibility floor; #913 owns independent enrollment. Legacy controls remain version-aware; this work never enables Auto on existing hosts silently.
- Shared resources build on the answer-card asset mechanism, not the answer HTTP route. TaskQ native adoption and DashQ rendering run the same fixtures.
- Release observation and ordinary answers independently. Enable resource writes only after installed target/guard/grant tests and exact-version evidence: duplicate/lost response, wrong node/workspace, stale policy, offline expiry, revoked grant, restart, partial results and restored baseline.
- Existing render fidelity applies: 16/20 px, both themes, narrow widths, keyboard/touch, charts and drafts preserved. Auto is not a spending or provider-quota guarantee.
